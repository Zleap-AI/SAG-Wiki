<script lang="ts">
 import {t} from '../lib/i18n';
 import {onMount} from 'svelte';
 import {highlightDocument} from '../lib/search-highlights';
 import MatchNavigation from './MatchNavigation.svelte';
 import {readReaderScroll,rememberReaderScroll} from '../lib/reader-scroll';
 export let body='';export let terms:string[]=[];export let initialMatch=0;
 export let showNavigation=true;export let memoryKey='';export let restoreScroll=false;
 export let paragraphHighlight=false;
 let html='',count=0,index=0,ready=false,frame:HTMLIFrameElement;
 const positionKey=memoryKey;
 let saved=restoreScroll?readReaderScroll(positionKey):undefined;
 $:{
  const doc=new DOMParser().parseFromString(body,'text/html');
  doc.querySelectorAll('script,meta,base,iframe,object,embed').forEach(node=>node.remove());
  for(const node of doc.querySelectorAll('*'))for(const attribute of [...node.attributes]){
   if(/^on/i.test(attribute.name)||attribute.name==='srcdoc'||(/^(?:href|xlink:href)$/i.test(attribute.name)&&/^(?:javascript|vbscript|data):/i.test(attribute.value.replace(/[\u0000-\u0020]/g,''))))node.removeAttribute(attribute.name);
  }
  const nonce=crypto.randomUUID();
  const policy=doc.createElement('meta');policy.httpEquiv='Content-Security-Policy';policy.content=`script-src 'nonce-${nonce}'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`;doc.head.prepend(policy);
  const bridge=doc.createElement('script');bridge.src=new URL('../lib/document-bridge.js?no-inline',import.meta.url).href;bridge.setAttribute('nonce',nonce);doc.body.append(bridge);
  doc.querySelectorAll('a[href^="#"]').forEach(link=>link.setAttribute('href','about:srcdoc'+link.getAttribute('href')));
  count=highlightDocument(doc,terms);ready=false;
  const style=doc.createElement('style');
  style.textContent=`mark.search-hit{background:rgba(80,80,80,.1)!important;color:inherit!important;border-radius:2px}mark.search-hit.active{background:rgba(80,80,80,.2)!important;box-shadow:0 1px 0 rgba(80,80,80,.35)}tspan.search-hit{stroke:rgba(80,80,80,.1)!important;stroke-width:4px!important;paint-order:stroke fill}tspan.search-hit.active{stroke:rgba(80,80,80,.2)!important}
   .search-active-paragraph{background:#202020!important;color:#fff!important;border-radius:2px}.search-active-paragraph *{color:inherit!important}.search-active-paragraph :is(mark.search-hit,code){background:transparent!important;box-shadow:none}
   *{scrollbar-width:thin;scrollbar-color:transparent transparent}html.sag-scroll-active,html.sag-scroll-active *{scrollbar-color:rgba(100,100,100,.4) transparent}::-webkit-scrollbar{width:6px;height:6px}::-webkit-scrollbar-track{background:transparent}::-webkit-scrollbar-thumb{background:transparent;border-radius:6px}html.sag-scroll-active::-webkit-scrollbar-thumb,html.sag-scroll-active ::-webkit-scrollbar-thumb{background:rgba(100,100,100,.4)}`;
  doc.head.append(style);
  html='<!DOCTYPE html>'+doc.documentElement.outerHTML;
 }
 onMount(()=>{
  const receive=(event:MessageEvent)=>{
   if(event.source!==frame.contentWindow)return;
   if(event.data?.type==='sag-external-link')window.dispatchEvent(new CustomEvent('open-external-link',{detail:event.data.url}));
   if(event.data?.type==='sag-reader-scroll'&&typeof event.data.top==='number'&&typeof event.data.left==='number')rememberReaderScroll(positionKey,{top:event.data.top,left:event.data.left});
  };
  window.addEventListener('message',receive);return()=>window.removeEventListener('message',receive);
 });
 function loaded(event:Event){
  if(event.currentTarget!==frame)return;ready=true;
  selectMatch(Math.min(initialMatch,Math.max(0,count-1)),!saved);
  if(saved){frame.contentWindow!.postMessage({type:'sag-restore-scroll',top:saved.top,left:saved.left},'*');saved=undefined}
 }
 function selectMatch(next:number,scroll=true){
  index=count?(next+count)%count:0;
  frame.contentWindow!.postMessage({type:'sag-select-match',index,scroll,paragraph:paragraphHighlight},'*');
 }
</script>

{#if ready&&showNavigation}<MatchNavigation {count} {index} onMove={step=>selectMatch(index+step)}/>{/if}
<iframe class="html-document" title={$t('HTML 文档')} sandbox="allow-scripts" srcdoc={html} bind:this={frame} on:load={loaded}></iframe>

<style>
 .html-document{display:block;flex:1;width:100%;min-height:0;border:0;background:white}
</style>
