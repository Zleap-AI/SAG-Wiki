import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {appendFileSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {performance} from 'node:perf_hooks';

export const profiles = ['baseline', 'line-tables', 'line-tables-cgu64'];
const intelCoreProfiles = ['baseline-core-debug0','baseline-core-cgu32-debug0','baseline-core-cgu64-debug0'];
export function profileCargoConfig(profile) {
  if (!intelCoreProfiles.includes(profile)) return [];
  const args = ['--config', 'profile.test.package.sag-core.debug=0'];
  const units = profile.match(/-cgu(32|64)-/);
  if (units) args.push('--config', 'profile.test.package.sag-core.codegen-units=' + units[1]);
  return args;
}
export function comparisonProfiles(comparison = 'debug-and-cgu') {
  assert.ok(['debug-and-cgu','cgu-only','core-debug-only','intel-core-cgu'].includes(comparison), 'Unknown profile comparison');
  if (comparison === 'intel-core-cgu') return intelCoreProfiles;
  if (comparison === 'core-debug-only') return ['baseline', 'baseline-core-debug0'];
  return comparison === 'cgu-only' ? ['baseline','baseline-cgu64'] : [...profiles];
}
export function profileOrder(round, comparison = 'debug-and-cgu') {
  assert.ok(Number.isInteger(round) && round >= 1 && round <= 3, 'Round must be 1, 2 or 3');
  const selected = comparisonProfiles(comparison);
  const offset = (round - 1) % selected.length;
  return [...selected.slice(offset), ...selected.slice(0, offset)];
}

export function profileEnvironment(profile, inherited = {}, comparison) {
  assert.ok([...profiles,'baseline-cgu64',...intelCoreProfiles].includes(profile), 'Unknown test profile');
  const environment = {...inherited, CARGO_INCREMENTAL:'0', RUST_TEST_THREADS:'2',
    CARGO_PROFILE_TEST_DEBUG:profile.startsWith('baseline') ? '1' : 'line-tables-only',
    CARGO_PROFILE_TEST_CODEGEN_UNITS:profile.endsWith('cgu64') ? '64' : '16',
    CARGO_PROFILE_TEST_DEBUG_ASSERTIONS:'true', CARGO_PROFILE_TEST_OVERFLOW_CHECKS:'true'};
  if (comparison === 'intel-core-cgu') delete environment.CARGO_PROFILE_TEST_CODEGEN_UNITS;
  return environment;
}

export function timingMetrics(html) {
  const match = html.match(/const UNIT_DATA\s*=\s*(\[[\s\S]*?\]);/);
  assert.ok(match, 'Cargo timing unit data unavailable');
  const units = JSON.parse(match[1]);
  assert.ok(Array.isArray(units) && units.length, 'Empty Cargo timing data');
  const tests = units.filter(unit => unit.name === 'sag-core' && /sag_core "lib" \(test\)/.test(unit.target));
  const libraries = units.filter(unit => unit.name === 'sag-core' && unit.target === '' && unit.duration > 0);
  assert.equal(tests.length, 1, 'Missing or duplicate core unit-test compilation');
  assert.equal(libraries.length, 1, 'Missing or duplicate normal core library compilation');
  for (const unit of units) assert.ok(Number.isFinite(unit.start) && Number.isFinite(unit.duration) && unit.start >= 0 && unit.duration >= 0, 'Invalid timing data');
  return {unit_test_seconds:tests[0].duration, normal_library_seconds:libraries[0].duration,
    compilation_seconds:Math.max(...units.map(unit => unit.start + unit.duration)),
    other_targets_finished_seconds:Math.max(0, ...units.filter(unit => unit !== tests[0]).map(unit => unit.start + unit.duration)),
    unit_test_sections:tests[0].sections};
}

export function inventoryFromListings(listings) {
  const inventory = [];
  for (const {target, list, ignored} of listings) {
    const names = text => text.split(/\r?\n/).flatMap(line => line.endsWith(': test') ? [line.slice(0, -6)] : []);
    const tests = names(list); const skipped = new Set(names(ignored));
    assert.ok(tests.every(name => typeof name === 'string' && name), 'Invalid named test');
    assert.equal(new Set(tests).size, tests.length, 'Duplicate named test');
    assert.ok([...skipped].every(name => tests.includes(name)), 'Ignored test missing from full inventory');
    inventory.push(...tests.map(name => ({target, name, ignored:skipped.has(name)})));
  }
  assert.ok(inventory.length, 'No named tests enumerated');
  const sorted = inventory.sort((a,b) => `${a.target}\0${a.name}`.localeCompare(`${b.target}\0${b.name}`));
  assert.equal(new Set(sorted.map(entry => `${entry.target}\0${entry.name}`)).size, sorted.length, 'Duplicate test target identity');
  return sorted;
}

const sortedCoverage = inventory => [...inventory].sort((a,b) => `${a.target}\0${a.name}`.localeCompare(`${b.target}\0${b.name}`));
export function assertSameCoverage(expected, actual) {
  assert.deepEqual(sortedCoverage(actual), sortedCoverage(expected), 'Named tests or ignored status changed');
}
const median = values => {
  assert.ok(values.length && values.every(Number.isFinite), 'Missing samples');
  const sorted = [...values].sort((a,b) => a-b); const middle = Math.floor(sorted.length/2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle-1]+sorted[middle])/2;
};

