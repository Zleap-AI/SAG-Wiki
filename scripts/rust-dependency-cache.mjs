import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, readFileSync, writeFileSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve, join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {targets} from './release.mjs';
import {gitAt, validateReleaseSource} from './automatic-release.mjs';
import {validateRun, componentState, selectArchive, downloadArtifacts, hashFile,latestJob} from './release-build-cache.mjs';

const repository = 'Zleap-AI/SAG-Wiki';
const localPackages = new Set(['sag-protocol', 'sag-client', 'sag-core', 'sag-cli', 'sag-desktop']);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const execute = (program, args, options = {}) => {
  const result = spawnSync(program, args, {encoding:'utf8', timeout:180000, maxBuffer:32 * 1024 * 1024, ...options});
  assert.equal(result.status, 0, `${program} failed: ${result.error?.message ?? result.stderr}`);
  return result.stdout.trim();
};
const api = path => JSON.parse(execute('gh', ['api', `repos/${repository}/${path}`]));
const pages = (path, field) => JSON.parse(execute('gh', ['api', '--paginate', '--slurp', `repos/${repository}/${path}${path.includes('?') ? '&' : '?'}per_page=100`])).flatMap(page => page[field]);

// Release preparation changes only our package versions. Keep every dependency,
// feature, profile and config byte in the key; never normalize third-party versions.
export function normalizeCargo(path, contents) {
  const lock = path.endsWith('Cargo.lock');
  return contents.split(lock ? /(?=^\[\[package\]\]$)/m : /(?=^\[[^\n]+\]$)/m).map(section => {
    if (!(lock ? section.startsWith('[[package]]') : section.startsWith('[package]'))) return section;
    const name = section.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    return localPackages.has(name) ? section.replace(/^version\s*=\s*"[^"]+"/m, 'version = "LOCAL"') : section;
  }).join('');
}

export function dependencyKey(git, commit, context) {
  const files = git('ls-tree', '-r', '--name-only', commit).split('\n').filter(path => /(?:^|\/)Cargo\.(toml|lock)$|(?:^|\/)\.cargo\/config\.toml$/.test(path)).sort();
  assert.ok(files.length, 'Missing Cargo inputs');
  return digest(JSON.stringify({schema:2, compression:'zstd', ...context, files:files.map(path => [path, normalizeCargo(path, git('show', `${commit}:${path}`))])}));
}

export function trustedProducer(run, jobs, context) {
  assert.match(run.head_branch ?? '', /^v\d+\.\d+\.\d+$/, 'Only version tags may supply dependency archives');
  assert.ok(run.id < context.currentRunId, 'Only older producers may be reused');
  validateRun(run, jobs, {...context, commit:run.head_sha});
  if (context.kind === 'build' && context.role === 'cli') {
    assert.equal(latestJob(jobs,'Validate release source')?.conclusion,'success','Producer validation did not pass');
    const job=latestJob(jobs,`Compile CLI ${context.platform}`);
    assert.equal(job?.conclusion,'success','Producer CLI compiler did not pass');
    return job;
  }
  const component = `${context.kind === 'test' ? 'rust' : 'build'}-${context.platform}`;
  const state = componentState(run, jobs, component);
  assert.equal(state.state, 'success', 'Producer checks did not pass');
  return state.job;
}

export function profileCompatibility(kind,env=process.env) {
  if(kind==='test') return {profile:`test-debug-${env.CARGO_PROFILE_TEST_DEBUG??'1'}-cgu-${env.CARGO_PROFILE_TEST_CODEGEN_UNITS??'16'}-assert-${env.CARGO_PROFILE_TEST_DEBUG_ASSERTIONS??'true'}-overflow-${env.CARGO_PROFILE_TEST_OVERFLOW_CHECKS??'true'}`};
  const role=env.CACHE_ROLE??'desktop';assert.ok(['cli','desktop'].includes(role),'Unknown build cache role');
  return {profile:'release',role};
}

export function verifyMetadata(metadata, expected, payloadHash) {
  assert.deepEqual(metadata, {...expected, payload_sha256:payloadHash}, 'Dependency archive metadata mismatch');
}

function context() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  const kind = process.env.CACHE_KIND;
  const platform = process.env.RELEASE_PLATFORM;
  assert.ok(['test', 'build'].includes(kind) && Object.hasOwn(targets, platform));
  const target = process.env.TARGET_TRIPLE;
  assert.equal(target, targets[platform]);
  const commit = process.env.GITHUB_SHA;
  assert.match(commit ?? '', /^[a-f0-9]{40}$/);
  const currentRunId = Number(process.env.GITHUB_RUN_ID);
  assert.ok(Number.isSafeInteger(currentRunId) && currentRunId > 0);
  const compatibility = {platform, kind, target, os:process.platform, arch:process.arch, rust:execute('rustc', ['--version']), ...profileCompatibility(kind)};
  const git = gitAt(process.cwd());
  const key = dependencyKey(git, commit, compatibility);
  return {kind, platform, target, commit, currentRunId, compatibility, key, git,role:compatibility.role};
}

