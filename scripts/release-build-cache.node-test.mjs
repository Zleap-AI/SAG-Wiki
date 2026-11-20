import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {validateRun, latestJob, componentState, selectArtifact, selectComponent, validateReceipt, verifyArchive, awaitComponents, components, releaseComponents, releaseJobTimings} from './release-build-cache.mjs';
const commit = 'a'.repeat(40);
const ctx = {commit, workflowId:4, currentRunId:20};
const names = {'rust-macos-intel':'Test Rust macos-intel','build-macos-intel':'Build macos-intel', documentation:'Build bilingual documentation', frontend:'Test frontend', 'rust-macos':'Test Rust macos', 'rust-windows':'Test Rust windows', 'build-macos':'Build macos', 'build-windows':'Build windows'};
const run = (id=10, patch={}) => ({id, repository:{id:3, full_name:'Zleap-AI/SAG-Wiki'}, head_repository:{full_name:'Zleap-AI/SAG-Wiki'}, head_sha:commit, workflow_id:4, path:'.github/workflows/desktop-release.yml', head_branch:'main', event:'push', status:'completed', conclusion:'success', ...patch});
const job = (name, id=1, patch={}) => ({id, name, conclusion:'success', status:'completed', run_attempt:1, started_at:'2026-01-01T00:00:00Z', completed_at:'2026-01-01T00:10:00Z', ...(name.startsWith('Compile CLI ')?{steps:[{name:'Compile the production CLI with its real embedded web assets',conclusion:'success'}]}:{}), ...patch});
const artifact = (sourceRun, platform, sourceJob, patch={}) => ({id:100+sourceRun.id, name:`desktop-${platform}-${commit}-${sourceJob.run_attempt}`, expired:false, expires_at:'2099-01-01T00:00:00Z', created_at:'2026-01-01T00:05:00Z', digest:`sha256:${'b'.repeat(64)}`, workflow_run:{id:sourceRun.id, head_sha:commit, repository_id:3, head_repository_id:3}, ...patch});
function source(id=10, patch={}) {
 const sourceRun=run(id,patch);
 const jobs=[job('Validate release source',id*100), ...components.map((c,i)=>job(names[c],id*100+i+1))];
 const receipts=Object.fromEntries(components.map((c,i)=>[c,{schema:1,commit,component:c,producer_run:id,producer_job:jobs[i+1].id,producer_attempt:1}]));
 return {run:sourceRun,jobs,receipts,artifacts:['macos','windows','macos-intel'].map(p=>artifact(sourceRun,p,jobs.find(j=>j.name===`Build ${p}`)))};
}
const receiptReader=(s,c)=>s.receipts[c];
const choose=(sources,c,options={})=>selectComponent(sources,c,{...ctx,receiptReader,...options});

test('release timing summary separates queue time and execution and uses the latest job attempt', () => {
 const first=job('Test Rust macos-intel',1,{created_at:'2025-12-31T23:55:00Z'});
 const retry=job('Test Rust macos-intel',2,{run_attempt:2,created_at:'2025-12-31T23:59:57Z'});
 const noQueue=job('Build windows',3);
 assert.deepEqual(releaseJobTimings([first,retry,noQueue]),[
  '| Test Rust macos-intel | 2 | 3 | 600 |',
  '| Build windows | 1 | — | 600 |',
 ]);
});

test('signing can start after the verified macOS build while native tests remain pending',async()=>{
 const s=source(20,{status:'in_progress',conclusion:null});
 const rust=s.jobs.find(j=>j.name==='Test Rust macos');
 Object.assign(rust,{status:'in_progress',conclusion:null});
 const result=await awaitComponents(ctx,releaseComponents('macos'),{
  gate:true,snapshot:()=>[s],receiptReader,pause:()=>{throw new Error('Signing unnecessarily waited for native tests');}
 });
 assert.deepEqual(result.map(r=>r.artifact?.platform),['macos']);
 assert.deepEqual(releaseComponents('all'),components);
 rust.status='completed';rust.conclusion='failure';
 await assert.rejects(awaitComponents(ctx,releaseComponents('all'),{gate:true,snapshot:()=>[s],receiptReader}),/checkpoint failed: rust-macos/);
});

