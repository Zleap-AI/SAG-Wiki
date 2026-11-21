import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,existsSync,chmodSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createCliManifest,verifyCliManifest,installCliPayload,desktopCompileConfig,validateCliProducer,recoverSkippedCli,verifyQpdfVersion} from './parallel-cli.mjs';
const context={commit:'a'.repeat(40),version:'0.3.0',platform:'macos',target:'aarch64-apple-darwin',run:20,job:41,attempt:2};
function fixture(t){const root=mkdtempSync(join(tmpdir(),'sag-cli-transfer-'));t.after(()=>rmSync(root,{recursive:true,force:true}));for(const file of ['source/sag','resources/bin/sag','resources/bin/qpdf/bin/qpdf','resources/bin/qpdf/licenses/LICENSE','resources/bin/pdfium/libpdfium.dylib']){mkdirSync(join(root,file,'..'),{recursive:true});writeFileSync(join(root,file),file);chmodSync(join(root,file),0o755);}return root;}
// These fixtures model POSIX metadata even when the tests run on Windows.
function fixtureManifest(root){const manifest=createCliManifest(root,context);for(const name of ['source/sag','resources/bin/sag','resources/bin/qpdf/bin/qpdf'])manifest.files[name].mode=0o755;return manifest;}
test('CLI payload binds every binary/library to exact source, platform, producing job and attempt',t=>{const root=fixture(t);const m=fixtureManifest(root);assert.equal(Object.keys(m.files).length,5);verifyCliManifest(root,m,context);for(const patch of [{commit:'b'.repeat(40)},{version:'0.3.1'},{target:'x86_64-pc-windows-msvc'},{job:42},{attempt:1}])assert.throws(()=>verifyCliManifest(root,m,{...context,...patch}));writeFileSync(join(root,'source/sag'),'corrupt');assert.throws(()=>verifyCliManifest(root,m,context),/checksum/);});
test('reject extra payload files and unsafe manifest paths before installing anything',t=>{const root=fixture(t);const m=fixtureManifest(root);const destination=join(root,'..','unused-'+Date.now());try{writeFileSync(join(root,'extra'),'untracked');assert.throws(()=>installCliPayload(root,m,context,destination,{}),/files/);assert.equal(existsSync(destination),false);delete m.files['source/sag'];m.files['../escape']={sha256:'0'.repeat(64),mode:0o755};assert.throws(()=>verifyCliManifest(root,m,context),/path/);}finally{rmSync(destination,{recursive:true,force:true});}});
test('install restores both CLI copies and the PDF components',t=>{const root=fixture(t);const m=fixtureManifest(root);const destination=join(root,'..','install-'+Date.now());try{installCliPayload(root,m,context,destination,{});assert.equal(readFileSync(join(destination,'src-tauri/bin/sag'),'utf8'),'resources/bin/sag');assert.equal(readFileSync(join(destination,'src-tauri/target/aarch64-apple-darwin/release/sag'),'utf8'),'source/sag');assert.equal(readFileSync(join(destination,'src-tauri/bin/qpdf/bin/qpdf'),'utf8'),'resources/bin/qpdf/bin/qpdf');assert.equal(readFileSync(join(destination,'src-tauri/bin/pdfium/libpdfium.dylib'),'utf8'),'resources/bin/pdfium/libpdfium.dylib');}finally{rmSync(destination,{recursive:true,force:true});}});
test('desktop compile hook excludes CLI while retaining full runtime/resource configuration',()=>{const patch=desktopCompileConfig();assert.deepEqual(Object.keys(patch),['build']);assert.equal(patch.build.beforeBuildCommand,'npm run build:clipper:desktop && npm run build');assert.ok(!/build:cli(?:\s|$)/.test(patch.build.beforeBuildCommand));});
test('a shared web build skips rebuilding the frontend while retaining platform extension packaging',()=>{
 assert.equal(desktopCompileConfig({reuseWeb:true}).build.beforeBuildCommand,'npm run build:clipper:desktop');
});
test('Mac staging bundles only the app while Windows keeps its installer configuration',async()=>{
 const {bundleArguments}=await import('./parallel-cli.mjs');
 for(const [platform,target] of [['macos','aarch64-apple-darwin'],['macos-intel','x86_64-apple-darwin']]) assert.deepEqual(bundleArguments(platform,target),['bundle','--ci','--target',target,'--bundles','app']);
 assert.deepEqual(bundleArguments('windows','x86_64-pc-windows-msvc'),['bundle','--ci','--target','x86_64-pc-windows-msvc','--config','src-tauri/tauri.windows.conf.json']);
 assert.throws(()=>bundleArguments('windows','aarch64-apple-darwin'));
});
test('current-run CLI producer must be latest successful attempt with source validation',()=>{const run={id:20,head_sha:context.commit,repository:{full_name:'Zleap-AI/SAG-Wiki'},head_repository:{full_name:'Zleap-AI/SAG-Wiki'},workflow_id:4,path:'.github/workflows/desktop-release.yml',event:'workflow_dispatch'};const jobs=[{name:'Validate release source',id:1,run_attempt:1,status:'completed',conclusion:'success'},{name:'Compile CLI macos',id:41,run_attempt:2,status:'completed',conclusion:'success',steps:[{name:'Compile the production CLI with its real embedded web assets',conclusion:'success'}]}];assert.equal(validateCliProducer(run,jobs,{...context,workflowId:4}).id,41);assert.throws(()=>validateCliProducer({...run,id:19},jobs,{...context,workflowId:4}));assert.throws(()=>validateCliProducer(run,[...jobs,{...jobs[1],id:42,run_attempt:3,conclusion:'failure'}],{...context,workflowId:4}));assert.throws(()=>validateCliProducer(run,[{...jobs[0],conclusion:'failure'},jobs[1]],{...context,workflowId:4}));});

