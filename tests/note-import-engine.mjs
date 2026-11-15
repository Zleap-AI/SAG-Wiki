import {JSDOM} from 'jsdom';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import {TextEncoder,TextDecoder} from 'node:util';
import {DatabaseSync} from 'node:sqlite';
import {gzipSync} from 'node:zlib';
import protobuf from 'protobufjs';
const {Root:ProtobufRoot}=protobuf;
import {build} from 'esbuild';
import vm from 'node:vm';
const appleDb=new DatabaseSync(':memory:');
appleDb.exec(await fs.readFile(new URL('./fixtures/note-import/apple.sql',import.meta.url),'utf8'));
const js=await fs.readFile('public/importer/frame.js','utf8');
async function run(source,files,extra={}){
 const dom=new JSDOM('<body></body>',{url:'http://localhost/importer/frame.html',runScripts:'outside-only',pretendToBeVisual:true});
 const w=dom.window;Object.assign(w,{structuredClone,TextEncoder,TextDecoder,Blob,File,ReadableStream,WritableStream,TransformStream,Response,Request,Headers,CompressionStream,DecompressionStream});
 const output=new Map();
 const result=await new Promise((resolve,reject)=>{
  const timeout=setTimeout(()=>reject(new Error('Timeout '+source)),20000);
  w.postMessage=(data)=>{
   if(data.type==='ready'){
    w.dispatchEvent(new w.MessageEvent('message',{source:w,origin:w.location.origin,data:{channel:'sag-note-import',type:'run',source,files,session:1,folders:[2],includeArchived:true,language:'en',...extra}}));
   }else if(data.type==='native'){
    let result=null;if(data.command==='read_note_import_files'){result=extra.existing??[]}else if(data.command==='query_apple_notes'){result=appleDb.prepare(data.args.sql).all(...data.args.params)}else{const {path,data:bytes}=data.args;output.set(path,Buffer.from(bytes,'base64'));}
    w.dispatchEvent(new w.MessageEvent('message',{source:w,origin:w.location.origin,data:{channel:'sag-note-import',type:'native-result',request:data.request,result}}));
   }else if(data.type==='error'){clearTimeout(timeout);reject(new Error(data.message));}
   else if(data.type==='complete'){clearTimeout(timeout);resolve(data);}
  };
  try{w.Function=function(){throw new Error("Dynamic code disabled by CSP")};w.eval(js)}catch(e){clearTimeout(timeout);reject(e)}
 });dom.window.close();return {result,output};
}
const enex=await fs.readFile(new URL('./fixtures/note-import/evernote.enex',import.meta.url));
const evernote=await run('evernote',[new File([enex],'notebook.enex')]);console.log('evernote',evernote.result,[...evernote.output.keys()]);assert(evernote.output.size>0);
const keepNote={title:'Example',textContent:'Keep body',isTrashed:false,isArchived:false,isPinned:false,color:'DEFAULT',createdTimestampUsec:1700000000000000,userEditedTimestampUsec:1700000000000000};
const {zipSync,strToU8}=await import('fflate');
const keepArchive=new File([zipSync({'Takeout/Keep/Example.json':strToU8(JSON.stringify(keepNote)),'Takeout/Keep/Archived.json':strToU8(JSON.stringify({...keepNote,title:'Archived',isArchived:true})),'Takeout/Keep/Deleted.json':strToU8(JSON.stringify({...keepNote,title:'Deleted',isTrashed:true}))})],'keep.zip');
const keep=await run('keep',[keepArchive]);console.log('keep',keep.result,[...keep.output.keys()]);assert([...keep.output.values()].some(value=>value.toString().includes('Keep body')));
assert.equal(keep.result.notes,2);
const activeKeep=await run('keep',[keepArchive],{includeArchived:false});assert.equal(activeKeep.result.notes,1);
assert(![...keep.output.keys()].some(path=>path.includes('Deleted')));
const notion=await run('notion',[new File([await fs.readFile(new URL('./fixtures/note-import/notion.zip',import.meta.url))],'notion.zip')]);console.log('notion',notion.result.notes,notion.result.attachments,notion.result.log.slice(0,3),[...notion.output.keys()].slice(0,5));assert(notion.output.size>0);

const invalidNotion=await run('notion',[new File(['invalid zip'],'broken.zip')]);assert.equal(invalidNotion.result.notes,0);assert(invalidNotion.result.log.some(item=>item.outcome==='failed'));
const schemaModule={module:{exports:{}}};
const schemaBundle=await build({entryPoints:['third-party/obsidian-importer/src/formats/apple-notes/descriptor.ts'],bundle:true,write:false,format:'cjs'});
vm.runInNewContext(schemaBundle.outputFiles[0].text,schemaModule);
const documentType=ProtobufRoot.fromJSON(schemaModule.module.exports.descriptor).lookupType('ciofecaforensics.Document');
const runs=[{text:'Test note\n'},{text:'Apple Notes body\n'},{text:'bold ',fontWeight:1},{text:'continues',fontWeight:1},{text:'\n',paragraphStyle:{styleType:100}},{text:'list item\n',paragraphStyle:{styleType:100}}];
const richDocument=documentType.create({version:1,note:{noteText:runs.map(run=>run.text).join(''),attributeRun:runs.map(({text,...style})=>({length:text.length,...style}))}});
appleDb.prepare('UPDATE zicnotedata SET ZDATA=?').run(gzipSync(documentType.encode(richDocument).finish()));
const apple=await run("apple",[]);console.log("apple",apple.result,[...apple.output.entries()].map(([p,c])=>[p,c.toString()]));assert.equal(apple.result.notes,1,JSON.stringify(apple.result.log));
assert([...apple.output.values()].some(bytes=>bytes.toString().includes("**bold continues**")));

assert(apple.output.get("Notes/Notes/Test note.md").toString().includes("Apple Notes body"));
assert(![...notion.output.entries()].some(([path,value])=>path.endsWith('.md')&&value.toString().includes('[[')));

const repeatedApple=await run('apple',[],{existing:[...apple.output].map(([path,bytes])=>({path,data:bytes.toString('base64')}))});
assert.equal(repeatedApple.output.size,0,'Apple Notes: repeated import must not duplicate existing notes');
appleDb.close();

for(const [name,content] of notion.output){
 if(!name.endsWith('.md'))continue;
 for(const match of content.toString().matchAll(/!?\[[^\]]*\]\(([^)]+)\)/g)){
  const target=decodeURIComponent(match[1].split('#')[0]);
  if(!target||/^[a-z]+:/i.test(target))continue;
  const {posix}=await import('node:path');
  assert(notion.output.has(posix.normalize(posix.join(posix.dirname(name),target))),`Broken link in ${name}: ${target}`);
 }
}

for(const [source,files,first] of [['notion',[new File([await fs.readFile(new URL('./fixtures/note-import/notion.zip',import.meta.url))],'notion.zip')],notion],['evernote',[new File([enex],'notebook.enex')],evernote],['keep',[keepArchive],keep]]){
 const repeated=await run(source,files,{existing:[...first.output].map(([path,bytes])=>({path,data:bytes.toString('base64')}))});
 assert.equal(repeated.output.size,0,`${source}: repeated import must not duplicate existing notes`);
}
