import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, mkdirSync, mkdtempSync, openSync, closeSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {validateSignedMacOSArtifacts, validateStagedArtifacts, validateVersions} from './release.mjs';

const repository = 'Zleap-AI/SAG-Wiki';
const workflow = '.github/workflows/desktop-release.yml';
const requiredJobs = ['Validate release source', 'Build bilingual documentation', 'Test frontend', 'Build and test macos', 'Build and test windows'];
const gh = (...args) => {
  const result = spawnSync('gh', args, {encoding:'utf8', maxBuffer:16 * 1024 * 1024, timeout:60000});
  assert.equal(result.status, 0, `GitHub request failed: ${result.stderr}`);
  return JSON.parse(result.stdout);
};
const api = path => gh('api', `repos/${repository}/${path}`);
const pages = (path, field) => gh('api', '--paginate', '--slurp', `repos/${repository}/${path}${path.includes('?') ? '&' : '?'}per_page=100`).flatMap(page => page[field]);

export function validateRun(run, jobs, {commit, workflowId, currentRunId}) {
  assert.equal(run.repository?.full_name, repository, 'Wrong repository');
  assert.equal(run.head_repository?.full_name, repository, 'Fork source');
  assert.equal(run.head_sha, commit, 'Wrong source commit');
  assert.equal(run.workflow_id, workflowId, 'Wrong workflow');
  assert.equal(run.path, workflow, 'Wrong workflow path');
  if (run.id !== currentRunId) {
    assert.equal(run.head_branch, 'main', 'Reuse requires main');
    assert.equal(run.event, 'push', 'Reuse requires trusted push');
    assert.equal(run.status, 'completed', 'Source still running');
    assert.equal(run.conclusion, 'success', 'Source failed');
  }
  for (const name of requiredJobs) successfulJob(jobs, name);
}

function successfulJob(jobs, name) {
  const candidates = jobs.filter(job => job.name === name);
  assert.ok(candidates.every(job => Number.isSafeInteger(job.run_attempt) && job.run_attempt > 0), 'Invalid job attempt');
  const latest = Math.max(...candidates.map(job => job.run_attempt));
  const matches = candidates.filter(job => job.run_attempt === latest);
  assert.equal(matches.length, 1, `Missing or duplicate job: ${name}`);
  assert.equal(matches[0].conclusion, 'success', `Unsuccessful job: ${name}`);
  return matches[0];
}

function selectArtifact(artifacts, run, name, now) {
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
    return a;
}

export function selectArtifacts(artifacts, run, now = Date.now()) {
  return ['macos', 'windows'].map(platform => ({...selectArtifact(artifacts, run, `desktop-${platform}-${run.head_sha}`, now), platform}));
}

function validateProtectedRun(run, jobs, ctx, jobName) {
  assert.equal(run.id, ctx.currentRunId, 'Signing must belong to this release run');
  assert.equal(run.repository?.full_name, repository, 'Wrong signing repository');
  assert.equal(run.head_repository?.full_name, repository, 'Fork signing source');
  assert.equal(run.head_sha, ctx.commit, 'Wrong signing commit');
  assert.equal(run.workflow_id, ctx.workflowId, 'Wrong signing workflow');
  assert.equal(run.path, workflow, 'Wrong signing workflow path');
  assert.match(run.head_branch ?? '', /^v\d+\.\d+\.\d+$/, 'Signing requires a version tag');
  assert.ok(['push', 'workflow_dispatch'].includes(run.event), 'Untrusted signing event');
  return successfulJob(jobs, jobName);
}

export function validateSignedRun(run, jobs, ctx) {
  return validateProtectedRun(run, jobs, ctx, 'Sign and notarize macOS');
}

export function selectSignedArtifact(artifacts, run, now = Date.now()) {
  return selectArtifact(artifacts, run, `desktop-macos-signed-${run.head_sha}`, now);
}

export function verifyArchive(bytes, expected) {
  assert.equal(`sha256:${createHash('sha256').update(bytes).digest('hex')}`, expected, 'Artifact archive digest mismatch');
}

