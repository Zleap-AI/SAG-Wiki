<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import type {ChatSource} from '../lib/types';
 import {noteSources,type NoteSource} from '../lib/note-import';
 import ChatImport from './ChatImport.svelte';
 import NoteImport from './NoteImport.svelte';
 import ShareSettings from './ShareSettings.svelte';
 import ShareDialog from './ShareDialog.svelte';
 export let showSharing=true;
 export let onShareReceived:((path:string)=>void)|undefined=undefined;
 let receiving=false;
 export let onImported:(()=>Promise<void>)|undefined=undefined;
 export let onDestinationChange:((folder:string)=>void)|undefined=undefined;
 let chatSource:ChatSource|null=null,noteSource:NoteSource|null=null;
</script>
<h3 class="settings-section-title">{$t('第三方应用')}</h3>
<section class="data-import-sources">
 {#each [{source:'claude' as const,name:'Claude Code'},{source:'codex' as const,name:'Codex'}] as item}
  <div class="data-import-row"><strong>{item.name}</strong><Button.Root type="button" class="outline" aria-label={$t('导入 {value0} 聊天记录',{value0:item.name})} onclick={()=>chatSource=item.source}>{$t('导入聊天记录')}</Button.Root></div>
 {/each}
 {#each Object.entries(noteSources) as [id,item]}
  <div class="data-import-row"><strong>{item.name}</strong><Button.Root type="button" class="outline" aria-label={$t('从 {value0} 导入',{value0:item.name})} onclick={()=>noteSource=id as NoteSource}>{$t('导入笔记')}</Button.Root></div>
 {/each}
</section>
{#if showSharing}<ShareSettings onReceive={()=>receiving=true}/>{/if}
{#if receiving}<ShareDialog onClose={()=>receiving=false} onReceived={path=>onShareReceived?.(path)}/>{/if}
{#if chatSource}<ChatImport source={chatSource} onClose={()=>chatSource=null} onImported={async()=>{await onImported?.()}} {onDestinationChange}/>{/if}
{#if noteSource}<NoteImport source={noteSource} onClose={()=>noteSource=null} onImported={async()=>{await onImported?.()}}/>{/if}
<style>
 .data-import-sources{margin-bottom:18px;background:var(--color-paper);border:1px solid var(--color-rule);border-radius:11px;overflow:hidden}
 .data-import-row{display:flex;align-items:center;justify-content:space-between;gap:24px;min-height:61px;padding:15px 17px;border-bottom:1px solid var(--color-rule)}
 .data-import-row:last-child{border-bottom:0}
 strong{font-size:var(--text-ui);font-weight:500;line-height:1.5}
 .data-import-row :global(button){flex-shrink:0}
</style>
