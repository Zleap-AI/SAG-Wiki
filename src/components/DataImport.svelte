<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import type {ChatSource} from '../lib/types';
 import {noteSources,type NoteSource} from '../lib/note-import';
 import ChatImport from './ChatImport.svelte';
 import NoteImport from './NoteImport.svelte';
 import ShareSettings from './ShareSettings.svelte';
 import ShareDialog from './ShareDialog.svelte';
 import OnlineImport from './OnlineImport.svelte';
 import {onlinePlatforms,type OnlinePlatform} from '../lib/online-import';
 import {icon} from '../lib/icons';
 import {web} from '../lib/web-connection';
 export let showSharing=true;
 export let onShareReceived:((path:string)=>void)|undefined=undefined;
 let receiving=false;
 export let onImported:(()=>Promise<void>)|undefined=undefined;
 export let onDestinationChange:((folder:string)=>void)|undefined=undefined;
 let chatSource:ChatSource|null=null,noteSource:NoteSource|null=null;
 const platformOrder:OnlinePlatform[]=['notion','feishu','lark','yuque','rss'];
 let onlinePlatform:OnlinePlatform|null=null;
 function openNotionZip(){onlinePlatform=null;noteSource='notion';}
</script>
{#if !web}
 <h3 class="settings-section-title">{$t('第三方应用')}</h3>
 <section class="data-import-sources">
  {#each [{source:'claude' as const,name:'Claude Code'},{source:'codex' as const,name:'Codex'}] as item}
   <div class="data-import-row"><strong>{item.name}</strong><Button.Root type="button" class="outline" aria-label={$t('导入 {value0} 聊天记录',{value0:item.name})} onclick={()=>chatSource=item.source}>{$t('导入聊天记录')}</Button.Root></div>
  {/each}
  {#each Object.entries(noteSources).filter(([id])=>id!=='notion') as [id,item]}
   <div class="data-import-row"><strong>{item.name}</strong><Button.Root type="button" class="outline" aria-label={$t('从 {value0} 导入',{value0:item.name})} onclick={()=>noteSource=id as NoteSource}>{$t('导入笔记')}</Button.Root></div>
  {/each}
 </section>
{/if}
<h3 class="settings-section-title">{$t('在线数据源')}</h3>
<section class="data-import-sources">
 {#each platformOrder as platform}{@render onlineRow(platform)}{/each}
</section>
{#if showSharing&&!web}<ShareSettings onReceive={()=>receiving=true}/>{/if}
{#snippet onlineRow(platform:OnlinePlatform)}
 <div class="data-import-row"><div class="data-import-name"><strong>{$t(onlinePlatforms[platform])}</strong><Button.Root type="button" class="data-import-clock" aria-label={$t('配置 {value0} 定时同步',{value0:$t(onlinePlatforms[platform])})} title={$t('支持定时同步')} onclick={()=>onlinePlatform=platform}>{@html icon('clock')}</Button.Root></div><Button.Root type="button" class="outline" aria-label={$t('配置 {value0}',{value0:$t(onlinePlatforms[platform])})} onclick={()=>onlinePlatform=platform}>{$t('配置')}</Button.Root></div>
{/snippet}
{#if receiving}<ShareDialog onClose={()=>receiving=false} onReceived={path=>onShareReceived?.(path)}/>{/if}
{#if chatSource}<ChatImport source={chatSource} onClose={()=>chatSource=null} onImported={async()=>{await onImported?.()}} {onDestinationChange}/>{/if}
{#if noteSource}<NoteImport source={noteSource} onClose={()=>noteSource=null} onImported={async()=>{await onImported?.()}}/>{/if}
{#if onlinePlatform}<OnlineImport platform={onlinePlatform} onClose={()=>onlinePlatform=null} onImported={async()=>{await onImported?.()}} onNotionZip={openNotionZip}/>{/if}
<style>
 .data-import-sources{margin-bottom:18px;background:var(--color-paper);border:1px solid var(--color-rule);border-radius:11px;overflow:hidden}
 .data-import-row{display:flex;align-items:center;justify-content:space-between;gap:24px;min-height:61px;padding:15px 17px;border-bottom:1px solid var(--color-rule)}
 .data-import-row:last-child{border-bottom:0}
 strong{font-size:var(--text-ui);font-weight:500;line-height:1.5}
 .data-import-row :global(button){flex-shrink:0}
 .data-import-name{display:flex;align-items:center;gap:6px}
 .data-import-name :global(.data-import-clock){display:grid;place-items:center;width:24px;height:24px;padding:4px;color:var(--color-muted);border:0;border-radius:5px;background:transparent;cursor:pointer}
 .data-import-name :global(.data-import-clock:hover){background:var(--color-hover);color:var(--color-ink)}
 .data-import-name :global(.data-import-clock svg){width:14px;height:14px}
</style>
