import assert from 'node:assert/strict';
import {mkdtempSync, readFileSync, rmSync, statSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {verifyUpdate} from './verify-update.mjs';
import {verifyEnvironment} from './release.mjs';

const repository = 'Zleap-AI/SAG-Wiki';
const root = resolve(import.meta.dirname, '..');
const {values} = parseArgs({options: Object.fromEntries(['key-file', 'password-file', 'reviewer'].map(name => [name, {type: 'string'}]))});
for (const name of ['key-file', 'password-file', 'reviewer']) assert.ok(values[name], `--${name} is required`);
const run = (args, input, sensitive = false) => {
  const result = spawnSync('gh', args, {encoding: 'utf8', input});
  assert.equal(result.status, 0, sensitive ? 'Secret upload failed; secret content was not logged' : `GitHub request failed: ${result.stderr?.trim()}`);
  return result.stdout;
};
const api = (path, method, body) => JSON.parse(run(['api', ...(method ? ['--method', method] : []), path, ...(body ? ['--input', '-'] : [])], body && JSON.stringify(body)));
const project = api(`repos/${repository}`);
assert.equal(project.permissions?.admin, true, 'Repository Admin is required to configure release protection and Environment Secrets');
const reviewer = api(`users/${encodeURIComponent(values.reviewer)}`);
const permission = api(`repos/${repository}/collaborators/${encodeURIComponent(reviewer.login)}/permission`);
assert.ok(['admin', 'maintain', 'write', 'triage', 'read'].includes(permission.permission), 'Reviewer needs repository access');

function secretFile(path) {
  const file = resolve(path);
  const stat = statSync(file);
  assert.ok(stat.isFile(), 'Secret input must be a file');
  if (process.platform !== 'win32') assert.equal(stat.mode & 0o077, 0, 'Secret files must be readable only by their owner (chmod 600)');
  return readFileSync(file, 'utf8').trim();
}
const key = secretFile(values['key-file']);
const password = secretFile(values['password-file']);
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const publicKey = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).plugins.updater.pubkey;
// Prove the supplied pair matches the embedded public key before changing the
// remote secrets. The challenge is ephemeral and contains no production data.
const temporary = mkdtempSync(join(tmpdir(), 'sag-key-check-'));
try {
  const challenge = join(temporary, 'challenge');
  writeFileSync(challenge, 'SAG Wiki release key validation\n', {mode: 0o600});
  const result = spawnSync(process.execPath, [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'sign', '--app-version', version, challenge], {
    encoding: 'utf8', env: {...process.env, TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password},
  });
  assert.equal(result.status, 0, 'Unable to sign the challenge; check the supplied encrypted key and password');
  verifyUpdate(readFileSync(challenge), readFileSync(challenge + '.sig', 'utf8').trim(), publicKey, version);
} finally {
  rmSync(temporary, {recursive: true, force: true});
}

api(`repos/${repository}/environments/release`, 'PUT', {
  wait_timer: 0, prevent_self_review: true, reviewers: [{type: 'User', id: reviewer.id}],
  deployment_branch_policy: {protected_branches: false, custom_branch_policies: true},
});
const policiesPath = `repos/${repository}/environments/release/deployment-branch-policies`;
const policies = api(policiesPath).branch_policies;
assert.ok(policies.every(policy => policy.type === 'tag' && policy.name === 'v*'), 'Existing release policies allow other refs; review them in Settings before continuing');
if (!policies.length) api(policiesPath, 'POST', {type: 'tag', name: 'v*'});
verifyEnvironment();
run(['secret', 'set', 'TAURI_SIGNING_PRIVATE_KEY', '--repo', repository, '--env', 'release'], key, true);
run(['secret', 'set', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '--repo', repository, '--env', 'release'], password, true);
const names = JSON.parse(run(['secret', 'list', '--repo', repository, '--env', 'release', '--json', 'name'])).map(secret => secret.name);
assert.ok(['TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD'].every(name => names.includes(name)), 'Both release secrets must exist');
console.log('Protected release environment and both signing secrets configured. No secret values were logged.');
