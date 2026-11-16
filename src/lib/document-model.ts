import {chunkAt,scanMarkdown,type Chunk} from './markdown-chunks';
import {splitFrontmatter} from './markdown-source';
import {collectMarkdownLinks,finishHtml,markdownTokens,renderMarkdownPiece,type LinkDefinition} from './markdown';
import {blockTexts} from './search-highlights';
import {escaped} from './domain';
import {isLargeMarkdown,richPieceLimit,sourcePieceLimit} from './large-document-policy';
import type {Candidate,Document} from './types';

type Piece=Chunk&{source?:boolean;group?:number};
export type MarkdownModel={
 key:string; body:string; offset:number; content:string; pieces:Piece[]; definitions:Map<string,LinkDefinition>; bytes?:Uint8Array;
};
const modelCache=new Map<string,{model:MarkdownModel;weight:number}>();
const htmlCache=new Map<string,string>();
const modelLimit=16*1024*1024,htmlLimit=4*1024*1024;
let modelBytes=0,htmlBytes=0,serial=0;

function retain(id:string,model:MarkdownModel){
 const previous=modelCache.get(id);
 if(previous){modelBytes-=previous.weight;modelCache.delete(id);}
 const weight=model.body.length*2+(model.bytes?.byteLength??0);
 if(weight>modelLimit){
  if(previous)for(const key of htmlCache.keys())if(key.startsWith(model.key+':')){
   htmlBytes-=htmlCache.get(key)!.length*2;htmlCache.delete(key);
  }
  return;
 }
 modelBytes+=weight;modelCache.set(id,{model,weight});
 while(modelBytes>modelLimit||modelCache.size>6){
  const first=modelCache.keys().next().value!;
  modelBytes-=modelCache.get(first)!.weight;modelCache.delete(first);
 }
}

