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

export function nativeTestKey(git, ref) {
  const records = git('ls-tree','-rz',ref).split('\0').filter(Boolean).map(record => {
    const match = record.match(/^(\d+) (?:blob|commit) ([a-f0-9]+)\t([\s\S]+)$/);
    assert.ok(match, 'Invalid native input tree'); return {mode:match[1],blob:match[2],path:match[3]};
  });
  const paths = records.filter(({path}) =>
    /^(?:core|client|protocol|cli|src-tauri|prompts|\.cargo)\//.test(path) ||
    (path.startsWith('tests/') && !/^tests\/[^/]+\.test\.[cm]?[jt]s$/.test(path)) ||
    /^src\/lib\/(?:model-capabilities|sag-tools)\.json$/.test(path) ||
    /(?:^|\/)(?:Cargo\.(?:toml|lock)|rust-toolchain(?:\.toml)?)$/.test(path) ||
    path.startsWith('scripts/docling/') || /^scripts\/(?:prepare-qpdf\.mjs|test-windows-desktop\.ps1|windows-test\.manifest\.xml|install\.sh)$/.test(path)).sort((a,b)=>a.path.localeCompare(b.path));
  assert.ok(paths.some(({path})=>path==='core/Cargo.toml'), 'Missing native test inputs');
  const inputs = paths.map(({path,mode,blob}) => {
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
  const testSteps = native.steps.filter(step=>step.name==='Run native tests with locked dependencies');
  assert.equal(testSteps.length,1,'Missing native Rust test recipe');
  const commands=testSteps[0].run.match(/cargo test[^\n]+/g) ?? [];
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
    testEnvironment:testSteps[0].env,recipes};
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

function plan() {
  let reuse = false, baseline = '';
  try {
    assert.equal(process.env.GITHUB_REPOSITORY,'Zleap-AI/SAG-Wiki');
    const api = path => {
      const result = spawnSync('gh',['api',`repos/Zleap-AI/SAG-Wiki/${path}`],{encoding:'utf8',timeout:15000,maxBuffer:16*1024*1024});
      assert.equal(result.status,0,'Baseline lookup unavailable'); return JSON.parse(result.stdout);
    };
    const git = gitAt(process.cwd()), commit = process.env.GITHUB_SHA;
    assert.match(commit ?? '',/^[a-f0-9]{40}$/);
    const release = latestPublished(api('releases?per_page=10'));
    assert.ok(release,'No published baseline');
    const previous = git('rev-parse',`refs/tags/${release.tag_name}^{commit}`);
    const provenance = validateReleaseSource(git,previous,release.tag_name);
    git('merge-base','--is-ancestor',provenance.source,commit);
    assert.equal(nativeTestKey(git,commit),nativeTestKey(git,previous),'Native inputs changed');
    const workflowId = api('actions/workflows/desktop-release.yml').id;
    const runs = api(`actions/workflows/desktop-release.yml/runs?head_sha=${previous}&status=success&per_page=10`).workflow_runs;
    const currentRunId = Number(process.env.GITHUB_RUN_ID);
    assert.ok(Number.isSafeInteger(currentRunId) && currentRunId > 0);
    const run = runs.find(candidate=>candidate.head_branch===release.tag_name && candidate.id < currentRunId);
    assert.ok(run,'No successful native baseline');
    const jobs = api(`actions/runs/${run.id}/jobs?filter=latest&per_page=100`).jobs;
    verifiedNativeBaseline(run,jobs,{commit:previous,workflowId,currentRunId});
    reuse = true; baseline = release.tag_name;
  } catch (error) { console.log(`Run complete native Rust tests: ${error.message}`); }
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT,`native_tests=${reuse?'reuse':'full'}\n`);
  const message = reuse ? `Reuse unchanged native Rust tests from successful published ${baseline}; run Python, Safari and final production installer checks.` : 'Run complete native Rust tests on all three platforms.';
  console.log(message);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY,message+'\n');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) plan();