const name = (ctx, attempt, validation = false) => `${validation ? 'validation' : 'trusted'}-rust-deps-${ctx.platform}-${ctx.kind}${ctx.role==='cli'?'-cli':''}-${ctx.key}-${attempt}`;
const metadataFor = (ctx, run, job) => ({schema:1, commit:run.head_sha, ref:run.head_branch, run:run.id, job:job.id, attempt:job.run_attempt, key:ctx.key, compatibility:ctx.compatibility});
const archive = (command, directory, file, ctx) => execute(process.env.PYTHON ?? (process.platform === 'win32' ? 'python' : 'python3'), [resolve('scripts/rust-dependency-archive.py'), command, directory, file, ctx.kind, ctx.target], {timeout:300000});
const output = (key, value) => appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${value}\n`);

async function restore(ctx) {
  const temporary = mkdtempSync(join(tmpdir(), 'sag-rust-cache-'));
  try {
    const workflowId = api('actions/workflows/desktop-release.yml').id;
    const runs = api('actions/workflows/desktop-release.yml/runs?per_page=30').workflow_runs;
    for (const run of runs) {
      if (run.id >= ctx.currentRunId || !/^v\d+\.\d+\.\d+$/.test(run.head_branch ?? '')) continue;
      try {
        // Reject incompatible inputs before fetching job/artifact histories.
        if (dependencyKey(ctx.git, run.head_sha, ctx.compatibility) !== ctx.key) continue;
        const jobs = pages(`actions/runs/${run.id}/jobs?filter=all`, 'jobs');
        const job = trustedProducer(run, jobs, {...ctx, workflowId});
        // Recompute trust and compatibility from Git, not from artifact metadata.
        validateReleaseSource(ctx.git, run.head_sha, run.head_branch);
        const artifacts = pages(`actions/runs/${run.id}/artifacts`, 'artifacts');
        const artifact = selectArchive(artifacts, run, name(ctx, job.run_attempt), job);
        const destination = join(temporary, String(run.id));
        downloadArtifacts([{...artifact, platform:'dependencies'}], destination);
        const directory = join(destination, 'dependencies');
        verifyMetadata(JSON.parse(readFileSync(join(directory, 'metadata.json'))), metadataFor(ctx, run, job), hashFile(join(directory, 'payload.tar.zst')));
        console.log(archive('restore', process.env.CARGO_TARGET_DIR, join(directory, 'payload.tar.zst'), ctx));
        console.log(`Restored trusted dependency outputs from ${run.head_branch}, run ${run.id}, job ${job.id}`);
        output('hit', 'true');
        return;
      } catch (error) { console.log(`Dependency candidate ${run.id} unavailable: ${error.message.slice(0, 300)}`); }
    }
  } finally { rmSync(temporary, {recursive:true, force:true}); }
  output('hit', 'false');
  console.log('No compatible trusted dependency archive; compile normally');
}

function pack(ctx) {
  const tag = process.env.GITHUB_REF?.startsWith('refs/tags/');
  if (tag) validateReleaseSource(ctx.git, ctx.commit, process.env.GITHUB_REF_NAME);
  const run = api(`actions/runs/${ctx.currentRunId}`);
  const jobs = pages(`actions/runs/${ctx.currentRunId}/jobs?filter=all`, 'jobs');
  const job = jobs.find(job => job.name === `${ctx.kind === 'test' ? 'Test Rust' : ctx.role === 'cli' ? 'Compile CLI' : 'Build'} ${ctx.platform}` && job.run_attempt === Number(process.env.GITHUB_RUN_ATTEMPT));
  assert.ok(job, 'Current producing job unavailable');
  if (tag) validateRun(run, jobs, {commit:ctx.commit, currentRunId:ctx.currentRunId, workflowId:api('actions/workflows/desktop-release.yml').id});
  const directory = mkdtempSync(join(process.env.RUNNER_TEMP ?? tmpdir(), 'rust-dependency-export-'));
  const file = join(directory, 'payload.tar.zst');
  console.log(archive('pack', process.env.CARGO_TARGET_DIR, file, ctx));
  writeFileSync(join(directory, 'metadata.json'), JSON.stringify({...metadataFor(ctx, run, job), payload_sha256:hashFile(file)}) + '\n');
  if (!tag) {
    // Feature-branch CI measures a round trip without emitting a trusted cache.
    const validation = mkdtempSync(join(process.env.RUNNER_TEMP ?? tmpdir(), 'rust-dependency-validation-'));
    try { console.log(archive('restore', validation, file, ctx)); }
    finally { rmSync(validation, {recursive:true, force:true}); }
  }
  output('directory', directory);
  output('artifact', name(ctx, job.run_attempt, !tag));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const command = process.argv[2];
  assert.ok(['restore', 'pack'].includes(command));
  try {
    const ctx = context();
    if (command === 'restore') await restore(ctx);
    else pack(ctx);
  } catch (error) {
    // Cache availability must never become a release gate or bypass a check.
    console.log(`${command === 'restore' ? 'Dependency cache unavailable; compile normally' : 'Dependency export unavailable; keep verified outputs'}: ${error.message.slice(0, 500)}`);
    if (command === 'restore') output('hit', 'false');
    else process.exitCode = 1;
  }
}
