import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, mkdirSync, mkdtempSync, openSync, closeSync, existsSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {validateStagedArtifacts, validateVersions} from './release.mjs';

const repository = 'Zleap-AI/SAG-Wiki';
const workflow = '.github/workflows/desktop-release.yml';
export const components = ['documentation', 'frontend', 'rust-macos', 'rust-windows', 'build-macos', 'build-windows'];
const jobNames = {documentation:'Build bilingual documentation', frontend:'Test frontend', 'rust-macos':'Test Rust macos', 'rust-windows':'Test Rust windows', 'build-macos':'Build macos', 'build-windows':'Build windows'};
const gh = (...args) => {
  const result = spawnSync('gh', args, {encoding:'utf8', maxBuffer:16 * 1024 * 1024, timeout:60000});
  assert.equal(result.status, 0, `GitHub request failed: ${result.stderr}`);
  return JSON.parse(result.stdout);
};
const api = path => gh('api', `repos/${repository}/${path}`);
const pages = (path, field) => gh('api', '--paginate', '--slurp', `repos/${repository}/${path}${path.includes('?') ? '&' : '?'}per_page=100`).flatMap(page => page[field]);

// The workflow conclusion is deliberately irrelevant: each checkpoint stands on
// its own, including successful platforms from failed or still-running runs.
export function validateRun(run, _jobs, {commit, workflowId, currentRunId}) {
  assert.equal(run.repository?.full_name, repository, 'Wrong repository');
  assert.equal(run.head_repository?.full_name, repository, 'Fork source');
  assert.equal(run.head_sha, commit, 'Wrong source commit');
  assert.equal(run.workflow_id, workflowId, 'Wrong workflow');
  assert.equal(run.path, workflow, 'Wrong workflow path');
  assert.ok((run.head_branch === 'main' && (run.event === 'push' || (run.id === currentRunId && run.event === 'workflow_dispatch'))) ||
    (/^v\d+\.\d+\.\d+$/.test(run.head_branch ?? '') && ['push', 'workflow_dispatch'].includes(run.event)), 'Untrusted source ref or event');
  assert.ok(Number.isSafeInteger(run.id) && run.id > 0, 'Invalid run ID');
}

export function latestJob(jobs, name) {
  const candidates = jobs.filter(job => job.name === name);
  if (!candidates.length) return undefined;
  assert.ok(candidates.every(job => Number.isSafeInteger(job.run_attempt) && job.run_attempt > 0), 'Invalid job attempt');
  const latest = Math.max(...candidates.map(job => job.run_attempt));
  const matches = candidates.filter(job => job.run_attempt === latest);
  assert.equal(matches.length, 1, `Duplicate job: ${name}`);
  return matches[0];
}

export function componentState(run, jobs, component) {
  assert.ok(components.includes(component), 'Unknown component');
  const validation = latestJob(jobs, 'Validate release source');
  const legacy = component.match(/^(?:build|rust)-(macos|windows)$/)?.[1];
  const job = latestJob(jobs, jobNames[component]) ?? (legacy && latestJob(jobs, `Build and test ${legacy}`));
  for (const prerequisite of [validation, job]) {
    if (!prerequisite) return {state:run.status === 'completed' ? 'failure' : 'pending'};
    if (prerequisite.conclusion !== 'success') return {state:prerequisite.status === 'completed' || prerequisite.conclusion ? 'failure' : 'pending', job:prerequisite};
  }
  return {state:'success', job};
}

function selectArchive(artifacts, run, name, job, now = Date.now()) {
  const matches = artifacts.filter(a => a.name === name);
  assert.equal(matches.length, 1, `Missing or duplicate artifact: ${name}`);
  const a = matches[0];
  assert.equal(a.expired, false, 'Expired artifact');
  assert.ok(Date.parse(a.expires_at) > now, 'Expired artifact timestamp');
  assert.match(a.digest, /^sha256:[a-f0-9]{64}$/, 'Missing artifact digest');
  assert.ok(Number.isSafeInteger(a.id) && a.id > 0, 'Invalid artifact ID');
  assert.equal(a.workflow_run?.id, run.id, 'Wrong artifact run');
  assert.equal(a.workflow_run?.head_sha, run.head_sha, 'Wrong artifact commit');
  assert.equal(a.workflow_run?.repository_id, run.repository.id, 'Wrong artifact repository');
  assert.equal(a.workflow_run?.head_repository_id, run.repository.id, 'Fork artifact');
  assert.ok(Date.parse(a.created_at) >= Date.parse(job.started_at) && Date.parse(a.created_at) <= Date.parse(job.completed_at), 'Artifact predates the accepted platform build');
  return a;
}

