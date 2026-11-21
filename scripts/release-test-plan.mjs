import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {parse} from 'yaml';
import {gitAt, latestPublished, validateReleaseSource} from './automatic-release.mjs';
import {latestJob, validateRun} from './release-build-cache.mjs';
import {normalizeCargo} from './rust-dependency-cache.mjs';

export const suites = {
  protocol: {manifest: 'protocol', inputs: ['protocol']},
  client: {manifest: 'client', inputs: ['client', 'protocol']},
  core: {manifest: 'core', inputs: ['core', 'client', 'protocol', 'prompts', 'cli/skills', 'scripts/docling']},
  cli: {manifest: 'cli', inputs: ['cli', 'core', 'client', 'protocol', 'prompts', 'scripts/docling']},
  desktop: {manifest: 'src-tauri', inputs: ['src-tauri', 'client', 'protocol']},
};
const legacyStep = 'Run native tests with locked dependencies';
const suiteStep = suite => `Run native Rust suite: ${suite}`;

function matchesSuite(path, suite) {
  if (!suite) return true;
  if (suites[suite].inputs.some(dir => path.startsWith(dir + '/'))) return true;
  if (path.startsWith('.cargo/') || /(?:^|\/)rust-toolchain(?:\.toml)?$/.test(path)) return true;
  // An unassigned shared fixture or build input invalidates every suite.
  if (path.startsWith('tests/') || /^Cargo\.(?:toml|lock)$/.test(path)) return true;
  if (/^src\/lib\/(?:model-capabilities|sag-tools)\.json$/.test(path)) return ['core', 'cli'].includes(suite);
  if (path.startsWith('docs-site/content/docs/')) return ['core', 'cli', 'desktop'].includes(suite);
  if (/^scripts\/(?:test-windows-desktop\.ps1|windows-test\.manifest\.xml)$/.test(path)) return suite === 'desktop';
  if (/^third-party\/(?:qpdf|pdfium)\//.test(path)) return ['core', 'cli', 'desktop'].includes(suite);
  if (/^scripts\/(?:install\.sh|prepare-(?:qpdf|pdfium)\.mjs|qpdf-packaging\.node-test\.mjs)$/.test(path)) return ['core', 'cli', 'desktop'].includes(suite);
  return false;
}

export function nativeTestKey(git, ref, suite) {
  assert.ok(!suite || Object.hasOwn(suites, suite), 'Unknown native suite');
  const records = git('ls-tree','-rz',ref).split('\0').filter(Boolean).map(record => {
    const match = record.match(/^(\d+) (?:blob|commit) ([a-f0-9]+)\t([\s\S]+)$/);
    assert.ok(match, 'Invalid native input tree'); return {mode:match[1],blob:match[2],path:match[3]};
  });
  const paths = records.filter(({path}) =>
    /^(?:core|client|protocol|cli|src-tauri|prompts|\.cargo)\//.test(path) ||
    /^docs-site\/content\/docs\/[^/]+\.mdx$/.test(path) ||
    (path.startsWith('tests/') && !/^tests\/(?:local-jev-e2e\.py|http-api-e2e\.py|web-service-(?:e2e\.py|browser\.cjs))$/.test(path) && !/^tests\/[^/]+\.test\.[cm]?[jt]s$/.test(path)) ||
    /^src\/lib\/(?:model-capabilities|sag-tools)\.json$/.test(path) ||
    /(?:^|\/)(?:Cargo\.(?:toml|lock)|rust-toolchain(?:\.toml)?)$/.test(path) ||
    /^third-party\/(?:qpdf|pdfium)\//.test(path) || path.startsWith('scripts/docling/') || /^scripts\/(?:prepare-(?:qpdf|pdfium)\.mjs|qpdf-packaging\.node-test\.mjs|test-windows-desktop\.ps1|windows-test\.manifest\.xml|install\.sh)$/.test(path)).sort((a,b)=>a.path.localeCompare(b.path));
  assert.ok(paths.some(({path})=>path==='core/Cargo.toml'), 'Missing native test inputs');
  const inputs = paths.filter(({path}) => matchesSuite(path, suite)).map(({path,mode,blob}) => {
    // Blob IDs preserve every byte, including binary fixtures and file renames.
    let content = blob;
    if (/Cargo\.(toml|lock)$/.test(path)) {
      const text=git('show',`${ref}:${path}`).replace(/\r\n/g,'\n');
      const sections=text.split(path.endsWith('Cargo.lock')?/(?=^\[\[package\]\]$)/m:/(?=^\[[^\n]+\]$)/m);
      content=sections.map(section=>/^name\s*=\s*"(?:sag-cli|sag-desktop)"/m.test(section)?normalizeCargo(path,section):section).join('');
    }
    if (path === 'src-tauri/tauri.conf.json') { const config = JSON.parse(git('show',`${ref}:${path}`)); delete config.version; content = JSON.stringify(config); }
    return [path,mode,content];
  });
  // Scheduling and packaging changes do not invalidate native unit-test results.
  // Changes to the compiler, runner families or test compilation flags do.
  const config = parse(git('show',`${ref}:.github/workflows/desktop-release.yml`),{maxAliasCount:10000});
  const native = config.jobs?.native;
  assert.ok(native?.strategy?.matrix?.include && native.steps, 'Missing native test configuration');
  const runners=['${{ matrix.runner }}', "${{ needs.validate.outputs.native_tests == 'reuse' && 'ubuntu-24.04' || matrix.runner }}"];
  assert.ok(runners.includes(native['runs-on']), 'Unknown native test runner mapping');
  const legacy = native.steps.find(step => step.name === legacyStep);
  const testSteps = legacy ? [legacy] : native.steps.filter(step =>
    Object.keys(suites).some(name => step.name === suiteStep(name)) && (!suite || step.name === suiteStep(suite)));
  assert.equal(testSteps.length, legacy || suite ? 1 : Object.keys(suites).length, 'Missing native Rust test recipe');
  const commands = testSteps.flatMap(step => step.run.match(/cargo test[^\n]+/g) ?? [])
    .filter(command => !suite || command.includes(`--manifest-path ${suites[suite].manifest}/Cargo.toml`));
  if (suite) assert.ok(commands.length > 0, 'Missing Cargo test command for ' + suite);
  const spawnProbe=commands.some(command=>/--manifest-path cli\/Cargo.toml --test service_spawn -- --nocapture$/.test(command));
  const recipes = [...new Set(commands.map(command=>{
    // Preserve package identities and selective filters. Only known equivalent
    // unit/integration selectors and the separately executed probe normalize.
    let recipe=command.replace(/ --(?:locked|timings)\b/g,'').replace(/ --(?:lib|bins) --tests\b/g,'');
    if(spawnProbe && /--manifest-path cli\/Cargo.toml/.test(recipe)) recipe=recipe.replace(/ --skip background_service_releases_both_foreground_capture_pipes_while_still_running$/,'');
    return recipe.replace(/ -- --nocapture$/,'').trim();
  }))].sort();
  const environment = {global:config.env,native:native.env,runsOn:'${{ matrix.runner }}',
    matrix:native.strategy.matrix.include.filter(entry=>entry.kind==='test'),
    setup:native.steps.filter(step=>/^(?:dtolnay\/rust-toolchain|actions\/setup-(?:node|python))@/.test(step.uses ?? '')).map(step=>({uses:step.uses,with:step.with})),
    testEnvironment:testSteps[0].env,recipes,
    ...(suite === 'desktop' ? {windowsRunner: testSteps.flatMap(step => step.run.split('\n'))
      .map(line => line.trim()).filter(line => line.startsWith('pwsh '))} : {})};
  return createHash('sha256').update(JSON.stringify({schema:1,inputs,environment})).digest('hex');
}

export function verifiedNativeBaseline(run, jobs, context) {
  validateRun(run,jobs,context);
  assert.ok(run.id < context.currentRunId && /^v\d+\.\d+\.\d+$/.test(run.head_branch), 'Expected an older tag producer');
  assert.equal(run.status,'completed'); assert.equal(run.conclusion,'success');
  for (const name of ['Validate release source','Test Rust macos','Test Rust macos-intel','Test Rust windows']) {
    assert.equal(latestJob(jobs,name)?.conclusion,'success',`Missing successful baseline: ${name}`);
  }
  return true;
}

export function suiteActuallyPassed(jobs, suite) {
  return ['macos', 'macos-intel', 'windows'].every(platform => {
    const job = latestJob(jobs, `Test Rust ${platform}`);
    return job?.conclusion === 'success' && job.steps?.some(step =>
      (step.name === legacyStep || step.name === suiteStep(suite)) && step.conclusion === 'success');
  });
}

function plan() {
  const pending = new Set(Object.keys(suites));
  const reused = {};
  try {
    assert.equal(process.env.GITHUB_REPOSITORY, 'Zleap-AI/SAG-Wiki');
    const api = path => {
      const result = spawnSync('gh', ['api', `repos/Zleap-AI/SAG-Wiki/${path}`],
        {encoding: 'utf8', timeout: 15000, maxBuffer: 16 * 1024 * 1024});
      assert.equal(result.status, 0, 'Baseline lookup unavailable');
      return JSON.parse(result.stdout);
    };
    const git = gitAt(process.cwd());
    const commit = process.env.GITHUB_SHA;
    assert.match(commit ?? '', /^[a-f0-9]{40}$/);
    const currentRunId = Number(process.env.GITHUB_RUN_ID);
    assert.ok(Number.isSafeInteger(currentRunId) && currentRunId > 0);
    const keys = Object.fromEntries([...pending].map(suite => [suite, nativeTestKey(git, commit, suite)]));
    const workflowId = api('actions/workflows/desktop-release.yml').id;
    const releases = api('releases?per_page=10').filter(release =>
      latestPublished([release]) && /^v\d+\.\d+\.\d+$/.test(release.tag_name));
    for (const release of releases) {
      if (!pending.size) break;
      try {
        const previous = git('rev-parse', `refs/tags/${release.tag_name}^{commit}`);
        const provenance = validateReleaseSource(git, previous, release.tag_name);
        git('merge-base', '--is-ancestor', provenance.source, commit);
        const matching = [...pending].filter(suite => keys[suite] === nativeTestKey(git, previous, suite));
        if (!matching.length) continue;
        const {workflow_runs: runs} = api(`actions/runs?head_sha=${previous}&status=success&per_page=10`);
        const run = runs.find(candidate => candidate.workflow_id === workflowId &&
          candidate.head_branch === release.tag_name && candidate.id < currentRunId);
        assert.ok(run, 'No successful native baseline');
        const {jobs} = api(`actions/runs/${run.id}/jobs?filter=latest&per_page=100`);
        verifiedNativeBaseline(run, jobs, {commit: previous, workflowId, currentRunId});
        for (const suite of matching) {
          if (!suiteActuallyPassed(jobs, suite)) continue;
          pending.delete(suite);
          reused[suite] = `${release.tag_name} (run ${run.id})`;
        }
      } catch (error) {
        console.log(`Native baseline ${release.tag_name} unavailable: ${error.message}`);
      }
    }
  } catch (error) {
    console.log(`Native test reuse unavailable: ${error.message}`);
  }
  const output = [`native_tests=${pending.size ? 'full' : 'reuse'}`,
    ...Object.keys(suites).map(suite => `test_${suite}=${pending.has(suite)}`)];
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, output.join('\n') + '\n');
  const message = Object.keys(suites).map(suite =>
    `- ${suite}: ${reused[suite] ? `reuse ${reused[suite]}` : 'run on all three platforms'}`).join('\n');
  console.log(message);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, message + '\n');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) plan();
