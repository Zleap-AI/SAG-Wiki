import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import test from 'node:test';

const implementation = () => import('./import-apple-signing-secrets.mjs');
const commit = 'a'.repeat(40);
const context = {runId: 123, commit, workflowId: 456};
const target = {repository: 'Zleap-AI/SAG-Wiki', environment: 'release', key_id: '3380204578043523366', public_key: 'hkOy5CM6+Y3LSFZADmvSnbb/MMnWTpCL+U6XQn/PzSU='};
const names = ['APPLE_CERTIFICATE_BASE64', 'APPLE_CERTIFICATE_PASSWORD', 'APPLE_ID', 'APPLE_APP_SPECIFIC_PASSWORD', 'APPLE_TEAM_ID'];
const ciphertext = Buffer.alloc(64, 91).toString('base64'); // Disposable format fixture; producer tests prove encryption.
const run = () => ({id: 123, repository: {id: 12, full_name: 'Zleap-AI/SAG'}, head_repository: {id: 12, full_name: 'Zleap-AI/SAG'}, head_sha: commit, workflow_id: 456, path: '.github/workflows/wiki-apple-secrets.yml', head_branch: 'main', event: 'workflow_dispatch', status: 'completed', conclusion: 'success', run_attempt: 2});
const job = () => ({name: 'Seal Apple credentials for Wiki release', run_attempt: 2, status: 'completed', conclusion: 'success', started_at: '2026-10-27T10:00:00Z', completed_at: '2026-10-27T10:03:00Z'});
const artifact = () => ({id: 789, name: 'apple-secrets-for-sag-wiki-123-2', expired: false, expires_at: '2099-01-01T00:00:00Z', created_at: '2026-10-27T10:02:00Z', digest: `sha256:${'b'.repeat(64)}`, workflow_run: {id: 123, head_sha: commit, repository_id: 12, head_repository_id: 12}});
const envelope = () => ({schema: 1, target: {...target}, source: {repository: 'Zleap-AI/SAG', workflow: '.github/workflows/wiki-apple-secrets.yml', commit, run_id: 123, run_attempt: 2, ref: 'refs/heads/main'}, credentials: Object.fromEntries(names.map(name => [name, ciphertext]))});

test('CLI requires an explicit full approved source commit and defaults to dry-run', async () => {
  const {parseCli} = await implementation();
  assert.deepEqual(parseCli(['--run-id', '123', '--source-commit', commit]), {runId: 123, commit, apply: false});
  assert.equal(parseCli(['--run-id', '123', '--source-commit', commit, '--apply']).apply, true);
  for (const args of [[], ['--run-id', '123'], ['--run-id', '123', '--source-commit', 'main'], ['--run-id', '1e2', '--source-commit', commit], ['--run-id', '123', '--source-commit', commit, '--unknown']]) assert.throws(() => parseCli(args));
});

test('CLI invoked through a directory alias still rejects invalid arguments before any GitHub request', () => {
  const directory = mkdtempSync(join(tmpdir(), 'wiki-import-cli-'));
  try {
    const alias = join(directory, 'scripts');
    symlinkSync(fileURLToPath(new URL('.', import.meta.url)), alias, process.platform === 'win32' ? 'junction' : 'dir');
    const result = spawnSync(process.execPath, [join(alias, 'import-apple-signing-secrets.mjs'), '--unknown'], {encoding: 'utf8'});
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /Use --run-id/);
    assert.equal(result.stdout, '');
  } finally { rmSync(directory, {recursive: true, force: true}); }
});

test('importing from node stdin does not execute the CLI', () => {
  const result = spawnSync(process.execPath, ['--input-type=module', '-'], {input: `await import(${JSON.stringify(new URL('./import-apple-signing-secrets.mjs', import.meta.url).href)});`, encoding: 'utf8'});
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, '');
  assert.equal(result.stdout, '');
});

test('accepts only the approved manual SAG main run and its current successful seal job', async () => {
  const {validateSourceRun} = await implementation();
  assert.deepEqual(validateSourceRun(run(), [job()], context), job());
  for (const patch of [{id: 1}, {repository: {id: 12, full_name: 'evil/repo'}}, {head_repository: {id: 9, full_name: 'evil/repo'}}, {head_sha: 'b'.repeat(40)}, {workflow_id: 9}, {path: 'other.yml'}, {event: 'push'}, {head_branch: 'feature'}, {status: 'in_progress'}, {conclusion: 'failure'}, {run_attempt: 1}]) assert.throws(() => validateSourceRun({...run(), ...patch}, [job()], context));
  for (const jobs of [[], [job(), job()], [{...job(), run_attempt: 1}], [{...job(), conclusion: 'failure'}], [job(), {...job(), run_attempt: 3, conclusion: 'failure'}]]) assert.throws(() => validateSourceRun(run(), jobs, context));
});