export function summarize(reports) {
  assert.ok(reports.length, 'No profile reports');
  const first = reports[0]; const seen = new Set();
  const comparison = first.comparison ?? 'debug-and-cgu';
  const selected = comparisonProfiles(comparison);
  for (const report of reports) {
    assert.equal(report.schema, 1); assert.equal(report.commit, first.commit, 'Mixed source commits');
    assert.equal(report.platform, first.platform, 'Compare each operating system separately');
    assert.equal(report.comparison ?? 'debug-and-cgu', comparison, 'Mixed profile comparisons');
    assert.ok(!seen.has(report.round), 'Duplicate measurement round'); seen.add(report.round);
    assert.deepEqual(report.results.map(result => result.profile).sort(), [...selected].sort(), 'Incomplete profile comparison');
    for (const result of report.results) assertSameCoverage(first.results[0].inventory, result.inventory);
  }
  const result = {commit:first.commit, platform:first.platform, comparison, rounds:reports.length, profiles:{}, paired:{}};
  for (const profile of selected) {
    const samples = reports.map(report => report.results.find(result => result.profile === profile).metrics.unit_test_seconds);
    result.profiles[profile] = {samples, median_unit_test_seconds:median(samples)};
  }
  const pairs = comparison === 'debug-and-cgu'
    ? [['baseline', 'line-tables'], ['line-tables', 'line-tables-cgu64']]
    : selected.slice(1).map(profile => [selected[0], profile]);
  for (const [before,after] of pairs) {
    const saved = reports.map(report => report.results.find(result => result.profile === before).metrics.unit_test_seconds - report.results.find(result => result.profile === after).metrics.unit_test_seconds);
    result.paired[after] = {compared_with:before, saved_seconds:saved, median_saved_seconds:median(saved)};
    const endToEnd = reports.map(report => {
      const baseline = report.results.find(result => result.profile === before);
      const candidate = report.results.find(result => result.profile === after);
      return baseline.build_wall_seconds + baseline.execution_seconds
        - candidate.build_wall_seconds - candidate.execution_seconds;
    });
    if (comparison === 'intel-core-cgu') {
      const fullSuite = endToEnd.map((saved,index) => {
        const results = reports[index].results;
        return saved + results.find(r => r.profile === before).cli_seconds - results.find(r => r.profile === after).cli_seconds;
      });
      assert.ok(fullSuite.every(Number.isFinite), 'Missing Intel CLI measurements');
      result.paired[after].core_and_cli_saved_seconds = fullSuite;
      result.paired[after].median_core_and_cli_saved_seconds = median(fullSuite);
    }
    if (endToEnd.every(Number.isFinite)) {
      result.paired[after].median_build_and_test_saved_seconds = median(endToEnd);
    }
  }
  return result;
}

