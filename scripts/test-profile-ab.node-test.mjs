import test from 'node:test';
import assert from 'node:assert/strict';
import {chmodSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {profileCargoConfig, profileEnvironment, profileOrder, timingMetrics, inventoryFromListings, assertSameCoverage, summarize} from './test-profile-ab.mjs';

test('single-variable profiles retain assertions, overflow checks and nonincremental compilation', () => {
  const baseline=profileEnvironment('baseline', {CARGO_PROFILE_RELEASE_LTO:'true'});
  const lines=profileEnvironment('line-tables', {CARGO_PROFILE_RELEASE_LTO:'true'});
  const parallel=profileEnvironment('line-tables-cgu64', {CARGO_PROFILE_RELEASE_LTO:'true'});
  for (const env of [baseline,lines,parallel]) {
    assert.equal(env.CARGO_INCREMENTAL,'0');
    assert.equal(env.CARGO_PROFILE_TEST_DEBUG_ASSERTIONS,'true');
    assert.equal(env.CARGO_PROFILE_TEST_OVERFLOW_CHECKS,'true');
    assert.equal(env.CARGO_PROFILE_RELEASE_LTO,'true');
    assert.equal(env.RUST_TEST_THREADS,'2');
  }
  assert.deepEqual(Object.keys(baseline).filter(key=>baseline[key]!==lines[key]),['CARGO_PROFILE_TEST_DEBUG']);
  assert.deepEqual(Object.keys(lines).filter(key=>lines[key]!==parallel[key]),['CARGO_PROFILE_TEST_CODEGEN_UNITS']);
  assert.throws(()=>profileEnvironment('unknown'));
  assert.deepEqual(profileOrder(2),['line-tables','line-tables-cgu64','baseline']);
});

test('CGU-only comparison changes exactly one setting and rotates two profiles', () => {
  const baseline=profileEnvironment('baseline');
  const candidate=profileEnvironment('baseline-cgu64');
  assert.deepEqual(Object.keys(baseline).filter(key=>baseline[key]!==candidate[key]),['CARGO_PROFILE_TEST_CODEGEN_UNITS']);
  assert.equal(candidate.CARGO_PROFILE_TEST_DEBUG,'1');
  assert.deepEqual(profileOrder(1,'cgu-only'),['baseline','baseline-cgu64']);
  assert.deepEqual(profileOrder(2,'cgu-only'),['baseline-cgu64','baseline']);
  assert.deepEqual(profileOrder(3,'cgu-only'),['baseline','baseline-cgu64']);
  assert.throws(()=>profileOrder(1,'unknown'));
});

test('Intel core comparison only changes the sag-core package codegen units', () => {
  assert.deepEqual(profileOrder(1,'intel-core-cgu'),['baseline-core-debug0','baseline-core-cgu32-debug0','baseline-core-cgu64-debug0']);
  assert.deepEqual(profileOrder(2,'intel-core-cgu'),['baseline-core-cgu32-debug0','baseline-core-cgu64-debug0','baseline-core-debug0']);
  assert.equal(profileEnvironment('baseline-core-debug0', {CARGO_PROFILE_TEST_CODEGEN_UNITS:'64'}, 'intel-core-cgu').CARGO_PROFILE_TEST_CODEGEN_UNITS, undefined);
  assert.deepEqual(profileCargoConfig('baseline-core-debug0'),['--config','profile.test.package.sag-core.debug=0']);
  for (const units of [32,64]) {
    const profile='baseline-core-cgu'+units+'-debug0';
    assert.deepEqual(profileEnvironment(profile),profileEnvironment('baseline-core-debug0'));
    assert.deepEqual(profileCargoConfig(profile),['--config','profile.test.package.sag-core.debug=0','--config','profile.test.package.sag-core.codegen-units='+units]);
  }
});

test('Cargo metrics select the slow unit-test target rather than summing parallel units', () => {
  const units=[
    {name:'sag-core',target:'',start:1,duration:35,sections:[['frontend',{start:0,end:17}],['codegen',{start:17,end:35}]]},
    {name:'sag-core',target:' sag_core "lib" (test)',start:14,duration:529.5,sections:null},
    {name:'sag-core',target:' progressive_index "test" (test)',start:36,duration:79.9,sections:null},
    {name:'serde',target:'',start:0,duration:4,sections:null},
  ];
  const html=`<script>const UNIT_DATA = ${JSON.stringify(units)};\nconst unrelated = "not data";</script>`;
  const metrics=timingMetrics(html);
  assert.equal(metrics.unit_test_seconds,529.5);
  assert.equal(metrics.normal_library_seconds,35);
  assert.equal(metrics.compilation_seconds,543.5);
  assert.equal(metrics.unit_test_sections,null);
  assert.equal(metrics.other_targets_finished_seconds,115.9);
  assert.throws(()=>timingMetrics(`const UNIT_DATA = ${JSON.stringify(units.filter(unit=>!unit.target.includes('sag_core')))};`));
  assert.throws(()=>timingMetrics('<script>const UNIT_DATA = [];</script>'));
});

test('coverage binds target and named test identity and ignored status across all profiles', () => {
  const a=inventoryFromListings([{target:'lib:sag_core',list:'same_name: test\nmod::works: test\n2 tests, 0 benchmarks\n',ignored:'same_name: test\n'},{target:'test:storage',list:'same_name: test\n',ignored:''}]);
  assert.equal(a.length,3);
  assertSameCoverage(a,[...a].reverse());
  assert.throws(()=>assertSameCoverage(a,a.slice(1)));
  assert.throws(()=>assertSameCoverage(a,a.map((entry,i)=>i===0?{...entry,ignored:!entry.ignored}:entry)));
  assert.throws(()=>inventoryFromListings([{target:'lib:sag_core',list:'one: test\none: test\n',ignored:''}]));
  assertSameCoverage(a,inventoryFromListings([{target:'empty-os-target',list:'0 tests, 0 benchmarks\n',ignored:''},{target:'lib:sag_core',list:'same_name: test\nmod::works: test\n',ignored:'same_name: test\n'},{target:'test:storage',list:'same_name: test\n',ignored:''}]));
});

test('paired summaries reject mixed commits/platforms or missing profiles', () => {
  const coverage=[{target:'lib:sag_core',name:'one',ignored:false}];
  const reports=[1,2,3].map(round=>({schema:1,commit:'a'.repeat(40),platform:'macos',round,results:profileOrder(round).map(profile=>({profile,inventory:coverage,metrics:{unit_test_seconds:profile==='baseline'?500+round:profile==='line-tables'?400+round:300+round}}))}));
  const summary=summarize(reports);
  assert.equal(summary.comparison,'debug-and-cgu');
  assert.equal(summary.profiles.baseline.median_unit_test_seconds,502);
  assert.equal(summary.paired['line-tables'].median_saved_seconds,100);
  assert.equal(summary.paired['line-tables-cgu64'].median_saved_seconds,100);
  assert.throws(()=>summarize([reports[0],{...reports[1],commit:'b'.repeat(40)}]));
  assert.throws(()=>summarize([{...reports[0],results:reports[0].results.slice(1)}]));
});

test('CGU-only summaries preserve legacy reports and reject mixed comparison modes', () => {
  const inventory=[{target:'lib:sag_core',name:'one',ignored:false}];
  const records=[1,2,3].map(round=>({schema:1,commit:'a'.repeat(40),platform:'macos',round,comparison:'cgu-only',results:[
    {profile:'baseline',inventory,metrics:{unit_test_seconds:500+round}},
    {profile:'baseline-cgu64',inventory,metrics:{unit_test_seconds:400+round}},
  ]}));
  const result=summarize(records);
  assert.equal(result.comparison,'cgu-only');
  assert.equal(result.paired['baseline-cgu64'].compared_with,'baseline');
  assert.equal(result.paired['baseline-cgu64'].median_saved_seconds,100);
  assert.throws(()=>summarize([records[0],{...records[1],comparison:'debug-and-cgu'}]),/comparison/i);
});

test('CLI performs warm/clean/measure/full-test in rotated order and never accepts a version tag', {skip:process.platform==='win32'}, () => {
  const directory=mkdtempSync(join(tmpdir(),'sag-profile-orchestration-'));
  try {
    const script=`#!${process.execPath}
const fs=require('node:fs');const path=require('node:path');
const command=path.basename(process.argv[1]);const args=process.argv.slice(2);
if(command==='git'){process.stdout.write('a'.repeat(40));process.exit();}
if(command==='rustc'){process.stdout.write('rustc 1.98.1');process.exit();}
if(command==='test-binary'){process.stdout.write(args.includes('--ignored')?'ignored: test\\n':'works: test\\nignored: test\\n');process.exit();}
if(args.includes('--version')){process.stdout.write('cargo 1.98.1');process.exit();}
const target=process.env.CARGO_TARGET_DIR;
fs.appendFileSync(process.env.AB_TRACE,JSON.stringify({args,target,debug:process.env.CARGO_PROFILE_TEST_DEBUG,cgu:process.env.CARGO_PROFILE_TEST_CODEGEN_UNITS})+'\\n');
if(args[0]==='build'){fs.writeFileSync(path.join(target,'warm'),'ready');}
if(args.includes('--no-run')){
 if(!fs.existsSync(path.join(target,'warm')))throw Error('dependencies were not warmed');
 const units=[{name:'sag-core',target:'',start:0,duration:35,sections:null},{name:'sag-core',target:' sag_core "lib" (test)',start:1,duration:500,sections:null}];
 fs.mkdirSync(path.join(target,'cargo-timings'));fs.writeFileSync(path.join(target,'cargo-timings/cargo-timing.html'),'const UNIT_DATA = '+JSON.stringify(units)+';');
 process.stdout.write(JSON.stringify({reason:'compiler-artifact',package_id:'sag-core@0.1.0',profile:{test:true},target:{kind:['lib'],name:'sag_core'},executable:path.join(process.env.AB_SHIMS,'test-binary')})+'\\n');
} else if(args[0]==='test'){process.stdout.write('test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\\n');}
`;
    for(const name of ['git','rustc','cargo','test-binary']){writeFileSync(join(directory,name),script);chmodSync(join(directory,name),0o755);}
    const env={...process.env,PATH:`${directory}:${process.env.PATH}`,RUNNER_TEMP:directory,GITHUB_EVENT_NAME:'workflow_dispatch',GITHUB_REF:'refs/heads/feature',GITHUB_SHA:'a'.repeat(40),AB_PLATFORM:'macos',AB_ROUND:'2',AB_COMPARISON:'debug-and-cgu',AB_TRACE:join(directory,'trace'),AB_SHIMS:directory};
    const command=resolve(import.meta.dirname,'test-profile-ab.mjs');
    const rejected=spawnSync(process.execPath,[command,'run'],{cwd:directory,env:{...env,GITHUB_REF:'refs/tags/v0.3.0'},encoding:'utf8'});
    assert.notEqual(rejected.status,0);
    const result=spawnSync(process.execPath,[command,'run'],{cwd:directory,env,encoding:'utf8'});
    assert.equal(result.status,0,result.stderr);
    const report=JSON.parse(readFileSync(join(directory,'profile-ab-results/report.json')));
    assert.deepEqual(report.order,profileOrder(2));
    assert.equal(report.results.length,3);assert.equal(report.results[0].inventory.length,2);
    const trace=readFileSync(join(directory,'trace'),'utf8').trim().split('\n').map(JSON.parse);
    assert.equal(trace.length,12);
    for(let index=0;index<3;index++){
      const operations=trace.slice(index*4,index*4+4);
      assert.deepEqual(operations.map(operation=>operation.args[0]),['build','clean','test','test']);
      assert.deepEqual(operations[1].args.slice(-2),['-p','sag-core']);
      assert.ok(operations[2].args.includes('--no-run'));
      assert.ok(operations[3].args.includes('--nocapture'));
    }
    assert.equal(new Set(trace.map(operation=>operation.target)).size,3);
    assert.equal(readdirSync(directory).filter(name=>name.startsWith('sag-profile-ab-')).length,0);
    const focused=mkdtempSync(join(directory,'focused-'));
    const focusedResult=spawnSync(process.execPath,[command,'run'],{cwd:directory,env:{...env,RUNNER_TEMP:focused,AB_TRACE:join(focused,'trace'),AB_COMPARISON:'cgu-only'},encoding:'utf8'});
    assert.equal(focusedResult.status,0,focusedResult.stderr);
    const focusedReport=JSON.parse(readFileSync(join(focused,'profile-ab-results/report.json')));
    assert.equal(focusedReport.comparison,'cgu-only');
    assert.deepEqual(focusedReport.order,['baseline-cgu64','baseline']);
    assert.ok(focusedReport.results.every(result=>result.environment.CARGO_PROFILE_TEST_DEBUG==='1'));
    assert.equal(readFileSync(join(focused,'trace'),'utf8').trim().split('\n').length,8);
    const coreDebug = mkdtempSync(join(directory, 'core-debug-'));
    const resultDebug = spawnSync(process.execPath, [command, 'run'], {
      cwd: directory, encoding: 'utf8',
      env: {...env, RUNNER_TEMP: coreDebug, AB_TRACE: join(coreDebug, 'trace'), AB_COMPARISON: 'core-debug-only'},
    });
    assert.equal(resultDebug.status, 0, resultDebug.stderr);
    const debugTrace = readFileSync(join(coreDebug, 'trace'), 'utf8').trim().split('\n').map(JSON.parse);
    assert.ok(debugTrace.every(operation => operation.debug === '1' && operation.cgu === '16'));
    assert.deepEqual(debugTrace.filter(operation => operation.args.includes('profile.test.package.sag-core.debug=0'))
      .map(operation => operation.args[0]), ['build', 'test', 'test']);
  } finally {rmSync(directory,{recursive:true,force:true});}
});