test('trusts exact-SHA main and version-tag components even when whole run failed or running',()=>{
 for(const patch of [{conclusion:'failure'},{status:'in_progress',conclusion:null},{head_branch:'v0.2.5',event:'workflow_dispatch',conclusion:'failure'}]){
  const s=source(10,patch);validateRun(s.run,s.jobs,ctx);assert.equal(choose([s],'build-macos').state,'success');
 }
});
test('rejects foreign repository, fork, wrong SHA/workflow and untrusted events/refs',()=>{
 for(const patch of [{repository:{full_name:'evil/repo'}},{head_repository:{full_name:'evil/repo'}},{head_sha:'b'.repeat(40)},{workflow_id:9},{path:'other.yml'},{head_branch:'feature'},{event:'pull_request'},{head_branch:'main',event:'workflow_dispatch'},{head_branch:'vbad',event:'push'}])assert.throws(()=>validateRun(run(10,patch),[],ctx));
});
test('manual main is trusted only for its own fresh local checkpoints',()=>{
 validateRun(run(20,{event:'workflow_dispatch'}),[],ctx);
 assert.throws(()=>validateRun(run(10,{event:'workflow_dispatch'}),[],ctx),/Untrusted/);
});
test('latest job attempts supersede success, but retained prerequisites survive failed-jobs retries',()=>{
 const s=source();s.jobs.push(job('Sign and create a draft release',999,{run_attempt:2,conclusion:'failure'}));
 assert.equal(choose([s],'build-macos').state,'success');
 s.jobs.push(job('Build macos',1000,{run_attempt:2,conclusion:'failure'}));
 assert.equal(choose([s],'build-macos').state,'failure');
 assert.equal(choose([s],'build-windows').state,'success');
 assert.throws(()=>latestJob([job('x'),job('x')],'x'),/Duplicate/);
 assert.throws(()=>latestJob([job('x',1,{run_attempt:0})],'x'),/Invalid/);
});
test('partial platform failure does not invalidate independent build or Rust-test checkpoints',()=>{
 const s=source();s.jobs.find(j=>j.name==='Test Rust windows').conclusion='failure';
 for(const c of ['build-macos','rust-macos','build-windows'])assert.equal(choose([s],c).state,'success');
 assert.equal(choose([s],'rust-windows').state,'failure');
});
test('legacy aggregate success is accepted per platform; failed native tests never count',()=>{
 const s=source();s.jobs=s.jobs.filter(j=>!/^Build (macos|windows)$|^Test Rust/.test(j.name));
 for(const p of ['macos','windows'])s.jobs.push(job(`Build and test ${p}`,42+(p==='macos'?0:1)));
 s.artifacts=s.artifacts.map(a=>({...a,name:a.name.replace(/-1$/,'')}));
 assert.equal(choose([s],'rust-macos').state,'success');assert.equal(choose([s],'build-macos').state,'success');
 s.jobs.find(j=>j.name==='Build and test windows').conclusion='failure';
 assert.equal(choose([s],'rust-windows').state,'failure');assert.equal(choose([s],'build-windows').state,'failure');
});
test('pending checkpoints wait; failed or missing terminal checkpoints require local fallback',()=>{
 const s=source(10,{status:'in_progress'});s.jobs.find(j=>j.name==='Build windows').conclusion=null;
 s.jobs.find(j=>j.name==='Build windows').status='in_progress';
 assert.equal(choose([s],'build-windows').state,'pending');
 s.jobs=s.jobs.filter(j=>j.name!=='Build windows');assert.equal(choose([s],'build-windows').state,'pending');
 s.run.status='completed';assert.equal(choose([s],'build-windows').state,'failure');
});
test('archive provenance, expiry, checksum and latest attempt timestamps fail closed',()=>{
 const s=source();const j=s.jobs.find(j=>j.name==='Build macos');const a=s.artifacts[0];
 for(const patch of [{expired:true},{expires_at:'2000-01-01'},{digest:null},{id:-1},{created_at:'2025-01-01'},{created_at:'2026-01-02'},{workflow_run:{...a.workflow_run,id:9}},{workflow_run:{...a.workflow_run,head_sha:'wrong'}},{workflow_run:{...a.workflow_run,repository_id:9}},{workflow_run:{...a.workflow_run,head_repository_id:9}}])assert.throws(()=>selectArtifact([{...a,...patch}],s.run,'macos',j));
 assert.throws(()=>selectArtifact([a,a],s.run,'macos',j));assert.throws(()=>selectArtifact([],s.run,'macos',j));
 const bytes=Buffer.from('archive');verifyArchive(bytes,`sha256:${createHash('sha256').update(bytes).digest('hex')}`);
 assert.throws(()=>verifyArchive(Buffer.from('tampered'),`sha256:${createHash('sha256').update(bytes).digest('hex')}`));
});
test('retry archives use producing attempt suffix and cannot silently reuse old upload',()=>{
 const s=source();const old=s.jobs.find(j=>j.name==='Build macos');const next=job('Build macos',2000,{run_attempt:2,started_at:'2026-01-02T00:00:00Z',completed_at:'2026-01-02T00:10:00Z'});
 s.jobs.push(next);s.artifacts.push(artifact(s.run,'macos',next,{id:999,created_at:'2026-01-02T00:05:00Z'}));
 s.receipts['build-macos']={...s.receipts['build-macos'],producer_job:next.id,producer_attempt:2};
 assert.equal(choose([s],'build-macos').artifact.id,999);
 s.artifacts.pop();assert.equal(choose([s],'build-macos').state,'failure');
 assert.equal(selectArtifact(s.artifacts,s.run,'macos',old).id,s.artifacts[0].id);
});
test('receipts flatten reused producer identity and final gate rechecks latest producer attempt',()=>{
 for(const component of components){
  const producer=source();const current=source(20,{head_branch:'v0.2.5',event:'workflow_dispatch',status:'in_progress'});
  current.receipts[component]=producer.receipts[component];current.artifacts=[];
  assert.equal(choose([current,producer],component,{gate:true}).run.id,10);
  producer.jobs.push(job(names[component],3000,{run_attempt:2,conclusion:'failure'}));
  assert.throws(()=>choose([current,producer],component,{gate:true}),/latest attempt/);
 }
});
test('missing, forged and superseded receipts cannot satisfy current release gate',()=>{
 const producer=source();const current=source(20);const c='frontend';
 for(const patch of [{schema:2},{component:'documentation'},{commit:'bad'},{producer_run:30},{producer_job:0},{producer_attempt:0}])assert.throws(()=>validateReceipt({...current.receipts[c],...patch},current,c,current.jobs.find(j=>j.name===names[c])));
 current.receipts[c]=producer.receipts[c];producer.jobs.push(job(names[c],3000,{run_attempt:2}));
 assert.throws(()=>choose([current,producer],c,{gate:true}),/superseded/);
 delete current.receipts[c];assert.throws(()=>choose([current,producer],c,{gate:true}));
});
test('current failure or pending state cannot be masked by an older passed checkpoint',()=>{
 const old=source();const current=source(20);const j=current.jobs.find(j=>j.name==='Test frontend');
 for(const conclusion of ['failure','skipped','cancelled']){j.conclusion=conclusion;assert.throws(()=>choose([current,old],'frontend',{gate:true}),/failed/);}
 j.conclusion=null;j.status='in_progress';assert.equal(choose([current,old],'frontend',{gate:true}).state,'pending');
 assert.throws(()=>choose([old],'frontend',{gate:true}),/missing/);
});
test('macOS readiness never waits for Windows, but final gate rejects Windows failure promptly',async()=>{
 const current=source(20,{status:'in_progress'});current.jobs.find(j=>j.name==='Test Rust windows').conclusion='failure';
 const options={gate:true,snapshot:()=>[current],receiptReader,timeoutMs:0};
 assert.equal((await awaitComponents(ctx,['rust-macos','build-macos'],options)).length,2);
 await assert.rejects(awaitComponents(ctx,components,options),/rust-windows/);
});
test('bounded waits poll pending work, reject timeout/invalid settings and never report false success',async()=>{
 const current=source(20,{status:'in_progress'});const j=current.jobs.find(j=>j.name==='Build windows');j.status='in_progress';j.conclusion=null;
 let polls=0;const options={gate:true,snapshot:()=>{polls++;return[current];},receiptReader,pollMs:1,timeoutMs:100,pause:async()=>{j.status='completed';j.conclusion='success';}};
 assert.equal((await awaitComponents(ctx,['build-windows'],options))[0].state,'success');assert.equal(polls,2);
 j.status='in_progress';j.conclusion=null;await assert.rejects(awaitComponents(ctx,['build-windows'],{...options,timeoutMs:0}),/Timed out/);
 for(const timeoutMs of [NaN,-1,Infinity,210*60*1000+1])await assert.rejects(awaitComponents(ctx,[],{...options,timeoutMs}),/Invalid wait timeout/);
 for(const pollMs of [0,-1,NaN,60001])await assert.rejects(awaitComponents(ctx,[],{...options,pollMs}),/Invalid poll interval/);
});