function execute(program, args, env, log) {
  const started = performance.now();
  const result = spawnSync(program, args, {env, encoding:'utf8', timeout:65*60*1000, maxBuffer:64*1024*1024});
  if (log) writeFileSync(log, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
  assert.equal(result.status, 0, `${program} ${args.join(' ')} failed: ${result.error?.message ?? result.stderr?.slice(-12000)}`);
  return {stdout:result.stdout, seconds:(performance.now()-started)/1000};
}

function runComparison() {
  assert.equal(process.env.GITHUB_EVENT_NAME, 'workflow_dispatch', 'Only explicit branch dispatches may benchmark');
  assert.match(process.env.GITHUB_REF ?? '', /^refs\/heads\//, 'Benchmark a branch, never a version tag');
  const platform = process.env.AB_PLATFORM;
  assert.ok(['macos','macos-intel','windows'].includes(platform));
  assert.equal(platform === 'windows', process.platform === 'win32');
  if (platform === 'macos-intel') {
    assert.equal(process.platform, 'darwin');
    assert.equal(process.arch, 'x64', 'Intel benchmark requires an x64 runner');
  }
  assert.ok(process.env.RUNNER_TEMP, 'Missing dedicated runner temporary directory');
  const round = Number(process.env.AB_ROUND);
  const comparison = process.env.AB_COMPARISON || 'debug-and-cgu';
  assert.ok(comparison !== 'intel-core-cgu' || platform === 'macos-intel', 'Intel codegen comparison requires macos-intel');
  const order = profileOrder(round,comparison);
  const directory = join(process.env.RUNNER_TEMP, 'profile-ab-results');
  mkdirSync(directory, {recursive:true});
  const commit = execute('git', ['rev-parse','HEAD'], process.env).stdout.trim();
  assert.equal(commit, process.env.GITHUB_SHA, 'Benchmark source differs from dispatch');
  const report = {schema:1, commit, platform, round, comparison, order,
    runner:{name:process.env.RUNNER_NAME, os:process.env.RUNNER_OS, arch:process.env.RUNNER_ARCH, image:process.env.ImageVersion},
    rustc:execute('rustc',['--version','--verbose'],process.env).stdout.trim(),
    cargo:execute('cargo',['--version','--verbose'],process.env).stdout.trim(), results:[]};
  let expected;
  for (const profile of order) {
    const target = mkdtempSync(join(process.env.RUNNER_TEMP, 'sag-profile-ab-'));
    const env = profileEnvironment(profile, {...process.env, CARGO_TARGET_DIR:target, CARGO_TERM_COLOR:'never'}, comparison);
    assert.ok(!env.RUSTFLAGS && !env.CARGO_ENCODED_RUSTFLAGS, 'Ambient Rust flags make the comparison ambiguous');
    // Keep the dependency profile identical. Only sag-core loses debug metadata.
    const cargoConfig = profileCargoConfig(profile);
    const output = join(directory, profile); mkdirSync(output);
    console.log(`${new Date().toISOString()} ${platform} round ${round}: warm dependencies for ${profile}`);
    try {
      const warmup = execute('cargo', ['build',...cargoConfig,'--locked','--profile','test','--lib','--manifest-path','core/Cargo.toml'], env, join(output,'warmup.log'));
      execute('cargo', ['clean','--profile','test','--manifest-path','core/Cargo.toml','-p','sag-core'], env, join(output,'clean.log'));
      console.log(`${new Date().toISOString()} ${platform} round ${round}: measure ${profile}`);
      const build = execute('cargo', ['test',...cargoConfig,'--locked','--no-run','--timings','--message-format=json-render-diagnostics','--manifest-path','core/Cargo.toml'], env, join(output,'compile.log'));
      const timing = join(target,'cargo-timings','cargo-timing.html');
      copyFileSync(timing,join(output,'cargo-timing.html'));
      const metrics = timingMetrics(readFileSync(timing,'utf8'));
      const messages = build.stdout.split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
      const binaries = messages.filter(message => message.reason === 'compiler-artifact' && message.executable && message.profile?.test && message.package_id?.includes('sag-core'));
      assert.ok(binaries.length, 'No core test executables found');
      const listings = binaries.map(message => ({target:`${message.target.kind.join(',')}:${message.target.name}`,
        list:execute(message.executable,['--list','--format','terse'],env).stdout,
        ignored:execute(message.executable,['--ignored','--list','--format','terse'],env).stdout}));
      const inventory = inventoryFromListings(listings);
      if (expected) assertSameCoverage(expected,inventory); else expected=inventory;
      const execution = execute('cargo',['test',...cargoConfig,'--locked','--manifest-path','core/Cargo.toml','--','--nocapture'],env,join(output,'tests.log'));
      const counts = [...execution.stdout.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)].map(match => ({passed:Number(match[1]),failed:Number(match[2]),ignored:Number(match[3])}));
      assert.ok(counts.length && counts.every(count => count.failed === 0), 'Missing successful test results');
      assert.equal(counts.reduce((sum,count) => sum+count.passed+count.ignored,0),inventory.length,'Executed test count differs from named inventory');
      const result = {profile, cargoConfig, environment:Object.fromEntries(Object.entries(env).filter(([key]) => key.startsWith('CARGO_PROFILE_TEST_') || ['CARGO_INCREMENTAL','RUST_TEST_THREADS'].includes(key))),
        warmup_seconds:warmup.seconds, build_wall_seconds:build.seconds, execution_seconds:execution.seconds, metrics, inventory, outcomes:counts};
      if (comparison === 'intel-core-cgu') {
        const args = [...cargoConfig,'--locked','--manifest-path','cli/Cargo.toml'];
        const probe = execute('cargo',['test',...args,'--test','service_spawn','--','--nocapture'],env,join(output,'cli-probe.log'));
        const cli = execute('cargo',['test',...args,'--bins','--tests','--','--nocapture','--skip','background_service_releases_both_foreground_capture_pipes_while_still_running'],env,join(output,'cli-tests.log'));
        result.cli_seconds = probe.seconds + cli.seconds;
      }
      report.results.push(result);
      writeFileSync(join(directory,'report.json'), JSON.stringify(report,null,2)+'\n');
      console.log(`${new Date().toISOString()} ${profile}: test target ${metrics.unit_test_seconds}s, build ${build.seconds.toFixed(1)}s, test execution ${execution.seconds.toFixed(1)}s; ${inventory.length} named tests unchanged`);
    } finally { rmSync(target,{recursive:true,force:true}); }
  }
  const summary = summarize([report]);
  writeFileSync(join(directory,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## ${platform} paired round ${round}\n\nDependency warm-up is excluded. All profiles retain the same named tests and ignored status.\n\n| Profile | Core test target | Whole compilation | Test execution |\n| --- | ---: | ---: | ---: |\n${report.results.map(result => `| ${result.profile} | ${result.metrics.unit_test_seconds.toFixed(2)}s | ${result.build_wall_seconds.toFixed(2)}s | ${result.execution_seconds.toFixed(2)}s |`).join('\n')}\n`);
}

function summarizeDirectory(directory) {
  const reports = [];
  const walk = path => {for (const entry of readdirSync(path,{withFileTypes:true})) {const file=join(path,entry.name); if(entry.isDirectory()) walk(file); else if(entry.name === 'report.json') reports.push(JSON.parse(readFileSync(file)));}};
  walk(resolve(directory));
  for (const platform of ['macos','macos-intel','windows']) {
    const matching = reports.filter(report => report.platform === platform);
    if(matching.length) console.log(JSON.stringify(summarize(matching),null,2));
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [command, directory] = process.argv.slice(2);
  assert.ok(['run','summarize'].includes(command), 'Use run or summarize DIRECTORY');
  if(command === 'run') runComparison(); else summarizeDirectory(directory);
}
