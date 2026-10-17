<script lang="ts">
 import{tick}from 'svelte';import{highlightedHtml}from '../lib/search-highlights';import MatchNavigation from './MatchNavigation.svelte';
 export let html='';export let terms:string[]=[];export let path='';export let initialMatch=0;
 let host:HTMLDivElement,index=0;
 $:highlighted=highlightedHtml(html,terms);
 $:if(highlighted){index=0;void showDocument(highlighted);}
 async function showDocument(content:{html:string;count:number}){
  await tick();if(content!==highlighted)return;
  selectMatch(Math.min(initialMatch,Math.max(0,highlighted.count-1)));
  if(path&&host.querySelector('img[data-source]')){const {hydrateImages}=await import('../lib/markdown');await hydrateImages(host,path);}
 }
 function selectMatch(next:number){
  if(!highlighted.count)return;index=(next+highlighted.count)%highlighted.count;
  host.querySelectorAll('.search-hit.active').forEach(mark=>mark.classList.remove('active'));
  const marks=host.querySelectorAll<HTMLElement>(`[data-search-match="${index}"]`);
  marks.forEach(mark=>mark.classList.add('active'));marks[0]?.scrollIntoView({block:'center'});
 }
</script>
<MatchNavigation count={highlighted.count} {index} onMove={step=>selectMatch(index+step)}/>
<div class="search-preview-body tiptap" bind:this={host}>{@html highlighted.html}</div>
