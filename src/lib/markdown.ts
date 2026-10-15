import {get} from 'svelte/store';
import {t} from './i18n';
import{TaskList,TaskItem}from '@tiptap/extension-list';
import {generateHTML} from '@tiptap/core';import StarterKit from '@tiptap/starter-kit';import {MarkdownManager}from '@tiptap/markdown';import{TableKit}from '@tiptap/extension-table';import Image from '@tiptap/extension-image';import DOMPurify from 'dompurify';import{api}from './api';import{dirname}from './domain';
export const LocalImage=Image.extend({renderHTML({node,HTMLAttributes}){return['img',{...HTMLAttributes,src:'',alt:node.attrs.alt||get(t)('图片'),'data-source':node.attrs.src}]}});
const extensions=[StarterKit,TableKit,LocalImage,TaskList,TaskItem.configure({nested:true})];const parser=new MarkdownManager({extensions});
import {splitFrontmatter,protectHtml} from './markdown-source';
export {splitFrontmatter,protectHtml,restoreHtml} from './markdown-source';
export function renderMarkdown(text:string):string{return DOMPurify.sanitize(generateHTML(parser.parse(protectHtml(splitFrontmatter(text).body)),extensions),{ADD_ATTR:['data-source']});}
export async function hydrateImages(host:HTMLElement,path:string):Promise<void>{for(const image of host.querySelectorAll<HTMLImageElement>('img[data-source]')){const src=image.dataset.source!;if(/^(?:https?:|data:|javascript:)/i.test(src)){image.alt=get(t)('外部图片（未自动加载）');continue;}const base=dirname(path).replace(/\\/g,'/');const url=new URL(src,'file://'+(base.startsWith('/')?'':'/')+base+'/');let absolute=decodeURIComponent(url.pathname);if(/^[A-Za-z]:/.test(base))absolute=absolute.replace(/^\//,'');try{image.src=await api<string>('load_image',{path:absolute});}catch(error){image.alt=get(t)('图片不可用：{value0}',{value0:get(t)(String(error))});}}}
export function renderAnswer(text:string,evidenceCount:number):string{
 const html=renderMarkdown(text);const doc=new DOMParser().parseFromString(html,'text/html');const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT);const nodes:Text[]=[];while(walker.nextNode())nodes.push(walker.currentNode as Text);
 for(const node of nodes){const value=node.textContent||'';const matches=[...value.matchAll(/\[(\d+)\]/g)].filter(m=>Number(m[1])>0&&Number(m[1])<=evidenceCount);if(!matches.length)continue;const fragment=doc.createDocumentFragment();let end=0;for(const m of matches){fragment.append(value.slice(end,m.index));const button=doc.createElement('button');button.className='citation';button.dataset.citation=String(Number(m[1])-1);button.textContent=m[1];fragment.append(button);end=m.index!+m[0].length;}fragment.append(value.slice(end));node.replaceWith(fragment);}
 return doc.body.innerHTML;
}
