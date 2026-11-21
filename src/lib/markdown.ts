import {answerEvents} from './answer-events';
import {mermaidPreview} from './mermaid';
import {get} from 'svelte/store';
import {t} from './i18n';
import type {Candidate,ToolCallRecord,ReasoningBlock} from './types';
import {getHTMLFromFragment,getSchema,type JSONContent} from '@tiptap/core';import {Node as ProseMirrorNode} from '@tiptap/pm/model';import StarterKit from '@tiptap/starter-kit';import {MarkdownManager}from '@tiptap/markdown';import {BoundedTableKit,BoundedTaskList,TaskItem,ReadHtmlAlignment,Subscript,Superscript} from './markdown-extensions';import {splitMarkdown} from './markdown-chunks';import Image from '@tiptap/extension-image';import DOMPurify from 'dompurify';import{api}from './api';import{dirname}from './domain';import{mathExtensions}from './math';
export const LocalImage=Image.extend({renderHTML({node,HTMLAttributes}){return['img',{...HTMLAttributes,src:'',alt:node.attrs.alt||get(t)('图片'),'data-source':node.attrs.src}]}});
const extensions=[StarterKit,...BoundedTableKit,LocalImage.configure({inline:true}),ReadHtmlAlignment,Subscript,Superscript,BoundedTaskList,TaskItem.configure({nested:true}),...mathExtensions];const parser=new MarkdownManager({extensions});
// generateHTML rebuilds the schema on every call; documents render in many pieces.
const schema=getSchema(extensions);
type Links=Record<string,{href:string;title:string|null}>;
// `offset` is the original content offset of the containing top-level block.
// Pieces never split that block, so it determines which definitions precede a
// piece without pretending stripped blockquote/list prefixes have exact offsets.
export type LinkDefinition={tag:string;href:string;title:string|null;offset:number};
type Lexer={tokenizer:{rules:{inline:{reflinkSearch:RegExp}}};blockTokens(src:string,tokens:unknown[]):unknown[];inlineTokens(src:string,tokens:unknown[]):unknown[];inlineQueue:{src:string;tokens:unknown[]}[];tokens:unknown[]&{links:Links}};
type ParserInternals={createLexer():Lexer;parseTokens(tokens:unknown[],top:boolean):JSONContent[];activeParseLexer:unknown};
const internals=parser as unknown as ParserInternals;
import {splitFrontmatter,protectHtml} from './markdown-source';
import {isHelpDocument} from './domain';
export {splitFrontmatter,protectHtml,restoreHtml} from './markdown-source';
// Link reference definitions in document order, including nested containers.
export function markdownLinks(body:string):LinkDefinition[]{
 const content=splitFrontmatter(body).body;
 return content.includes(']:')?collectMarkdownLinks(content,splitMarkdown(content)):[];
}

export function collectMarkdownLinks(content:string,pieces:{from:number;to:number;source?:boolean}[]):LinkDefinition[]{
 const links:LinkDefinition[]=[];
 for(const piece of pieces){
  if(piece.source)continue;
  const text=content.slice(piece.from,piece.to);
  if(!/\]:/.test(text))continue;
  const normalized=text.replace(/\r\n|\r/g,'\n');
  let cursor=0,original=0,normalizedPosition=0;
  for(const token of parser.instance.lexer(normalized)){
   // Marked omits repeated definitions from its token array; summing raw.length
   // would put later blocks before their real positions. Locate each raw block.
   const start=normalized.indexOf(token.raw,cursor);
   while(normalizedPosition<start){
    if(text[original]==='\r'&&text[original+1]==='\n')original++;
    original++;normalizedPosition++;
   }
   const offset=piece.from+original;
   const visit:Parameters<typeof parser.instance.walkTokens>[1]=nested=>{
    if(nested.type==='def')links.push({tag:nested.tag,href:nested.href,title:nested.title??null,offset});
    // Tiptap's custom task-list tokenizer keeps child blocks outside `tokens`.
    if(nested.type==='taskList')for(const item of nested.items)parser.instance.walkTokens(item.nestedTokens??[],visit);
   };
   // Rendering protects raw HTML before lexing. Apply that same transformation
   // inside the original block: any revealed definitions still belong to this
   // block's source offset, independent of the inserted protection fences.
   const protectedBlock=protectHtml(token.raw,true);
   parser.instance.walkTokens(protectedBlock===token.raw?[token]:parser.instance.lexer(protectedBlock),visit);
   cursor=start+token.raw.length;
  }
 }
 return links;
}
type DefinitionIndex=ReadonlyMap<string,LinkDefinition>;
function indexDefinitions(links:LinkDefinition[]){
 const index=new Map<string,LinkDefinition>();
 for(const link of links)if(!index.has(link.tag))index.set(link.tag,link);
 return index;
}

