<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import IndexHistoryEntry from './IndexHistoryEntry.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {indexStages} from '../lib/index-stages';
 import type {Root,IndexStatus,IndexHistory} from '../lib/types';
 export let roots:Root[]=[];
 export let onClose:()=>void;
 export let onPauseChange:(paused:boolean)=>void;
 let status:IndexStatus={paused:false,tasks:[]},loading=true,changing=false,error='';
 let history:IndexHistory[]=[],historyKey:string|undefined,clearing=false,historyRevision=0;
 let disposed=false,timer:ReturnType<typeof setTimeout>;
 const stageLabel=(stage:string)=>$t(indexStages.find(item=>item.stage===stage)?.progress??(stage==='indexing'?'正在索引文件':'正在扫描文件'));
 async function refresh(){
  try{
   const next=await api<IndexStatus>('get_index_status');
   if(disposed)return;
   status=next;onPauseChange(next.paused);
   const key=next.tasks.map(task=>task.path).join('\n');
   if(key!==historyKey&&!clearing){const revision=historyRevision;const records=await api<IndexHistory[]>('get_index_history');if(disposed||revision!==historyRevision)return;history=records;historyKey=key;}
   error='';
  }
  catch(e){if(!disposed)error=$t(String(e));}
  finally{if(!disposed){loading=false;timer=setTimeout(refresh,600);}}
 }
 onMount(()=>{void refresh();return()=>{disposed=true;clearTimeout(timer);};});
 async function togglePause(){
  changing=true;error='';
  try{status=await api<IndexStatus>('set_index_paused',{paused:!status.paused});onPauseChange(status.paused);}
  catch(e){error=$t(String(e));}
  finally{changing=false;}
 }
 async function clearHistory(){
  clearing=true;error='';historyRevision++;
  try{await api('clear_index_history');if(!disposed){history=[];historyKey=undefined;}}
  catch(e){if(!disposed)error=$t(String(e));}
  finally{if(!disposed)clearing=false;}
 }
 $:working=roots.some(root=>root.status==='pending'||root.status==='indexing')||status.tasks.length>0;
 $:finishing=status.paused&&status.tasks.some(task=>task.files.length>0);
 $:activeRoots=roots.filter(root=>root.status==='pending'||root.status==='indexing'||status.tasks.some(task=>task.path===root.path));
 $:issueRoots=roots.filter(root=>(root.status==='partial'||root.status==='error')&&!activeRoots.includes(root)&&!history.some(record=>record.path===root.path));
</script>

