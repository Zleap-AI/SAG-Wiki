import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync, spawn } from 'node:child_process';
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
test('npm package installs globally and forwards CLI arguments, stdin and exit codes', {timeout: 240000}, () => {
  execFileSync(process.execPath, ['scripts/package-npm-cli.mjs'], {cwd: root, stdio: 'pipe'});
  const version = readFileSync(join(root, 'cli/Cargo.toml'), 'utf8').match(/^version = "(.+)"/m)[1];
  const archive = join(root, 'releases/npm', `zleap-ai-sag-${version}.tgz`);
  const files = execFileSync('tar', ['-tzf', archive], {encoding: 'utf8'}).trim().split('\n');
  assert(files.includes('package/bin/sag.cjs'));
  assert(files.includes(`package/native/${process.platform}-${process.arch}/sag`));
  assert(files.every(file => ['package/README.md', 'package/package.json', 'package/bin/sag.cjs'].includes(file) || /^package\/native\/(darwin|linux|win32)-(arm64|x64)\/sag(\.exe)?$/.test(file)));
  const temp = mkdtempSync(join(tmpdir(), 'sag-npm-test-'));
  try {
    const project = join(temp, 'project'); mkdirSync(project);
    writeFileSync(join(project, 'note.md'), 'npm installation test content\n');
    execFileSync('npm', ['install', '--global', '--prefix', join(temp, 'prefix'), '--offline', '--ignore-scripts', '--no-audit', '--no-fund', archive], {cwd: temp, stdio: 'pipe'});
    const env = {...process.env, SAG_DATA_DIR: join(temp, 'data'), PATH: `${join(temp, 'prefix/bin')}:${dirname(process.execPath)}:${process.env.PATH}`};
    const run = (args, input) => spawnSync('sag', args, {cwd: project, env, input, encoding: 'utf8'});
    const versionOutput = run(['--version']);
    assert.equal(versionOutput.status, 0, versionOutput.stderr);
    assert.equal(versionOutput.stdout.trim(), `sag ${version}`);
    const added = run(['add', '--json']);
    assert.equal(added.status, 0, added.stderr);
    assert.equal(JSON.parse(added.stdout).index_status, 'ready');
    const config = run(['config', '--url', 'https://example.com/decisions', '--model', 'model with spaces', '--key-stdin'], 'npm-stdin-test-key\n');
    assert.equal(config.status, 0, config.stderr);
    assert.equal(JSON.parse(config.stdout).model, 'model with spaces');
    assert.equal(JSON.parse(config.stdout).has_key, true);
    assert(!`${config.stdout}${config.stderr}`.includes('npm-stdin-test-key'));
    assert.equal(run(['query', '-m', '0']).status, 2);
    assert.equal(run(['remove']).status, 0);
    assert.equal(readFileSync(join(project, 'note.md'), 'utf8'), 'npm installation test content\n');
  } finally { rmSync(temp, {recursive: true, force: true}); }
});


test('npm launcher forwards cancellation and reaps its native process', {timeout: 10000}, async () => {
  const temp = mkdtempSync(join(tmpdir(), 'sag-npm-cancel-'));
  let parent, nativePid;
  const stop = pid => {
    try { process.kill(pid, 'SIGKILL'); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
  };
  try {
    mkdirSync(join(temp, 'bin'));
    const nativeDir = join(temp, 'native', `${process.platform}-${process.arch}`);
    mkdirSync(nativeDir, {recursive: true});
    copyFileSync(join(root, 'npm/sag/bin/sag.cjs'), join(temp, 'bin/sag.cjs'));
    writeFileSync(join(nativeDir, 'sag'), `#!/bin/sh
exec "$SAG_TEST_NODE" -e 'require("node:fs").writeFileSync(process.env.SAG_TEST_PID,String(process.pid));setInterval(()=>{},1000)'
`, {mode: 0o755});
    const pidFile = join(temp, 'pid');
    parent = spawn(process.execPath, [join(temp, 'bin/sag.cjs')], {env: {...process.env, SAG_TEST_NODE: process.execPath, SAG_TEST_PID: pidFile}, stdio: 'ignore'});
    const exited = new Promise(resolve => parent.once('exit', (code, signal) => resolve({code, signal})));
    const deadline = Date.now() + 3000;
    while (!existsSync(pidFile) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
    assert(existsSync(pidFile), 'native command did not start');
    nativePid = Number(readFileSync(pidFile, 'utf8'));
    parent.kill('SIGTERM');
    assert.equal((await exited).code, 143);
    assert.throws(() => process.kill(nativePid, 0), {code: 'ESRCH'}, 'native command survived cancellation');
  } finally {
    if (parent && parent.exitCode === null && parent.signalCode === null) stop(parent.pid);
    if (nativePid) stop(nativePid);
    rmSync(temp, {recursive: true, force: true});
  }
});
