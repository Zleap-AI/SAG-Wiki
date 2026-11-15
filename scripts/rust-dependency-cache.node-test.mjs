import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {normalizeCargo, dependencyKey, trustedProducer, verifyMetadata} from './rust-dependency-cache.mjs';
import {hashFile} from './release-build-cache.mjs';

test('cache key ignores only local package versions, retaining dependencies, features, profiles and configuration', () => {
  const manifest = '[package]\nname = "sag-core"\nversion = "0.3.0"\n[dependencies]\nfoo = "1.0"\n[profile.release]\nlto = true\n';
  assert.equal(normalizeCargo('core/Cargo.toml', manifest), normalizeCargo('core/Cargo.toml', manifest.replace('0.3.0','0.3.1')));
  assert.notEqual(normalizeCargo('core/Cargo.toml', manifest), normalizeCargo('core/Cargo.toml', manifest.replace('1.0','2.0')));
  const lock = 'version = 4\n[[package]]\nname = "sag-core"\nversion = "0.3.0"\n[[package]]\nname = "third-party"\nversion = "0.3.0"\n';
  assert.equal(normalizeCargo('Cargo.lock',lock), normalizeCargo('Cargo.lock',lock.replace('version = "0.3.0"','version = "0.3.1"')));
  assert.notEqual(normalizeCargo('Cargo.lock',lock), normalizeCargo('Cargo.lock',lock.replaceAll('0.3.0','0.3.1')));
  const files = {'core/Cargo.toml':manifest, '.cargo/config.toml':'[build]\njobs=2'};
  const git = (...args) => args[0] === 'ls-tree' ? Object.keys(files).join('\n') : files[args[1].split(':')[1]];
  const key = dependencyKey(git, 'abc', {target:'a', profile:'debug1'});
  assert.notEqual(key,dependencyKey(git,'abc',{target:'b',profile:'debug1'}));
  files['.cargo/config.toml'] += '\nrustflags=["-Copt-level=2"]';
  assert.notEqual(key,dependencyKey(git,'abc',{target:'a',profile:'debug1'}));
});

test('only an older own-repository tag with the latest successful validated job can supply dependencies', () => {
  const run = {id:10, head_sha:'a'.repeat(40), head_branch:'v0.3.0', event:'workflow_dispatch', path:'.github/workflows/desktop-release.yml', workflow_id:4, repository:{full_name:'Zleap-AI/SAG-Wiki'}, head_repository:{full_name:'Zleap-AI/SAG-Wiki'}, status:'completed', conclusion:'failure'};
  const jobs = ['Validate release source','Test Rust macos'].map((name,i)=>({id:i+1,name,run_attempt:1,status:'completed',conclusion:'success'}));
  const ctx = {currentRunId:20,workflowId:4,kind:'test',platform:'macos'};
  assert.equal(trustedProducer(run,jobs,ctx).id,2); // Unrelated publish failure is harmless.
  for (const override of [{head_branch:'feature/cache'},{head_branch:'main'},{event:'pull_request'},{id:21},{path:'.github/workflows/other.yml'},{head_repository:{full_name:'attacker/fork'}}]) {
    assert.throws(()=>trustedProducer({...run,...override},jobs,ctx));
  }
  assert.throws(()=>trustedProducer(run,[...jobs,{...jobs[1],id:3,run_attempt:2,conclusion:'failure'}],ctx));
  assert.throws(()=>trustedProducer(run,[{...jobs[0],conclusion:'failure'},jobs[1]],ctx));
});

test('metadata and payload digest bind the producer, attempt and compatibility key', () => {
  const expected = {schema:1,run:10,job:2,attempt:1,key:'key'};
  const bytes = Buffer.from('payload');
  const metadata = {...expected,payload_sha256:createHash('sha256').update(bytes).digest('hex')};
  verifyMetadata(metadata,expected,metadata.payload_sha256);
  assert.throws(()=>verifyMetadata({...metadata,attempt:2},expected,metadata.payload_sha256));
  assert.throws(()=>verifyMetadata(metadata,expected,createHash('sha256').update('tampered').digest('hex')));
});