// Unix executable shim exercises the actual CLI/API orchestration. The pure
// policy tests above also run on Windows, where shebang executables differ.
test('mocked gh CLI reuses an older failed tag platform, ignores newer runs and records original producer', {skip:process.platform==='win32'},()=>{
 const directory=mkdtempSync(join(tmpdir(),'sag-release-cli-'));
 try{
  const producer=source(10,{head_branch:'v0.2.5',event:'workflow_dispatch',conclusion:'failure'});
  producer.jobs=producer.jobs.filter(j=>j.name==='Validate release source');producer.jobs.push(job('Build and test macos',88));
  producer.artifacts=[{...producer.artifacts[0],name:`desktop-macos-${commit}`}];
  const current=source(20,{head_branch:'v0.2.5',event:'workflow_dispatch',status:'in_progress'});
  current.jobs.find(j=>j.name==='Build macos').conclusion=null;current.jobs.find(j=>j.name==='Build macos').status='in_progress';
  const newer=source(30,{status:'in_progress'});
  writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  const shim=`#!${process.execPath}\nconst fs=require('node:fs');const sources=JSON.parse(fs.readFileSync(process.env.CACHE_FIXTURE));const args=process.argv.slice(2);const url=args.at(-1);if(process.env.CACHE_FAIL_API)throw Error('API unavailable');let value;if(url.includes('/runs?'))value={workflow_runs:sources.map(s=>s.run)};else if(url.includes('/runs/')){const id=Number(url.match(/runs\\/(\\d+)/)[1]);if(id>20)throw Error('newer run consulted');const s=sources.find(s=>s.run.id===id);value=url.includes('/jobs')?{jobs:s.jobs}:url.includes('/artifacts')?{artifacts:s.artifacts}:s.run;}else value={id:4};process.stdout.write(JSON.stringify(args.includes('--slurp')?[value]:value));\n`;
  writeFileSync(join(directory,'gh'),shim);chmodSync(join(directory,'gh'),0o755);
  const output=join(directory,'outputs');const env={...process.env,PATH:`${directory}:${process.env.PATH}`,CACHE_FIXTURE:join(directory,'fixture.json'),GITHUB_REPOSITORY:'Zleap-AI/SAG-Wiki',GITHUB_SHA:commit,GITHUB_RUN_ID:'20',GITHUB_OUTPUT:output,RELEASE_WAIT_TIMEOUT_MS:'0',RELEASE_WAIT_POLL_MS:'1'};
  const cli=resolve(import.meta.dirname,'release-build-cache.mjs');
  const reused=spawnSync(process.execPath,[cli,'reuse','build-macos'],{cwd:directory,env,encoding:'utf8'});
  assert.equal(reused.status,0,reused.stderr);assert.match(readFileSync(output,'utf8'),/reuse=true/);
  const receipt=JSON.parse(readFileSync(join(directory,'release-checkpoints/build-macos.json')));assert.equal(receipt.producer_run,10);assert.equal(receipt.producer_job,88);
  producer.run.head_branch='main';producer.run.event='push';producer.run.status='in_progress';producer.run.conclusion=null;
  writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  const running=spawnSync(process.execPath,[cli,'reuse','build-macos'],{cwd:directory,env,encoding:'utf8'});assert.equal(running.status,0,running.stderr);assert.match(running.stdout,/Reuse build-macos checkpoint from run 10/);
  current.run.head_branch='main';current.run.event='workflow_dispatch';writeFileSync(join(directory,'fixture.json'),JSON.stringify([current]));
  const manual=spawnSync(process.execPath,[cli,'reuse','build-macos'],{cwd:directory,env,encoding:'utf8'});assert.equal(manual.status,0,manual.stderr);assert.match(manual.stdout,/Execute build-macos locally/);
  const unavailableOutput=join(directory,'unavailable-output');
  const unavailable=spawnSync(process.execPath,[cli,'reuse','build-macos'],{cwd:directory,env:{...env,CACHE_FAIL_API:'1',GITHUB_OUTPUT:unavailableOutput},encoding:'utf8'});assert.notEqual(unavailable.status,0);assert.equal(existsSync(unavailableOutput),false);
  current.run.head_branch='v0.2.5';current.run.event='workflow_dispatch';
  current.jobs.find(j=>j.name==='Build macos').conclusion='failure';writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  writeFileSync(join(directory,'.release-wait-deadline'),String(Date.now()+60000));
  const failed=spawnSync(process.execPath,[cli,'wait','macos'],{cwd:directory,env,encoding:'utf8'});assert.notEqual(failed.status,0);assert.match(failed.stderr,/checkpoint failed: build-macos/);
  current.jobs.find(j=>j.name==='Build macos').conclusion=null;current.jobs.find(j=>j.name==='Build macos').status='in_progress';writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  const pending=spawnSync(process.execPath,[cli,'wait','macos'],{cwd:directory,env,encoding:'utf8'});assert.notEqual(pending.status,0);assert.match(pending.stderr,/Timed out/);
 }finally{rmSync(directory,{recursive:true,force:true});}
});

