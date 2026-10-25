<script lang="ts">
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {sourceFolder,sourceName} from '../lib/domain';
 import {highlightedText,plainText,searchSnippet} from '../lib/search-highlights';
 import type {Candidate} from '../lib/types';
 // One quiet row per file: title, a two-line excerpt and a single meta line.
 // Passage details open in the reader's picker, not inside the list.
 export let group:{first:Candidate;matches:Candidate[]};
 export let id:string;
 export let terms:string[]=[];
 export let roots:{path:string;name:string}[]=[];
 export let selected=false;
 export let active=false;
 export let focused:Candidate|null=null;
 export let cited:number[]=[];
 export let expanded=false;
 export let onSelect:(candidate:Candidate)=>void;
 export let onToggle:()=>void;
 export let onFocus:()=>void=()=>{};
 $:first=group.first;
 $:folder=sourceFolder(first.path,roots);
 $:format=(sourceName(first).match(/\.([^.]+)$/)?.[1]??'').toUpperCase();
 const snippet=(candidate:Candidate)=>searchSnippet(plainText(candidate.path,candidate.content),terms);
</script>

<!-- The whole row selects the file; keyboard selection is handled by the listbox (aria-activedescendant). -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="search-result-group" class:selected class:active on:click={event=>{if(!(event.target instanceof Element&&event.target.closest('.search-result-more')))onSelect(first)}}>
 <div {id} class="search-result" class:selected role="option" aria-selected={selected} tabindex="-1">
  <span class="search-result-title"><strong title={sourceName(first)}>{@html highlightedText(sourceName(first),terms)}</strong>{#if selected}<span class="search-result-reading" title={focused?.path===first.path?focused.heading:undefined}>{$t('阅读中')}</span>{/if}</span>
  <p>{@html snippet(first)}</p>
 </div>
 <div class="search-result-bottom">
  <span class="search-result-meta">
   {#if format}<span class="search-result-format">{format}</span><span aria-hidden="true">·</span>{/if}
   <small class="search-result-folder" title={folder}>{folder}</small>
   {#if cited.length}<span class="search-result-reference" title={$t('搜索总结引用了这个文件')}>{@html icon('blockquote')}<span class="sr-only">{$t('总结引用')}</span><span class="search-result-cited">{#each cited as number}<span>{number}</span>{/each}</span></span>{/if}
  </span>
  {#if group.matches.length}<button type="button" class="search-result-more" aria-haspopup="dialog" aria-expanded={expanded} aria-controls="search-passage-picker" tabindex="0" on:focus={onFocus} on:click={onToggle}><span>{$t('{value0} 处相关片段',{value0:group.matches.length})}</span>{@html icon('chevron')}</button>{/if}
 </div>
</div>

<style>
 .search-result-group{position:relative;margin-bottom:2px;padding:10px 12px 9px;border-radius:8px;background:transparent;cursor:pointer}
 .search-result-group.selected{background:color-mix(in srgb,var(--color-ink) 5%,var(--color-paper))}
 .search-result-group:not(.selected):hover{background:color-mix(in srgb,var(--color-ink) 2.5%,var(--color-paper))}
 .search-result-group.active{outline:2px solid var(--color-focus);outline-offset:-2px}
 /* The option itself carries no box; the row above owns selection and hover. */
 .search-result-group :global(.search-result),.search-result-group :global(.search-result.selected),.search-result-group :global(.search-result:hover){display:block;width:100%;margin:0;padding:0;border:0;border-radius:0;background:transparent;text-align:left}
 .search-result-title{display:flex;align-items:baseline;gap:10px;min-width:0}
 .search-result-title strong{flex:1;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-size:14px;font-weight:600;line-height:1.5}
 .search-result-reading{flex-shrink:0;color:var(--color-secondary);font-size:11px;line-height:1.5}
 .search-result-group p{display:-webkit-box;margin:3px 0 0;overflow:hidden;color:var(--color-secondary);font-size:12px;line-height:1.65;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow-wrap:anywhere}
 .search-result-bottom{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:6px;min-height:18px}
 .search-result-meta{display:flex;align-items:center;gap:5px;min-width:0;color:var(--color-muted);font-size:11px;line-height:18px}
 .search-result-format{flex-shrink:0;letter-spacing:.02em}
 .search-result-folder{overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-size:inherit;line-height:inherit}
 .search-result-reference{display:inline-flex;align-items:center;gap:3px;flex-shrink:0;margin-left:4px;color:var(--color-secondary)}
 .search-result-reference :global(.icon){width:11px;height:11px}
 .search-result-cited{display:inline-flex;gap:2px;font-variant-numeric:tabular-nums}
 .search-result-cited span{display:inline-flex;align-items:center;justify-content:center;min-width:15px;height:15px;padding:0 3px;border-radius:4px;background:color-mix(in srgb,var(--color-ink) 7%,transparent);color:var(--color-ink);font-size:10px;line-height:1}
 .search-result-more{display:inline-flex;align-items:center;gap:2px;flex-shrink:0;padding:0;border:0;border-radius:4px;background:transparent;color:var(--color-secondary);font-size:11px;line-height:18px;white-space:nowrap}
 .search-result-more:hover{background:transparent;color:var(--color-ink)}
 .search-result-more[aria-expanded="true"]{color:var(--color-ink)}
 .search-result-more:focus-visible{outline:2px solid var(--color-focus);outline-offset:3px}
 .search-result-more :global(.icon){width:12px;height:12px}
 @media(forced-colors:active){.search-result-group.selected{outline:1px solid Highlight}}
</style>