export function selectArtifact(artifacts, run, platform, job, now = Date.now()) {
  return {...selectArchive(artifacts, run, `desktop-${platform}-${run.head_sha}${job.name.startsWith('Build and test ') ? '' : `-${job.run_attempt}`}`, job, now), platform};
}

export function verifyArchive(bytes, expected) {
  assert.equal(`sha256:${createHash('sha256').update(bytes).digest('hex')}`, expected, 'Artifact archive digest mismatch');
}

function context() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository, 'Unexpected repository');
  assert.match(process.env.GITHUB_SHA ?? '', /^[a-f0-9]{40}$/);
  const currentRunId = Number(process.env.GITHUB_RUN_ID);
  assert.ok(Number.isSafeInteger(currentRunId) && currentRunId > 0);
  return {commit:process.env.GITHUB_SHA, currentRunId, workflowId:api('actions/workflows/desktop-release.yml').id};
}

const completedSnapshots = new Map();
function snapshots(ctx, includeCurrent) {
  const runs = pages(`actions/workflows/desktop-release.yml/runs?head_sha=${ctx.commit}`, 'workflow_runs');
  if (includeCurrent && !runs.some(run => run.id === ctx.currentRunId)) runs.unshift(api(`actions/runs/${ctx.currentRunId}`));
  const result = [];
  for (const run of runs) {
    // Only older producers can be reused: concurrent tag/main runs cannot wait
    // on each other or create circular receipt chains.
    if (run.id > ctx.currentRunId || (!includeCurrent && run.id === ctx.currentRunId)) continue;
    try { validateRun(run, [], ctx); }
    catch { continue; }
    const key = `${run.id}:${run.run_attempt}:${run.updated_at}`;
    if (run.status === 'completed' && completedSnapshots.has(key)) { result.push(completedSnapshots.get(key)); continue; }
    const source = {run, jobs:pages(`actions/runs/${run.id}/jobs?filter=all`, 'jobs')};
    let artifacts;
    Object.defineProperty(source, 'artifacts', {get:() => artifacts ??= pages(`actions/runs/${run.id}/artifacts`, 'artifacts')});
    result.push(source);
    if (run.status === 'completed') completedSnapshots.set(key, source);
  }
  return result;
}

export function validateReceipt(receipt, source, component, job) {
  assert.equal(receipt.schema, 1, 'Unknown checkpoint schema');
  assert.equal(receipt.commit, source.run.head_sha, 'Wrong checkpoint commit');
  assert.equal(receipt.component, component, 'Wrong checkpoint component');
  assert.ok(Number.isSafeInteger(receipt.producer_run) && receipt.producer_run > 0 && receipt.producer_run <= source.run.id, 'Invalid checkpoint producer');
  assert.ok(Number.isSafeInteger(receipt.producer_job) && receipt.producer_job > 0, 'Invalid checkpoint job');
  assert.ok(Number.isSafeInteger(receipt.producer_attempt) && receipt.producer_attempt > 0, 'Invalid checkpoint attempt');
  if (receipt.producer_run === source.run.id) {
    assert.equal(receipt.producer_job, job.id, 'Local checkpoint job mismatch');
    assert.equal(receipt.producer_attempt, job.run_attempt, 'Local checkpoint attempt mismatch');
  }
  return receipt;
}

const receiptCache = new Map();
function readReceipt(source, component, job) {
  const artifact = selectArchive(source.artifacts, source.run, `checkpoint-${component}-${source.run.head_sha}-${job.run_attempt}`, job);
  const cacheKey = `${artifact.id}:${artifact.digest}`;
  if (receiptCache.has(cacheKey)) return receiptCache.get(cacheKey);
  const directory = mkdtempSync(join(tmpdir(), 'sag-checkpoint-'));
  try {
    downloadArtifacts([{...artifact, platform:component}], directory);
    assert.deepEqual(readdirSync(join(directory, component)), [`${component}.json`], 'Unexpected checkpoint archive contents');
    const receipt = JSON.parse(readFileSync(join(directory, component, `${component}.json`), 'utf8'));
    receiptCache.set(cacheKey, receipt);
    return receipt;
  } finally { rmSync(directory, {recursive:true, force:true}); }
}