test('both Mac architectures reuse parallel native builds and signing before existing assembly',()=>{
 const workflow=readFileSync(new URL('../.github/workflows/desktop-release.yml',import.meta.url),'utf8').replace(/\r\n/g,'\n');
 for(const name of ['Test Rust macos','Test Rust macos-intel','Test Rust windows','Build macos','Build macos-intel','Build windows','Compile CLI macos-intel'])assert.ok(workflow.includes(`job_name: ${name}`));
 const signing=workflow.slice(workflow.indexOf('\n  sign-macos:'),workflow.indexOf('\n  release:'));
 assert.match(signing,/needs: \[validate, build-macos\]/);
 assert.match(signing,/needs: \[validate, build-macos-intel\]/);
 assert.ok(signing.indexOf('release-build-cache.mjs wait')<signing.indexOf('APPLE_CERTIFICATE_BASE64'));
 assert.match(signing,/runner: macos-15-intel/);assert.match(signing,/fail-fast: false/);
 assert.ok(signing.includes('name: desktop-${{ matrix.platform }}-signed-${{ github.sha }}\n'));
 assert.match(signing,/overwrite: true/);
 const network=signing.slice(signing.indexOf('      - name: Use TCP for Apple'),signing.indexOf('      - name: Sign, notarize'));
 assert.match(network,/if: matrix.platform == 'macos-intel'/);
 assert.match(network,/proto udp from any to any port 443/);
 assert.match(network,/sudo pfctl -E/);
 const release=workflow.slice(workflow.indexOf('\n  release:'));
 assert.ok(release.includes('pattern: desktop-*-signed-${{ github.sha }}\n'));
 assert.ok(!release.includes('signed-${{ github.sha }}-${{ github.run_attempt }}'));
 assert.match(release,/needs: \[validate, sign-macos, sign-macos-intel\]/);
 assert.match(release,/runs-on: ubuntu-24.04/);
 assert.ok(release.indexOf('wait all')<release.indexOf('TAURI_SIGNING_PRIVATE_KEY'));
 assert.match(release,/automatic-release\.mjs create-draft/);assert.match(workflow,/needs: \[validate, release\]/);
 assert.doesNotMatch(workflow.slice(0,workflow.indexOf('\n  sign-macos:')),/secrets\.|contents: write|id-token: write/);
});

