import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {validateRun, latestJob, componentState, selectArtifact, selectComponent, validateReceipt, verifyArchive, awaitComponents, components} from './release-build-cache.mjs';
const commit = 'a'.repeat(40);
const ctx = {commit, workflowId:4, currentRunId:20};
const names = {documentation:'Build bilingual documentation', frontend:'Test frontend', 'rust-macos':'Test Rust macos', 'rust-windows':'Test Rust windows', 'build-macos':'Build macos', 'build-windows':'Build windows'};
const run = (id=10, patch={}) => ({id, repository:{id:3, full_name:'Zleap-AI/SAG-Wiki'}, head_repository:{full_name:'Zleap-AI/SAG-Wiki'}, head_sha:commit, workflow_id:4, path:'.github/workflows/desktop-release.yml', head_branch:'main', event:'push', status:'completed', conclusion:'success', ...patch});
const job = (name, id=1, patch={}) => ({id, name, conclusion:'success', status:'completed', run_attempt:1, started_at:'2026-01-01T00:00:00Z', completed_at:'2026-01-01T00:10:00Z', ...patch});
const artifact = (sourceRun, platform, sourceJob, patch={}) => ({id:100+sourceRun.id, name:`desktop-${platform}-${commit}-${sourceJob.run_attempt}`, expired:false, expires_at:'2099-01-01T00:00:00Z', created_at:'2026-01-01T00:05:00Z', digest:`sha256:${'b'.repeat(64)}`, workflow_run:{id:sourceRun.id, head_sha:commit, repository_id:3, head_repository_id:3}, ...patch});
function source(id=10, patch={}) {
 const sourceRun=run(id,patch);
 const jobs=[job('Validate release source',id*100), ...components.map((c,i)=>job(names[c],id*100+i+1))];
 const receipts=Object.fromEntries(components.map((c,i)=>[c,{schema:1,commit,component:c,producer_run:id,producer_job:jobs[i+1].id,producer_attempt:1}]));
 return {run:sourceRun,jobs,receipts,artifacts:['macos','windows'].map(p=>artifact(sourceRun,p,jobs.find(j=>j.name===`Build ${p}`)))};
}
const receiptReader=(s,c)=>s.receipts[c];
const choose=(sources,c,options={})=>selectComponent(sources,c,{...ctx,receiptReader,...options});

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
  current.jobs.find(j=>j.name==='Test Rust macos').conclusion='failure';writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  const failed=spawnSync(process.execPath,[cli,'wait','macos'],{cwd:directory,env,encoding:'utf8'});assert.notEqual(failed.status,0);assert.match(failed.stderr,/checkpoint failed: rust-macos/);
  current.jobs.find(j=>j.name==='Test Rust macos').conclusion=null;current.jobs.find(j=>j.name==='Test Rust macos').status='in_progress';writeFileSync(join(directory,'fixture.json'),JSON.stringify([producer,current,newer]));
  const pending=spawnSync(process.execPath,[cli,'wait','macos'],{cwd:directory,env,encoding:'utf8'});assert.notEqual(pending.status,0);assert.match(pending.stderr,/Timed out/);
 }finally{rmSync(directory,{recursive:true,force:true});}
});

test('workflow has independent native stages, one protected deployment and a final draft-only gate',()=>{
 const workflow=readFileSync(new URL('../.github/workflows/desktop-release.yml',import.meta.url),'utf8');
 for(const name of ['Test Rust macos','Test Rust windows','Build macos','Build windows'])assert.match(workflow,new RegExp(`job_name: ${name}`));
 assert.equal((workflow.match(/^    environment:$/gm)||[]).length,1);
 const release=workflow.slice(workflow.indexOf('\n  release:'));
 assert.match(release,/needs: validate/);assert.doesNotMatch(release,/needs:.*(?:native|frontend|documentation)/);
 assert.ok(release.indexOf('wait macos')<release.indexOf('APPLE_CERTIFICATE_BASE64'));
 assert.ok(release.indexOf('APPLE_CERTIFICATE_BASE64')<release.indexOf('wait all'));
 assert.ok(release.indexOf('wait all')<release.indexOf('TAURI_SIGNING_PRIVATE_KEY'));
 assert.match(release,/--draft/);assert.match(workflow,/checkpoint-\$\{\{ matrix.component \}\}/);
 assert.match(workflow,/desktop-\$\{\{ matrix.platform \}\}-\$\{\{ github.sha \}\}-\$\{\{ github.run_attempt \}\}/);
 assert.doesNotMatch(workflow.slice(0,workflow.indexOf('\n  release:')),/secrets\.|contents: write|id-token: write/);
});
