<script lang="ts">
 import {t} from '../lib/i18n';
 import {onMount} from 'svelte';
 import {highlightDocument} from '../lib/search-highlights';
 import MatchNavigation from './MatchNavigation.svelte';
 export let body='';export let terms:string[]=[];export let initialMatch=0;
 let html='',count=0,index=0,ready=false,frame:HTMLIFrameElement;
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
  style.textContent='mark.search-hit{background:#ffe39c!important;color:#151515!important}mark.search-hit.active{background:#ffb847!important}tspan.search-hit{fill:#151515!important;stroke:#ffe39c!important;stroke-width:4px!important;paint-order:stroke fill}tspan.search-hit.active{stroke:#ffb847!important}';
  doc.head.append(style);
  html='<!DOCTYPE html>'+doc.documentElement.outerHTML;
 }
 onMount(()=>{
  const receive=(event:MessageEvent)=>{
   if(event.source===frame.contentWindow&&event.data?.type==='sag-external-link')window.dispatchEvent(new CustomEvent('open-external-link',{detail:event.data.url}));
  };
  window.addEventListener('message',receive);return()=>window.removeEventListener('message',receive);
 });
 function loaded(event:Event){if(event.currentTarget!==frame)return;ready=true;selectMatch(Math.min(initialMatch,Math.max(0,count-1)));}
 function selectMatch(next:number){
  if(!count)return;index=(next+count)%count;
  frame.contentWindow!.postMessage({type:'sag-select-match',index},'*');
 }
</script>

{#if ready}<MatchNavigation {count} {index} onMove={step=>selectMatch(index+step)}/>{/if}
<iframe class="html-document" title={$t('HTML 文档')} sandbox="allow-scripts" srcdoc={html} bind:this={frame} on:load={loaded}></iframe>

<style>
 .html-document{display:block;flex:1;width:100%;min-height:0;border:0;background:white}
</style>
