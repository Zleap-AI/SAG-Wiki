<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {chatSources,type ArtifactSource,type ChatSource} from '../lib/types';
 import {noteSources,type NoteSource} from '../lib/note-import';
 import ChatImport from './ChatImport.svelte';
 import AgentArtifacts from './AgentArtifacts.svelte';
 import NoteImport from './NoteImport.svelte';
 import ShareSettings from './ShareSettings.svelte';
 import ShareDialog from './ShareDialog.svelte';
 import OnlineImport from './OnlineImport.svelte';
 import ApifyImport from './ApifyImport.svelte';
 import {onlinePlatforms,type OnlinePlatform} from '../lib/online-import';
 import {icon} from '../lib/icons';
 import {web} from '../lib/web-connection';
 import claudeLogo from '../assets/data-sources/claude.webp';
 import codexLogo from '../assets/data-sources/codex.webp';
 import workbuddyLogo from '../assets/data-sources/workbuddy.png';
 import evernoteLogo from '../assets/data-sources/evernote.webp';
 import keepLogo from '../assets/data-sources/keep.webp';
 import appleLogo from '../assets/data-sources/apple.webp';
 import notionLogo from '../assets/data-sources/notion.webp';
 import feishuLogo from '../assets/data-sources/feishu.webp';
 import larkLogo from '../assets/data-sources/lark.webp';
 import yuqueLogo from '../assets/data-sources/yuque.webp';
 import githubLogo from '../assets/data-sources/github.webp';
 import gitlabLogo from '../assets/data-sources/gitlab.webp';
 import dingtalkLogo from '../assets/data-sources/dingtalk.webp';
 import imaLogo from '../assets/data-sources/ima.webp';
 import apifyLogo from '../assets/data-sources/apify.webp';
 export let showSharing=true;
 export let onShareReceived:((path:string)=>void)|undefined=undefined;
 let receiving=false;
 let artifactSource:{id:ArtifactSource;name:string}|null=null;
 export let onImported:(()=>Promise<void>)|undefined=undefined;
 export let onOpenImportedFolder:((path:string)=>Promise<void>)|undefined=undefined;
 export let onDestinationChange:((folder:string)=>void)|undefined=undefined;
 let chatSource:ChatSource|null=null,noteSource:NoteSource|null=null;
 type ScheduledPlatform=OnlinePlatform|'apify';
 const platformOrder:ScheduledPlatform[]=['notion','feishu','lark','yuque','github','gitlab','dingtalk','ima','rss','api','apify'];
 let onlinePlatform:ScheduledPlatform|null=null;
 const sourceLogos={
  claude:claudeLogo,codex:codexLogo,claude_desktop:claudeLogo,workbuddy:workbuddyLogo,evernote:evernoteLogo,keep:keepLogo,apple:appleLogo,
  notion:notionLogo,feishu:feishuLogo,lark:larkLogo,yuque:yuqueLogo,github:githubLogo,gitlab:gitlabLogo,
  dingtalk:dingtalkLogo,ima:imaLogo,apify:apifyLogo,
 } satisfies Record<ChatSource|NoteSource|Exclude<OnlinePlatform,'rss'|'api'>|'apify',string>;
 const agents:{id:ArtifactSource;name:string;chat:ChatSource}[]=[
  {id:'claude-code',name:chatSources.claude.name,chat:'claude'},
  {id:'claude-desktop',name:chatSources.claude_desktop.name,chat:'claude_desktop'},
  {id:'codex',name:chatSources.codex.name,chat:'codex'},
  {id:'workbuddy',name:chatSources.workbuddy.name,chat:'workbuddy'},
 ];
 function openNotionZip(){onlinePlatform=null;noteSource='notion';}