function sourcePieces(content:string,chunk:Chunk):Piece[]{
 const pieces:Piece[]=[];
 for(let from=chunk.from;from<chunk.to;){
  let to=Math.min(chunk.to,from+sourcePieceLimit);
  if(to<chunk.to&&/[\uD800-\uDBFF]/.test(content[to-1]))to--;
  pieces.push({from,to,source:true,group:chunk.from});from=to;
 }
 return pieces;
}
function* makePieces(content:string,large:boolean):Generator<void,Piece[]>{
 const chunks=yield* scanMarkdown(content,4000,large,large?richPieceLimit:Infinity);
 if(!large)return chunks;
 const pieces:Piece[]=[];
 for(const chunk of chunks){
  const text=content.slice(chunk.from,chunk.to);
  const safe=text.length>richPieceLimit?(yield* scanMarkdown(text,1,true,richPieceLimit)).map(p=>({from:p.from+chunk.from,to:p.to+chunk.from})):[chunk];
  for(const p of safe){
   const raw=content.slice(p.from,p.to);
   // Dense inline constructs can expand to thousands of DOM nodes even below the character ceiling.
   const dense=raw.length<=richPieceLimit&&(raw.match(/<\w|\$|\*\*|\]\(|\x60|\|/g)?.length??0)>1200;
   if(raw.length>richPieceLimit||dense)pieces.push(...sourcePieces(content,p));
   else pieces.push(p);
  }
  yield;
 }
 return pieces;
}
type ModelDocument=Pick<Document,'path'|'body'|'hash'>;
function* createModel(doc:ModelDocument):Generator<void,MarkdownModel>{
 const id=doc.path+'\u0000'+doc.hash,entry=modelCache.get(id);
 if(entry&&entry.model.body===doc.body){modelCache.delete(id);modelCache.set(id,entry);return entry.model;}
 const {prefix,body:content}=splitFrontmatter(doc.body),pieces=yield* makePieces(content,isLargeMarkdown(doc));
 const definitions=new Map<string,LinkDefinition>();
 if(content.includes(']:'))for(const p of pieces){
  for(const link of collectMarkdownLinks(content,[p]))if(!definitions.has(link.tag))definitions.set(link.tag,link);
  yield;
 }
 const model:MarkdownModel={key:id+'\u0000'+(++serial),body:doc.body,offset:prefix.length,content,pieces,definitions};
 retain(id,model);return model;
}
export function markdownModel(doc:ModelDocument):MarkdownModel{
 const creation=createModel(doc);let step=creation.next();
 while(!step.done)step=creation.next();return step.value;
}
export async function prepareMarkdownModel(doc:ModelDocument,current:()=>boolean):Promise<MarkdownModel|null>{
 const creation=createModel(doc);let start=performance.now();
 while(current()){
  const step=creation.next();if(step.done)return step.value;
  if(performance.now()-start>=5){await new Promise(resolve=>setTimeout(resolve,0));start=performance.now();}
 }
 return null;
}
export function documentCacheUsage(){
 return {modelBytes,htmlBytes,models:modelCache.size,htmlPieces:htmlCache.size};
}
export function clearMarkdownModels(){modelCache.clear();htmlCache.clear();modelBytes=htmlBytes=0;}
export function pieceText(model:MarkdownModel,index:number){
 const p=model.pieces[index];return model.content.slice(p.from,p.to);
}
export function pieceHtml(model:MarkdownModel,index:number):string{
 const key=model.key+':'+index,hit=htmlCache.get(key);
 if(hit!==undefined){htmlCache.delete(key);htmlCache.set(key,hit);return hit;}
 const piece=model.pieces[index],text=pieceText(model,index);
 let html='';
 if(!piece.source){
  html=finishHtml(renderMarkdownPiece(text,model.definitions,{offset:piece.from,last:index===model.pieces.length-1}));
  // Formula/diagram output may expand independently of source length. Inspect the sanitized
  // HTML before attaching it or handing it to an offscreen match counter.
  if(model.body.length>=100_000&&(html.length>250_000||(html.match(/<[a-z][^>]*>/gi)?.length??0)>1600)){
   piece.source=true;piece.group=piece.from;
  }
 }
 if(piece.source){
  const parts=sourcePieces(model.content,piece);
  html='<pre class="source-presentation">'+parts.map(p=>'<span class="source-subpiece">'+escaped(model.content.slice(p.from,p.to))+'</span>').join('')+'</pre>';
 }
 const weight=html.length*2;
 if(weight<=htmlLimit&&model.body.length*2+(model.bytes?.byteLength??0)<=modelLimit){
  htmlCache.set(key,html);htmlBytes+=weight;
  while(htmlBytes>htmlLimit||htmlCache.size>8192){
   const first=htmlCache.keys().next().value!;htmlBytes-=htmlCache.get(first)!.length*2;htmlCache.delete(first);
  }
 }
 return html;
}
// A source presentation has exact original text nodes; HTML parsing normalizes CR and CRLF.
export function pieceDocument(model:MarkdownModel,index:number){
 const piece=model.pieces[index];
 const html=piece.source?'':pieceHtml(model,index);
 if(!piece.source)return new DOMParser().parseFromString(html,'text/html');
 const doc=document.implementation.createHTMLDocument(),pre=doc.createElement('pre');
 pre.className='source-presentation';doc.body.append(pre);
 for(const part of sourcePieces(model.content,piece)){
  const span=doc.createElement('span');span.className='source-subpiece';
  span.dataset.from=String(part.from);span.dataset.to=String(part.to);
  span.textContent=model.content.slice(part.from,part.to);pre.append(span);
 }
 return doc;
}
export function piecePrefixHtml(model:MarkdownModel,index:number,end:number):string{
 const p=model.pieces[index],text=model.content.slice(p.from,Math.max(p.from,end));
 return p.source?'<pre>'+escaped(text)+'</pre>':finishHtml(renderMarkdownPiece(text,model.definitions,{offset:p.from,last:true}));
}
export function pieceIndex(model:MarkdownModel,offset:number){return chunkAt(model.pieces,offset);}
export function contentOffset(model:MarkdownModel,byte:number){
 if(!model.bytes){
  model.bytes=new TextEncoder().encode(model.body);
  const id=model.key.slice(0,model.key.lastIndexOf('\u0000'));
  if(modelCache.get(id)?.model===model)retain(id,model);
 }
 return Math.max(0,new TextDecoder().decode(model.bytes.subarray(0,byte)).length-model.offset);
}
function lineOffset(model:MarkdownModel,line:number){
 let index=0;
 for(let current=1;current<line;current++){
  const next=model.body.indexOf('\n',index);
  if(next<0)return model.content.length;
  index=next+1;
 }
 return Math.max(0,index-model.offset);
}
export function candidateRanges(model:MarkdownModel,candidate:Candidate):Chunk[]{
 if(candidate.source_spans?.length)return candidate.source_spans.map(span=>({from:contentOffset(model,span.start_byte),to:contentOffset(model,span.end_byte)}));
 return [{from:lineOffset(model,candidate.start_line),to:lineOffset(model,candidate.end_line+1)}];
}
// Resolve all citation endpoints in one cancellable source scan. An offset inside a
// UTF-8 sequence maps past its replacement character, matching the legacy TextDecoder.
export async function prepareCandidateRanges(model:MarkdownModel,candidate:Candidate,current:()=>boolean):Promise<Chunk[]>{
 const spans=candidate.source_spans;
 if(!spans?.length){
  let line=1,at=0,time=performance.now(),from=candidate.start_line<=1?0:model.content.length;
  while(line<=candidate.end_line&&at<model.body.length){
   if(!current())return [];
   const newline=model.body.indexOf('\n',at);if(newline<0){at=model.body.length;break;}
   at=newline+1;line++;
   if(line===candidate.start_line)from=Math.max(0,at-model.offset);
   if(performance.now()-time>=5){await new Promise(resolve=>setTimeout(resolve,0));time=performance.now();}
  }
  return [{from,to:Math.max(0,at-model.offset)}];
 }
 const offsets=new Map<number,number>();
 const endpoints=[...new Set(spans.flatMap(span=>[span.start_byte,span.end_byte]))].sort((a,b)=>a-b);
 let pos=0,bytes=0,time=performance.now(),next=0;
 while(next<endpoints.length){
  if(!current())return [];
  while(next<endpoints.length&&(endpoints[next]<=bytes||pos===model.body.length)){
   offsets.set(endpoints[next++],Math.max(0,pos-model.offset));
  }
  if(next===endpoints.length)break;
  const end=Math.min(model.body.length,pos+8192);
  while(pos<end&&bytes<endpoints[next]){
   const code=model.body.codePointAt(pos)!,width=code>0xffff?2:1;
   const nextByte=bytes+(code<128?1:code<2048?2:width===2?4:3);
   while(next<endpoints.length&&endpoints[next]>bytes&&endpoints[next]<nextByte)offsets.set(endpoints[next++],Math.max(0,pos+1-model.offset));
   bytes=nextByte;pos+=width;
  }
  if(performance.now()-time>=5){await new Promise(resolve=>setTimeout(resolve,0));time=performance.now();}
 }
 return spans.map(span=>({from:offsets.get(span.start_byte)!,to:offsets.get(span.end_byte)!}));
}
export async function modelCitationTerms(model:MarkdownModel,_candidate:Candidate,ranges:Chunk[],current=()=>true):Promise<string[]>{
 const terms=new Set<string>();
 const touched=new Set<number>();
 for(const r of ranges)for(let i=pieceIndex(model,r.from);i<model.pieces.length&&model.pieces[i].from<Math.max(r.to,r.from+1);i++)touched.add(i);
 let started=performance.now();
 for(const index of touched){
  if(!current())break;
  const p=model.pieces[index],text=pieceText(model,index);
  // Determine dense-output fallback before deriving its visible citation terms.
  if(model.body.length>=100_000&&!p.source)pieceHtml(model,index);
  if(p.source){
   for(const range of ranges){
    const from=Math.max(range.from,p.from),to=Math.min(range.to,p.to);
    if(to>from)terms.add(model.content.slice(from,to));
   }
  }else{
   // Marked folds CRLF. Advance through both strings to preserve original source spans.
   const normalized=text.replace(/\r\n?/g,'\n');let cursor=0,original=0;
   const advance=(end:number)=>{while(cursor<end){if(text[original]==='\r'&&text[original+1]==='\n')original++;original++;cursor++;}};
   const kept:string[]=[];
   for(const token of markdownTokens(normalized)){
    const pos=normalized.indexOf(token.raw,cursor);if(pos<0)continue;
    advance(pos);const from=p.from+original;advance(pos+token.raw.length);
    if(token.type!=='def'&&ranges.some(r=>from<r.to&&p.from+original>r.from))kept.push(token.raw);
   }
   if(kept.length)for(const term of blockTexts(finishHtml(renderMarkdownPiece(kept.join('\n\n'),model.definitions,{prior:false}))))terms.add(term);
  }
  if(performance.now()-started>=6){await new Promise(resolve=>setTimeout(resolve,0));started=performance.now();}
 }
 return [...terms];
}