test('build identity covers changes in the deferred-resource build helper',t=>{
 const root=mkdtempSync(join(tmpdir(),'sag-build-identity-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
 for(const name of ['protocol','cli','src-tauri'])mkdirSync(join(root,name),{recursive:true});
 writeFileSync(join(root,'cli/Cargo.toml'),'[package]\nname = "sag-cli"\nversion = "0.3.0"\n');
 const helper=join(root,'src-tauri/build-resource-config.rs');writeFileSync(helper,'first');
 const executable=join(root,process.platform==='win32'?'identity.exe':'identity');
 const compiled=spawnSync('rustc',['--edition=2021',resolve('protocol/build.rs'),'-o',executable],{encoding:'utf8',env:{...process.env,CARGO_MANIFEST_DIR:join(root,'protocol')}});
 assert.equal(compiled.status,0,compiled.stderr);
 const identity=()=>{const result=spawnSync(executable,[],{encoding:'utf8',env:{...process.env,TARGET:'aarch64-apple-darwin',PROFILE:'release'}});assert.equal(result.status,0,result.stderr);return result.stdout.match(/cargo:rustc-env=SAG_BUILD_ID=(.+)/)[1];};
 const before=identity();writeFileSync(helper,'second');assert.notEqual(identity(),before);
});

test('resource-copy policy preserves the original release configuration in the real Rust build helper',t=>{
 const root=mkdtempSync(join(tmpdir(),'sag-resource-policy-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
 const binary=join(root,process.platform==='win32'?'policy.exe':'policy');
 const compiled=spawnSync('rustc',['--edition=2021','--test',resolve('src-tauri/build-resource-config.rs'),'-o',binary],{encoding:'utf8'});
 assert.equal(compiled.status,0,compiled.stderr);
 const result=spawnSync(binary,[],{encoding:'utf8'});assert.equal(result.status,0,result.stdout+result.stderr);assert.match(result.stdout,/4 passed/);
});

test('QPDF notices cannot substitute for the required real QPDF executable',t=>{
 const root=fixture(t);const manifest=fixtureManifest(root);
 rmSync(join(root,'resources/bin/qpdf/bin/qpdf'));delete manifest.files['resources/bin/qpdf/bin/qpdf'];
 assert.throws(()=>verifyCliManifest(root,manifest,context),/QPDF executable/);
});

test('POSIX CLI and QPDF binaries must remain executable after transport',t=>{
 const root=fixture(t);for(const name of ['source/sag','resources/bin/sag','resources/bin/qpdf/bin/qpdf']){
  const manifest=fixtureManifest(root);manifest.files[name].mode=0o644;
  assert.throws(()=>verifyCliManifest(root,manifest,context),/executable mode/);
 }
});
test('a stale successful reuse probe recovers inline without bypassing compiler failures',()=>{
 const calls=[];const actions={build:()=>calls.push('build'),verify:()=>calls.push('verify'),record:()=>calls.push('record')};
 const skipped={conclusion:'success',steps:[{name:'Skip compilation when the complete platform build is reusable',conclusion:'success'},{name:'Compile the production CLI with its real embedded web assets',conclusion:'skipped'}]};
 recoverSkippedCli(skipped,actions);assert.deepEqual(calls,['build','verify','record']);
 for(const job of [{...skipped,conclusion:'failure'},{...skipped,conclusion:null},{conclusion:'success',steps:[]},{conclusion:'success',steps:[{name:'Compile the production CLI with its real embedded web assets',conclusion:'success'}]}]){
  calls.length=0;assert.throws(()=>recoverSkippedCli(job,actions));assert.deepEqual(calls,[]);
 }
});

test('QPDF runtime check matches the pinned complete version rather than a prefix',()=>{
 verifyQpdfVersion('qpdf version 12.4.2\r\nRun qpdf --copyright');
 assert.throws(()=>verifyQpdfVersion('qpdf version 12.4.20'));
 assert.throws(()=>verifyQpdfVersion('qpdf version 12.4.2-custom'));
});

test('PR release-tooling routing covers every parallel build input',()=>{
 const workflow=readFileSync(new URL('../.github/workflows/pr-e2e.yml',import.meta.url),'utf8');
 const start=workflow.indexOf('const relevant = paths.some');
 const end=workflow.indexOf('\n          appendFileSync',start);
 assert.ok(start>=0&&end>start,'Release-tooling selector unavailable');
 const relevant=new Function('paths',`${workflow.slice(start,end)}; return relevant;`);
 for(const path of ['src-tauri/build-resource-config.rs','src-tauri/build.rs','protocol/build.rs','scripts/prepare-qpdf.mjs','scripts/prepare-pdfium.mjs'])assert.equal(relevant([path]),true,path);
 assert.equal(relevant(['docs/en/development.md']),false);
});
