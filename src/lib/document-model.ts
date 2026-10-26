import {chunkAt,splitMarkdown,type Chunk} from './markdown-chunks';
import {splitFrontmatter} from './markdown-source';
import {finishHtml,markdownLinks,markdownTokens,renderMarkdownPiece,type LinkDefinition} from './markdown';
import {blockTexts,citationTerms} from './search-highlights';
import type {Candidate,Document} from './types';

// One parsed Markdown document, shared by the search preview and the citation
// panel. Pieces render on demand and are kept, so reopening the same file, or
// another passage of it, costs nothing.
export type MarkdownModel={
 key:string;
 body:string;
 // Characters of front matter before `content`; offsets below are into `content`.
 offset:number;
 content:string;
 pieces:Chunk[];
 links:LinkDefinition[];
 html:(string|undefined)[];
 bytes?:Uint8Array;
};

const cache=new Map<string,MarkdownModel>();
const cacheSize=6;

export function markdownModel(doc:Pick<Document,'path'|'body'|'hash'>):MarkdownModel{
 const key=`${doc.path}\u0000${doc.hash}`;
 let model=cache.get(key);
 if(model&&model.body===doc.body){cache.delete(key);cache.set(key,model);return model}
 const {prefix,body:content}=splitFrontmatter(doc.body);
 const pieces=splitMarkdown(content);
 model={key,body:doc.body,offset:prefix.length,content,pieces,links:markdownLinks(doc.body),html:new Array(pieces.length)};
 cache.set(key,model);
 while(cache.size>cacheSize)cache.delete(cache.keys().next().value!);
 return model;
}

export function clearMarkdownModels(){cache.clear()}

export function pieceText(model:MarkdownModel,index:number){
 const piece=model.pieces[index];
 return model.content.slice(piece.from,piece.to);
}

export function pieceHtml(model:MarkdownModel,index:number):string{
 return model.html[index]??=finishHtml(renderMarkdownPiece(pieceText(model,index),model.links,{offset:model.pieces[index].from,last:index===model.pieces.length-1}));
}

// HTML of a piece up to `end` (an offset into content), used to count the
// matches that come before a passage, exactly as a whole-prefix render would.
export function piecePrefixHtml(model:MarkdownModel,index:number,end:number):string{
 const piece=model.pieces[index];
 return finishHtml(renderMarkdownPiece(model.content.slice(piece.from,Math.max(piece.from,end)),model.links,{offset:piece.from,last:true}));
}

export function pieceIndex(model:MarkdownModel,offset:number){return chunkAt(model.pieces,offset)}

// Character offset in content for a UTF-8 byte offset in the body.
export function contentOffset(model:MarkdownModel,byte:number){
 model.bytes??=new TextEncoder().encode(model.body);
 return new TextDecoder().decode(model.bytes.subarray(0,byte)).length-model.offset;
}

// Start of a 1-based line as a content offset; lines past the end map to the end.
function lineOffset(model:MarkdownModel,line:number){
 let index=0;
 for(let current=1;current<line;current++){
  const next=model.body.indexOf('\n',index);
  if(next<0)return model.body.length-model.offset;
  index=next+1;
 }
 return Math.max(0,index-model.offset);
}

// The passages a candidate points at, as content offsets.
export function candidateRanges(model:MarkdownModel,candidate:Candidate):{from:number;to:number}[]{
 if(candidate.source_spans?.length)return candidate.source_spans.map(span=>({from:contentOffset(model,span.start_byte),to:contentOffset(model,span.end_byte)}));
 return [{from:lineOffset(model,candidate.start_line),to:lineOffset(model,candidate.end_line+1)}];
}

// Text of the cited blocks, as citationTerms computes it, but lexing only the
// pieces the ranges touch instead of passing over the whole body again.
export async function modelCitationTerms(model:MarkdownModel,candidate:Candidate,ranges:{from:number;to:number}[]):Promise<string[]>{
 // Token offsets are counted on text with \r\n folded, so leave such files to the general path.
 if(model.content.includes('\r'))return citationTerms(candidate,model.body);
 const kept:string[]=[],from=Math.min(...ranges.map(range=>range.from)),to=Math.max(...ranges.map(range=>range.to));
 for(let index=pieceIndex(model,from);index<model.pieces.length&&model.pieces[index].from<Math.max(to,from+1);index++){
  let offset=model.pieces[index].from;
  for(const token of markdownTokens(pieceText(model,index))){
   const start=offset;offset+=token.raw.length;
   if(token.type!=='def'&&ranges.some(range=>start<range.to&&offset>range.from))kept.push(token.raw);
  }
 }
 const links=model.links.map(link=>({...link,offset:Infinity}));
 return blockTexts(finishHtml(renderMarkdownPiece(kept.join('\n\n'),links)));
}
