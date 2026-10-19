import {answerEvents} from './answer-events';
import {get} from 'svelte/store';
import {t} from './i18n';
import type {Candidate,ToolCallRecord,ReasoningBlock} from './types';
import{TaskList,TaskItem}from '@tiptap/extension-list';
import {generateHTML} from '@tiptap/core';import StarterKit from '@tiptap/starter-kit';import {MarkdownManager}from '@tiptap/markdown';import{TableKit}from '@tiptap/extension-table';import Image from '@tiptap/extension-image';import DOMPurify from 'dompurify';import{api}from './api';import{dirname}from './domain';
export const LocalImage=Image.extend({renderHTML({node,HTMLAttributes}){return['img',{...HTMLAttributes,src:'',alt:node.attrs.alt||get(t)('图片'),'data-source':node.attrs.src}]}});
const extensions=[StarterKit,TableKit,LocalImage,TaskList,TaskItem.configure({nested:true})];const parser=new MarkdownManager({extensions});
import {splitFrontmatter,protectHtml} from './markdown-source';
export {splitFrontmatter,protectHtml,restoreHtml} from './markdown-source';
export function renderMarkdown(text:string,referenceBody?:string):string{
 if(referenceBody){const definitions=parser.instance.lexer(splitFrontmatter(referenceBody).body).filter(token=>token.type==='def').map(token=>token.raw).join('\n\n');if(definitions)text+='\n\n'+definitions;}
 return DOMPurify.sanitize(generateHTML(parser.parse(protectHtml(splitFrontmatter(text).body)),extensions),{ADD_ATTR:['data-source']});
}
export function markdownCitation(body:string,ranges:{from:number;to:number}[]):string{
 const normalized=body.replace(/\r\n?/g,'\n');
 const positions=ranges.map(({from,to})=>({from:body.slice(0,from).replace(/\r\n?/g,'\n').length,to:body.slice(0,to).replace(/\r\n?/g,'\n').length}));
 const {prefix,body:content}=splitFrontmatter(normalized);let offset=prefix.length;
 return parser.instance.lexer(content).filter(token=>{
  const from=offset;offset+=token.raw.length;
  return token.type==='def'||positions.some(range=>from<range.to&&offset>range.from);
 }).map(token=>token.raw).join('\n\n');
}
export async function hydrateImages(host:HTMLElement,path:string):Promise<void>{for(const image of host.querySelectorAll<HTMLImageElement>('img[data-source]')){const src=image.dataset.source!;if(/^(?:https?:|data:|javascript:)/i.test(src)){image.alt=get(t)('外部图片（未自动加载）');continue;}const base=dirname(path).replace(/\\/g,'/');const url=new URL(src,'file://'+(base.startsWith('/')?'':'/')+base+'/');let absolute=decodeURIComponent(url.pathname);if(/^[A-Za-z]:/.test(base))absolute=absolute.replace(/^\//,'');try{image.src=await api<string>('load_image',{path:absolute});}catch(error){image.alt=get(t)('图片不可用：{value0}',{value0:get(t)(String(error))});}}}
export function renderAnswer(text:string,evidence:Candidate[],tools:ToolCallRecord[]=[],reasoning:ReasoningBlock[]=[]){
 const sources:{first:Candidate;number:number}[]=[];
 const numbers=new Map<string,number>();
 function renderPart(content:string){
 const html=renderMarkdown(content,text);const doc=new DOMParser().parseFromString(html,'text/html');const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT);const nodes:Text[]=[];while(walker.nextNode())nodes.push(walker.currentNode as Text);
 for(const node of nodes){
  if(node.parentElement?.closest('code,pre,a'))continue;
  const value=node.textContent||'';
  const matches=[...value.matchAll(/\[(\d+)\]/g)].filter(m=>Number(m[1])>0&&Number(m[1])<=evidence.length);
  if(!matches.length)continue;
  const fragment=doc.createDocumentFragment();let end=0;
  for(const match of matches){
   const index=Number(match[1])-1;
   const candidate=evidence[index];
   const key=candidate.hash||candidate.path;
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
