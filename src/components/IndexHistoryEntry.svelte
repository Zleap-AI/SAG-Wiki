<script lang="ts">
 import {onDestroy,onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {t,language} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {basename,displayPath} from '../lib/domain';
 import {indexErrorHint} from '../lib/index-stages';
 import type {IndexHistory,IndexHistoryLog,IndexFileLog} from '../lib/types';
 export let record:IndexHistory;
 let expanded=false,loading=false,loaded=false,showLog=false,disposed=false,error='',openError='';
 let raw:string|null=null,rows:{raw:string;file:IndexFileLog}[]=[];
 const date=(time:number)=>new Date(time).toLocaleString($language==='en'?'en-US':'zh-CN');
 $:files=[...new Map(rows.map(row=>[row.file.path,row.file])).values()];
 $:failedFiles=files.filter(file=>file.status==='failed');
 $:label=$t(record.status==='ready'?'已完成':record.status==='partial'?'部分失败':record.status==='stopped'?'已停止':'读取失败');
 $:hint=indexErrorHint(record.error);
 $:failed=record.status==='partial'||record.status==='error';
 // One sentence per run: how many files missed, the recognized cause, and what still works.
 $:notice=record.status==='partial'?[failedFiles.length?$t('{value0} 个文件未进入索引。',{value0:failedFiles.length}):'',hint?$t(hint):'',$t('其他文件可正常检索。')].filter(Boolean).join($language==='en'?' ':''):record.error?$t(hint??'索引未能完成，查看文件记录了解详情。'):'';
 function when(time:number){const day=new Date(time),today=new Date(),pad=(n:number)=>String(n).padStart(2,'0'),clock=`${pad(day.getHours())}:${pad(day.getMinutes())}`,start=new Date(today.getFullYear(),today.getMonth(),today.getDate()).getTime();return time>=start?$t('今天 {value0}',{value0:clock}):time>=start-86400000?$t('昨天 {value0}',{value0:clock}):`${day.getFullYear()}-${pad(day.getMonth()+1)}-${pad(day.getDate())} ${clock}`}
 onMount(()=>{if(record.status==='partial')void loadFiles();});
 onDestroy(()=>{disposed=true});
 function toggle(){
  expanded=!expanded;
  if(expanded)void loadFiles();
 }
 async function loadFiles(){
  if(loaded||loading)return;
  loading=true;error='';
  try{
   const log=await api<IndexHistoryLog>('get_index_history_log',{id:record.id});
   if(disposed)return;
   raw=log.raw;
   rows=raw?raw.split('\n').map(line=>({raw:line,file:JSON.parse(line) as IndexFileLog})):[];
   loaded=true;
  }catch(e){if(!disposed)error=$t(String(e));}
  finally{if(!disposed)loading=false;}
 }
 async function openFile(path:string,reveal=false){
  openError='';
  try{if(reveal)await api('reveal_path',{path});else await api('open_index_history_file',{id:record.id,path});}
  catch(e){if(!disposed)openError=$t(String(e));}
 }
 function viewLog(){openError='';showLog=true;}
</script>

<Button.Root class="index-history-toggle" aria-expanded={expanded} aria-controls={`index-files-${record.id}`} onclick={toggle}>
 <span class="index-entry-icon" class:failed>{@html icon(failed?'alert':'folder')}</span>
 <span class="index-entry-main">
  <span class="index-root-title"><strong>{record.name}</strong><span class:failed>{label}</span></span>
  <span class="index-entry-meta"><span class="index-count">{$t('已处理 {value0} / {value1} 个文件',{value0:record.completed,value1:record.total})}</span><span class="index-root-path" title={displayPath(record.path)}>{displayPath(record.path)}</span></span>
 </span>
 <span class="index-entry-side"><time datetime={new Date(record.started_at).toISOString()} title={date(record.started_at)}>{when(record.started_at)}</time><span class="index-entry-chevron" class:open={expanded}>{@html icon('chevron')}</span></span>
</Button.Root>
{#if notice}<div class="index-entry-notice">
 <p>{notice}</p>
 {#if record.error}<details class="index-failure" on:toggle={event=>{if(event.currentTarget.open)void loadFiles();}}><summary>{failedFiles.length?$t('查看 {value0} 个失败文件',{value0:failedFiles.length}):$t('查看失败详情')}</summary>
  {#if loading}<p class="index-note" role="status">{$t('正在读取文件记录…')}</p>{/if}
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  {#if failedFiles.length}<ul class="index-failed-files" aria-label={$t('未能索引的文件')}>{#each failedFiles as file(file.path)}<li><Button.Root class="index-history-file index-failed-file" title={$t('在访达或文件管理器中定位')} aria-label={$t('定位文件 {value0}',{value0:basename(file.path)})} onclick={()=>openFile(file.path,true)}>{@html icon('file')}<span><strong>{basename(file.path)}</strong><span class="index-file-path">{displayPath(file.path)}</span></span>{@html icon('forward')}</Button.Root></li>{/each}</ul>{/if}
  {#if openError}<p class="form-error" role="alert">{openError}</p>{/if}
  <p class="index-tech-label">{$t('技术详情')}</p>
  <pre>{record.error.split('\n').map(line=>$t(displayPath(line))).join('\n')}</pre>
 </details>{/if}
</div>{/if}
{#if expanded}
 <div class="index-file-preview" id={`index-files-${record.id}`}>
  {#if loading}<p class="index-note" role="status">{$t('正在读取文件记录…')}</p>
  {:else if error}<p class="form-error" role="alert">{error}</p>
  {:else if loaded&&raw===null}<p class="index-note">{$t('此记录未保存文件明细')}</p>
  {:else if loaded}
   {#if files.length}
    <ul aria-label={$t('本次处理的原文件')}>
     {#each files.slice(0,10) as file (file.path)}
      <li><Button.Root class="index-history-file" title={displayPath(file.path)} onclick={()=>openFile(file.path)}>{@html icon('file')}<span><strong>{basename(file.path)}</strong><span class="index-file-path">{displayPath(file.path)}</span></span>{#if file.status==='failed'}<small>{$t('读取失败')}</small>{/if}</Button.Root></li>
     {/each}
    </ul>
   {:else}<p class="index-note">{$t('本次没有文件处理记录')}</p>{/if}
   <div class="index-preview-footer"><span>{$t('每次最多保留 100 条文件记录')}</span><Button.Root class="index-view-log" onclick={viewLog}>{$t('查看更多')}{@html icon('forward')}</Button.Root></div>
  {/if}
  {#if openError&&!showLog}<p class="form-error" role="alert">{openError}</p>{/if}
 </div>
{/if}

{#if showLog}
 <Modal label={$t('原始索引日志')} class="index-log-dialog" onClose={()=>showLog=false}>
  <div class="dialog-head"><h2>{$t('原始索引日志')}</h2><Button.Root class="icon-button" aria-label={$t('关闭索引日志')} onclick={()=>showLog=false}>{@html icon('close')}</Button.Root></div>
  <div class="index-log-meta"><strong>{record.name}</strong><span>{date(record.started_at)} · {label}</span><p>{displayPath(record.path)}</p></div>
  <div class="index-log-body">
   {#if rows.length}<pre aria-label={$t('原始索引日志内容')}>{#each rows as row}{@const path=JSON.stringify(row.file.path)}{@const start=row.raw.indexOf(path)}<span>{row.raw.slice(0,start)}<Button.Root class="index-log-path" title={$t('用系统默认应用打开原文件')} onclick={()=>openFile(row.file.path)}>{path}</Button.Root>{row.raw.slice(start+path.length)}</span>{'\n'}{/each}</pre>
   {:else}<p class="index-note">{$t('本次没有文件处理记录')}</p>{/if}
   {#if record.error}<pre class="index-log-error">{record.error}</pre>{/if}
   {#if openError}<p class="form-error" role="alert">{openError}</p>{/if}
  </div>
  <div class="index-log-footer">{$t('每次最多保留 100 条文件记录')} · {$t('点击文件路径，用系统默认应用打开')}</div>
 </Modal>
{/if}

<style>
 :global(.index-history-toggle){display:grid;grid-template-columns:30px minmax(0,1fr) auto;align-items:center;gap:12px;width:100%;min-height:0;padding:12px 14px;border:0;border-radius:0;background:transparent;text-align:left;color:var(--color-ink);white-space:normal;transition:background-color .12s}
 :global(.index-history-toggle:hover){background:var(--color-panel)}
 :global(.index-history-toggle:focus-visible){outline:2px solid var(--color-focus);outline-offset:-2px}
 .index-entry-icon{display:grid;place-items:center;width:30px;height:30px;border-radius:8px;background:var(--color-sidebar);color:var(--color-secondary)}
 .index-entry-icon.failed{background:var(--color-paper);border:1px solid var(--color-rule);color:var(--color-ink)}
 .index-entry-icon :global(.icon){width:15px;height:15px}
 .index-entry-main{display:flex;flex-direction:column;gap:3px;min-width:0}
 .index-root-title{display:flex;align-items:center;gap:8px;min-width:0;font-size:13px;line-height:1.45}
 .index-root-title strong{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:550}
 .index-root-title>span{flex-shrink:0;padding:1px 7px;border:1px solid transparent;border-radius:999px;background:var(--color-hover);white-space:nowrap;color:var(--color-secondary);font-size:11px;line-height:1.6}
 .index-root-title>span.failed{border-color:var(--color-rule);background:var(--color-paper);color:var(--color-ink);font-weight:550}
 .index-entry-meta{display:flex;align-items:baseline;gap:6px;min-width:0;font-size:12px;line-height:1.5;color:var(--color-muted)}
 .index-count{flex-shrink:0;font-variant-numeric:tabular-nums}
 .index-count::after{content:'·';margin-left:6px}
 .index-root-path{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .index-entry-side{display:flex;align-items:center;gap:6px;font-size:12px;color:var(--color-muted);white-space:nowrap;font-variant-numeric:tabular-nums}
 .index-entry-chevron{display:flex;transition:transform .15s}
 .index-entry-chevron.open{transform:rotate(90deg)}
 .index-entry-chevron :global(.icon){width:14px;height:14px}
 .index-entry-notice{margin:-4px 14px 12px 56px;padding:9px 12px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-panel);font-size:12px;line-height:1.6;color:var(--color-secondary)}
 .index-entry-notice>p{margin:0}
 .index-note{font-size:12px;line-height:1.6;color:var(--color-muted);margin:8px 0 0}
 .index-failure{margin-top:6px}
 .index-failure summary{display:inline-flex;align-items:center;gap:4px;font-size:12px;font-weight:500;color:var(--color-ink);cursor:pointer;list-style:none;text-decoration:underline;text-decoration-color:var(--color-rule);text-underline-offset:3px}
 .index-failure summary::-webkit-details-marker{display:none}
 .index-failure summary:hover{text-decoration-color:currentColor}
 .index-failure[open] summary{margin-bottom:8px}
 .index-tech-label{margin:10px 0 4px;font-size:11px;font-weight:550;letter-spacing:.2px;color:var(--color-muted)}
 .index-failure pre{margin:0;padding:8px 10px;max-height:160px;overflow:auto;border-radius:7px;background:var(--color-paper);border:1px solid var(--color-rule);font:11.5px/1.7 var(--font-mono);color:var(--color-secondary);white-space:pre-wrap;overflow-wrap:anywhere}
 .index-file-preview{margin:0 14px 12px 56px;padding:6px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-paper)}
 .index-file-preview>.index-note{margin:4px 6px}
 ul{list-style:none;padding:0;margin:0}
 .index-failed-files{display:flex;flex-direction:column;gap:2px;margin:0 -6px}
 :global(.index-history-file){width:100%;display:flex;align-items:center;gap:9px;min-height:0;padding:6px 8px;border:0;border-radius:6px;background:transparent;color:var(--color-ink);text-align:left;font-size:12px;line-height:1.45;white-space:normal}
 :global(.index-history-file:hover){background:var(--color-hover)}
 :global(.index-history-file .icon){width:14px;height:14px;flex-shrink:0;color:var(--color-muted)}
 :global(.index-history-file>span){min-width:0;flex:1}
 :global(.index-history-file strong){display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
 :global(.index-history-file .index-file-path){display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--color-muted)}
 :global(.index-failed-file>.icon:last-child){opacity:0;transition:opacity .12s}
 :global(.index-failed-file:hover>.icon:last-child){opacity:1}
 :global(.index-history-file small){margin-left:auto;padding:1px 6px;border:1px solid var(--color-rule);border-radius:999px;white-space:nowrap;color:var(--color-ink);font-size:11px;font-weight:500}
 .index-preview-footer{display:flex;align-items:center;justify-content:space-between;gap:12px;margin:4px 0 0;padding:8px 8px 2px;border-top:1px solid var(--color-rule);font-size:11px;color:var(--color-muted)}
 :global(.index-view-log){display:flex;align-items:center;gap:4px;min-height:0;background:transparent;border:0;padding:2px 0;color:var(--color-ink);font-size:12px;font-weight:500}
 :global(.index-view-log:hover){background:transparent;text-decoration:underline;text-underline-offset:3px}
 :global(.index-view-log .icon){width:12px;height:12px}
 :global(.dialog-panel.index-log-dialog){width:min(860px,calc(100vw - 40px));max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head{padding:20px 24px 16px;flex-shrink:0}
 .dialog-head h2{font-size:18px;line-height:1.4}
 .index-log-meta{margin:0 24px 16px;font-size:13px;line-height:1.6;flex-shrink:0}
 .index-log-meta span{margin-left:12px;color:var(--color-muted);font-size:12px}
 .index-log-meta p{font-size:12px;color:var(--color-muted);margin:4px 0 0;overflow-wrap:anywhere}
 .index-log-body{margin:0 24px 20px;padding:14px;min-height:100px;overflow:auto;background:var(--color-panel);border:1px solid var(--color-rule);border-radius:8px}
 .index-log-body pre{margin:0;font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-size:12px;line-height:1.8;white-space:pre-wrap;overflow-wrap:anywhere}
 :global(.index-log-path){display:inline;padding:0;border:0;background:transparent;font:inherit;color:var(--color-ink);text-decoration:underline;text-underline-offset:3px;word-break:break-all;white-space:pre-wrap;text-align:left;vertical-align:baseline}
 :global(.index-log-path:hover){color:var(--color-secondary)}
 .index-log-body .index-log-error{margin-top:12px;color:var(--color-danger)}
 .index-log-footer{padding:12px 24px;background:var(--color-panel);border-top:1px solid var(--color-rule);font-size:12px;color:var(--color-muted);flex-shrink:0;line-height:1.6}
</style>