function inspect(runId, context) {
  const run = api(`actions/runs/${runId}`);
  // A release-only retry retains successful prerequisite jobs from earlier attempts.
  // For each job, a newer failed/skipped attempt always supersedes an older success.
  const jobs = pages(`actions/runs/${runId}/jobs?filter=all`, 'jobs');
  validateRun(run, jobs, context);
  const artifacts = selectArtifacts(pages(`actions/runs/${runId}/artifacts`, 'artifacts'), run);
  for (const artifact of artifacts) {
    const job = jobs.filter(job => job.name === `Build and test ${artifact.platform}`).sort((a,b) => b.run_attempt-a.run_attempt)[0];
    assert.ok(Date.parse(artifact.created_at) >= Date.parse(job.started_at) && Date.parse(artifact.created_at) <= Date.parse(job.completed_at), 'Artifact predates the accepted platform build');
  }
  return artifacts;
}

function context(lookupWorkflow = true) {
  assert.equal(process.env.GITHUB_REPOSITORY, repository, 'Unexpected repository');
  assert.match(process.env.GITHUB_SHA ?? '', /^[a-f0-9]{40}$/);
  const currentRunId = Number(process.env.GITHUB_RUN_ID);
  assert.ok(Number.isSafeInteger(currentRunId) && currentRunId > 0);
  return {commit:process.env.GITHUB_SHA, currentRunId, workflowId:lookupWorkflow ? api('actions/workflows/desktop-release.yml').id : undefined};
}

function resolveBuild() {
  const ctx = context(false);
  let runId = ctx.currentRunId;
  if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
    let runs = [];
    try {
      ctx.workflowId = api('actions/workflows/desktop-release.yml').id;
      runs = pages(`actions/workflows/desktop-release.yml/runs?head_sha=${ctx.commit}&branch=main&event=push&status=success`, 'workflow_runs');
    } catch { console.log('Reuse lookup unavailable; build and test this commit instead.'); }
    for (const run of runs) {
      try { inspect(run.id, ctx); runId = run.id; break; }
      catch (error) { console.log(`Cannot reuse run ${run.id}: ${error.message}`); }
    }
  }
  const reuse = runId !== ctx.currentRunId;
  appendFileSync(process.env.GITHUB_OUTPUT, `source_run=${runId}\nreuse=${reuse}\n`);
  console.log(reuse ? `Reuse verified main build ${runId}` : 'Build and test this commit');
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

function downloadBuild() {
  const ctx = context();
  const runId = Number(process.env.SOURCE_RUN);
  assert.ok(Number.isSafeInteger(runId) && runId > 0, 'Invalid source run');
  downloadArtifacts(inspect(runId, ctx), 'release-artifacts');
  validateStagedArtifacts(resolve('release-artifacts'), validateVersions(), ctx.commit);
  console.log(`Verified both platform archives and contents from run ${runId}`);
}

function downloadSignedBuild() {
  const ctx = context();
  const run = api(`actions/runs/${ctx.currentRunId}`);
  const jobs = pages(`actions/runs/${run.id}/jobs?filter=all`, 'jobs');
  const job = validateSignedRun(run, jobs, ctx);
  const artifact = selectSignedArtifact(pages(`actions/runs/${run.id}/artifacts`, 'artifacts'), run);
  assert.ok(Date.parse(artifact.created_at) >= Date.parse(job.started_at) && Date.parse(artifact.created_at) <= Date.parse(job.completed_at), 'Signed artifact predates the accepted signing job');
  downloadArtifacts([{...artifact, platform:'macos'}], 'signed-release-artifacts');
  const source = validateStagedArtifacts(resolve('release-artifacts'), validateVersions(), ctx.commit);
  validateSignedMacOSArtifacts(resolve('signed-release-artifacts/macos'), source.macos);
  console.log(`Verified signed macOS archive and notarization metadata from run ${run.id}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const commands = {resolve:resolveBuild, download:downloadBuild, 'download-signed':downloadSignedBuild};
  assert.ok(commands[process.argv[2]], 'Use resolve, download, download-signed');
  commands[process.argv[2]]();
}
