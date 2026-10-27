import {createHash} from 'node:crypto';
import {chmodSync, closeSync, existsSync, mkdtempSync, openSync, readFileSync, realpathSync, rmSync, statSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {fileURLToPath} from 'node:url';
import {verifyEnvironment} from './release.mjs';

const sourceRepository = 'Zleap-AI/SAG';
const workflow = '.github/workflows/wiki-apple-secrets.yml';
const jobName = 'Seal Apple credentials for Wiki release';
const target = Object.freeze({repository: 'Zleap-AI/SAG-Wiki', environment: 'release', key_id: '3380204578043523366', public_key: 'hkOy5CM6+Y3LSFZADmvSnbb/MMnWTpCL+U6XQn/PzSU='});
const names = ['APPLE_CERTIFICATE_BASE64', 'APPLE_CERTIFICATE_PASSWORD', 'APPLE_ID', 'APPLE_APP_SPECIFIC_PASSWORD', 'APPLE_TEAM_ID'];
const destination = `repos/${target.repository}/environments/${target.environment}/secrets`;
class SafeError extends Error {}
const check = (condition, message) => { if (!condition) throw new SafeError(message); };
const positive = value => Number.isSafeInteger(value) && value > 0;
const exactKeys = (value, keys) => value !== null && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));

export function parseCli(args) {
  let values;
  try { ({values} = parseArgs({args, options: {'run-id': {type: 'string'}, 'source-commit': {type: 'string'}, apply: {type: 'boolean', default: false}}})); }
  catch { throw new SafeError('Use --run-id <id> --source-commit <approved full SAG main commit> [--apply].'); }
  check(/^[1-9][0-9]*$/.test(values['run-id'] ?? '') && positive(Number(values['run-id'])), 'A positive --run-id is required.');
  check(/^[a-f0-9]{40}$/.test(values['source-commit'] ?? ''), '--source-commit must be the explicitly approved full SAG main commit.');
  return {runId: Number(values['run-id']), commit: values['source-commit'], apply: values.apply};
}

export function validateSourceRun(run, jobs, {runId, commit, workflowId}) {
  check(positive(runId) && /^[a-f0-9]{40}$/.test(commit ?? '') && positive(workflowId), 'Invalid approved source context.');
  check(run?.id === runId && run.repository?.full_name === sourceRepository && run.head_repository?.full_name === sourceRepository && positive(run.repository.id) && run.head_repository.id === run.repository.id, 'Wrong source run or repository.');
  check(run.head_sha === commit && run.workflow_id === workflowId && run.path === workflow, 'Source commit or workflow does not match approval.');
  check(run.event === 'workflow_dispatch' && run.head_branch === 'main' && run.status === 'completed' && run.conclusion === 'success' && positive(run.run_attempt), 'Source must be a successful manual SAG main run.');
  const candidates = jobs.filter(job => job.name === jobName);
  check(candidates.length > 0 && candidates.every(job => positive(job.run_attempt) && job.run_attempt <= run.run_attempt), 'Invalid sealing job attempts.');
  const current = candidates.filter(job => job.run_attempt === run.run_attempt);
  check(current.length === 1 && current[0].status === 'completed' && current[0].conclusion === 'success', 'The current run attempt must have exactly one successful sealing job.');
  check(Number.isFinite(Date.parse(current[0].started_at)) && Date.parse(current[0].completed_at) >= Date.parse(current[0].started_at), 'Invalid sealing job timestamps.');
  return current[0];
}

