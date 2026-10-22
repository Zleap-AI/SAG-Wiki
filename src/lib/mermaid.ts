import {PeleError, render} from 'pele';
import DOMPurify from 'dompurify';
import CodeBlock from '@tiptap/extension-code-block';
import {get} from 'svelte/store';
import {t} from './i18n';
import './mermaid.css';

let diagramId=0;

export function mermaidPreview(source:string,idPrefix=`sag-mermaid-${++diagramId}`):HTMLDivElement{
 const preview=document.createElement('div');
 preview.className='mermaid-diagram';
 try{
  const {svg}=render(source,{idPrefix,images:false,links:false});
  preview.append(DOMPurify.sanitize(svg,{USE_PROFILES:{svg:true,svgFilters:true},RETURN_DOM_FRAGMENT:true}));
 }catch(error){
  if(!(error instanceof PeleError))throw error;
  preview.classList.add('mermaid-error');
  preview.setAttribute('role','status');
  preview.textContent=get(t)('Mermaid 图表无法渲染：{value0}',{value0:error.message});
 }
 return preview;
}

// Keep the editable code in the document model; the preview is only a node view.
export const MermaidCodeBlock=CodeBlock.extend({addNodeView(){return ({node,editor,getPos})=>{
 const isMermaid=node.attrs.language?.toLowerCase()==='mermaid';
 const pre=document.createElement('pre'),code=pre.appendChild(document.createElement('code'));
 if(node.attrs.language)code.className=`language-${node.attrs.language}`;
 const dom=isMermaid?document.createElement('div'):pre;
 let source=node.textContent;
 const idPrefix=`sag-mermaid-${++diagramId}`;
 let preview:HTMLDivElement;
 let editing=false;
 const button=document.createElement('button');
 const showSource=(value:boolean)=>{
  editing=value;
  pre.hidden=!value;preview.hidden=value;
  dom.setAttribute('contenteditable',String(value));
  dom.classList.toggle('is-editing',value);
  button.textContent=get(t)(value?'预览':'编辑');
  button.setAttribute('aria-label',get(t)(value?'预览 Mermaid 图表':'编辑 Mermaid 源码'));
  button.setAttribute('aria-expanded',String(value));
 };
 let unsubscribe=()=>{};
 if(isMermaid){
  dom.className='mermaid-code-block';
  preview=mermaidPreview(source,idPrefix);
  preview.setAttribute('contenteditable','false');
  button.type='button';button.setAttribute('contenteditable','false');
  button.addEventListener('click',()=>{
   showSource(!editing);
   if(editing)editor.chain().focus().setTextSelection(getPos()!+1).run();
   else{editor.commands.setNodeSelection(getPos()!);button.focus();}
  });
  dom.append(button,pre,preview);
  unsubscribe=t.subscribe(()=>showSource(editing));
 }
 return {
  dom,contentDOM:code,
  update(next){
   if(next.type!==node.type||(next.attrs.language?.toLowerCase()==='mermaid')!==isMermaid)return false;
   node=next;
   code.className=next.attrs.language?`language-${next.attrs.language}`:'';
   if(isMermaid&&next.textContent!==source){
    source=next.textContent;
    const updated=mermaidPreview(source,idPrefix);
    updated.setAttribute('contenteditable','false');updated.hidden=editing;
    preview.replaceWith(updated);preview=updated;
   }
   return true;
  },
  ignoreMutation:mutation=>isMermaid&&mutation.type!=='selection'&&!code.contains(mutation.target),
  stopEvent:event=>isMermaid&&(button.contains(event.target as Node)||preview.contains(event.target as Node)),
  destroy(){unsubscribe()},
 };
};}});
