<script lang="ts">
 import {Button} from 'bits-ui';
 import ExploreView from './ExploreView.svelte';
 import LibraryStats from './LibraryStats.svelte';
 import Modal from './ui/Modal.svelte';
 import {t} from '../lib/i18n';import {icon} from '../lib/icons';import {basename,displayPath} from '../lib/domain';
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
 export let onSettings:(()=>void)|undefined=undefined;
 let conversionPrompt=false,fileFilter='all',promptPath='';
 // Index issues are reported per root as "path\nerror" pairs; find this file's reason.
 function issueFor(path:string):string{
  for(const root of roots){
   if(!root.error||!path.startsWith(root.path))continue;
   const lines=root.error.split('\n');
   const at=lines.indexOf(path);
   if(at>=0&&at+1<lines.length)return lines[at+1];
  }
  return '';
 }
 $:promptIssue=promptPath?issueFor(promptPath):'';
 $:promptNeedsOcr=/OCR/.test(promptIssue);
 export let onbrowse:(path:string)=>void;export let onopen:(path:string,terms:string[])=>void;export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void;
 $:indexedPaths=new Set(indexedFiles.map(file=>file.path));
 $:visibleEntries=entries.filter(entry=>!scope||entry.directory||fileFilter==='all'||(fileFilter==='indexed'?indexedPaths.has(entry.path):!indexedPaths.has(entry.path)));
 function restoreScroll(node:HTMLElement){node.scrollTop=session.fileScroll;return{destroy(){session.fileScroll=node.scrollTop}}}
</script>
<div class="library-workspace">
 {#if session.mode==='explore'}<div class="library-explore-heading"><Button.Root class="library-mode-back" onclick={()=>{session.mode='files';onModeChange?.()}}>{@html icon('back')}{$t('返回文件树模式')}</Button.Root><h1>{basename(scope)}</h1>{#if oncreate}<Button.Root class="outline library-folder-action" onclick={oncreate}>{@html icon('plus')}{$t('新建文档')}</Button.Root>{/if}</div><ExploreView {scope} {session} {revision} {onopen}/>{:else}
 <section class="page-wrap folder-browser library-files" use:restoreScroll onscroll={(e)=>{session.fileScroll=e.currentTarget.scrollTop}}>
  <div class="page-heading"><div class="library-folder-heading"><div class="library-folder-title"><h1>{scope?basename(scope):$t('已索引文件')}</h1>{#if scope}<Button.Root type="button" class="library-explore" onclick={()=>{session.mode='explore';onModeChange?.()}}>{$t('探索模式')}</Button.Root>{/if}{#if scope?oncreate:onadd}<Button.Root class="outline library-folder-action" onclick={scope?oncreate:onadd}>{@html icon('plus')}{$t(scope?'新建文档':'添加文件夹')}</Button.Root>{/if}</div><div class="library-folder-meta"><p class="folder-count">{!scope&&totalFiles!==undefined?$t('{value0} 个文件夹 · {value1} 个已索引文件',{value0:entries.length,value1:totalFiles}):$t('{value0} 个文件夹 · {value1} 个文件',{value0:entries.filter(e=>e.directory).length,value1:visibleEntries.filter(e=>!e.directory).length})}</p>{#if scope}<Button.Root type="button" class="library-file-filter" aria-label={$t('按索引状态筛选')} onclick={()=>fileFilter=fileFilter==='all'?'indexed':fileFilter==='indexed'?'unindexed':'all'}><span>{$t(fileFilter==='all'?'全部':fileFilter==='indexed'?'已索引':'未索引')}</span>{@html icon('switch')}</Button.Root>{/if}</div></div></div>
  {#if !scope}<LibraryStats files={indexedFiles} {roots} {revision} onDetails={onIndexDetails} {onPauseChange}/>{/if}
  <div class="simple-file-list">
   {#if scope}<Button.Root class="simple-file-row library-parent" aria-label={$t('返回上层')} onclick={()=>onbrowse(parent)}>{@html icon('back')}<span>{$t('返回上层')}</span></Button.Root>{/if}
   {#each visibleEntries as entry}{@const unindexed=!!scope&&!entry.directory&&!indexedPaths.has(entry.path)}<div class="simple-file-entry" class:unindexed role="presentation" oncontextmenu={(e:MouseEvent)=>{e.preventDefault();onmenu(e,entry.path,!scope,!entry.directory)}}><Button.Root class="simple-file-row" title={unindexed?undefined:displayPath(entry.path)} onclick={()=>{if(unindexed){promptPath=entry.path;conversionPrompt=true}else onbrowse(entry.path)}}>{@html icon(entry.directory?'folder':'file')}<span>{entry.name}</span>{#if entry.directory}{@html icon('chevron')}{/if}</Button.Root></div>{/each}
  </div>
  {#if reading}<div class="empty" role="status">{$t('正在读取文件夹…')}</div>{:else if !entries.length}<div class="empty">{scope?$t('文件夹是空的'):$t('添加文件夹并完成索引后，文件会显示在这里。')}</div>{:else if !visibleEntries.length}<div class="empty">{$t('没有符合条件的文件')}</div>{/if}
 </section>{/if}
</div>

{#if conversionPrompt}<Modal label={$t(promptNeedsOcr?'扫描件需要图片识别组件':'需要转换文档后才能检索')} onClose={()=>conversionPrompt=false}>
 <div class="dialog-head"><h2>{$t(promptNeedsOcr?'扫描件需要图片识别组件':'需要转换文档后才能检索')}</h2></div>
 <div class="dialog-body">{#if promptNeedsOcr}<p>{$t('这个文件没有文字层，需要安装图片识别（OCR）组件后才能转换。')}</p>{:else if promptIssue}<p>{$t('索引时出错：{value0}',{value0:$t(promptIssue)})}</p>{:else}<p>{$t('请前往文件设置，配置文件格式和文档转换。')}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" onclick={()=>conversionPrompt=false}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" onclick={()=>{conversionPrompt=false;onSettings?.()}}>{$t('去设置')}</Button.Root></div>
</Modal>{/if}