export function selectComponent(sources, component, {currentRunId, gate = false, receiptReader = readReceipt} = {}) {
  const current = sources.find(source => source.run.id === currentRunId);
  if (gate) {
    assert.ok(current, 'Current release run is missing');
    const state = componentState(current.run, current.jobs, component);
    assert.notEqual(state.state, 'failure', `Current release checkpoint failed: ${component}`);
    if (state.state === 'pending') return {state:'pending'};
  }
  let pending = false;
  for (const source of [...sources].sort((a,b) => b.run.id-a.run.id)) {
    const status = componentState(source.run, source.jobs, component);
    if (status.state === 'pending') { pending = true; continue; }
    if (status.state !== 'success') continue;
    try {
      let producer = source;
      let job = status.job;
      // Legacy aggregate jobs performed tests and compilation together. Modern
      // stages always upload a receipt, including local work, before succeeding.
      if (job.name === jobNames[component]) {
        const receipt = validateReceipt(receiptReader(source, component, job), source, component, job);
        producer = sources.find(candidate => candidate.run.id === receipt.producer_run);
        assert.ok(producer, 'Checkpoint producer unavailable');
        const actual = componentState(producer.run, producer.jobs, component);
        assert.equal(actual.state, 'success', 'Checkpoint producer latest attempt failed or is pending');
        job = actual.job;
        assert.equal(job.id, receipt.producer_job, 'Checkpoint producer job was superseded');
        assert.equal(job.run_attempt, receipt.producer_attempt, 'Checkpoint producer attempt was superseded');
      }
      const result = {state:'success', run:producer.run, job};
      if (component.startsWith('build-')) result.artifact = selectArtifact(producer.artifacts, producer.run, component.slice(6), job);
      return result;
    } catch (error) {
      // Current-run receipts are an explicit release gate: never mask a bad or
      // superseded current checkpoint using another source. Reuse discovery can
      // instead fall back to performing this component locally.
      if (gate && source.run.id === currentRunId) throw error;
      console.log(`Cannot reuse ${component} from run ${source.run.id}: ${error.message}`);
    }
  }
  return {state:pending ? 'pending' : 'failure'};
}

const sleep = milliseconds => new Promise(done => setTimeout(done, milliseconds));
export async function awaitComponents(ctx, requested, {gate, timeoutMs = 95 * 60 * 1000, pollMs = 60000, snapshot = snapshots, pause = sleep, receiptReader = readReceipt} = {}) {
  assert.ok(Number.isSafeInteger(timeoutMs) && timeoutMs >= 0 && timeoutMs <= 210 * 60 * 1000, 'Invalid wait timeout');
  assert.ok(Number.isSafeInteger(pollMs) && pollMs > 0 && pollMs <= 60000, 'Invalid poll interval');
  const deadline = Date.now() + timeoutMs;
  while (true) {
    const sources = snapshot(ctx, gate);
    const results = requested.map(component => ({component, ...selectComponent(sources, component, {...ctx, gate, receiptReader})}));
    if (results.every(result => result.state === 'success')) return results;
    if (!gate && results.some(result => result.state === 'failure')) return results;
    assert.ok(!results.some(result => result.state === 'failure'), `No verified source for: ${results.filter(r => r.state === 'failure').map(r => r.component).join(', ')}`);
    assert.ok(Date.now() < deadline, `Timed out waiting for: ${results.filter(r => r.state === 'pending').map(r => r.component).join(', ')}`);
    console.log(`Waiting for ${results.filter(r => r.state === 'pending').map(r => r.component).join(', ')}`);
    await pause(Math.min(pollMs, Math.max(1, deadline - Date.now())));
  }
}

