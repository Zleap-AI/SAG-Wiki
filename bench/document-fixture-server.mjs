import {readFile,realpath,lstat} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {dirname,resolve,relative,sep,extname} from 'node:path';
const mime={'.png':'image/png','.jpg':'image/jpeg','.jpeg':'image/jpeg','.webp':'image/webp','.gif':'image/gif','.bmp':'image/bmp'};
async function safeImage(root,path){
 const suffix=relative(root,path);
 if(!suffix||suffix.startsWith('..'+sep)||suffix==='..'||resolve(root,suffix)!==path)throw new Error('Outside fixture');
 let current=root;for(const part of suffix.split(sep)){current=resolve(current,part);if((await lstat(current)).isSymbolicLink())throw new Error('Symlink');}
 if(await realpath(path)!==path||!mime[extname(path).toLowerCase()])throw new Error('Invalid image');
 return path;
}
// Inline Markdown and quoted HTML images cover converted fixtures; remote/absolute refs stay unavailable.
function imageSources(body){return [...body.matchAll(/!\[[^\]\n]*\]\(\s*(?:<([^>\n]+)>|([^\s)]+))(?:\s+["'][^\n]*?["'])?\s*\)|<img\b[^>]*\bsrc=["']([^"']+)["'][^>]*>/gi)].map(match=>match[1]??match[2]??match[3]);}

/** Available only in this benchmark's loopback Vite server; never part of the app build. */
export function documentFixtureMiddleware(fixturePath){
 let images=[],fixtureRoot='';
 return async(request,response,next)=>{
  const imageId=/^\/__document-image\/(\d+)$/.exec(request.url??'');
  if(request.url!=='/__document-fixture'&&!imageId){
   if(next&&!/^\/__document-(?:fixture|image)/.test(request.url??'')){next();return;}
   response.writeHead(404);response.end();return;
  }
  const send=(status,payload)=>{response.writeHead(status,{'Content-Type':'application/json; charset=utf-8','Cache-Control':'no-store','X-Content-Type-Options':'nosniff'});response.end(JSON.stringify(payload));};
  const address=request.socket.remoteAddress;
  const local=address==='127.0.0.1'||address==='::1'||address==='::ffff:127.0.0.1';
  const origin=request.headers.origin;
  let host='';try{host=new URL(`http://${request.headers.host}`).hostname;}catch{/* Invalid hosts are denied below. */}
  if(!local||!['127.0.0.1','localhost','[::1]'].includes(host)||(origin&&origin!==`http://${request.headers.host}`)){send(403,{error:'Loopback same-origin access required'});return;}
  if(request.method!=='GET'){send(405,{error:'GET required'});return;}
  if(imageId){
   try{
    const image=images[Number(imageId[1])];if(!image)throw new Error('Undeclared image');
    await safeImage(fixtureRoot,image.local);
    const bytes=await readFile(image.local);send(200,{data:`data:${mime[extname(image.local).toLowerCase()]};base64,${bytes.toString('base64')}`,bytes:bytes.length});
   }catch{send(404,{error:'Image unavailable'});}
   return;
  }
  if(!fixturePath){send(503,{error:'Set SAG_DOCUMENT_FIXTURE to a local UTF-8 Markdown file before starting the benchmark server'});return;}
  try{
   const bytes=await readFile(fixturePath),sha256=createHash('sha256').update(bytes).digest('hex');
   const body=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
   fixtureRoot=await realpath(dirname(fixturePath));images=[];
   for(const source of new Set(imageSources(body))){
    try{
     if(/^(?:[a-z][a-z\d+.-]*:|\/|\\)/i.test(source))continue;
     const decoded=decodeURIComponent(source);
     if(decoded.split(/[\\/]/).includes('..'))continue;
     const local=await safeImage(fixtureRoot,resolve(fixtureRoot,decoded));
     const path=decodeURIComponent(new URL(source,'file:///benchmark/').pathname);
     if(!images.some(image=>image.path===path))images.push({path,local});
    }catch{/* Unsupported, missing and unsafe declarations remain unavailable. */}
   }
   send(200,{bytes:bytes.length,sha256,images:images.map(({path},id)=>({path,id})),document:{id:1,path:'/benchmark/fixture.md',name:'fixture.md',body,hash:sha256}});
  }catch{send(503,{error:'Cannot read the configured UTF-8 fixture'});}
 };
}
