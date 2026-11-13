// Audit-only harness. Does not alter prepare-qpdf.mjs or production payloads.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {cpSync,existsSync,lstatSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {join,relative,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
import {performance} from 'node:perf_hooks';
import {prepareQpdf} from './prepare-qpdf.mjs';
const target=process.env.SAG_AUDIT_TARGET||`${process.arch==='arm64'?'aarch64':'x86_64'}-${process.platform==='win32'?'pc-windows-msvc':process.platform==='darwin'?'apple-darwin':'unknown-linux-gnu'}`;
const output=resolve(process.env.SAG_AUDIT_OUTPUT||'qa/qpdf-resource-audit');
mkdirSync(output,{recursive:true});
const workspace=mkdtempSync(join(tmpdir(),'qpdf-resource-audit-'));
const baseline=join(workspace,'baseline/qpdf'),candidate=join(workspace,'relocated/qpdf');
const hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
const list=dir=>readdirSync(dir,{withFileTypes:true}).flatMap(entry=>{
 const path=join(dir,entry.name);assert.ok(!lstatSync(path).isSymbolicLink(),'payload must not depend on build-cache symlinks');
 return entry.isDirectory()?list(path):[path];
});
function makeFixture(path,pages){
 const objects=[];const add=value=>(objects.push(value),objects.length);
 add('<< /Type /Catalog /Pages 2 0 R >>');add('');
 const font=add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
 const image=add('<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length 7 >>\nstream\nFF0000>\nendstream');
 const kids=[];
 for(let page=1;page<=pages;page++){
  const body=`BT /F1 12 Tf 10 70 Td (Audit page ${page}) Tj ET q 20 0 0 20 30 30 cm /Im1 Do Q\n`;
  const content=add(`<< /Length ${Buffer.byteLength(body)} >>\nstream\n${body}endstream`);
  kids.push(add(`<< /Type /Page /Parent 2 0 R /Contents ${content} 0 R /Rotate ${page%10===0?90:0} >>`));
 }
 objects[1]=`<< /Type /Pages /Kids [${kids.map(n=>n+' 0 R').join(' ')}] /Count ${pages} /MediaBox [0 0 100 100] /Resources << /Font << /F1 ${font} 0 R >> /XObject << /Im1 ${image} 0 R >> >> >>`;
 let pdf='%PDF-1.4\n';const offsets=[];
 objects.forEach((body,index)=>{offsets.push(Buffer.byteLength(pdf));pdf+=`${index+1} 0 obj\n${body}\nendobj\n`;});
 const xref=Buffer.byteLength(pdf);pdf+=`xref\n0 ${objects.length+1}\n0000000000 65535 f \n${offsets.map(n=>String(n).padStart(10,'0')+' 00000 n ').join('\n')}\ntrailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
 writeFileSync(path,pdf);
}
const runtimeEnv={...process.env,LD_LIBRARY_PATH:'',DYLD_LIBRARY_PATH:'',DYLD_FALLBACK_LIBRARY_PATH:''};
if(process.platform==='win32')runtimeEnv.PATH=`${process.env.SystemRoot}\\System32;${process.env.SystemRoot}`;
function run(home,args){
 const result=spawnSync(join(home,'bin',process.platform==='win32'?'qpdf.exe':'qpdf'),args,{cwd:workspace,env:runtimeEnv,encoding:'utf8',timeout:120000});
 assert.equal(result.status,0,result.stderr||result.error?.message);return result.stdout.trim();
}
const results={target,removed:[],files:[],cases:[]};
try{
 await prepareQpdf(baseline,target);cpSync(baseline,candidate,{recursive:true});
 const keep=new Set(['share/doc/qpdf/manual-html/license.html','share/doc/qpdf/manual-html/_sources/license.rst.txt']);
 if(target.includes('windows'))for(const file of list(candidate)){
  const name=relative(candidate,file).replaceAll('\\','/');
  if(name.startsWith('share/')&&!keep.has(name)){results.removed.push({path:name,bytes:lstatSync(file).size});rmSync(file);}
 }
 for(const file of list(candidate)){
  const name=relative(candidate,file);assert.equal(hash(file),hash(join(baseline,name)));
  results.files.push({path:name.replaceAll('\\','/'),bytes:lstatSync(file).size,sha256:hash(file)});
 }
 results.before_bytes=list(baseline).reduce((n,p)=>n+lstatSync(p).size,0);
 results.after_bytes=results.files.reduce((n,p)=>n+p.bytes,0);
 assert.match(run(candidate,['--version']),/qpdf version 12\.4\.2/);
 const fixture=join(workspace,'1000-pages.pdf');makeFixture(fixture,1000);
 const sources=[{name:'generated-1000-pages',path:fixture,pages:1000}];
 if(process.env.SAG_AUDIT_PDF){const path=resolve(process.env.SAG_AUDIT_PDF);sources.push({name:'local-large-pdf',path,pages:Number(run(baseline,['--show-npages',path]))});}
 for(const source of sources){
  const original=hash(source.path);
  for(const chunk of [50,200]){
   let expected;
   for(let trial=0;trial<3;trial++)for(const [label,home] of trial%2===0?[['baseline',baseline],['candidate',candidate]]:[['candidate',candidate],['baseline',baseline]]){
    const dir=join(workspace,`${source.name}-${chunk}-${trial}-${label}`);mkdirSync(dir);
    const files=[];const started=performance.now();
    for(let start=1;start<=source.pages;start+=chunk){
     const end=Math.min(start+chunk-1,source.pages),file=join(dir,`pages-${start}-${end}.pdf`);
     run(home,['--warning-exit-0','--deterministic-id','--empty','--pages',source.path,`${start}-${end}`,'--',file]);files.push({file,start,end});
    }
    const elapsed_ms=Math.round(performance.now()-started);assert.ok(elapsed_ms<120000);
    const digests=files.map(({file,start,end})=>{
     assert.equal(Number(run(home,['--show-npages',file])),end-start+1);run(home,['--check',file]);
     return {start,end,bytes:lstatSync(file).size,sha256:hash(file)};
    });
    if(!expected)expected=digests;else assert.deepEqual(digests,expected,'every output PDF must be byte-identical');
    results.cases.push({source:source.name,pages:source.pages,chunk,trial,label,elapsed_ms,output_bytes:digests.reduce((n,p)=>n+p.bytes,0),parts:digests});rmSync(dir,{recursive:true});
   }
  }
  assert.equal(hash(source.path),original,'original source must not change');
 }
 const encrypted=join(workspace,'encrypted.pdf'),decrypted=join(workspace,'decrypted.pdf');
 run(candidate,[fixture,'--encrypt','audit','owner','256','--',encrypted]);
 run(candidate,['--password=audit','--decrypt',encrypted,decrypted]);run(candidate,['--check',decrypted]);assert.equal(Number(run(candidate,['--show-npages',decrypted])),1000);
 // Repeat after the original runtime has been removed: no hidden DLL/library dependency.
 rmSync(baseline,{recursive:true});assert.match(run(candidate,['--version']),/qpdf version 12\.4\.2/);run(candidate,['--check',decrypted]);
 results.passed=true;writeFileSync(join(output,'report.json'),JSON.stringify(results,null,2)+'\n');
 console.log(JSON.stringify({target,passed:true,before_bytes:results.before_bytes,after_bytes:results.after_bytes,cases:results.cases.map(({source,pages,chunk,trial,label,elapsed_ms})=>({source,pages,chunk,trial,label,elapsed_ms}))},null,2));
}finally{rmSync(workspace,{recursive:true,force:true});}
