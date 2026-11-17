import test from 'node:test';
import assert from 'node:assert/strict';
import {nativeTestKey, verifiedNativeBaseline} from './release-test-plan.mjs';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,chmodSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {gitAt} from './automatic-release.mjs';

const workflow = `env:
  CARGO_INCREMENTAL: '0'
  RUSTFLAGS: >-
    -C debuginfo=1
jobs:
  native:
    runs-on: \${{ matrix.runner }}
    strategy:
      matrix:
        include:
          - kind: test
            runner: macos-15-intel
            target: x86_64-apple-darwin
    steps:
      - uses: dtolnay/rust-toolchain@pinned
        with:
          toolchain: '1.98.1'
      - name: Run native tests with locked dependencies
        env:
          CARGO_PROFILE_TEST_DEBUG: '1'
        run: cargo test --locked --manifest-path core/Cargo.toml
`;
function fixture() {
  const files = {'core/src/docling.rs':'Intel and ARM branches', 'core/src/docling_tests.rs':'architecture tests', 'core/Cargo.toml':'[package]\nname = "sag-core"\nversion = "0.1.0"\n', 'core/Cargo.lock':'[[package]]\nname = "third-party"\nversion = "1.0"\n', '.github/workflows/desktop-release.yml':workflow, 'src/App.svelte':'UI', 'CHANGELOG.md':'notes'};
  const git = (...args) => args[0] === 'ls-tree' ? Object.entries(files).map(([path,content])=>`100644 blob ${createHash('sha1').update(content).digest('hex')}\t${path}\0`).join('') : files[args[1].slice(args[1].indexOf(':') + 1)];
  return {files, git};
}
test('native-test reuse ignores UI and release version changes but retains native code, tests, dependencies and profile inputs', () => {
  const {files, git} = fixture(), before = nativeTestKey(git, 'source');
  files['src/App.svelte'] = 'new UI'; files['CHANGELOG.md'] = 'new notes';
  files['core/Cargo.toml'] = files['core/Cargo.toml'].replace('0.1.0','0.1.1');
  // Only CLI/desktop versions are changed by automatic release preparation.
  assert.notEqual(nativeTestKey(git,'source'),before);
  files['core/Cargo.toml'] = files['core/Cargo.toml'].replace('0.1.1','0.1.0');
  assert.equal(nativeTestKey(git, 'source'), before);
  for (const path of ['core/src/docling.rs','core/src/docling_tests.rs','core/Cargo.lock','.github/workflows/desktop-release.yml']) {
    const original = files[path];
    files[path] = path.endsWith('.yml') ? original.replace('1.98.1','1.98.2') : original + '\nchanged';
    assert.notEqual(nativeTestKey(git, 'source'), before, path); files[path] = original;
  }
  files['.cargo/config.toml'] = '[build]\nrustflags=["-Ctarget-cpu=native"]';
  assert.notEqual(nativeTestKey(git, 'source'), before);
});
test('installer scripts, Windows test manifests and multiline Rust flags invalidate reuse', () => {
  const {files,git}=fixture(), before=nativeTestKey(git,'source');
  for(const path of ['scripts/install.sh','scripts/windows-test.manifest.xml']) {files[path]='new input';assert.notEqual(nativeTestKey(git,'source'),before);delete files[path];}
  files['.github/workflows/desktop-release.yml']=workflow.replace('-C debuginfo=1','-C debuginfo=2');
  assert.notEqual(nativeTestKey(git,'source'),before);
});
test('JSON configuration embedded by Rust invalidates native test reuse',()=>{
 const {files,git}=fixture();
 const paths=['src/lib/model-capabilities.json','src/lib/sag-tools.json'];
 for(const path of paths) files[path]='{"version":1}';
 const before=nativeTestKey(git,'source');
 for(const path of paths){files[path]='{"version":2}';assert.notEqual(nativeTestKey(git,'source'),before,path);files[path]='{"version":1}';}
});
test('only reused results may use a lightweight runner; full tests retain the native mapping',()=>{
 const {files,git}=fixture(), before=nativeTestKey(git,'source');
 files['.github/workflows/desktop-release.yml']=workflow.replace('${{ matrix.runner }}',"${{ needs.validate.outputs.native_tests == 'reuse' && 'ubuntu-24.04' || matrix.runner }}");
 assert.equal(nativeTestKey(git,'source'),before);
 files['.github/workflows/desktop-release.yml']=workflow.replace('${{ matrix.runner }}','ubuntu-24.04');
 assert.throws(()=>nativeTestKey(git,'source'),/runner mapping/);
});
test('native test recipe retains package coverage and selective test filters',()=>{
 const {files,git}=fixture(), before=nativeTestKey(git,'source');
 for(const command of ['cargo test --locked --manifest-path client/Cargo.toml','cargo test --locked --manifest-path core/Cargo.toml --lib','cargo test --locked --manifest-path core/Cargo.toml -- --exact one_test']) {
  files['.github/workflows/desktop-release.yml']=workflow.replace('cargo test --locked --manifest-path core/Cargo.toml',command);
  assert.notEqual(nativeTestKey(git,'source'),before,command);
 }
});
test('real Git native keys retain non-ASCII paths, newlines, binary fixtures and executable modes', t => {
  const directory=mkdtempSync(join(tmpdir(),'sag-native-key-'));t.after(()=>rmSync(directory,{recursive:true,force:true}));
  const git=gitAt(directory);git('init','--quiet');git('config','user.email','test@example.invalid');git('config','user.name','Test');git('config','core.filemode','true');
  const save=(path,value)=>{const target=join(directory,path);mkdirSync(dirname(target),{recursive:true});writeFileSync(target,value);git('add','--',path);return git('write-tree');};
  save('core/Cargo.toml','[package]\nname = "sag-core"\nversion = "0.1.0"\n');save('.github/workflows/desktop-release.yml',workflow);
  for(const path of ['core/tests/中文.fixture','core/tests/new\nline.fixture','core/tests/binary.fixture']) {
    const before=save(path,Buffer.from([0xff,0x00,0x81]));const after=save(path,Buffer.from([0xfe,0x00,0x81]));
    assert.notEqual(nativeTestKey(git,before),nativeTestKey(git,after),path);
  }
  if(process.platform!=='win32') {const before=save('scripts/install.sh','#!/bin/sh\n');chmodSync(join(directory,'scripts/install.sh'),0o755);git('add','--','scripts/install.sh');assert.notEqual(nativeTestKey(git,before),nativeTestKey(git,git('write-tree')));}
});
test('a native baseline needs a completed successful trusted tag and all three latest native jobs', () => {
  const commit = 'a'.repeat(40), context = {commit,workflowId:4,currentRunId:20};
  const run = {id:10,head_sha:commit,head_branch:'v0.3.9',event:'workflow_dispatch',workflow_id:4,path:'.github/workflows/desktop-release.yml',repository:{full_name:'Zleap-AI/SAG-Wiki'},head_repository:{full_name:'Zleap-AI/SAG-Wiki'},status:'completed',conclusion:'success'};
  const jobs = ['Validate release source','Test Rust macos','Test Rust macos-intel','Test Rust windows'].map((name,id)=>({name,id:id+1,run_attempt:1,status:'completed',conclusion:'success'}));
  assert.equal(verifiedNativeBaseline(run,jobs,context), true);
  for (const patch of [{conclusion:'failure'},{status:'in_progress'},{id:21},{head_branch:'main'},{head_repository:{full_name:'other/repo'}}]) assert.throws(()=>verifiedNativeBaseline({...run,...patch},jobs,context));
  assert.throws(()=>verifiedNativeBaseline(run,jobs.slice(0,-1),context));
  assert.throws(()=>verifiedNativeBaseline(run,[...jobs,{...jobs[2],id:8,run_attempt:2,conclusion:'failure'}],context));
});
