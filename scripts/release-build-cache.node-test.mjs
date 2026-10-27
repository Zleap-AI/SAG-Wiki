import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {validateRun, selectArtifacts, verifyArchive} from './release-build-cache.mjs';
import * as cache from './release-build-cache.mjs';
const commit = 'a'.repeat(40);
const run = () => ({id:1, repository:{id:3, full_name:'Zleap-AI/SAG-Wiki'}, head_repository:{full_name:'Zleap-AI/SAG-Wiki'}, head_sha:commit, workflow_id:4, path:'.github/workflows/desktop-release.yml', head_branch:'main', event:'push', status:'completed', conclusion:'success'});
const jobs = () => ['Validate release source','Build bilingual documentation','Build and test macos','Build and test windows'].map(name => ({name, conclusion:'success', run_attempt:1}));
const ctx = {commit, workflowId:4, currentRunId:2};
const artifacts = () => ['macos','windows'].map((platform,i) => ({id:i+10,name:`desktop-${platform}-${commit}`,expired:false,expires_at:'2099-01-01T00:00:00Z',digest:`sha256:${'b'.repeat(64)}`,workflow_run:{id:1,head_sha:commit,repository_id:3,head_repository_id:3}}));
test('accepts exact-commit successful trusted main build with both platforms', () => {
 validateRun(run(), jobs(), ctx); assert.equal(selectArtifacts(artifacts(), run()).length,2);
});
test('rejects wrong commit, workflow, fork, event, branch and unsuccessful source', () => {
 for(const patch of [{head_sha:'b'.repeat(40)},{workflow_id:9},{path:'other.yml'},{repository:{full_name:'evil/repo'}},{head_repository:{full_name:'evil/repo'}},{event:'pull_request'},{head_branch:'feature'},{status:'in_progress'},{conclusion:'failure'}]) assert.throws(()=>validateRun({...run(),...patch},jobs(),ctx));
});
test('requires each successful job, including documentation, on latest attempt', () => {
 for(let i=0;i<4;i++){ const j=jobs(); j[i].conclusion='skipped'; assert.throws(()=>validateRun(run(),j,ctx)); }
 assert.throws(()=>validateRun(run(),jobs().slice(1),ctx));
 assert.throws(()=>validateRun(run(),[...jobs(),jobs()[0]],ctx));
});
test('current tag run may still be running but must have completed all required jobs', () => {
 const current={...run(),id:2,head_branch:'v0.2.0',event:'workflow_dispatch',status:'in_progress',conclusion:null};
 validateRun(current,jobs(),ctx); assert.throws(()=>validateRun(current,[],ctx));
});
test('rejects expired, missing, duplicate, foreign and unverifiable archives', () => {
 for(const patch of [{expired:true},{expires_at:'2000-01-01'},{digest:null},{id:-1},{workflow_run:{...artifacts()[0].workflow_run,id:9}},{workflow_run:{...artifacts()[0].workflow_run,head_sha:'wrong'}},{workflow_run:{...artifacts()[0].workflow_run,head_repository_id:9}}]) {
 const a=artifacts();Object.assign(a[0],patch);assert.throws(()=>selectArtifacts(a,run())); }
 assert.throws(()=>selectArtifacts(artifacts().slice(1),run()));
 assert.throws(()=>selectArtifacts([...artifacts(),artifacts()[0]],run()));
});
test('archive checksum mismatch fails closed',()=>{
 const bytes=Buffer.from('archive'); const digest=`sha256:${createHash('sha256').update(bytes).digest('hex')}`;
 verifyArchive(bytes,digest);assert.throws(()=>verifyArchive(Buffer.from('tampered'),digest));
});

test('release-only retries retain passed prerequisites, but new failures supersede them', () => {
 validateRun(run(), [...jobs(), {name:'Sign and create a draft release',run_attempt:2,conclusion:null}],ctx);
 assert.throws(()=>validateRun(run(), [...jobs(), {...jobs()[2],run_attempt:2,conclusion:'failure'}],ctx));
 validateRun(run(), [...jobs(), {...jobs()[2],run_attempt:2}],ctx);
});