</script>
 <h3 class="settings-section-title">Agent</h3>
 <section class="data-import-sources">
  {#each agents as item (item.id)}
   <div class="data-import-row">
    <div class="data-import-name">{@render sourceName(item.chat,item.name)}</div>
    <div class="data-import-actions">
     {#if !web}<Button.Root type="button" class="outline" aria-label={$t('导入 {value0} 聊天记录',{value0:item.name})} onclick={()=>chatSource=item.chat}>{$t('导入聊天记录')}</Button.Root>{/if}
     <Button.Root type="button" class="outline" aria-label={$t('导入 {value0} 产物',{value0:item.name})} onclick={()=>artifactSource=item}>{$t('导入产物')}</Button.Root>
    </div>
   </div>
  {/each}
 </section>
<h3 class="settings-section-title">{$t('在线数据源')}</h3>
<section class="data-import-sources">
 {#each platformOrder as platform}{@render onlineRow(platform)}{/each}
</section>
{#if !web}
 <h3 class="settings-section-title">{$t('第三方应用')}</h3>
 <section class="data-import-sources">
  {#each Object.entries(noteSources).filter(([id])=>id!=='notion') as [id,item]}
   <div class="data-import-row"><div class="data-import-name">{@render sourceName(id as NoteSource,item.name)}</div><Button.Root type="button" class="outline" aria-label={$t('从 {value0} 导入',{value0:item.name})} onclick={()=>noteSource=id as NoteSource}>{$t('导入笔记')}</Button.Root></div>
  {/each}
 </section>
{/if}
{#if showSharing&&!web}<ShareSettings onReceive={()=>receiving=true}/>{/if}
{#snippet sourceName(source:keyof typeof sourceLogos|'rss'|'api',name:string)}
 {#if source==='rss'||source==='api'}
  <span class="data-import-logo" aria-hidden="true">{@html icon(source==='rss'?'rss':'codeBlock')}</span>
 {:else}
  <img class="data-import-logo" class:monochrome={source==='github'} src={sourceLogos[source]} width="18" height="18" alt="" draggable="false" />
 {/if}
 <strong>{name}</strong>
{/snippet}
{#snippet onlineRow(platform:ScheduledPlatform)}
 {@const name=platform==='apify'?'Apify':$t(onlinePlatforms[platform])}
 <div class="data-import-row"><div class="data-import-name">{@render sourceName(platform,name)}<Button.Root type="button" class="data-import-clock" aria-label={$t('配置 {value0} 定时同步',{value0:name})} title={$t('支持定时同步')} onclick={()=>onlinePlatform=platform}>{@html icon('clock')}</Button.Root></div><Button.Root type="button" class="outline" aria-label={$t('配置 {value0}',{value0:name})} onclick={()=>onlinePlatform=platform}>{$t('配置')}</Button.Root></div>
{/snippet}
{#if receiving}<ShareDialog onClose={()=>receiving=false} onReceived={path=>onShareReceived?.(path)}/>{/if}
{#if artifactSource}<AgentArtifacts agent={artifactSource.id} name={artifactSource.name} onClose={()=>artifactSource=null} {onImported}/>{/if}
{#if chatSource}<ChatImport source={chatSource} onClose={()=>chatSource=null} onImported={async()=>{await onImported?.()}} {onDestinationChange}/>{/if}
{#if noteSource}<NoteImport source={noteSource} onClose={()=>noteSource=null} onImported={async()=>{await onImported?.()}}/>{/if}
{#if onlinePlatform==='apify'}<ApifyImport onClose={()=>onlinePlatform=null} {onImported}/>
{:else if onlinePlatform}<OnlineImport platform={onlinePlatform} onClose={()=>onlinePlatform=null} onImported={async()=>{await onImported?.()}} {onOpenImportedFolder} onNotionZip={openNotionZip}/>{/if}
<style>
 .data-import-sources{margin-bottom:18px;background:var(--color-paper);border:1px solid var(--color-rule);border-radius:11px;overflow:hidden}
 .data-import-row{display:flex;align-items:center;justify-content:space-between;gap:24px;min-height:61px;padding:15px 17px;border-bottom:1px solid var(--color-rule)}
 .data-import-row:last-child{border-bottom:0}
 strong{font-size:var(--text-ui);font-weight:500;line-height:1.5}
 .data-import-row :global(button){flex-shrink:0}
 .data-import-name{display:flex;align-items:center;gap:6px}
 .data-import-actions{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px}
 .data-import-logo{display:grid;place-items:center;flex:0 0 18px;width:18px;height:18px;margin-inline-end:3px;object-fit:contain;color:var(--color-secondary)}
 .data-import-logo :global(svg){width:16px;height:16px}
 :global([data-theme=dark]) .data-import-logo.monochrome{filter:invert(1)}
 .data-import-name :global(.data-import-clock){display:grid;place-items:center;width:24px;height:24px;padding:4px;color:var(--color-muted);border:0;border-radius:5px;background:transparent;cursor:pointer}
 .data-import-name :global(.data-import-clock:hover){background:var(--color-hover);color:var(--color-ink)}
 .data-import-name :global(.data-import-clock svg){width:14px;height:14px}
</style>
