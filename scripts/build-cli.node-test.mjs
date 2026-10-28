import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {buildCli, cliBuildPlan, resolveCliTargetDir} from './build-cli.mjs';

test('standalone builds retain the CLI target directory and locked release flags', () => {
  const cwd = resolve('fixture repo');
  const plan = cliBuildPlan({cwd, env: {}, platform: 'linux'});
  assert.equal(plan.source, join(cwd, 'cli/target/release/sag'));
  assert.equal(plan.destination, join(cwd, 'src-tauri/bin'));
  assert.deepEqual(plan.args, ['build', '--locked', '--release', '--manifest-path', 'cli/Cargo.toml']);
});

test('an absolute shared target path with spaces selects the requested release target', () => {
  const cwd = resolve('fixture repo');
  const shared = resolve('shared build cache');
  const target = 'aarch64-apple-darwin';
  const env = {CARGO_TARGET_DIR: shared, TAURI_ENV_TARGET_TRIPLE: target};
  const plan = cliBuildPlan({cwd, env, platform: 'darwin'});
  assert.equal(resolveCliTargetDir(cwd, env), shared);
  assert.equal(plan.source, join(shared, target, 'release/sag'));
  assert.deepEqual(plan.args.slice(-2), ['--target', target]);
});

test('a relative shared directory is relative to the invocation cwd', () => {
  const cwd = resolve('fixture repo');
  const plan = cliBuildPlan({cwd, env: {CARGO_TARGET_DIR: 'src-tauri/target'}, platform: 'linux'});
  assert.equal(plan.source, join(cwd, 'src-tauri/target/release/sag'));
});

test('development builds ignore the Tauri target and retain development destination', () => {
  const cwd = resolve('fixture repo');
  const plan = cliBuildPlan({debug: true, cwd, env: {CARGO_TARGET_DIR: 'src-tauri/target', TAURI_ENV_TARGET_TRIPLE: 'x86_64-pc-windows-msvc'}, platform: 'darwin'});
  assert.equal(plan.target, undefined);
  assert.equal(plan.source, join(cwd, 'src-tauri/target/debug/sag'));
  assert.equal(plan.destination, join(cwd, 'src-tauri/bin/development'));
  assert.deepEqual(plan.args, ['build', '--locked', '--manifest-path', 'cli/Cargo.toml']);
});

test('Windows target and native builds select sag.exe', () => {
  const cwd = resolve('fixture repo');
  const target = 'x86_64-pc-windows-msvc';
  const cross = cliBuildPlan({cwd, platform: 'darwin', env: {TAURI_ENV_TARGET_TRIPLE: target, CARGO_TARGET_DIR: 'src-tauri/target'}});
  assert.equal(cross.source, join(cwd, 'src-tauri/target', target, 'release/sag.exe'));
  assert.equal(cross.windows, true);
  const native = cliBuildPlan({cwd, platform: 'win32', debug: true, env: {}});
  assert.equal(native.source, join(cwd, 'cli/target/debug/sag.exe'));
});

function fixture(t) {
  const cwd = mkdtempSync(join(tmpdir(), 'sag cli build '));
  t.after(() => rmSync(cwd, {recursive: true, force: true}));
  return cwd;
}

test('copying a shared release artifact retains signing and skips cross-target execution', t => {
  const cwd = fixture(t);
  const env = {CARGO_TARGET_DIR: join(cwd, 'shared target'), TAURI_ENV_TARGET_TRIPLE: 'aarch64-apple-darwin', APPLE_SIGNING_IDENTITY: 'fixture identity'};
  const plan = cliBuildPlan({cwd, env, platform: 'darwin'});
  const calls = [];
  const run = (command, args, options) => {
    calls.push([command, args]);
    assert.equal(options.cwd, cwd);
    assert.equal(options.env, env);
    if (command === 'cargo') {
      mkdirSync(dirname(plan.source), {recursive: true});
      writeFileSync(plan.source, 'compiled CLI fixture');
    } else {
      assert.equal(command, 'codesign');
      assert.deepEqual(args, ['--force', '--sign', 'fixture identity', join(plan.destination, 'sag')]);
    }
    return {status: 0};
  };
  assert.equal(buildCli({cwd, env, platform: 'darwin', run}), 0);
  assert.equal(readFileSync(join(plan.destination, 'sag'), 'utf8'), 'compiled CLI fixture');
  assert.equal(calls.length, 2);
  assert.equal(existsSync(join(plan.destination, 'build-identity.json')), false);
});

test('a development artifact is copied from the shared cache and its identity is checked', t => {
  const cwd = fixture(t);
  const env = {CARGO_TARGET_DIR: 'src-tauri/target'};
  const plan = cliBuildPlan({cwd, env, debug: true, platform: 'linux'});
  const run = (command, args) => {
    if (command === 'cargo') {
      mkdirSync(dirname(plan.source), {recursive: true});
      writeFileSync(plan.source, 'development CLI fixture');
      return {status: 0};
    }
    assert.equal(command, join(plan.destination, 'sag'));
    assert.deepEqual(args, ['--build-info']);
    return {status: 0, stdout: JSON.stringify({channel: 'development', version: 'fixture'})};
  };
  assert.equal(buildCli({cwd, env, debug: true, platform: 'linux', run}), 0);
  assert.deepEqual(JSON.parse(readFileSync(join(plan.destination, 'build-identity.json'), 'utf8')), {channel: 'development', version: 'fixture'});
});

test('build failure propagates without copying any artifact', t => {
  const cwd = fixture(t);
  assert.equal(buildCli({cwd, env: {}, run: () => ({status: 7})}), 7);
  assert.equal(existsSync(join(cwd, 'src-tauri/bin')), false);
});

test('a mismatched local artifact channel is rejected', t => {
  const cwd = fixture(t);
  const plan = cliBuildPlan({cwd, env: {}, platform: 'linux'});
  mkdirSync(dirname(plan.source), {recursive: true});
  writeFileSync(plan.source, 'wrong channel fixture');
  assert.throws(() => buildCli({cwd, env: {}, platform: 'linux', run: command => command === 'cargo' ? {status: 0} : {status: 0, stdout: '{"channel":"development"}'}}), /channel does not match/);
});