test('main needs no API and tag lookup failure safely selects a full build', async () => {
 const {mkdtempSync,readFileSync,rmSync}=await import('node:fs');
 const {tmpdir}=await import('node:os'); const {join}=await import('node:path');
 const {spawnSync}=await import('node:child_process');
 const dir=mkdtempSync(join(tmpdir(),'sag-reuse-test-'));
 try {
  for(const ref of ['refs/heads/main','refs/tags/v0.2.0']) {
   const output=join(dir,ref.endsWith('main')?'main':'tag');
   const result=spawnSync(process.execPath,['scripts/release-build-cache.mjs','resolve'],{encoding:'utf8',env:{...process.env,PATH:dir,GITHUB_REPOSITORY:'Zleap-AI/SAG-Wiki',GITHUB_SHA:commit,GITHUB_RUN_ID:'2',GITHUB_REF:ref,GITHUB_OUTPUT:output}});
   assert.equal(result.status,0,result.stderr);assert.equal(readFileSync(output,'utf8'),'source_run=2\nreuse=false\n');
  }
 }finally{rmSync(dir,{recursive:true,force:true});}
});

test('signed artifacts must come from this run and a successful latest macOS signing job', () => {
 const current = {...run(), id:2, head_branch:'v0.2.4', event:'workflow_dispatch', status:'in_progress', conclusion:null};
 const signing = {name:'Sign and notarize macOS',run_attempt:1,conclusion:'success'};
 cache.validateSignedRun(current,[signing],ctx);
 for (const patch of [{id:1}, {head_sha:'b'.repeat(40)}, {head_branch:'feature'}, {event:'pull_request'}, {workflow_id:9}]) {
  assert.throws(()=>cache.validateSignedRun({...current,...patch},[signing],ctx));
 }
 assert.throws(()=>cache.validateSignedRun(current,[],ctx));
 assert.throws(()=>cache.validateSignedRun(current,[signing,{...signing,run_attempt:2,conclusion:'failure'}],ctx));
 assert.throws(()=>cache.validateSignedRun(current,[signing,signing],ctx));
 const artifact = {...artifacts()[0],name:`desktop-macos-signed-${commit}`,workflow_run:{...artifacts()[0].workflow_run,id:2}};
 assert.equal(cache.selectSignedArtifact([artifact],current).id,artifact.id);
 assert.throws(()=>cache.selectSignedArtifact(artifacts(),current));
 assert.throws(()=>cache.selectSignedArtifact([artifact,artifact],current));
 assert.throws(()=>cache.selectSignedArtifact([{...artifact,workflow_run:{...artifact.workflow_run,id:1}}],current));
 assert.throws(()=>cache.selectSignedArtifact([{...artifact,digest:null}],current));
});

test('requires a current successful Windows signing verification job and its own proof artifact', () => {
 const current={...run(),id:2,head_branch:'v0.2.4',event:'workflow_dispatch',status:'in_progress',conclusion:null};
 const verified={name:'Verify Windows code signing',run_attempt:1,conclusion:'success'};
 cache.validateWindowsSigningRun(current,[verified],ctx);
 assert.throws(()=>cache.validateWindowsSigningRun(current,[],ctx));
 assert.throws(()=>cache.validateWindowsSigningRun(current,[verified,{...verified,run_attempt:2,conclusion:'failure'}],ctx));
 assert.throws(()=>cache.validateWindowsSigningRun({...current,id:1},[verified],ctx));
 const artifact={...artifacts()[1],name:`desktop-windows-signing-proof-${commit}`,workflow_run:{...artifacts()[1].workflow_run,id:2}};
 assert.equal(cache.selectWindowsSigningArtifact([artifact],current).id,artifact.id);
 assert.throws(()=>cache.selectWindowsSigningArtifact(artifacts(),current));
 assert.throws(()=>cache.selectWindowsSigningArtifact([{...artifact,digest:null}],current));
});