// Marked looks up duplicate definitions during block lexing and uses a document-wide
// lookup for inline references. Its emphasis masker additionally enumerates keys.
// Keep only this piece's local definitions and mask candidates enumerable.
function useDefinitions(lexer:Lexer,index:DefinitionIndex,source:string,priorOffset:number){
 const local:Links=Object.create(null);
 let inline=false;
 const lookup=(tag:string)=>{
  const def=index.get(tag);
  return def&&(inline||def.offset<priorOffset)?def:undefined;
 };
 lexer.tokens.links=new Proxy(local,{get:(obj,key)=>typeof key==='string'?obj[key]??lookup(key):undefined});
 const mask=new RegExp(lexer.tokenizer.rules.inline.reflinkSearch);
 const addMaskKeys=(text:string)=>{
  mask.lastIndex=0;
  for(let match=mask.exec(text);match;match=mask.exec(text)){
   const raw=match[0],key=raw.slice(raw.lastIndexOf('[')+1,-1);
   const found=lookup(key);if(found&&!local[key])local[key]=found;
  }
 };
 addMaskKeys(source);
 return ()=>{
  inline=true;
  for(const item of lexer.inlineQueue)addMaskKeys(item.src);
 };
}
// Renders one piece of a document so that the pieces, concatenated, equal the
// whole render. While lexing blocks, marked only knows definitions seen so far
// (a repeated definition is dropped, which changes the token list), and inline
// links resolve against all of them; both are reproduced here. Pieces from
// splitMarkdown end in one empty line, which tiptap would turn into a trailing
// empty paragraph unless the piece ends the document.
export function renderMarkdownPiece(text:string,links:LinkDefinition[]|DefinitionIndex=[],{offset=0,last=true,prior=true}:{offset?:number;last?:boolean;prior?:boolean}={}):string{
 const source=protectHtml(last?text:text.replace(/\r?\n$/,''),true).replace(/\r\n|\r/g,'\n');
 const lexer=internals.createLexer();
 const definitions=Array.isArray(links)?(links.length&&source.includes('[')?indexDefinitions(links):null):links;
 const enableInline=definitions?.size&&source.includes('[')?useDefinitions(lexer,definitions,source,prior?offset:-Infinity):null;
 const previous=internals.activeParseLexer;internals.activeParseLexer=lexer;
 try{
  lexer.blockTokens(source,lexer.tokens);
  // Marked ends an HTML block at a blank line. Parse each table as one HTML
  // fragment so Tiptap retains its rows and merged cells across blank lines.
  for(let index=0;index<lexer.tokens.length;index++){
   const token=lexer.tokens[index] as {type:string;raw:string};
   if(token.type!=='html'||!/^ {0,3}<table(?:\s|>)/i.test(token.raw))continue;
   const parts=[token.raw];let end=index+1;
   while(!/<\/table\s*>/i.test(parts[parts.length-1])&&end<lexer.tokens.length)parts.push((lexer.tokens[end++] as {raw:string}).raw);
   const raw=parts.join('');
   lexer.tokens.splice(index,end-index,{type:'html',raw,text:raw,block:true});
  }
  enableInline?.();
  for(const item of lexer.inlineQueue)lexer.inlineTokens(item.src,item.tokens);
  lexer.inlineQueue=[];
  // Tiptap supplies an empty paragraph when a document contains only definitions.
  // A definitions-only prefix is not an empty document of its own.
  if(!last&&lexer.tokens.every(token=>(token as {type:string}).type==='def'))return '';
  const content=internals.parseTokens(lexer.tokens,true);
  return getHTMLFromFragment(ProseMirrorNode.fromJSON(schema,{type:'doc',content}).content,schema);
 }finally{internals.activeParseLexer=previous}
}
export function markdownTokens(text:string){return parser.instance.lexer(text)}
export function finishHtml(html:string):string{
 html=DOMPurify.sanitize(html,{ADD_ATTR:['data-source','data-tex']});
 if(!/class="language-mermaid"/i.test(html))return html;
 const doc=new DOMParser().parseFromString(html,'text/html');
 for(const code of doc.querySelectorAll('pre > code')){
  if(!/^language-mermaid$/i.test(code.className))continue;
  const preview=mermaidPreview(code.textContent!);
  if(preview.classList.contains('mermaid-error'))code.parentElement!.after(preview);
  else code.parentElement!.replaceWith(preview);
 }
 return doc.body.innerHTML;
}
export function renderMarkdown(text:string,referenceBody?:string):string{
 const content=splitFrontmatter(text).body,pieces=splitMarkdown(content);
 // A reference body's definitions resolve links but, like appended text, come after every block.
 const links=indexDefinitions(referenceBody?markdownLinks(referenceBody):pieces.length>1&&content.includes(']:')?collectMarkdownLinks(content,pieces):[]);
 return finishHtml(pieces.map((piece,index)=>renderMarkdownPiece(content.slice(piece.from,piece.to),links,{offset:piece.from,last:index===pieces.length-1,prior:!referenceBody})).join(''));
}
export function markdownCitation(body:string,ranges:{from:number;to:number}[]):string{
 const normalized=body.replace(/\r\n?/g,'\n');
 const positions=ranges.map(({from,to})=>({from:body.slice(0,from).replace(/\r\n?/g,'\n').length,to:body.slice(0,to).replace(/\r\n?/g,'\n').length}));
 const {prefix,body:content}=splitFrontmatter(normalized);
 const kept:string[]=[];
 for(const piece of splitMarkdown(content)){
  const from=prefix.length+piece.from,to=prefix.length+piece.to,text=content.slice(piece.from,piece.to);
  const cited=positions.some(range=>from<range.to&&to>range.from);
  if(!cited&&!/\]:/.test(text))continue;
  let offset=from;
  for(const token of parser.instance.lexer(text)){
   const start=offset;offset+=token.raw.length;
   if(token.type==='def'||(cited&&positions.some(range=>start<range.to&&offset>range.from)))kept.push(token.raw);
  }
 }
 return kept.join('\n\n');
}
export async function hydrateImages(host:HTMLElement,path:string,current:()=>boolean=()=>true):Promise<void>{
 const normalized=path.replace(/\\/g,'/').replace(/^\/\/\?\/UNC\//i,'//').replace(/^\/\/\?\//,'');
 const base=dirname(normalized);
 for(const image of host.querySelectorAll<HTMLImageElement>('img[data-source]')){
  if(!current())return;
  const src=image.dataset.source!;
  if(/^(?:https?:|data:|javascript:)/i.test(src)){image.alt=get(t)('外部图片（未自动加载）');continue;}
  try{
   const url=new URL(src,'file://'+(base.startsWith('/')?'':'/')+base+'/');
   let absolute=(url.hostname?'//'+url.hostname:'')+decodeURIComponent(url.pathname);
   if(/^[A-Za-z]:/.test(base))absolute=absolute.replace(/^\//,'');
   const data=await api<string>('load_image',{path:absolute});
   if(current()&&host.contains(image))image.src=data;
  }catch(error){
   if(current()&&host.contains(image))image.alt=get(t)('图片不可用：{value0}',{value0:get(t)(String(error))});
  }
 }
}
export function renderAnswer(text:string,evidence:Candidate[],tools:ToolCallRecord[]=[],reasoning:ReasoningBlock[]=[]){
 const sources:{first:Candidate;number:number}[]=[];
 const numbers=new Map<string,number>();
 function renderPart(content:string){
 const html=renderMarkdown(content,text);const doc=new DOMParser().parseFromString(html,'text/html');const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT);const nodes:Text[]=[];while(walker.nextNode())nodes.push(walker.currentNode as Text);
 for(const node of nodes){
  if(node.parentElement?.closest('code,pre,a,svg,.mermaid-diagram,.math-inline,.math-block'))continue;
  const value=node.textContent||'';
  const matches=[...value.matchAll(/\[(\d+)\]/g)].filter(m=>Number(m[1])>0&&Number(m[1])<=evidence.length);
  if(!matches.length)continue;
  const fragment=doc.createDocumentFragment();let end=0;
  for(const match of matches){
   const index=Number(match[1])-1;
   const candidate=evidence[index];
   const key=isHelpDocument(candidate.path)?candidate.path+'#'+candidate.heading:(candidate.hash||candidate.path);
   let number=numbers.get(key);
   if(number===undefined){number=sources.length+1;numbers.set(key,number);sources.push({first:candidate,number});}
   fragment.append(value.slice(end,match.index));
   const button=doc.createElement('button');button.className='citation';button.dataset.citation=String(index);button.textContent=String(number);fragment.append(button);
   end=match.index!+match[0].length;
  }
  fragment.append(value.slice(end));node.replaceWith(fragment);
 }
 return doc.body.innerHTML;
 }
 let offset=0;
 const events=answerEvents(tools,reasoning).map(marker=>{const html=renderPart(text.slice(marker.from,marker.offset));offset=marker.offset;return {...marker,html}});
 return {html:renderPart(text.slice(offset)),events,sources};
}
