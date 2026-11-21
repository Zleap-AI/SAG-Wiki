import assert from 'node:assert/strict';
import {appendFileSync,chmodSync,copyFileSync,cpSync,existsSync,lstatSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,realpathSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {basename,dirname,join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {compilerJobMode,downloadArtifacts,hashFile,latestJob,selectArchive} from './release-build-cache.mjs';
import {targets,validateVersions} from './release.mjs';
import {version as qpdfVersion} from './prepare-qpdf.mjs';
const repository='Zleap-AI/SAG-Wiki';
const execute=(command,args,options={})=>{const result=spawnSync(command,args,{encoding:'utf8',timeout:180000,maxBuffer:32*1024*1024,...options});assert.equal(result.status,0,`${command} failed: ${result.error?.message??result.stderr}`);return result.stdout?.trim();};
const api=path=>JSON.parse(execute('gh',['api',`repos/${repository}/${path}`]));
const pages=(path,field)=>JSON.parse(execute('gh',['api','--paginate','--slurp',`repos/${repository}/${path}${path.includes('?')?'&':'?'}per_page=100`])).flatMap(page=>page[field]);
const files=(directory,prefix='')=>readdirSync(join(directory,prefix),{withFileTypes:true}).flatMap(entry=>{const name=prefix?`${prefix}/${entry.name}`:entry.name;assert.ok(!entry.isSymbolicLink(),'CLI payload cannot contain symlinks');assert.ok(entry.isDirectory()||entry.isFile(),'CLI payload contains a special file');return entry.isDirectory()?files(directory,name):[name];}).sort();
const allowed=(name,platform)=>!name.includes('\\')&&!name.includes(':')&&!name.split('/').some(part=>['','.','..'].includes(part))&&(
 name===`source/${platform==='windows'?'sag.exe':'sag'}`||name===`resources/bin/${platform==='windows'?'sag.exe':'sag'}`||name.startsWith('resources/bin/qpdf/')||name.startsWith('resources/bin/pdfium/'));
const manifestName='manifest.json';
export const desktopCompileConfig=({reuseWeb=false}={})=>({build:{beforeBuildCommand:reuseWeb?'npm run build:clipper:desktop':'npm run build:clipper:desktop && npm run build'}});
export function bundleArguments(platform,target){
 assert.equal(target,targets[platform],'Unexpected bundle target');
 return ['bundle','--ci','--target',target,...(platform==='windows'?['--config','src-tauri/tauri.windows.conf.json']:['--bundles','app'])];
}
export function recoverSkippedCli(job,{build,verify,record}){
 assert.equal(compilerJobMode(job),'reuse-skipped','Only a successful skipped reuse probe may recover inline');
 build();verify();record();
}
export function createCliManifest(directory,context){
 const entries=files(directory).filter(name=>name!==manifestName);
 assert.ok(entries.length>2,'Missing CLI/QPDF payload');
 const manifest={schema:1,...context,files:{}};
 for(const name of entries){assert.ok(allowed(name,context.platform),'Invalid CLI payload path');const file=join(directory,name);const info=lstatSync(file);assert.ok(info.isFile(),'Non-file CLI payload');manifest.files[name]={sha256:hashFile(file),mode:info.mode&0o777};}
 return manifest;
}
export function verifyCliManifest(directory,manifest,context){
 assert.equal(manifest.schema,1,'Unknown CLI payload schema');
 for(const field of ['commit','version','platform','target','run','job','attempt'])assert.equal(manifest[field],context[field],`CLI payload ${field} mismatch`);
 const expected=Object.keys(manifest.files).sort();
 for(const name of expected){assert.ok(allowed(name,context.platform),'Invalid CLI payload path');const entry=manifest.files[name];assert.match(entry.sha256,/^[a-f0-9]{64}$/);assert.ok(Number.isInteger(entry.mode)&&entry.mode>=0&&entry.mode<=0o777,'Unsafe CLI mode');}
 const binary=context.platform==='windows'?'sag.exe':'sag';
 assert.ok(expected.includes(`source/${binary}`)&&expected.includes(`resources/bin/${binary}`),'Missing CLI binaries');
 assert.ok(expected.includes(`resources/bin/qpdf/bin/${context.platform==='windows'?'qpdf.exe':'qpdf'}`),'Missing QPDF executable');
 assert.ok(expected.includes(`resources/bin/pdfium/${context.platform==='windows'?'pdfium.dll':'libpdfium.dylib'}`),'Missing PDFium library');
 if(context.platform!=='windows')for(const name of [`source/${binary}`,`resources/bin/${binary}`,'resources/bin/qpdf/bin/qpdf'])assert.ok(manifest.files[name].mode&0o111,'Missing executable mode for POSIX binary');
 assert.deepEqual(files(directory).filter(name=>name!==manifestName),expected,'Unexpected CLI payload files');
 for(const name of expected)assert.equal(hashFile(join(directory,name)),manifest.files[name].sha256,'CLI payload checksum mismatch');
 return manifest;
}
export function installCliPayload(directory,manifest,context,root=process.cwd(),env=process.env){
 verifyCliManifest(directory,manifest,context);
 root=existsSync(root)?realpathSync(root):join(realpathSync(dirname(root)),basename(root));
 const binary=context.platform==='windows'?'sag.exe':'sag';
 const destinations=Object.keys(manifest.files).map(name=>[name,name.startsWith('resources/')?join(root,'src-tauri',name.slice('resources/'.length)):join(resolve(root,env.CARGO_TARGET_DIR??'src-tauri/target'),context.target,'release',binary)]);
 for(const[,destination]of destinations){for(let p=destination;;p=dirname(p)){assert.ok(!existsSync(p)||!lstatSync(p).isSymbolicLink(),'CLI destination contains symlink');if(dirname(p)===p)break;}}
 for(const[name,destination]of destinations){mkdirSync(dirname(destination),{recursive:true});copyFileSync(join(directory,name),destination);chmodSync(destination,manifest.files[name].mode);}
}
export function validateCliProducer(run,jobs,context){
 assert.equal(run.id,context.run,'CLI transfer must remain in the current workflow run');
 assert.equal(run.head_sha,context.commit,'CLI transfer source mismatch');
 assert.equal(run.repository?.full_name,repository);assert.equal(run.head_repository?.full_name,repository);
 assert.equal(run.workflow_id,context.workflowId);assert.equal(run.path,'.github/workflows/desktop-release.yml');assert.equal(run.event,'workflow_dispatch');
 const validation=latestJob(jobs,'Validate release source');assert.equal(validation?.conclusion,'success','Release source validation did not pass');
 const job=latestJob(jobs,`Compile CLI ${context.platform}`);assert.equal(job?.conclusion,'success','Latest CLI compilation did not pass');
 compilerJobMode(job);
 return job;
}
export function verifyQpdfVersion(output){
 assert.equal(output.split(/\r?\n/)[0],`qpdf version ${qpdfVersion}`,'Restored QPDF executable or runtime libraries are invalid');
}
function verifyLocalBinaries(ctx){
 const binary=ctx.platform==='windows'?'sag.exe':'sag';
 const source=join(resolve(process.env.CARGO_TARGET_DIR??'src-tauri/target'),ctx.target,'release',binary);
 const resource=resolve('src-tauri/bin',binary);
 const identity=JSON.parse(execute(source,['--build-info']));assert.equal(identity.version,ctx.version);assert.equal(identity.channel,'release');assert.deepEqual(JSON.parse(execute(resource,['--build-info'])),identity,'Resource CLI identity differs from original cargo output');
 const qpdf=execute(resolve('src-tauri/bin/qpdf/bin',ctx.platform==='windows'?'qpdf.exe':'qpdf'),['--version']);
 verifyQpdfVersion(qpdf);
 return {source,resource,binary};
}
function recordCompiler(ctx,job,inline=false){
 const receiptPath=`release-checkpoints/build-${ctx.platform}.json`;
 if(existsSync(receiptPath)){const receipt=JSON.parse(readFileSync(receiptPath,'utf8'));assert.equal(receipt.producer_run,ctx.run);receipt.schema=2;receipt.cli_producer={run:ctx.run,job:job.id,attempt:job.run_attempt,...(inline?{inline:true}:{})};writeFileSync(receiptPath,JSON.stringify(receipt)+'\n');}
}
function context(){
 assert.equal(process.env.GITHUB_REPOSITORY,repository);
 const platform=process.env.RELEASE_PLATFORM;assert.ok(Object.hasOwn(targets,platform));assert.equal(process.env.TARGET_TRIPLE,targets[platform]);
 const commit=process.env.GITHUB_SHA;assert.match(commit??'',/^[a-f0-9]{40}$/);
 const run=Number(process.env.GITHUB_RUN_ID);assert.ok(Number.isSafeInteger(run)&&run>0);
 return{commit,version:validateVersions(),platform,target:targets[platform],run};
}
const artifactName=(ctx,attempt)=>`cli-compile-${ctx.platform}-${ctx.commit}-${attempt}`;
function stage(ctx){
 const run=api(`actions/runs/${ctx.run}`);const jobs=pages(`actions/runs/${ctx.run}/jobs?filter=all`,'jobs');
 const job=latestJob(jobs,`Compile CLI ${ctx.platform}`);assert.ok(job&&job.run_attempt===Number(process.env.GITHUB_RUN_ATTEMPT),'Current CLI job unavailable');
 // The job is still running here; consumers require its final successful state.
 assert.equal(run.head_sha,ctx.commit);assert.equal(run.repository?.full_name,repository);assert.equal(run.head_repository?.full_name,repository);
 const {source,resource,binary}=verifyLocalBinaries(ctx);
 const directory=resolve('release-compile',`cli-${ctx.platform}`);assert.ok(!existsSync(directory),'CLI staging must be fresh');mkdirSync(join(directory,'source'),{recursive:true});mkdirSync(join(directory,'resources/bin'),{recursive:true});copyFileSync(source,join(directory,'source',binary));copyFileSync(resource,join(directory,'resources/bin',binary));cpSync(resolve('src-tauri/bin/qpdf'),join(directory,'resources/bin/qpdf'),{recursive:true,dereference:true});
 cpSync(resolve('src-tauri/bin/pdfium'),join(directory,'resources/bin/pdfium'),{recursive:true,dereference:true});
 const manifest=createCliManifest(directory,{...ctx,job:job.id,attempt:job.run_attempt});writeFileSync(join(directory,manifestName),JSON.stringify(manifest)+'\n');
 appendFileSync(process.env.GITHUB_OUTPUT,`directory=${directory}\nartifact=${artifactName(ctx,job.run_attempt)}\n`);
}
async function restore(ctx){
 const workflowId=api('actions/workflows/desktop-release.yml').id;const deadline=Date.now()+95*60*1000;
 while(true){
  const run=api(`actions/runs/${ctx.run}`);const jobs=pages(`actions/runs/${ctx.run}/jobs?filter=all`,'jobs');const current=latestJob(jobs,`Compile CLI ${ctx.platform}`);
  assert.ok(!current||!current.conclusion||current.conclusion==='success','CLI compilation failed; do not bundle');
  if(current?.conclusion==='success'){
   const job=validateCliProducer(run,jobs,{...ctx,workflowId});
   if(compilerJobMode(job)==='reuse-skipped'){
    console.log('Earlier platform reuse changed after the CLI probe; compile the real CLI locally before bundling');
    recoverSkippedCli(job,{build:()=>execute(process.execPath,[resolve('scripts/build-cli.mjs')],{stdio:'inherit',timeout:190*60*1000,env:{...process.env,TAURI_ENV_TARGET_TRIPLE:ctx.target,SAG_CARGO_TIMINGS:'1'}}),verify:()=>verifyLocalBinaries(ctx),record:()=>recordCompiler(ctx,job,true)});
    return;
   }
   const artifact=selectArchive(pages(`actions/runs/${ctx.run}/artifacts`,'artifacts'),run,artifactName(ctx,job.run_attempt),job);
   const temporary=mkdtempSync(join(tmpdir(),'sag-cli-input-'));
   try{downloadArtifacts([{...artifact,platform:'cli'}],temporary);const directory=join(temporary,'cli');const manifest=JSON.parse(readFileSync(join(directory,manifestName),'utf8'));installCliPayload(directory,manifest,{...ctx,job:job.id,attempt:job.run_attempt});
    verifyLocalBinaries(ctx);recordCompiler(ctx,job);
   }finally{rmSync(temporary,{recursive:true,force:true});}
   console.log(`Verified CLI input from run ${ctx.run}, job ${job.id}, attempt ${job.run_attempt}`);return;
  }
  assert.notEqual(run.status,'completed','CLI producer is missing from the completed run');assert.ok(Date.now()<deadline,'Timed out waiting for CLI compiler');console.log(`Waiting only for CLI ${ctx.platform}`);await new Promise(done=>setTimeout(done,10000));
 }
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){const command=process.argv[2];assert.ok(['stage','restore','compile-desktop','bundle'].includes(command));const ctx=context();
 if(command==='stage')stage(ctx);else if(command==='restore')await restore(ctx);else{const platformConfig=ctx.platform==='windows'?['--config','src-tauri/tauri.windows.conf.json']:[];
  const args=command==='compile-desktop'?['build','--ci','--no-bundle','--target',ctx.target,...platformConfig,'--config',JSON.stringify(desktopCompileConfig({reuseWeb:process.env.SAG_RELEASE_WEB_READY==='1'})),'--','--locked','--timings']:bundleArguments(ctx.platform,ctx.target);
  const result=spawnSync(process.execPath,[resolve('node_modules/@tauri-apps/cli/tauri.js'),...args],{stdio:'inherit',env:{...process.env,...(command==='compile-desktop'?{SAG_DEFER_RELEASE_RESOURCES:'1'}:{})}});assert.equal(result.status,0,`${command} failed`);
 }
}