test('rejects wrong, duplicated, stale, expired and unverifiable artifacts', async () => {
  const {selectHandoffArtifact} = await implementation();
  assert.equal(selectHandoffArtifact([artifact()], run(), job()).id, 789);
  for (const patch of [{id: 0}, {name: 'apple-secrets-for-sag-wiki-123-1'}, {expired: true}, {expires_at: '2000-01-01'}, {digest: null}, {created_at: '2026-10-27T09:59:59Z'}, {created_at: '2026-10-27T10:03:01Z'}, ...['id', 'head_sha', 'repository_id', 'head_repository_id'].map(key => ({workflow_run: {...artifact().workflow_run, [key]: 'wrong'}}))]) assert.throws(() => selectHandoffArtifact([{...artifact(), ...patch}], run(), job()));
  assert.throws(() => selectHandoffArtifact([artifact(), artifact()], run(), job()));
  assert.throws(() => selectHandoffArtifact([], run(), job()));
});

test('envelope binds exact five sealed values to the reviewed target, source and live public key', async () => {
  const {validateEnvelope} = await implementation();
  validateEnvelope(envelope(), run(), {key_id: target.key_id, key: target.public_key});
  const invalid = [null, {...envelope(), schema: 2}, {...envelope(), unexpected: true}, {...envelope(), target: {...target, repository: 'evil/repo'}}, {...envelope(), target: {...target, key_id: '1'}}, {...envelope(), source: {...envelope().source, run_id: 124}}, {...envelope(), source: {...envelope().source, commit: 'b'.repeat(40)}}, {...envelope(), source: {...envelope().source, run_attempt: 1}}, {...envelope(), source: {...envelope().source, ref: 'refs/heads/feature'}}];
  for (const value of ['plaintext password', 'release@example.test', Buffer.from('short').toString('base64'), ciphertext + '\n', ciphertext.slice(0, -2) + '!=']) invalid.push({...envelope(), credentials: {...envelope().credentials, APPLE_ID: value}});
  invalid.push({...envelope(), credentials: {...envelope().credentials, EXTRA: ciphertext}});
  const missing = envelope(); delete missing.credentials.APPLE_TEAM_ID; invalid.push(missing);
  for (const value of invalid) assert.throws(() => validateEnvelope(value, run(), {key_id: target.key_id, key: target.public_key}));
  assert.throws(() => validateEnvelope(envelope(), run(), {key_id: 'new', key: target.public_key}));
  assert.throws(() => validateEnvelope(envelope(), run(), {key_id: target.key_id, key: Buffer.alloc(32).toString('base64')}));
});

function boundary({existing = [], failAfter = Infinity} = {}) {
  const writes = [];
  let protectedChecks = 0;
  const api = (path, method = 'GET', body) => {
    if (method === 'PUT') {
      if (writes.length >= failAfter) throw new Error(`unsafe upstream response ${ciphertext}`);
      writes.push({path, body}); return {};
    }
    if (path.endsWith('/public-key')) return {key_id: target.key_id, key: target.public_key};
    if (path.includes('/secrets?')) return {total_count: existing.length, secrets: existing.map(name => ({name}))};
    assert.fail(`Unexpected request: ${path}`);
  };
  return {api, writes, verifyProtectedEnvironment: () => { protectedChecks++; }, get protectedChecks() { return protectedChecks; }};
}

test('dry-run validates the live key and checks existing names without any writes', async () => {
  const {applyEncryptedSecrets} = await implementation();
  const api = boundary();
  assert.deepEqual(applyEncryptedSecrets(envelope(), run(), {}, api), {applied: false, count: 5});
  assert.deepEqual(api.writes, []);
  assert.equal(api.protectedChecks, 0);
});

test('existing destination credential aborts before every write, including across pagination', async () => {
  const {applyEncryptedSecrets} = await implementation();
  for (const name of names) {
    const api = boundary({existing: [name]});
    assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, api), /already exist/);
    assert.deepEqual(api.writes, []);
  }
  const api = boundary();
  const get = api.api;
  api.api = (path, ...args) => path.endsWith('&page=1') ? {total_count: 101, secrets: Array.from({length: 100}, (_, i) => ({name: `OTHER_${i}`}))} : path.endsWith('&page=2') ? {total_count: 101, secrets: [{name: 'APPLE_ID'}]} : get(path, ...args);
  assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, api), /already exist/);
  assert.deepEqual(api.writes, []);
});

test('key rotation and malformed handoffs are rejected before writing any destination secret', async () => {
  const {applyEncryptedSecrets} = await implementation();
  const rotated = boundary();
  const get = rotated.api;
  rotated.api = (path, ...args) => path.endsWith('/public-key') ? {key_id: 'rotated', key: target.public_key} : get(path, ...args);
  assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, rotated), /public key/);
  assert.deepEqual(rotated.writes, []);
  const api = boundary();
  const wrong = envelope(); wrong.credentials.APPLE_ID = 'plaintext@example.test';
  assert.throws(() => applyEncryptedSecrets(wrong, run(), {apply: true}, api), /encoding/);
  assert.deepEqual(api.writes, []);
});