async function reuseComponent(component) {
  assert.ok(components.includes(component), 'Unknown component');
  const ctx = context();
  let producer;
  try {
    const result = (await awaitComponents(ctx, [component], {gate:false}))[0];
    if (result.state === 'success') producer = result;
  } catch (error) { console.log(`Reuse unavailable; execute ${component} locally: ${error.message}`); }
  const run = api(`actions/runs/${ctx.currentRunId}`);
  validateRun(run, [], ctx);
  const local = latestJob(pages(`actions/runs/${ctx.currentRunId}/jobs?filter=all`, 'jobs'), jobNames[component]);
  assert.ok(local, 'Current checkpoint job unavailable');
  const receipt = {schema:1, commit:ctx.commit, component, producer_run:producer?.run.id ?? ctx.currentRunId,
    producer_job:producer?.job.id ?? local.id, producer_attempt:producer?.job.run_attempt ?? local.run_attempt};
  validateReceipt(receipt, {run}, component, local);
  mkdirSync('release-checkpoints', {recursive:true});
  writeFileSync(`release-checkpoints/${component}.json`, JSON.stringify(receipt) + '\n');
  appendFileSync(process.env.GITHUB_OUTPUT, `reuse=${Boolean(producer)}\n`);
  console.log(producer ? `Reuse ${component} checkpoint from run ${producer.run.id}` : `Execute ${component} locally`);
}

async function waitForRelease(platform) {
  assert.ok(['macos', 'all'].includes(platform), 'Use wait macos or wait all');
  const ctx = context();
  const requested = platform === 'macos' ? ['rust-macos', 'build-macos'] : components;
  const budget = Number(process.env.RELEASE_WAIT_TIMEOUT_MS ?? 210 * 60 * 1000);
  assert.ok(Number.isSafeInteger(budget) && budget >= 0 && budget <= 210 * 60 * 1000, 'Invalid wait timeout');
  const deadlinePath = '.release-wait-deadline';
  if (platform === 'macos') writeFileSync(deadlinePath, String(Date.now() + budget));
  assert.ok(existsSync(deadlinePath), 'Missing macOS release checkpoint deadline');
  const deadline = Number(readFileSync(deadlinePath, 'utf8'));
  assert.ok(Number.isSafeInteger(deadline), 'Invalid release checkpoint deadline');
  const results = await awaitComponents(ctx, requested, {gate:true,
    timeoutMs: Math.max(0, Math.min(budget, deadline - Date.now())),
    pollMs: Number(process.env.RELEASE_WAIT_POLL_MS ?? 60000)});
  const artifacts = results.flatMap(result => result.artifact ? [result.artifact] : []);
  // After Apple signing, preserve the exact original macOS input; final gating
  // rechecks provenance, but downloads only the still-missing Windows input.
  downloadArtifacts(artifacts.filter(a => platform === 'macos' || a.platform === 'windows'), 'release-artifacts');
  validateStagedArtifacts(resolve('release-artifacts'), validateVersions(), ctx.commit, platform === 'macos' ? ['macos'] : ['macos', 'windows']);
  console.log(`Verified ${requested.join(', ')}`);
}

function downloadArtifacts(artifacts, destination) {
  const temp = mkdtempSync(join(tmpdir(), 'sag-release-'));
  try {
    for (const artifact of artifacts) {
      const zip = join(temp, `${artifact.platform}.zip`);
      const fd = openSync(zip, 'wx');
      let result;
      try { result = spawnSync('gh', ['api', `repos/${repository}/actions/artifacts/${artifact.id}/zip`], {stdio:['ignore', fd, 'pipe'], timeout:180000}); }
      finally { closeSync(fd); }
      assert.equal(result.status, 0, `Artifact download failed: ${result.stderr}`);
      verifyArchive(readFileSync(zip), artifact.digest);
      const target = resolve(destination, artifact.platform);
      mkdirSync(target, {recursive:true});
      // Validate archive paths and reject symlinks before extracting into the signing workspace.
      const code = 'import zipfile,sys,pathlib,stat\nz=zipfile.ZipFile(sys.argv[1])\nfor i in z.infolist():\n p=pathlib.PurePosixPath(i.filename)\n assert not p.is_absolute() and ".." not in p.parts and "\\\\" not in i.filename and not stat.S_ISLNK(i.external_attr >> 16), "Unsafe archive entry"\nz.extractall(sys.argv[2])';
      const extracted = spawnSync(process.platform === 'win32' ? 'python' : 'python3', ['-c', code, zip, target], {encoding:'utf8'});
      assert.equal(extracted.status, 0, extracted.stderr);
    }
  } finally { rmSync(temp, {recursive:true, force:true}); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [command, argument] = process.argv.slice(2);
  assert.ok(['reuse', 'wait'].includes(command), 'Use reuse COMPONENT or wait macos|all');
  if (command === 'reuse') await reuseComponent(argument);
  else await waitForRelease(argument);
}
