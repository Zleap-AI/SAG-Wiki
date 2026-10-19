<script lang="ts">
 import {t} from '../lib/i18n';
 import {highlightDocument} from '../lib/search-highlights';
 import MatchNavigation from './MatchNavigation.svelte';
 export let body='';export let terms:string[]=[];export let initialMatch=0;
 let html='',count=0,index=0,ready=false,frame:HTMLIFrameElement;
 $:{
  const doc=new DOMParser().parseFromString(body,'text/html');
  doc.querySelectorAll('a[href^="#"]').forEach(link=>link.setAttribute('href','about:srcdoc'+link.getAttribute('href')));
  count=highlightDocument(doc,terms);ready=false;
  const style=doc.createElement('style');
  style.textContent='mark.search-hit{background:#ffe39c!important;color:#151515!important}mark.search-hit.active{background:#ffb847!important}tspan.search-hit{fill:#151515!important;stroke:#ffe39c!important;stroke-width:4px!important;paint-order:stroke fill}tspan.search-hit.active{stroke:#ffb847!important}';
  doc.head.append(style);
  html='<!DOCTYPE html>'+doc.documentElement.outerHTML;
 }
 function loaded(event:Event){if(event.currentTarget!==frame)return;ready=!!frame.contentDocument;if(ready){frame.contentDocument!.addEventListener('contextmenu',event=>event.preventDefault());selectMatch(Math.min(initialMatch,Math.max(0,count-1)));}}
 function selectMatch(next:number){
  if(!count)return;index=(next+count)%count;
  const doc=frame.contentDocument!;
  doc.querySelectorAll('.search-hit.active').forEach(mark=>mark.classList.remove('active'));
  const marks=doc.querySelectorAll(`[data-search-match="${index}"]`);
  marks.forEach(mark=>mark.classList.add('active'));marks[0].scrollIntoView({block:'center'});
 }
</script>

{#if ready}<MatchNavigation {count} {index} onMove={step=>selectMatch(index+step)}/>{/if}
<iframe class="html-document" title={$t('HTML 文档')} sandbox="allow-same-origin" srcdoc={html} bind:this={frame} on:load={loaded}></iframe>

<style>
 .html-document{display:block;flex:1;width:100%;min-height:0;border:0;background:white}
</style>
