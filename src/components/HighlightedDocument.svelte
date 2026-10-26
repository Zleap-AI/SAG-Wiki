<script lang="ts">
 import{onDestroy,tick}from 'svelte';import{highlightedHtml,selectSearchMatches}from '../lib/search-highlights';import MatchNavigation from './MatchNavigation.svelte';
 import {quietScroll} from '../lib/quiet-scroll';
 import {readReaderScroll,rememberReaderScroll} from '../lib/reader-scroll';
 export let html='';export let terms:string[]=[];export let path='';export let initialMatch=0;
 export let showNavigation=true;export let memoryKey='';export let restoreScroll=false;
 let host:HTMLDivElement,index=0;
 const positionKey=memoryKey;
 let saved=restoreScroll?readReaderScroll(positionKey):undefined;
 $:highlighted=highlightedHtml(html,terms);
 $:if(highlighted){index=0;void showDocument(highlighted);}
 async function showDocument(content:{html:string;count:number}){
  await tick();if(content!==highlighted||!host)return;
  selectMatch(Math.min(initialMatch,Math.max(0,highlighted.count-1)),!saved);
  if(saved){host.scrollTop=saved.top;host.scrollLeft=saved.left;saved=undefined}
  if(path&&host.querySelector('img[data-source]')){const {hydrateImages}=await import('../lib/markdown');await hydrateImages(host,path);}
 }
 function selectMatch(next:number,scroll=true){
  if(!highlighted.count)return;index=(next+highlighted.count)%highlighted.count;
  const marks=[...host.querySelectorAll<HTMLElement>(`[data-search-match="${index}"]`)];
  selectSearchMatches(host,marks);if(scroll)marks[0]?.scrollIntoView({block:'center'});
 }
 function remember(){if(host)rememberReaderScroll(positionKey,{top:host.scrollTop,left:host.scrollLeft})}
 onDestroy(remember);
</script>
{#if showNavigation}<MatchNavigation count={highlighted.count} {index} onMove={step=>selectMatch(index+step)}/>{/if}
<div class="search-preview-body tiptap" bind:this={host} use:quietScroll on:scroll={remember}>{@html highlighted.html}</div>