test('parallel build receipts bind the CLI compiler and reject superseding failed or pending attempts',()=>{
 const current=source(20);
 const compiler=job('Compile CLI macos',899);
 current.jobs.push(compiler);
 current.receipts['build-macos']={...current.receipts['build-macos'],schema:2,cli_producer:{run:20,job:899,attempt:1}};
 assert.equal(choose([current],'build-macos',{gate:true}).job.name,'Build macos');
 current.jobs.push(job('Compile CLI macos',900,{run_attempt:2,conclusion:'failure'}));
 assert.throws(()=>choose([current],'build-macos',{gate:true}),/CLI compiler/);
 current.jobs.at(-1).conclusion=null;current.jobs.at(-1).status='in_progress';
 assert.throws(()=>choose([current],'build-macos',{gate:true}),/CLI compiler/);
});
test('reused build receipts recheck the original CLI compiler, not a passing current compiler',()=>{
 const producer=source(10);producer.jobs.push(job('Compile CLI windows',901));
 producer.receipts['build-windows']={...producer.receipts['build-windows'],schema:2,cli_producer:{run:10,job:901,attempt:1}};
 const current=source(20);current.receipts['build-windows']=producer.receipts['build-windows'];
 assert.equal(choose([current,producer],'build-windows',{gate:true}).run.id,10);
 producer.jobs.push(job('Compile CLI windows',902,{run_attempt:2}));
 assert.throws(()=>choose([current,producer],'build-windows',{gate:true}),/CLI compiler/);
});

