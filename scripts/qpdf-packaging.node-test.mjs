import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,copyFileSync,cpSync,writeFileSync,existsSync,renameSync,rmSync,realpathSync,readdirSync,readFileSync} from 'node:fs';
import {join,resolve,relative,delimiter} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync,execFileSync} from 'node:child_process';
import {prepareQpdf} from './prepare-qpdf.mjs';
const root=resolve('.');
const windows=process.platform==='win32';
const arch=process.arch==='arm64'?'aarch64':'x86_64';
const target=`${arch}-${windows?'pc-windows-msvc':process.platform==='darwin'?'apple-darwin':'unknown-linux-gnu'}`;
const variant=windows?'msvc64':`bin-${process.platform==='darwin'?'macos':'linux'}-${process.platform==='darwin'?(process.arch==='arm64'?'arm64':'x86_64'):arch}`;
const relativeCache=join('node_modules/.cache/qpdf','12.4.2',variant);
async function fixture(t,variantName=variant,targetName=target) {
 const cachePath=join('node_modules/.cache/qpdf','12.4.2',variantName);
 const dir=mkdtempSync(join(tmpdir(),'sag-qpdf-package-'));
 t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const archive=join(root,cachePath,'package.zip');
 if(!existsSync(archive))await prepareQpdf(join(dir,'seed'),targetName);
 mkdirSync(join(dir,cachePath),{recursive:true});
 copyFileSync(archive,join(dir,cachePath,'package.zip'));
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
 writeFileSync(join(dir,'scripts/build-web-assets.mjs'),"export const buildWebAssets = () => 'unused-web-fixture';\n");
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


test('Windows payload retains every runtime and notice byte while omitting manuals',async t=>{
 const dir=await fixture(t,'msvc64','x86_64-pc-windows-msvc'),previous=process.cwd();
 try{
  process.chdir(dir);const destination=join(dir,'delivery/qpdf');
  await prepareQpdf(destination,'x86_64-pc-windows-msvc');
  const source=join(dir,'node_modules/.cache/qpdf/12.4.2/msvc64/extracted/qpdf-12.4.2-msvc64');
  const binaries=['concrt140.dll','msvcp140.dll','msvcp140_1.dll','msvcp140_2.dll','msvcp140_atomic_wait.dll','msvcp140_codecvt_ids.dll','qpdf.exe','qpdf30.dll','vcruntime140.dll','vcruntime140_1.dll'];
  const notices=['share/doc/qpdf/manual-html/license.html','share/doc/qpdf/manual-html/_sources/license.rst.txt'];
  const files=folder=>readdirSync(folder,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?files(join(folder,entry.name)):[relative(destination,join(folder,entry.name)).replaceAll('\\','/')]);
  assert.deepEqual(files(destination).sort(),[...binaries.map(name=>'bin/'+name),...notices,'licenses/LICENSE.txt','licenses/NOTICE.md'].sort());
  for(const name of [...binaries.map(name=>'bin/'+name),...notices])assert.deepEqual(readFileSync(join(destination,name)),readFileSync(join(source,name)),name);
  for(const name of ['LICENSE.txt','NOTICE.md'])assert.deepEqual(readFileSync(join(destination,'licenses',name)),readFileSync(join(dir,'third-party/qpdf',name)));
 }finally{process.chdir(previous);}
});


// Shared resources, inherited page attributes and rotation stress page selection.
function largePdf(pages){
 const objects=['<< /Type /Catalog /Pages 2 0 R >>','',
  '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  '<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length 7 >>\nstream\nFF0000>\nendstream'];
 const kids=[];
 for(let page=1;page<=pages;page++){
  const body=`BT /F1 12 Tf 10 70 Td (Page ${page}) Tj ET q 20 0 0 20 30 30 cm /Im1 Do Q\n`;
  objects.push(`<< /Length ${Buffer.byteLength(body)} >>\nstream\n${body}endstream`);const content=objects.length;
  objects.push(`<< /Type /Page /Parent 2 0 R /Contents ${content} 0 R /Rotate ${page%10===0?90:0} >>`);kids.push(objects.length);
 }
 objects[1]=`<< /Type /Pages /Kids [${kids.map(n=>n+' 0 R').join(' ')}] /Count ${pages} /MediaBox [0 0 100 100] /Resources << /Font << /F1 3 0 R >> /XObject << /Im1 4 0 R >> >> >>`;
 let pdf='%PDF-1.4\n';const offsets=[];
 objects.forEach((body,index)=>{offsets.push(Buffer.byteLength(pdf));pdf+=`${index+1} 0 obj\n${body}\nendobj\n`;});
 const xref=Buffer.byteLength(pdf);pdf+=`xref\n0 ${objects.length+1}\n0000000000 65535 f \n${offsets.map(n=>String(n).padStart(10,'0')+' 00000 n ').join('\n')}\ntrailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
 return pdf;
}
test('1000-page split output matches the upstream runtime after relocation',async t=>{
 const dir=await fixture(t),previous=process.cwd();
 try{
  process.chdir(dir);const destination=join(dir,'delivery/qpdf');await prepareQpdf(destination,target);
  const locate=folder=>{
   if(existsSync(join(folder,'bin',windows?'qpdf.exe':'qpdf')))return folder;
   for(const entry of readdirSync(folder,{withFileTypes:true}))if(entry.isDirectory()){const found=locate(join(folder,entry.name));if(found)return found;}
  };
  const upstream=locate(join(dir,relativeCache,'extracted'));assert.ok(upstream);
  const input=join(dir,'1000-pages.pdf');writeFileSync(input,largePdf(1000));
  const digest=file=>createHash('sha256').update(readFileSync(file)).digest('hex');const original=digest(input);
  const split=binary=>{
   const results=[];
   for(let start=1;start<=1000;start+=50){
    const output=join(dir,'part.pdf');
    execFileSync(binary,['--warning-exit-0','--deterministic-id','--empty','--pages',input,`${start}-${start+49}`,'--',output]);
    assert.equal(execFileSync(binary,['--show-npages',output],{encoding:'utf8'}).trim(),'50');
    execFileSync(binary,['--check',output]);results.push(digest(output));
   }
   return results;
  };
  const expected=split(join(upstream,'bin',windows?'qpdf.exe':'qpdf'));
  renameSync(join(dir,'node_modules'),join(dir,'hidden-build-cache'));
  assert.deepEqual(split(join(destination,'bin',windows?'qpdf.exe':'qpdf')),expected);
  assert.equal(digest(input),original);
 }finally{process.chdir(previous);}
});