test('an incomplete destination secret inventory fails closed before any write', async () => {
  const {applyEncryptedSecrets} = await implementation();
  const api = boundary();
  const get = api.api;
  api.api = (path, ...args) => path.includes('/secrets?') ? {total_count: 1, secrets: []} : get(path, ...args);
  assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, api), /collection/);
  assert.deepEqual(api.writes, []);
});

test('apply checks protection then sends only five expected encrypted values and reviewed key ID', async () => {
  const {applyEncryptedSecrets} = await implementation();
  const api = boundary();
  assert.deepEqual(applyEncryptedSecrets(envelope(), run(), {apply: true}, api), {applied: true, count: 5});
  assert.equal(api.protectedChecks, 1);
  assert.deepEqual(api.writes, names.map(name => ({path: `repos/Zleap-AI/SAG-Wiki/environments/release/secrets/${name}`, body: {encrypted_value: ciphertext, key_id: '3380204578043523366'}})));
  const denied = boundary();
  denied.verifyProtectedEnvironment = () => { throw new Error('unprotected'); };
  assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, denied), /protection/);
  assert.deepEqual(denied.writes, []);
});

test('a partial upload failure reports only completed names and never upstream payloads', async () => {
  const {applyEncryptedSecrets} = await implementation();
  const api = boundary({failAfter: 2});
  assert.throws(() => applyEncryptedSecrets(envelope(), run(), {apply: true}, api), error => /partial/i.test(error.message) && /2 of 5/.test(error.message) && /APPLE_CERTIFICATE_BASE64/.test(error.message) && !error.message.includes(ciphertext));
  assert.equal(api.writes.length, 2);
});

test('ZIP digest and exact single regular filename are verified before parsing in memory', async () => {
  const {readHandoffArchive} = await implementation();
  const python = [process.env.SAG_TEST_PYTHON, process.platform === 'win32' ? 'python' : 'python3'].filter(Boolean).find(command => spawnSync(command, ['-c', 'import zipfile'], {stdio: 'ignore'}).status === 0);
  assert.ok(python, 'Python 3 is required for standard-library ZIP validation');
  const directory = mkdtempSync(join(tmpdir(), 'wiki-sealed-test-'));
  try {
    const zip = join(directory, 'handoff.zip');
    const make = (entry, mode = 'regular', duplicate = false, payload = JSON.stringify(envelope())) => {
      const result = spawnSync(python, ['-c', 'import sys,zipfile\np,n,k,d=sys.argv[1:]\nwith zipfile.ZipFile(p,"w") as z:\n i=zipfile.ZipInfo(n); i.create_system=3; i.external_attr=({"regular":0o100600,"symlink":0o120777,"fifo":0o010600}[k]<<16); z.writestr(i,sys.stdin.read());\n if d=="true": z.writestr(n,b"{}");', zip, entry, mode, String(duplicate)], {input: payload, encoding: 'utf8'});
      assert.equal(result.status, 0, result.stderr);
      return `sha256:${createHash('sha256').update(readFileSync(zip)).digest('hex')}`;
    };
    const digest = make('apple-secrets-for-sag-wiki.json');
    assert.deepEqual(readHandoffArchive(zip, digest, python), envelope());
    assert.throws(() => readHandoffArchive(zip, `sha256:${'0'.repeat(64)}`, python), /digest/);
    for (const [entry, mode, duplicate] of [['../apple-secrets-for-sag-wiki.json'], ['/apple-secrets-for-sag-wiki.json'], ['other.json'], ['dir/'], ['apple-secrets-for-sag-wiki.json', 'symlink'], ['apple-secrets-for-sag-wiki.json', 'fifo'], ['apple-secrets-for-sag-wiki.json', 'regular', true]]) {
      assert.throws(() => readHandoffArchive(zip, make(entry, mode, duplicate), python), /archive/);
    }
    make('apple-secrets-for-sag-wiki.jsonXX');
    writeFileSync(zip, Buffer.from(readFileSync(zip).toString('latin1').replaceAll('apple-secrets-for-sag-wiki.jsonXX', 'apple-secrets-for-sag-wiki.json\0X'), 'latin1'));
    assert.throws(() => readHandoffArchive(zip, `sha256:${createHash('sha256').update(readFileSync(zip)).digest('hex')}`, python), /archive/);
    const duplicateKeyDigest = make('apple-secrets-for-sag-wiki.json', 'regular', false, JSON.stringify(envelope()).replace('"schema":1', '"schema":1,"schema":1'));
    assert.throws(() => readHandoffArchive(zip, duplicateKeyDigest, python), /archive/);
  } finally { rmSync(directory, {recursive: true, force: true}); }
});