test('dependency archive round trip excludes app/test outputs and rejects traversal and symlinks before writing', () => {
  const directory = mkdtempSync(join(tmpdir(),'sag-dependency-test-'));
  const python = process.env.PYTHON ?? (process.platform === 'win32' ? 'python' : 'python3');
  const script = resolve('scripts/rust-dependency-archive.py');
  try {
    const source = join(directory,'source'); const restored = join(directory,'restored'); const archive = join(directory,'cache.tar.gz');
    for (const file of ['debug/deps/libthird_party.rlib','debug/.fingerprint/third-party-123/lib-third','debug/build/third-party-123/build-script-build','debug/deps/libsag_core.rlib','debug/deps/test-executable','debug/.fingerprint/sag-core-123/lib-sag','debug/sag']) {
      mkdirSync(join(source,file,'..'),{recursive:true});writeFileSync(join(source,file),file);
    }
    const call = (command,root,file=archive) => spawnSync(python,[script,command,root,file,'test','aarch64-apple-darwin'],{encoding:'utf8'});
    const packed = call('pack',source);assert.equal(packed.status,0,packed.stderr);
    assert.equal(hashFile(archive),createHash('sha256').update(readFileSync(archive)).digest('hex'));
    const result = call('restore',restored);assert.equal(result.status,0,result.stderr);
    assert.equal(readFileSync(join(restored,'debug/deps/libthird_party.rlib'),'utf8'),'debug/deps/libthird_party.rlib');
    for (const file of ['debug/deps/libsag_core.rlib','debug/deps/test-executable','debug/.fingerprint/sag-core-123/lib-sag','debug/sag']) assert.equal(existsSync(join(restored,file)),false);
    const interrupted = spawnSync(python,['-c',`import importlib.util,pathlib,sys
spec=importlib.util.spec_from_file_location("archive",sys.argv[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
root=pathlib.Path(sys.argv[2]);old=root/"debug/deps/libthird_party.rlib"
def fail_copy(source,destination):
 pathlib.Path(destination).write_bytes(b"partial")
 raise OSError("injected disk failure")
m.shutil.copy2=fail_copy
try:m.transfer("restore",str(root),sys.argv[3],"test","aarch64-apple-darwin")
except OSError:pass
else:raise AssertionError("copy must fail")
assert old.read_text()=="debug/deps/libthird_party.rlib"
assert not list(root.rglob(".sag-cache-*"))`,script,restored,archive],{encoding:'utf8'});
    assert.equal(interrupted.status,0,interrupted.stderr);
    for (const [name,type] of [['../escape', 'file'],['debug/deps/libsag_core.rlib','file'],['debug/deps/libevil.rlib','symlink']]) {
      const evil = join(directory,'evil.tar.gz');
      const made = spawnSync(python,['-c','import tarfile,sys,io\nwith tarfile.open(sys.argv[1],"w:gz") as t:\n i=tarfile.TarInfo(sys.argv[2]);i.size=1\n if sys.argv[3]=="symlink":i.type=tarfile.SYMTYPE;i.linkname="../../escape";i.size=0\n t.addfile(i,io.BytesIO(b"x"))',evil,name,type],{encoding:'utf8'});
      assert.equal(made.status,0,made.stderr);
      const fresh = join(directory,`rejected-${type}-${name.length}`);
      assert.notEqual(call('restore',fresh,evil).status,0);
      assert.equal(existsSync(fresh),false);
    }
  } finally {rmSync(directory,{recursive:true,force:true});}
});

test('workflow cache changes retain all native tests, protected signing and final shared gate', () => {
  const workflow = readFileSync(new URL('../.github/workflows/desktop-release.yml',import.meta.url),'utf8');
  for (const command of ['scripts/test-windows-desktop.ps1','--manifest-path core/Cargo.toml','scripts/release-build-cache.mjs wait all','scripts/release.mjs assemble']) assert.ok(workflow.includes(command),command);
  assert.match(workflow,/environment:\s+name: release/);
  assert.match(workflow,/Pack verified Rust dependency outputs[\s\S]*continue-on-error: true/);
  assert.match(workflow,/scripts\/rust-dependency-cache.mjs restore/);
  assert.match(workflow,/scripts\/rust-dependency-cache.mjs pack/);
});