export function selectHandoffArtifact(artifacts, run, job, now = Date.now()) {
  const matches = artifacts.filter(artifact => artifact.name === `apple-secrets-for-sag-wiki-${run.id}-${run.run_attempt}`);
  check(matches.length === 1, 'Expected exactly one handoff artifact for the current run attempt.');
  const artifact = matches[0];
  check(positive(artifact.id) && artifact.expired === false && Date.parse(artifact.expires_at) > now && /^sha256:[a-f0-9]{64}$/.test(artifact.digest ?? ''), 'Expired or unverifiable handoff artifact.');
  check(artifact.workflow_run?.id === run.id && artifact.workflow_run.head_sha === run.head_sha && artifact.workflow_run.repository_id === run.repository.id && artifact.workflow_run.head_repository_id === run.repository.id, 'Artifact source does not match the approved run.');
  check(Date.parse(artifact.created_at) >= Date.parse(job.started_at) && Date.parse(artifact.created_at) <= Date.parse(job.completed_at), 'Artifact was not created during the accepted sealing job.');
  return artifact;
}

export function validateEnvelope(envelope, run, publicKey) {
  check(exactKeys(envelope, ['schema', 'target', 'source', 'credentials']) && envelope.schema === 1, 'Invalid handoff envelope schema.');
  check(exactKeys(envelope.target, Object.keys(target)) && Object.entries(target).every(([key, value]) => envelope.target[key] === value), 'Handoff target differs from the reviewed destination.');
  check(publicKey?.key_id === target.key_id && publicKey.key === target.public_key, 'Live destination public key differs from the reviewed key.');
  const expected = {repository: sourceRepository, workflow, commit: run.head_sha, run_id: run.id, run_attempt: run.run_attempt, ref: 'refs/heads/main'};
  check(exactKeys(envelope.source, Object.keys(expected)) && Object.entries(expected).every(([key, value]) => envelope.source[key] === value), 'Handoff source does not match the approved run.');
  check(exactKeys(envelope.credentials, names), 'Handoff must contain exactly the five Apple credential names.');
  for (const name of names) {
    const value = envelope.credentials[name];
    check(typeof value === 'string' && value.length <= 4 * 1024 * 1024 && /^[A-Za-z0-9+/]+={0,2}$/.test(value), 'Invalid sealed credential encoding.');
    const bytes = Buffer.from(value, 'base64');
    check(bytes.length >= 48 && bytes.toString('base64') === value, 'Invalid sealed credential size or encoding.');
  }
}

export function readHandoffArchive(path, digest, python = process.platform === 'win32' ? 'python' : 'python3') {
  check(statSync(path).size <= 4 * 1024 * 1024, 'Handoff archive exceeds the size limit.');
  check(/^sha256:[a-f0-9]{64}$/.test(digest ?? '') && `sha256:${createHash('sha256').update(readFileSync(path)).digest('hex')}` === digest, 'Handoff archive digest mismatch.');
  const code = 'import sys,zipfile,stat,json\ndef unique(pairs):\n assert len(dict(pairs))==len(pairs)\n return dict(pairs)\nwith zipfile.ZipFile(sys.argv[1]) as z:\n items=z.infolist(); assert len(items)==1\n i=items[0]; mode=i.external_attr>>16\n assert i.orig_filename==i.filename=="apple-secrets-for-sag-wiki.json" and not i.is_dir() and stat.S_IFMT(mode) in (0,stat.S_IFREG) and i.file_size<=4194304 and not i.flag_bits&1\n data=z.read(i); json.loads(data.decode("utf-8"),object_pairs_hook=unique); sys.stdout.buffer.write(data)';
  const result = spawnSync(python, ['-c', code, path], {encoding: 'utf8', maxBuffer: 4 * 1024 * 1024, timeout: 60000});
  check(result.status === 0, 'Unsafe or unreadable handoff archive. Python 3 is required.');
  try { return JSON.parse(result.stdout); }
  catch { throw new SafeError('Invalid handoff archive JSON.'); }
}