test('parallel producer cannot downgrade a package checkpoint to the old inline schema',()=>{
 const current=source(20);current.jobs.push(job('Compile CLI macos',903));
 assert.throws(()=>choose([current],'build-macos',{gate:true}),/Parallel build.*schema/);
});
test('forwarded legacy aggregate producer needs no modern receipt',()=>{
 const producer=source(10);producer.jobs=producer.jobs.filter(j=>j.name!=='Build macos');producer.jobs.push(job('Build and test macos',88));producer.artifacts[0].name=`desktop-macos-${commit}`;delete producer.receipts['build-macos'];
 const current=source(20);current.receipts['build-macos']={schema:1,commit,component:'build-macos',producer_run:10,producer_job:88,producer_attempt:1};
 assert.equal(choose([current,producer],'build-macos',{gate:true}).run.id,10);
});
test('the flattened original producer must have a self-canonical modern receipt',()=>{
 const older=source(5);const producer=source(10);producer.receipts['build-macos']=older.receipts['build-macos'];
 const current=source(20);current.receipts['build-macos']={schema:1,commit,component:'build-macos',producer_run:10,producer_job:producer.jobs.find(j=>j.name==='Build macos').id,producer_attempt:1};
 assert.throws(()=>choose([current,producer,older],'build-macos',{gate:true}),/canonical/);
});

test('PR E2E bounds outer test capacity while retaining the complete CLI suite',()=>{
 const workflow=readFileSync(new URL('../.github/workflows/pr-e2e.yml',import.meta.url),'utf8');
 const e2e=workflow.slice(workflow.indexOf('\n  e2e:'),workflow.indexOf('\n  qpdf-linux:'));
 assert.match(e2e,/RUST_TEST_THREADS:\s*['"]?2['"]?(?:\s|$)/);
 assert.match(e2e,/run: cargo test --locked --manifest-path cli\/Cargo.toml\s*\n/);
 assert.doesNotMatch(e2e,/--skip|--ignored/);
});

test('Intel signing readiness is independent of ARM builds and tests while using its own checkpoint', async () => {
 const s=source(20,{status:'in_progress',conclusion:null});
 for(const name of ['Build macos','Test Rust macos-intel','Build windows'])Object.assign(s.jobs.find(j=>j.name===name),{status:'in_progress',conclusion:null});
 const result=await awaitComponents(ctx,releaseComponents('macos-intel'),{gate:true,snapshot:()=>[s],receiptReader,pause:()=>{throw new Error('Intel signing waited for another architecture');}});
 assert.deepEqual(result.map(r=>r.artifact?.platform),['macos-intel']);
});

test('retained successful job copies resolve to their actual artifact-producing execution', () => {
 const s=source(20); const c='build-macos-intel';
 const original=s.jobs.find(j=>j.name===names[c]);
 original.steps=[{name:'Build',status:'completed',conclusion:'success',started_at:original.started_at,completed_at:original.completed_at}];
 const copied={...original,id:99001,run_attempt:2};s.jobs.push(copied);
 assert.equal(latestJob(s.jobs,names[c]).id,original.id);
 assert.equal(choose([s],c,{gate:true}).artifact.name,`desktop-macos-intel-${commit}-1`);
 // A real rerun, even one with no new artifact yet, must supersede the old success.
 copied.started_at='2026-01-02T00:00:00Z';copied.completed_at='2026-01-02T00:10:00Z';
 assert.equal(latestJob(s.jobs,names[c]).id,copied.id);
 assert.throws(()=>choose([s],c,{gate:true}),/superseded|artifact|checkpoint job/);
 copied.conclusion='failure';assert.throws(()=>choose([s],c,{gate:true}),/checkpoint failed/);
 copied.conclusion=null;copied.status='in_progress';assert.equal(choose([s],c,{gate:true}).state,'pending');
});

test('retained CLI copies preserve compiler receipt identity without masking new executions', () => {
 const s=source(20), compiler=job('Compile CLI macos-intel',8801);
 compiler.steps[0].started_at=compiler.started_at;compiler.steps[0].completed_at=compiler.completed_at;
 s.jobs.push(compiler,{...compiler,id:8802,run_attempt:2});
 s.receipts['build-macos-intel']={...s.receipts['build-macos-intel'],schema:2,cli_producer:{run:20,job:8801,attempt:1}};
 assert.equal(choose([s],'build-macos-intel',{gate:true}).state,'success');
 s.jobs.at(-1).conclusion='failure';assert.throws(()=>choose([s],'build-macos-intel',{gate:true}),/CLI compiler/);
});
