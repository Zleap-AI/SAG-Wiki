import '../third-party/obsidian-importer/web/obsidian/runtime';
import {installDomExtensions} from '../third-party/obsidian-importer/web/obsidian/dom-extensions';
import {MemoryVault} from '../third-party/obsidian-importer/web/obsidian/vault';
import {browserApp} from '../third-party/obsidian-importer/web/obsidian/metadata';
import {browserPath} from '../third-party/obsidian-importer/web/node-shims';
import {WebPickedFile,provideNodeModules} from '../third-party/obsidian-importer/src/filesystem';
import {ImportContext} from '../third-party/obsidian-importer/src/import-context';
import {DEFAULT_DATA} from '../third-party/obsidian-importer/src/plugin-data';
import {TFile} from './obsidian';
import {DuplicateHandling} from '../third-party/obsidian-importer/src/format-importer';
import {setLanguage} from '../third-party/obsidian-importer/src/i18n';
import {NotionImporter} from '../third-party/obsidian-importer/src/formats/notion';
import {KeepImporter} from '../third-party/obsidian-importer/src/formats/keep-json';
import {EvernoteEnexImporter} from '../third-party/obsidian-importer/src/formats/evernote-enex';
import {AppleNotesImporter} from '../third-party/obsidian-importer/src/formats/apple-notes';
import Turndown from 'turndown';
import {Buffer} from 'buffer';
import {decompressSync} from 'fflate';
Object.assign(globalThis,{Buffer});
installDomExtensions(window,{turndown:Turndown});
provideNodeModules({path:browserPath,os:{homedir:()=>''},zlib:{unzipSync:decompressSync}} as never);
let sequence=0;
const pending=new Map<number,{resolve:(v:any)=>void;reject:(e:Error)=>void}>();
function send(value:object){parent.postMessage({channel:'sag-note-import',...value},location.origin);}
function native(command:string,args:object):Promise<any>{return new Promise((resolve,reject)=>{const request=++sequence;pending.set(request,{resolve,reject});send({type:'native',request,command,args});});}
function database(session:number){
 const all=async(strings:TemplateStringsArray,...values:any[])=>{
  const params:any[]=[];let sql=strings[0];
  values.forEach((value,i)=>{const list=Array.isArray(value)?value:[value];sql+=list.map(item=>{params.push(item);return '?';}).join(',')+strings[i+1];});
  return native('query_apple_notes',{session,sql,params});
 };
 return {all,get:async(strings:TemplateStringsArray,...values:any[])=>(await all(strings,...values))[0],close(){}};
}
class LocalAppleNotes extends AppleNotesImporter {
 session=0;
 async getNotesDatabase(){return database(this.session) as never;}
 async getAttachmentSource(account:any,path:string){return Buffer.from(await native('read_apple_attachment',{session:this.session,account:account.uuid,path}),'base64') as never;}
}
window.addEventListener('message',async event=>{
 if(event.source!==parent||event.origin!==location.origin||event.data?.channel!=='sag-note-import')return;
 const data=event.data;
 if(data.type==='native-result'){
  const call=pending.get(data.request);if(!call)return;pending.delete(data.request);
  if(data.error)call.reject(new Error(data.error));else call.resolve(data.result);return;
 }
 if(data.type!=='run')return;
 try{
  setLanguage(data.language);
  const vault=new MemoryVault();vault.config.set('useMarkdownLinks',true);vault.config.set('newLinkFormat','relative');
  const existing=new Map<string,string>();
  const attachmentAliases=new Map<string,string>();
  for(const file of await native('read_note_import_files',{session:data.session})){
   const bytes=Buffer.from(file.data,'base64');
   const content=/\.md$/i.test(file.path)?new TextDecoder().decode(bytes):bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength);
   const parts=file.path.split('/');parts.pop();let folder='';
   for(const part of parts){folder=folder?`${folder}/${part}`:part;if(!vault.getAbstractFileByPathInsensitive(folder))await vault.createFolder(folder);}
   if(typeof content==='string')await vault.create(file.path,content);else await vault.createBinary(file.path,content);
   existing.set(file.path,file.data);
  }
  const constructors={notion:NotionImporter,evernote:EvernoteEnexImporter,keep:KeepImporter,apple:LocalAppleNotes};
  const Constructor=constructors[data.source as keyof typeof constructors];
  const app=browserApp(vault);
  app.metadataCache.getFirstLinkpathDest=(linkpath:string,sourcePath:string)=>{
   if(!linkpath)return vault.getAbstractFileByPath(sourcePath);
   const paths=[linkpath,browserPath.join(browserPath.dirname(sourcePath),linkpath)];
   for(const path of paths){for(const candidate of [path,`${path}.md`]){const file=vault.getAbstractFileByPathInsensitive(candidate);if(file instanceof TFile)return file;}}
   return vault.getAllLoadedFiles().find(file=>file instanceof TFile&&(file.name===linkpath||file.name===`${linkpath}.md`))??null;
  };
  app.fileManager.generateMarkdownLink=(file:{path:string},sourcePath:string,subpath='',display='')=>{
   const filePath=attachmentAliases.get(file.path)??file.path;
   const path=filePath===sourcePath&&subpath?'':browserPath.relative(browserPath.dirname(sourcePath),filePath);
   const heading=subpath.startsWith('#')?subpath.slice(1).toLowerCase().replace(/[^\p{L}\p{N}_\s-]/gu,'').trim().replace(/\s+/g,'-'):'';
   const target=path.split('/').map(encodeURIComponent).join('/')+(subpath?'#'+encodeURIComponent(heading):'');
   return `[${display||(subpath?subpath.slice(1):browserPath.basename(file.path,'.md'))}](${target})`;
  };
  const importer=new Constructor(app as never,{sourceEl:null,outputEl:null,optionsEl:null,importerId:data.source,abortController:new AbortController(),plugin:{loadData:async()=>structuredClone(DEFAULT_DATA),saveData:async()=>{},registerAuthCallback:()=>{}}} as never);
  await importer.ready;
  importer.outputLocation='Notes';
  importer.duplicateHandling=DuplicateHandling.Skip;
  importer.files=data.files.map((file:File)=>new WebPickedFile(file));
  if(importer instanceof KeepImporter){importer.importArchived=data.includeArchived;}
  if(importer instanceof LocalAppleNotes){importer.session=data.session;importer.selectedFolders=data.folders;}
  importer.indexImportedNotes();
  class Context extends ImportContext{
   protected onStatus(message:string){send({type:'progress',message});}
   protected onProgress(current:number,total:number){send({type:'progress',current,total});}
  }
  const context=new Context();
  await importer.import(context);
  const originalAttachments=new Map<string,string>();
  for(const [path,encoded] of existing)if(!/\.md$/i.test(path))originalAttachments.set(encoded,path);
  for(const [path,content] of vault.contents){
   if(existing.has(path)||typeof content==='string')continue;
   const original=originalAttachments.get(Buffer.from(content).toString('base64'));
   if(original)attachmentAliases.set(path,original);
  }
  // Apple Notes creates placeholders for cross-note links before conversion.
  // Failed placeholders must not become empty imported documents.
  for(const [path,content] of vault.contents)if(!existing.has(path)&&/\.md$/i.test(path)&&content==='')vault.remove(path);
  await importer.finalizeMarkdownOutput(context);
  for(const path of attachmentAliases.keys())vault.remove(path);
  let written=0;
  for(const [path,content] of vault.contents){
   const bytes=typeof content==='string'?new TextEncoder().encode(content):new Uint8Array(content);
   const encoded=Buffer.from(bytes).toString('base64');
   if(existing.get(path)===encoded)continue;
   await native('write_note_import_file',{session:data.session,path,data:encoded});
   send({type:'progress',current:++written,total:vault.contents.size});
  }
  send({type:'complete',notes:context.notes,attachments:context.attachments,log:context.log.map(item=>({...item,reason:item.reason?String(item.reason):''}))});
 }catch(error){send({type:'error',message:String(error)});}
});
send({type:'ready'});