export function applyEncryptedSecrets(envelope, run, {apply = false} = {}, {api = githubApi, verifyProtectedEnvironment = verifyEnvironment} = {}) {
  validateEnvelope(envelope, run, api(`${destination}/public-key`));
  const existing = paged(api, destination, 'secrets').map(secret => secret.name);
  check(!names.some(name => existing.includes(name)), 'Apple destination secrets already exist; no values were written. Review existing or partial credentials first.');
  if (!apply) return {applied: false, count: names.length};
  try { verifyProtectedEnvironment(); }
  catch { throw new SafeError('Release environment protection verification failed; no values were written.'); }
  const completed = [];
  for (const name of names) {
    try { api(`${destination}/${name}`, 'PUT', {encrypted_value: envelope.credentials[name], key_id: target.key_id}); }
    catch { throw new SafeError(`Apple credential import stopped: partial result, ${completed.length} of 5 writes confirmed (${completed.join(', ') || 'none'}). The failed request may also have completed. Inspect destination names before retrying; existing values will not be overwritten.`); }
    completed.push(name);
  }
  return {applied: true, count: names.length};
}

function githubApi(path, method = 'GET', body) {
  const result = spawnSync('gh', ['api', '--method', method, path, ...(body ? ['--input', '-'] : [])], {encoding: 'utf8', input: body && JSON.stringify(body), maxBuffer: 4 * 1024 * 1024, timeout: 60000});
  check(result.status === 0, 'GitHub request failed; response details were suppressed.');
  try { return result.stdout.trim() ? JSON.parse(result.stdout) : {}; }
  catch { throw new SafeError('GitHub returned an invalid response.'); }
}

function paged(api, path, field) {
  const items = [];
  let total;
  for (let page = 1; page <= 100; page++) {
    const response = api(`${path}${path.includes('?') ? '&' : '?'}per_page=100&page=${page}`);
    check(Array.isArray(response[field]) && response[field].length <= 100 && Number.isSafeInteger(response.total_count) && response.total_count >= 0, 'Invalid GitHub collection response.');
    total ??= response.total_count;
    check(response.total_count === total, 'GitHub collection changed during review.');
    items.push(...response[field]);
    check(items.length <= total, 'Invalid GitHub collection count.');
    if (response[field].length < 100) { check(items.length === total, 'Incomplete GitHub collection response.'); return items; }
  }
  throw new SafeError('GitHub collection exceeds the review limit.');
}

function main() {
  const options = parseCli(process.argv.slice(2));
  const prefix = `repos/${sourceRepository}/actions`;
  const workflowInfo = githubApi(`${prefix}/workflows/wiki-apple-secrets.yml`);
  check(workflowInfo.path === workflow, 'Unexpected source workflow path.');
  const run = githubApi(`${prefix}/runs/${options.runId}`);
  const job = validateSourceRun(run, paged(githubApi, `${prefix}/runs/${run.id}/jobs?filter=all`, 'jobs'), {...options, workflowId: workflowInfo.id});
  const artifact = selectHandoffArtifact(paged(githubApi, `${prefix}/runs/${run.id}/artifacts`, 'artifacts'), run, job);
  const directory = mkdtempSync(join(tmpdir(), 'wiki-apple-sealed-'));
  try {
    chmodSync(directory, 0o700);
    const path = join(directory, 'sealed.zip');
    const fd = openSync(path, 'wx', 0o600);
    let download;
    try { download = spawnSync('gh', ['api', `${prefix}/artifacts/${artifact.id}/zip`], {stdio: ['ignore', fd, 'pipe'], timeout: 60000}); }
    finally { closeSync(fd); }
    check(download.status === 0, 'Encrypted handoff archive download failed.');
    const result = applyEncryptedSecrets(readHandoffArchive(path, artifact.digest), run, options);
    console.log(`${result.applied ? 'Applied' : 'Dry-run verified'} ${result.count} encrypted Apple credentials for ${target.repository} / ${target.environment}, SAG run ${run.id} attempt ${run.run_attempt}.`);
  } finally { rmSync(directory, {recursive: true, force: true}); }
}

if (process.argv[1] && existsSync(process.argv[1]) && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); }
  catch (error) {
    console.error(error instanceof SafeError ? error.message : 'Apple credential import failed; response details were suppressed.');
    process.exitCode = 1;
  }
}
