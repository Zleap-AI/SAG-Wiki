// Real-layout regression: generated local fixture, delayed image and width reflow.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('node:fs');
const path=require('node:path'),os=require('node:os'),zlib=require('node:zlib');
function crc32(bytes){let crc=0xffffffff;for(const byte of bytes){crc^=byte;for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}return (crc^0xffffffff)>>>0;}
function pngChunk(type,data){const tag=Buffer.from(type),length=Buffer.alloc(4),crc=Buffer.alloc(4);length.writeUInt32BE(data.length);crc.writeUInt32BE(crc32(Buffer.concat([tag,data])));return Buffer.concat([length,tag,data,crc]);}
function fixture(folder){
 const header=Buffer.alloc(13);header.writeUInt32BE(800,0);header.writeUInt32BE(600,4);header[8]=8;header[9]=2;
 const row=Buffer.alloc(2401);for(let i=1;i<row.length;i+=3){row[i]=204;row[i+1]=221;row[i+2]=238;}
 const image=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),pngChunk('IHDR',header),pngChunk('IDAT',zlib.deflateSync(Buffer.concat(Array(600).fill(row)))),pngChunk('IEND',Buffer.alloc(0))]);
 fs.writeFileSync(path.join(folder,'figure.png'),image);
 const paragraph=Array.from({length:540},(_,i)=>'word'+String(i).padStart(4,'0')).join(' ');
 const body='# Delayed image and reflow\n\n![Test figure](figure.png)\n\n## Reading anchor\n\n'+paragraph+'\n\n'+Array.from({length:100},(_,i)=>'## Section '+i+'\n\n'+'Synthetic handbook text for reading and navigation. '.repeat(30)+'\n\n').join('');
 const file=path.join(folder,'book.md');fs.writeFileSync(file,body);return file;
}
(async()=>{
 const folder=fs.mkdtempSync(path.join(os.tmpdir(),'sag-reader-layout-'));
 const previous=process.env.SAG_DOCUMENT_FIXTURE;
 let browser,server;
 try{
 process.env.SAG_DOCUMENT_FIXTURE=fixture(folder);
 const {preview}=await import('vite');
 server=await preview({configFile:path.resolve(__dirname,'document.config.ts'),preview:{host:'127.0.0.1',port:0,strictPort:true}});
 const port=server.httpServer.address().port;
 browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1280,height:850},deviceScaleFactor:2});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.route('**/__document-image/*',async route=>{await new Promise(r=>setTimeout(r,1800));await route.continue();});
 await page.goto('http://127.0.0.1:'+port+'/bench/document.html');
 await page.waitForFunction(()=>window.__SAG_DOCUMENT_BENCHMARK__?.ready);
 await page.evaluate(()=>{window.__anchorRunPromise=window.__SAG_DOCUMENT_BENCHMARK__.run();return true;});
 await page.waitForFunction(()=>Array.from(document.querySelectorAll('.large-document-reader p')).some(p=>p.textContent.startsWith('word0000')));
 const before=await page.evaluate(async()=>{
  const host=document.querySelector('.large-document-reader .markdown-reader');
  const p=Array.from(host.querySelectorAll('p')).find(p=>p.textContent.startsWith('word0000'));
  const range=document.createRange();range.setStart(p.firstChild,1500);range.setEnd(p.firstChild,1501);
  window.__anchorRange=range;window.__anchorHost=host;
  host.scrollTop+=range.getBoundingClientRect().top-host.getBoundingClientRect().top-30;
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  return {top:range.getBoundingClientRect().top-host.getBoundingClientRect().top,scrollTop:host.scrollTop,width:host.clientWidth};
 });
 await page.waitForFunction(()=>Array.from(document.querySelectorAll('.large-document-reader img')).some(i=>i.complete&&i.naturalHeight===600));
 const image=await page.evaluate(async()=>{await window.__anchorRunPromise;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));return {top:window.__anchorRange.getBoundingClientRect().top-window.__anchorHost.getBoundingClientRect().top,scrollTop:window.__anchorHost.scrollTop};});
 const resize=await page.evaluate(async()=>{
  const range=window.__anchorRange,host=window.__anchorHost;
  const beforeTop=range.getBoundingClientRect().top-host.getBoundingClientRect().top;
  // Observe the character actually sampled by the reader's public browser caret
  // API before reflow. A different word farther along a line can legitimately
  // wrap; preserving that word's old y-position is not the viewport contract.
  const sampled=[];
  const restore=[];
  for(const name of ['caretPositionFromPoint','caretRangeFromPoint']){
   const original=document[name];
   if(typeof original!=='function')continue;
   document[name]=function(...args){
    const value=original.apply(this,args);
    const node=value?.offsetNode??value?.startContainer;
    const offset=value?.offset??value?.startOffset;
    if(node?.nodeType===Node.TEXT_NODE&&host.contains(node)&&node.textContent.length){
     const observed=document.createRange(),start=Math.min(offset,node.textContent.length-1);
     observed.setStart(node,start);observed.setEnd(node,start+1);
     const box=observed.getBoundingClientRect(),viewport=host.getBoundingClientRect();
     if(box.height>0&&box.bottom>=viewport.top&&box.top<viewport.bottom)
      sampled.push({range:observed,top:box.top-viewport.top,width:host.clientWidth});
    }
    return value;
   };
   restore.push(()=>{document[name]=original;});
  }
  // Trigger the reader's pre-reflow observation while geometry is unchanged.
  host.dispatchEvent(new Event('scroll'));
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const chosen=sampled.at(-1);
  document.querySelector('.large-document-reader').style.maxWidth='600px';
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  const topAfter3=chosen?.range.getBoundingClientRect().top-host.getBoundingClientRect().top;
  for(let i=0;i<10;i++)await new Promise(r=>requestAnimationFrame(r));
  const topAfter10=chosen?.range.getBoundingClientRect().top-host.getBoundingClientRect().top;
  restore.forEach(fn=>fn());
  return {beforeTop,top:range.getBoundingClientRect().top-host.getBoundingClientRect().top,width:host.clientWidth,scrollTop:host.scrollTop,connected:range.startContainer.isConnected,
   anchor:chosen?{beforeTop:chosen.top,beforeWidth:chosen.width,topAfter3,topAfter10,shiftAfter3:topAfter3-chosen.top,shiftAfter10:topAfter10-chosen.top,connected:chosen.range.startContainer.isConnected}:null};
 });
 const result={before,image,resize,imageShift:image.top-before.top,resizeShift:resize.top-resize.beforeTop,errors};
 result.pass=Math.abs(result.imageShift)<=2&&resize.anchor!==null&&Math.abs(resize.anchor.shiftAfter3)<=2&&Math.abs(resize.anchor.shiftAfter10)<=2&&resize.anchor.connected&&resize.connected&&errors.length===0;
 if(process.argv[2])fs.writeFileSync(process.argv[2],JSON.stringify(result,null,2));
 console.log(JSON.stringify(result));if(!result.pass)process.exitCode=1;
 }finally{
  await browser?.close();
  if(server)await new Promise(resolve=>server.httpServer.close(resolve));
  if(previous===undefined)delete process.env.SAG_DOCUMENT_FIXTURE;else process.env.SAG_DOCUMENT_FIXTURE=previous;
  fs.rmSync(folder,{recursive:true,force:true});
 }
})().catch(e=>{console.error(e.message);process.exit(1)});
