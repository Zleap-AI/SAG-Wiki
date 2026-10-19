<script lang="ts">
 import {Button} from 'bits-ui';
 import ExploreView from './ExploreView.svelte';
 import LibraryStats from './LibraryStats.svelte';
 import {t} from '../lib/i18n';import {icon} from '../lib/icons';import {basename} from '../lib/domain';
 import type {Entry,FileInfo,Root} from '../lib/types';import type {LibrarySession} from '../lib/explore';
 import '../styles/explore.css';
 export let scope:string;export let session:LibrarySession;export let entries:Entry[];export let reading:boolean;export let revision:number;
 export let totalFiles:number|undefined=undefined;
 export let indexedFiles:FileInfo[]=[];
 export let roots:Root[]=[];
 export let onIndexDetails:(()=>void)|undefined=undefined;
 export let onPauseChange:((paused:boolean)=>void)|undefined=undefined;
 export let parent='';
 export let oncreate:(()=>void)|undefined=undefined;
 export let onadd:(()=>void)|undefined=undefined;
 export let onModeChange:(()=>void)|undefined=undefined;
 export let onbrowse:(path:string)=>void;export let onopen:(path:string,terms:string[])=>void;export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void;
 function restoreScroll(node:HTMLElement){node.scrollTop=session.fileScroll;return{destroy(){session.fileScroll=node.scrollTop}}}
</script>
<div class="library-workspace">
 {#if session.mode==='explore'}<div class="library-explore-heading"><Button.Root class="library-mode-back" onclick={()=>{session.mode='files';onModeChange?.()}}>{@html icon('back')}{$t('返回文件树模式')}</Button.Root><h1>{basename(scope)}</h1>{#if oncreate}<Button.Root class="outline library-folder-action" onclick={oncreate}>{@html icon('plus')}{$t('新建文档')}</Button.Root>{/if}</div><ExploreView {scope} {session} {revision} {onopen}/>{:else}
 <section class="page-wrap folder-browser library-files" use:restoreScroll onscroll={(e)=>{session.fileScroll=e.currentTarget.scrollTop}}>
  <div class="page-heading"><div class="library-folder-heading"><div class="library-folder-title"><h1>{scope?basename(scope):$t('已索引文件')}</h1>{#if scope}<Button.Root type="button" class="library-explore" onclick={()=>{session.mode='explore';onModeChange?.()}}>{$t('探索模式')}</Button.Root>{/if}{#if scope?oncreate:onadd}<Button.Root class="outline library-folder-action" onclick={scope?oncreate:onadd}>{@html icon('plus')}{$t(scope?'新建文档':'添加文件夹')}</Button.Root>{/if}</div><p class="folder-count">{!scope&&totalFiles!==undefined?$t('{value0} 个文件夹 · {value1} 个已索引文件',{value0:entries.length,value1:totalFiles}):$t('{value0} 个文件夹 · {value1} 个文件',{value0:entries.filter(e=>e.directory).length,value1:entries.filter(e=>!e.directory).length})}</p></div></div>
  {#if !scope}<LibraryStats files={indexedFiles} {roots} {revision} onDetails={onIndexDetails} {onPauseChange}/>{/if}
  <div class="simple-file-list">
   {#if scope}<Button.Root class="simple-file-row library-parent" aria-label={$t('返回上层')} onclick={()=>onbrowse(parent)}>{@html icon('back')}<span>{$t('返回上层')}</span></Button.Root>{/if}
   {#each entries as entry}<div class="simple-file-entry" role="presentation" oncontextmenu={(e:MouseEvent)=>{e.preventDefault();onmenu(e,entry.path,!scope,!entry.directory)}}><Button.Root class="simple-file-row" title={entry.path} onclick={()=>onbrowse(entry.path)}>{@html icon(entry.directory?'folder':'file')}<span>{entry.name}</span>{#if entry.directory}{@html icon('chevron')}{/if}</Button.Root></div>{/each}
  </div>
  {#if reading}<div class="empty" role="status">{$t('正在读取文件夹…')}</div>{:else if !entries.length}<div class="empty">{scope?$t('文件夹是空的'):$t('添加文件夹并完成索引后，文件会显示在这里。')}</div>{/if}
 </section>{/if}
</div>
