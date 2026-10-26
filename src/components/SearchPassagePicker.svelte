<script lang="ts">
 import {onMount} from 'svelte';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {plainText,searchSnippet} from '../lib/search-highlights';
 import {quietScroll} from '../lib/quiet-scroll';
 import type {Candidate} from '../lib/types';
 // A dropdown under the reader's passage control: one line of chrome, then the passages.
 export let candidates:Candidate[]=[];
 export let selectedId:number|null=null;
 export let terms:string[]=[];
 export let id='search-passage-picker';
 export let onSelect:(candidate:Candidate)=>void;
 export let onClose:()=>void;
 let panel:HTMLElement,closeButton:HTMLButtonElement;
 let activeIndex=0;
 $:activeIndex=Math.max(0,candidates.findIndex(candidate=>candidate.id===selectedId));
 const snippet=(candidate:Candidate)=>searchSnippet(plainText(candidate.path,candidate.content),terms);
 function focusOption(index:number){
  if(!candidates.length)return;
  activeIndex=Math.max(0,Math.min(candidates.length-1,index));
  const button=panel.querySelectorAll<HTMLButtonElement>('.search-passage-option')[activeIndex];
  button?.focus({preventScroll:true});
  button?.scrollIntoView({block:'nearest'});
 }
 function keys(event:KeyboardEvent){
  if(event.isComposing)return;
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();onClose();return}
  if(event.metaKey||event.ctrlKey||event.altKey)return;
  if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
   event.preventDefault();event.stopPropagation();
   focusOption(event.key==='Home'?0:event.key==='End'?candidates.length-1:activeIndex+(event.key==='ArrowDown'?1:-1));
  }else if(event.key==='Enter'&&event.target instanceof Element&&event.target.closest('.search-passage-option')){
   event.preventDefault();event.stopPropagation();
   const candidate=candidates[activeIndex];if(candidate)onSelect(candidate);
  }
 }
 onMount(()=>{if(candidates.length)focusOption(activeIndex);else closeButton.focus({preventScroll:true})});
</script>

<div {id} class="search-passage-picker" role="dialog" aria-modal="false" aria-label={$t('选择相关片段')} tabindex="-1" bind:this={panel} on:keydown={keys}>
 <div class="search-passage-head">
  <strong>{$t('相关片段')}<small>{candidates.length}</small></strong>
  <span class="search-passage-hint" aria-hidden="true">{$t('↑ ↓ 选择 · Enter 定位 · Esc 关闭')}</span>
  <button type="button" class="icon-button search-passage-close" aria-label={$t('关闭片段选择器')} bind:this={closeButton} on:click={onClose}>{@html icon('close')}</button>
 </div>
 <div class="search-passage-options" use:quietScroll>
  {#each candidates as candidate,index (candidate.id)}
   <button type="button" class="search-passage-option" tabindex={index===activeIndex?0:-1} aria-pressed={candidate.id===selectedId} on:focus={()=>activeIndex=index} on:click={()=>onSelect(candidate)}>
    <span class="search-passage-index" aria-hidden="true">{index+1}</span>
    <span class="search-passage-copy"><strong>{candidate.heading||$t('片段 {value0}',{value0:index+1})}</strong><span class="search-passage-excerpt">{@html snippet(candidate)}</span></span>
   </button>
  {:else}<p class="search-passage-empty">{$t('没有可用的相关片段')}</p>{/each}
 </div>
</div>

<style>
 .search-passage-picker{position:absolute;z-index:20;display:flex;flex-direction:column;overflow:hidden;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-paper);box-shadow:0 10px 30px var(--color-shadow),0 1px 3px var(--color-shadow)}
 .search-passage-head{display:flex;align-items:center;gap:10px;flex-shrink:0;height:40px;padding:0 6px 0 14px;border-bottom:1px solid var(--color-rule)}
 .search-passage-head>strong{color:var(--color-ink);font-size:12px;font-weight:600;line-height:1.5}
 .search-passage-head>strong small{margin-left:6px;color:var(--color-secondary);font-size:11px;font-weight:400;font-variant-numeric:tabular-nums}
 .search-passage-hint{flex:1;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;text-align:right;color:var(--color-muted);font-size:10px}
 .search-passage-close{display:inline-flex;align-items:center;justify-content:center;width:26px;height:26px;flex-shrink:0;padding:5px;border:0;border-radius:6px;background:transparent;color:var(--color-secondary)}
 .search-passage-close:hover{background:var(--color-hover);color:var(--color-ink)}
 .search-passage-close :global(.icon){width:15px;height:15px}
 .search-passage-options{min-height:0;overflow:auto;overscroll-behavior:contain;scrollbar-gutter:stable;padding:6px}
 .search-passage-option{display:flex;align-items:flex-start;gap:10px;width:100%;padding:8px 10px;border:0;border-radius:6px;background:transparent;text-align:left;color:var(--color-ink);cursor:pointer}
 .search-passage-option:hover{background:var(--color-hover)}
 .search-passage-option[aria-pressed="true"]{background:color-mix(in srgb,var(--color-ink) 5%,var(--color-paper))}
 .search-passage-option[aria-pressed="true"] .search-passage-index{color:var(--color-ink);font-weight:600}
 .search-passage-option:focus-visible,.search-passage-close:focus-visible{outline:2px solid var(--color-focus);outline-offset:-2px}
 .search-passage-index{min-width:18px;flex-shrink:0;padding-top:1px;color:var(--color-muted);font-size:11px;line-height:1.5;font-variant-numeric:tabular-nums;text-align:right}
 .search-passage-copy{display:block;flex:1;min-width:0}
 .search-passage-copy strong{display:block;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-size:12px;font-weight:550;line-height:1.5}
 .search-passage-excerpt{display:-webkit-box;margin-top:2px;overflow:hidden;color:var(--color-secondary);font-size:12px;line-height:1.6;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow-wrap:anywhere}
 .search-passage-empty{padding:18px 12px;color:var(--color-secondary);font-size:12px}
 @media(forced-colors:active){.search-passage-option[aria-pressed="true"]{outline:1px solid Highlight}}
</style>
