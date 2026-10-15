<script lang="ts">
 import {Tabs,Button} from 'bits-ui';
 import ExploreView from './ExploreView.svelte';
 import {t} from '../lib/i18n';import {icon} from '../lib/icons';import {basename,dirname} from '../lib/domain';
 import type {Entry} from '../lib/types';import type {LibrarySession} from '../lib/explore';
 import '../styles/explore.css';
 export let scope:string;export let session:LibrarySession;export let entries:Entry[];export let isRoot:boolean;export let reading:boolean;export let revision:number;
 export let onbrowse:(path:string)=>void;export let onopen:(path:string,terms:string[])=>void;export let oncreate:()=>void;export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void;
 function restoreScroll(node:HTMLElement){node.scrollTop=session.fileScroll;return{destroy(){session.fileScroll=node.scrollTop}}}
</script>
<div class="library-workspace">
 <header class="topbar library-topbar"><div class="breadcrumb">{#if !isRoot}<Button.Root class="icon-button" aria-label={$t("返回上一层")} onclick={()=>onbrowse(dirname(scope))}>{@html icon("back")}</Button.Root>{/if}<strong title={scope}>{basename(scope)||$t('文件夹')}</strong></div>
  <Tabs.Root value={session.mode} onValueChange={(value)=>{session.mode=value as 'files'|'explore'}}>
   <Tabs.List class="library-tabs" aria-label={$t('知识库视图')}><Tabs.Trigger value="files" data-files-tab>{$t('文件')}</Tabs.Trigger><Tabs.Trigger value="explore" data-explore-tab>{$t('探索')}</Tabs.Trigger></Tabs.List>
  </Tabs.Root><div class="library-top-action">{#if scope&&session.mode==='files'}<Button.Root class="primary" onclick={oncreate}>{$t('新建文档')}</Button.Root>{/if}</div>
 </header>
 {#if session.mode==='explore'}<ExploreView {scope} {session} {revision} {onopen}/>{:else}
 <section class="page-wrap folder-browser library-files" use:restoreScroll onscroll={(e)=>{session.fileScroll=e.currentTarget.scrollTop}}>
  <div class="page-heading"><div><h1>{scope?basename(scope):$t('文件夹')}</h1><p class="folder-count">{$t('{value0} 个文件夹 · {value1} 个文件',{value0:entries.filter(e=>e.directory).length,value1:entries.filter(e=>!e.directory).length})}</p></div></div>
  <div class="simple-file-list">{#if scope&&!isRoot}<Button.Root class="simple-file-row folder-parent" onclick={()=>onbrowse(dirname(scope))}>{@html icon('back')}<span>{$t('返回上一层')}</span><small>{basename(dirname(scope))}</small></Button.Root>{/if}
   {#each entries as entry}<Button.Root class="simple-file-row" title={entry.path} oncontextmenu={(e:MouseEvent)=>{e.preventDefault();onmenu(e,entry.path,false,!entry.directory)}} onclick={()=>onbrowse(entry.path)}>{@html icon(entry.directory?'folder':'file')}<span>{entry.name}</span>{#if entry.directory}{@html icon('chevron')}{/if}</Button.Root>{/each}
  </div>
  {#if reading}<div class="empty" role="status">{$t('正在读取文件夹…')}</div>{:else if !entries.length}<div class="empty">{scope?$t('文件夹是空的'):$t('在左侧选择一个文件夹')}</div>{/if}
 </section>{/if}
</div>