<Modal label={$t('索引详情')} class="index-issues-dialog" {onClose}>
 <div class="dialog-head"><h2>{$t('索引详情')}</h2><Button.Root class="icon-button" aria-label={$t('关闭索引详情')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="index-detail-summary">
  <span class="index-summary-state" aria-live="polite"><span class="index-summary-dot" aria-hidden="true"></span>{$t(loading?'正在读取索引状态…':finishing?'正在暂停…':status.paused?'索引已暂停':working?'正在索引':'索引已完成')}</span>
  <Button.Root class="outline index-pause-button" aria-label={$t(status.paused?'继续索引':'暂停索引')} disabled={loading||changing||!working&&!status.paused} onclick={togglePause}>{@html icon(status.paused?'play':'pause')}{$t(status.paused?'继续索引':'暂停索引')}</Button.Root>
 </div>
 <div class="dialog-body index-detail-body">
  {#if activeRoots.length}<section class="index-active" aria-label={$t('当前任务')}>
  <div class="index-section-heading"><h3>{$t('当前任务')}</h3></div>
  {#each activeRoots as root (root.path)}
   {@const task=status.tasks.find(task=>task.path===root.path)}
   <section class="index-root">
    <div class="index-root-title">{@html icon('folder')}<strong>{root.name}</strong><span>{status.paused?$t(task&&task.files.length?'正在暂停…':'索引已暂停'):task?stageLabel(task.stage):$t('等待处理')}</span></div>
    <p class="index-root-path" title={root.path}>{root.path}</p>
    {#if task}
     {@const step=indexStages.findIndex(item=>item.stage===task.stage)}
     <ol class="index-pipeline" aria-label={$t('索引流程')}>
      {#each indexStages as item,index}<li class:current={step===index} class:done={step>index} aria-current={step===index?'step':undefined}><span>{step>index?'✓':index+1}</span>{$t(item.title)}</li>{/each}
     </ol>
     <p class="index-stage-description">{step>=0?$t(indexStages[step].description):$t('先索引文本，再转换其他格式，最后索引转换结果。')}</p>
     {#if task.total>0}<div class="index-count">{$t('已处理 {value0} / {value1} 个文件',{value0:task.completed,value1:task.total})}</div><progress aria-label={$t('{value0} 的索引进度',{value0:root.name})} value={task.completed} max={task.total}></progress>{/if}
     {#if task.files.length}<ul class="index-current-files" aria-label={$t('正在处理的文件')}>{#each task.files as file}<li>{@html icon('file')}<span>{file}</span></li>{/each}</ul>{/if}
    {/if}
   </section>
  {/each}
  </section>{/if}
  <section class="index-history" aria-label={$t('索引历史')}>
   <div class="index-history-heading index-section-heading"><div class="index-history-title"><h3>{$t('索引历史')}</h3><Button.Root class="icon-button clear-index-history" aria-label={$t('清空索引记录')} title={$t('清空索引记录，仅删除历史，不影响已有索引和当前任务')} disabled={loading||clearing||!history.length} onclick={clearHistory}>{@html icon('delete')}</Button.Root></div><span>{$t('最近 100 次索引')}</span></div>
   {#each history as record (record.id)}
    <div class="index-root">
     <IndexHistoryEntry {record}/>
    </div>
   {/each}
   {#if !loading&&!history.length}<p class="small muted">{$t('暂无索引历史')}</p>{/if}
  </section>
  {#if issueRoots.length}<section class="index-issues" aria-label={$t('索引异常')}>
   <div class="index-section-heading"><h3>{$t('索引异常')}</h3></div>
   {#each issueRoots as root (root.path)}
    <div class="index-root">
     <div class="index-root-title">{@html icon('folder')}<strong>{root.name}</strong><span>{$t(root.status==='error'?'读取失败':root.status==='partial'?'部分失败':'已完成')}</span></div>
     <p class="index-root-path" title={root.path}>{root.path}</p>
     <p class="small muted">{$t('已索引 {value0} 个文件',{value0:root.count})}</p>
     {#if root.status==='partial'}<p class="small muted">{$t('以下文件未进入索引，其他文件可正常检索。')}</p>{/if}
     {#if root.error}<details class="index-failure"><summary>{$t('查看失败详情')}</summary><pre class="index-issue-details">{root.error.split('\n').map(line=>$t(line)).join('\n')}</pre></details>{/if}
    </div>
   {/each}
  </section>{/if}
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 </div>
 <div class="index-detail-footer">{@html icon('info')}<span>{$t('暂停后，正在处理的文件完成后停下；已索引内容仍可搜索。')}</span></div>
</Modal>

<style>
 :global(.dialog-panel.index-issues-dialog){width:min(620px,calc(100vw - 40px));max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head{padding:20px 24px 16px;flex-shrink:0}
 .dialog-head h2{font-size:18px;line-height:1.4;letter-spacing:0}
 .index-detail-summary{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 14px;margin:0 24px 20px;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-panel);font-size:13px;flex-shrink:0}
 .index-summary-state{display:flex;align-items:center;gap:8px;min-width:0;line-height:1.5;font-weight:500}
 .index-summary-dot{width:6px;height:6px;border-radius:50%;background:var(--color-secondary);flex-shrink:0}
 :global(.index-pause-button){display:flex;align-items:center;gap:6px;white-space:nowrap;font-size:12px;min-height:30px;padding:5px 10px;flex-shrink:0}
 :global(.index-pause-button .icon){width:13px;height:13px}
 .index-detail-body{min-height:0;overflow:auto;overscroll-behavior:contain;padding:0 24px 20px;gap:16px}
 .index-active,.index-history,.index-issues{border:1px solid var(--color-rule);border-radius:11px;background:var(--color-paper);overflow:hidden;flex-shrink:0}
 .index-section-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:11px 16px;background:var(--color-sidebar);border-bottom:1px solid var(--color-rule)}
 .index-section-heading h3{margin:0;font-size:13px;font-weight:600;line-height:1.5;color:var(--color-ink)}
 .index-section-heading>span{font-size:12px;color:var(--color-muted)}
 .index-history-title{display:flex;align-items:center;gap:6px}
 :global(.clear-index-history.icon-button){width:24px;height:24px;padding:4px;color:var(--color-muted);border-radius:5px}
 :global(.clear-index-history>.icon){width:14px;height:14px}
 :global(.clear-index-history:hover:not(:disabled)){color:var(--color-danger);background:var(--color-hover)}
 .index-root{padding:14px 16px;border-bottom:1px solid var(--color-rule)}
 .index-root:last-of-type{border-bottom:0}
 .index-history>.small{padding:0 16px 14px}
 .index-root-title{display:flex;align-items:center;gap:8px;font-size:13px;line-height:1.5}
 .index-root-title :global(.icon){width:16px;height:16px;flex-shrink:0;color:var(--color-secondary)}
 .index-root-title strong{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:550}
 .index-root-title>span{margin-left:auto;padding:2px 7px;border-radius:5px;background:var(--color-panel);white-space:nowrap;color:var(--color-secondary);font-size:12px;flex-shrink:0}
 .index-root-path{margin:5px 0 0;font-size:12px;line-height:1.6;color:var(--color-muted);overflow-wrap:anywhere}
 .index-count{margin:10px 0 7px;font-size:12px;line-height:1.6;color:var(--color-secondary);font-variant-numeric:tabular-nums}
 .index-pipeline{display:grid;grid-template-columns:1fr 1fr 1.4fr;list-style:none;padding:0;margin:14px 0 0;border:1px solid var(--color-rule);border-radius:7px;overflow:hidden;background:var(--color-panel)}
 .index-pipeline li{display:flex;align-items:center;justify-content:center;gap:6px;padding:9px 5px;font-size:12px;line-height:1.5;color:var(--color-muted);border-right:1px solid var(--color-rule)}
 .index-pipeline li:last-child{border-right:0}
 .index-pipeline li.current{color:var(--color-ink);font-weight:550;background:var(--color-selected)}
 .index-pipeline li.done{color:var(--color-secondary)}
 .index-pipeline li>span{font-variant-numeric:tabular-nums;font-size:11px}
 .index-stage-description{margin:8px 0 0;font-size:12px;line-height:1.6;color:var(--color-muted)}
 .index-detail-body .small{font-size:12px;line-height:1.6;margin-top:8px;margin-bottom:0}
 progress{display:block;width:100%;height:4px;border:0;border-radius:3px;appearance:none;background:var(--color-rule);overflow:hidden}
 progress::-webkit-progress-bar{background:var(--color-rule)}
 progress::-webkit-progress-value{background:var(--color-ink)}
 progress::-moz-progress-bar{background:var(--color-ink)}
 .index-current-files{max-height:120px;overflow:auto;list-style:none;padding:10px;margin:12px 0 0;border:1px solid var(--color-rule);border-radius:7px;background:var(--color-paper);display:flex;flex-direction:column;gap:7px}
 .index-current-files li{display:flex;gap:7px;align-items:flex-start;color:var(--color-secondary);font-size:12px;line-height:1.6}
 .index-current-files :global(.icon){width:13px;height:13px;flex-shrink:0;margin-top:3px}
 .index-current-files span{overflow-wrap:anywhere}
 .index-failure{margin-top:8px}
 .index-failure summary{font-size:12px;color:var(--color-secondary);cursor:pointer}
 .index-failure pre{font-size:12px;margin-top:8px}
 .index-detail-footer{display:flex;align-items:flex-start;gap:8px;padding:12px 24px;border-top:1px solid var(--color-rule);background:var(--color-panel);font-size:12px;line-height:1.6;color:var(--color-muted);flex-shrink:0}
 .index-detail-footer :global(.icon){width:14px;height:14px;margin-top:3px;flex-shrink:0}
</style>
