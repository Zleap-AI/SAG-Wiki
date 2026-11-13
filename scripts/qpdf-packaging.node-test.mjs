import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,copyFileSync,cpSync,writeFileSync,existsSync,renameSync,rmSync,realpathSync,readdirSync} from 'node:fs';
import {join,resolve,delimiter} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync,execFileSync} from 'node:child_process';
import {prepareQpdf} from './prepare-qpdf.mjs';
const root=resolve('.');
const windows=process.platform==='win32';
const arch=process.arch==='arm64'?'aarch64':'x86_64';
const target=`${arch}-${windows?'pc-windows-msvc':process.platform==='darwin'?'apple-darwin':'unknown-linux-gnu'}`;
const variant=windows?'msvc64':`bin-${process.platform==='darwin'?'macos':'linux'}-${process.platform==='darwin'?(process.arch==='arm64'?'arm64':'x86_64'):arch}`;
const relativeCache=join('node_modules/.cache/qpdf','12.4.2',variant);
async function fixture(t) {
 const dir=mkdtempSync(join(tmpdir(),'sag-qpdf-package-'));
 t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const archive=join(root,relativeCache,'package.zip');
 if(!existsSync(archive))await prepareQpdf(join(dir,'seed'),target);
 mkdirSync(join(dir,relativeCache),{recursive:true});
 copyFileSync(archive,join(dir,relativeCache,'package.zip'));
 mkdirSync(join(dir,'third-party'),{recursive:true});
 cpSync(join(root,'third-party/qpdf'),join(dir,'third-party/qpdf'),{recursive:true});
 return dir;
}
test('QPDF remains executable after its build cache is removed, including an old destination',async t=>{
 const dir=await fixture(t);const previous=process.cwd();
 try {
  process.chdir(dir);
  const destination=join(dir,'delivery/qpdf');
  await prepareQpdf(destination,target);
  assert.ok(!existsSync(join(destination,'bin',windows?'zlib-flate.exe':'zlib-flate')));
  assert.ok(!existsSync(join(destination,'bin','fix-qdf')));
  if(process.platform==='darwin')assert.deepEqual(readdirSync(join(destination,'lib')),['libqpdf.30.dylib']);
  if(process.platform==='linux'){assert.ok(existsSync(join(destination,'lib/libqpdf.so.30')));assert.ok(!existsSync(join(destination,'lib/libqpdf.so.30.4.2')));}
  if(windows){assert.ok(!existsSync(join(destination,'lib')));assert.ok(!existsSync(join(destination,'include')));}
  // Repeated preparation must also repair an older destination layout.
  if(!windows) {
   const {symlinkSync,unlinkSync}=await import('node:fs');
   const link=join(destination,'lib/libqpdf.30.dylib');
   if(process.platform==='darwin') {
    unlinkSync(link);symlinkSync(join(dir,relativeCache,'extracted/lib/libqpdf.30.4.2.dylib'),link);
   }
  }
  await prepareQpdf(destination,target);
  renameSync(join(dir,'node_modules'),join(dir,'hidden-build-cache'));
  const run=spawnSync(join(destination,'bin',windows?'qpdf.exe':'qpdf'),['--version'],{encoding:'utf8'});
  assert.equal(run.status,0,run.stderr||run.error?.message);
  assert.match(run.stdout,/qpdf version 12\.4\.2/);
  // A real PDF round trip exercises the loader, parser, compression and writer.
  const binary=join(destination,'bin',windows?'qpdf.exe':'qpdf');
  const input=join(dir,'empty.pdf');
  const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> /Contents 4 0 R >>','<< /Length 0 >>\nstream\nendstream'];
  let pdf='%PDF-1.4\n';const offsets=[0];
  for(const [index,object] of objects.entries()){offsets.push(Buffer.byteLength(pdf));pdf+=`${index+1} 0 obj\n${object}\nendobj\n`;}
  const xref=Buffer.byteLength(pdf);
  pdf+=`xref\n0 5\n0000000000 65535 f \n${offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n ').join('\n')}\ntrailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  writeFileSync(input,pdf);
  const split=join(dir,'split.pdf');
  execFileSync(binary,['--empty','--pages',input,'1','--',split]);
  assert.equal(execFileSync(binary,['--show-npages',split],{encoding:'utf8'}).trim(),'1');
  const encrypted=join(dir,'encrypted.pdf');
  execFileSync(binary,[input,'--encrypt','test','owner','256','--',encrypted]);
  execFileSync(binary,['--password=test','--check',encrypted]);
  assert.ok(existsSync(join(destination,'licenses/NOTICE.md')));
 } finally {process.chdir(previous);}
});
test('npm tarball carries a relocatable QPDF beside the native CLI',{skip:windows},async t=>{
 const dir=await fixture(t);
 for(const file of ['scripts/package-npm-cli.mjs','scripts/prepare-qpdf.mjs','npm/sag/bin/sag.cjs','npm/sag/package.json','npm/sag/README.md','cli/Cargo.toml']) {
  const destination=join(dir,file);mkdirSync(resolve(destination,'..'),{recursive:true});copyFileSync(join(root,file),destination);
 }
 const fake=join(dir,'fake-bin');mkdirSync(fake);
 writeFileSync(join(fake,'cargo'),'#!/bin/sh\nexit 0\n',{mode:0o755});
 const native=join(dir,'cli/target',target,'release');mkdirSync(native,{recursive:true});
 writeFileSync(join(native,'sag'),'#!/bin/sh\nexec "$(dirname "$0")/qpdf/bin/qpdf" --version\n',{mode:0o755});
 const npm=realpathSync(execFileSync('which',['npm'],{encoding:'utf8'}).trim());
 const run=spawnSync(process.execPath,[join(dir,'scripts/package-npm-cli.mjs')],{cwd:dir,encoding:'utf8',env:{...process.env,SAG_TARGET:target,PATH:fake+delimiter+process.env.PATH,npm_execpath:npm}});
 assert.equal(run.status,0,run.stderr||run.stdout);
 const archive=run.stdout.trim().split('\n').at(-1);
 const customer=join(dir,'customer');mkdirSync(customer);
 execFileSync('tar',['-xzf',archive,'-C',customer]);
 const helper=join(customer,'package/native',`${process.platform}-${process.arch}`,'qpdf/bin/qpdf');
 assert.ok(existsSync(helper),'npm package must contain its required PDF helper');
 renameSync(join(dir,'node_modules'),join(dir,'hidden-build-cache'));
 const launch=spawnSync(process.execPath,[join(customer,'package/bin/sag.cjs'),'--version'],{encoding:'utf8'});
 assert.equal(launch.status,0,launch.stderr);
 assert.match(launch.stdout,/qpdf version 12\.4\.2/);
});
