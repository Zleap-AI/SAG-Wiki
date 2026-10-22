// Run with node --test; kept outside Vitest's *.test.* discovery pattern.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join} from 'node:path';
import {createHash} from 'node:crypto';
import {generateKeyPairSync} from 'node:crypto';
import test from 'node:test';
import {verifyUpdate} from './verify-update.mjs';
import {targets, validateStagedArtifacts, validateEnvironment, validateVersions} from './release.mjs';
import {legacyDigest} from './package-updates.mjs';

const fixture = new URL('../src-tauri/src/update-fixtures/', import.meta.url);
const payload = readFileSync(new URL('SAG-Test.app.tar.gz', fixture));
const publicKey = readFileSync(new URL('public.key', fixture), 'utf8').trim();
const signature = readFileSync(new URL('SAG-Test.app.tar.gz.sig', fixture), 'utf8').trim();

test('verifies the real Tauri signed-version fixture', () => {
  assert.equal(verifyUpdate(payload, signature, publicKey, '9.9.9'), '9.9.9');
});

test('rejects a corrupted package and a different announced version', () => {
  assert.throws(() => verifyUpdate(Buffer.concat([payload, Buffer.from('changed')]), signature, publicKey, '9.9.9'));
  assert.throws(() => verifyUpdate(payload, signature, publicKey, '0.1.9'), /version/i);
});

test('rejects wrong public keys even when the key ID is copied', () => {
  const lines = Buffer.from(publicKey, 'base64').toString().trim().split('\n');
  const record = Buffer.from(lines[1], 'base64');
  const {publicKey: other} = generateKeyPairSync('ed25519');
  other.export({format: 'der', type: 'spki'}).subarray(-32).copy(record, 10);
  lines[1] = record.toString('base64');
  const substituted = Buffer.from(lines.join('\n') + '\n').toString('base64');
  assert.throws(() => verifyUpdate(payload, signature, substituted, '9.9.9'), /signature/i);
});

test('authenticates trusted version comments and the global signature', () => {
  const lines = Buffer.from(signature, 'base64').toString().trim().split('\n');
  const changedComment = [...lines];
  changedComment[2] = changedComment[2].replace('version:9.9.9', 'version:0.1.9');
  assert.throws(() => verifyUpdate(payload, Buffer.from(changedComment.join('\n')).toString('base64'), publicKey, '0.1.9'), /signature/i);
  const global = Buffer.from(lines[3], 'base64');
  global[0] ^= 1;
  lines[3] = global.toString('base64');
  assert.throws(() => verifyUpdate(payload, Buffer.from(lines.join('\n')).toString('base64'), publicKey, '9.9.9'), /signature/i);
});

test('rejects malformed or missing signatures', () => {
  for (const invalid of ['', 'not-base64', Buffer.from('incomplete').toString('base64')]) {
    assert.throws(() => verifyUpdate(payload, invalid, publicKey, '9.9.9'));
  }
});

test('checks current manifests and preserves the published legacy manifest byte for byte', () => {
  assert.match(validateVersions(), /^\d+\.\d+\.\d+$/);
  const legacy = readFileSync(new URL('../config/updates/legacy-latest.json', import.meta.url));
  assert.equal(createHash('sha256').update(legacy).digest('hex'), legacyDigest);
  assert.equal(JSON.parse(legacy).version, '0.1.8');
});

for (const [label, newline] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`accepts consistent release versions with ${label} checkout line endings`, () => {
    const directory = mkdtempSync(join(tmpdir(), 'sag-version-checkout-'));
    const version = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
    try {
      for (const name of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json',
        'cli/Cargo.toml', 'cli/Cargo.lock', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']) {
        const destination = join(directory, name);
        mkdirSync(dirname(destination), {recursive: true});
        const source = readFileSync(new URL(`../${name}`, import.meta.url), 'utf8');
        writeFileSync(destination, source.replace(/\r?\n/g, newline));
      }
      assert.equal(validateVersions(directory), version);
    } finally {
      rmSync(directory, {recursive: true, force: true});
    }
  });
}

test('requires both platforms from one commit and detects tampered transfer artifacts', () => {
  const directory = mkdtempSync(join(tmpdir(), 'sag-release-'));
  const version = '0.1.9';
  const commit = 'a'.repeat(40);
  const bytes = Buffer.from('isolated artifact transfer fixture');
  const checksum = createHash('sha256').update(bytes).digest('hex');
  try {
    for (const platform of Object.keys(targets)) {
      const path = join(directory, platform);
      mkdirSync(path);
      const names = platform === 'macos'
        ? [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`]
        : [`SAG-Wiki-${version}-Windows-x64-setup.exe`];
      for (const name of names) writeFileSync(join(path, name), bytes);
      writeFileSync(join(path, 'metadata.json'), JSON.stringify({schema: 1, version, commit, platform, target: targets[platform], build_identity: {version, channel: 'release'}, files: Object.fromEntries(names.map(name => [name, checksum]))}));
    }
    assert.equal(Object.keys(validateStagedArtifacts(directory, version, commit)).length, 2);
    assert.throws(() => validateStagedArtifacts(directory, version, 'b'.repeat(40)), /same release commit/);
    const windows = join(directory, 'windows', `SAG-Wiki-${version}-Windows-x64-setup.exe`);
    writeFileSync(windows, 'corrupted after upload');
    assert.throws(() => validateStagedArtifacts(directory, version, commit), /checksum/);
    writeFileSync(windows, bytes);
    rmSync(join(directory, 'windows'), {recursive: true});
    assert.throws(() => validateStagedArtifacts(directory, version, commit));
  } finally {
    rmSync(directory, {recursive: true, force: true});
  }
});

test('fails closed when release environment protection is incomplete', () => {
  const environment = {can_admins_bypass: false, deployment_branch_policy: {custom_branch_policies: true, protected_branches: false}, protection_rules: [{type: 'required_reviewers', prevent_self_review: true, reviewers: [{reviewer: {login: 'release-owner'}}]}]};
  const policies = [{type: 'tag', name: 'v*'}];
  assert.doesNotThrow(() => validateEnvironment(environment, policies));
  assert.throws(() => validateEnvironment({...environment, protection_rules: []}, policies));
  assert.throws(() => validateEnvironment(environment, [{type: 'branch', name: 'main'}]));
  assert.throws(() => validateEnvironment(environment, [...policies, {type: 'branch', name: '*'}]));
  assert.throws(() => validateEnvironment(environment, []));
});
