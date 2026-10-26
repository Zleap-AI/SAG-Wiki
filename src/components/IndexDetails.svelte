<script lang="ts">
 import {displayPath} from '../lib/domain';
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import IndexHistoryEntry from './IndexHistoryEntry.svelte';
 import {api} from '../lib/api';
 import {subscribeDataChanges} from '../lib/data-changes';
 import {sameIndexStatus,stableRows} from '../lib/stable-snapshot';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {indexErrorHint,indexStages} from '../lib/index-stages';
 import type {Root,IndexStatus,IndexHistory} from '../lib/types';
 export let roots:Root[]=[];
 export let onClose:()=>void;
 export let onPauseChange:(paused:boolean)=>void;
 let status:IndexStatus={paused:false,tasks:[]},loading=true,changing=false,error='';
 let history:IndexHistory[]=[],historyKey:string|undefined,clearing=false;
 let disposed=false,timer:ReturnType<typeof setTimeout>;
 let running:Promise<void>|null=null,requested=false,historyDirty=true,generation=0,lastPaused:boolean|undefined;
 const stageLabel=(stage:string)=>$t(indexStages.find(item=>item.stage===stage)?.progress??(stage==='indexing'?'正在索引文件':'正在扫描文件'));
 const humanError=(message:string|null|undefined)=>{const hint=indexErrorHint(message);return hint?$t('{value0}其他文件可正常检索。',{value0:$t(hint)}):$t(message?'部分文件未能处理，其他已索引文件仍可搜索。':'索引未能完成，查看文件记录了解详情。')};
 const newestByFolder=(records:IndexHistory[])=>{
  const newest=new Map<string,IndexHistory>();
  for(const record of records){const old=newest.get(record.path);if(!old||record.started_at>old.started_at)newest.set(record.path,record);}
  return [...newest.values()].sort((a,b)=>b.started_at-a.started_at);
 };
 // Task state is authoritative; a parent root can lag a completion event.
 const active=()=>status.tasks.length>0;
 function acceptStatus(next:IndexStatus){
  if(!sameIndexStatus(status,next))status=next;
  if(lastPaused!==next.paused){lastPaused=next.paused;onPauseChange(next.paused);}
 }
 function refresh(includeHistory=false):Promise<void>{
  if(disposed)return Promise.resolve();
  clearTimeout(timer);requested=true;historyDirty||=includeHistory;generation++;
  if(!running&&!changing){
   running=refreshSnapshots().finally(()=>{
    running=null;
    if(requested&&!disposed&&!changing)return refresh();
    if(!disposed&&!changing){loading=false;timer=setTimeout(()=>void refresh(!active()),active()?600:5000);}
   });
  }
  return running??Promise.resolve();
 }
 async function refreshSnapshots(){
  while(requested&&!disposed&&!changing){
   requested=false;const current=generation,includeHistory=historyDirty;
   try{
    const next=await api<IndexStatus>('get_index_status');
    if(disposed||current!==generation)continue;
    acceptStatus(next);
    const key=JSON.stringify(next.tasks.map(task=>task.path));
    const readHistory=includeHistory||key!==historyKey;
    let records:IndexHistory[]|undefined;
    if(readHistory&&!clearing)records=await api<IndexHistory[]>('get_index_history');
    else if(readHistory)historyDirty=true;
    if(disposed||current!==generation)continue;
    if(records){const stable=stableRows(history,records,record=>record.id);if(stable!==history)history=stable;historyKey=key;historyDirty=false;}
    error='';
   }catch(e){if(!disposed&&current===generation){error=$t(String(e));historyDirty=true;}}
  }
 }
 onMount(()=>{
  let unsubscribe=()=>{};
  void(async()=>{
   try{const stop=await subscribeDataChanges(()=>void refresh(true));if(disposed){stop();return;}unsubscribe=stop;}
   catch(e){if(!disposed)error=$t(String(e));}
   if(!disposed)void refresh(true);
  })();
  return()=>{disposed=true;generation++;clearTimeout(timer);unsubscribe();};
 });
 async function togglePause(){
  changing=true;error='';generation++;clearTimeout(timer);
  try{const next=await api<IndexStatus>('set_index_paused',{paused:!status.paused});if(!disposed)acceptStatus(next);}
  catch(e){error=$t(String(e));}
  finally{changing=false;if(!disposed)void refresh();}
 }
 async function clearHistory(){
  clearing=true;error='';generation++;let cleared=false;
  try{await api('clear_index_history');if(!disposed){history=[];cleared=true;}}
  catch(e){if(!disposed)error=$t(String(e));}
  finally{if(!disposed){clearing=false;if(cleared)void refresh(true);}}
 }
 $:working=roots.some(root=>root.status==='pending')||status.tasks.length>0;
 $:finishing=status.paused&&status.tasks.some(task=>task.files.length>0);
 $:activeRoots=roots.filter(root=>root.status==='pending'||status.tasks.some(task=>task.path===root.path));
 $:issueRoots=roots.filter(root=>(root.status==='partial'||root.status==='error')&&!activeRoots.includes(root)&&!history.some(record=>record.path===root.path));
 $:latestHistory=newestByFolder(history);
 $:olderHistory=history.filter(record=>record.id!==latestHistory.find(item=>item.path===record.path)?.id);
 $:folderCount=new Set(history.map(record=>record.path)).size;
 $:issueCount=latestHistory.filter(record=>record.status==='partial'||record.status==='error').length+issueRoots.length;
 $:state=loading?'loading':finishing?'finishing':status.paused?'paused':working?'working':issueCount?'issues':'done';
</script>

<Modal label={$t('索引详情')} class="index-issues-dialog" {onClose}>
 <div class="dialog-head"><h2>{$t('索引详情')}</h2><Button.Root class="icon-button" aria-label={$t('关闭索引详情')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="index-detail-summary" data-state={state}>
  <span class="index-summary-mark" aria-hidden="true">{@html icon(state==='paused'||state==='finishing'?'pause':state==='working'||state==='loading'?'refresh':state==='issues'?'alert':'check')}</span>
  <div class="index-summary-copy">
   <strong class="index-summary-state" aria-live="polite">{$t(state==='loading'?'正在读取索引状态…':state==='finishing'?'正在暂停…':state==='paused'?'索引已暂停':state==='working'?'正在索引':state==='issues'?'索引已完成，部分文件未能处理':'索引已完成')}</strong>
   <span>{state==='working'||state==='paused'||state==='finishing'?$t('暂停后，正在处理的文件完成后停下；已索引内容仍可搜索。'):issueCount?$t('{value0} 个文件夹 · {value1} 个需要处理',{value0:folderCount,value1:issueCount}):$t('{value0} 个文件夹，全部可正常检索',{value0:folderCount})}</span>
  </div>
  {#if working||status.paused}<Button.Root class="outline index-pause-button" aria-label={$t(status.paused?'继续索引':'暂停索引')} disabled={loading||changing} onclick={togglePause}>{@html icon(status.paused?'play':'pause')}{$t(status.paused?'继续索引':'暂停索引')}</Button.Root>{/if}
 </div>
 <div class="dialog-body index-detail-body">
  {#if activeRoots.length}<section class="index-active" aria-label={$t('当前任务')}>
  <div class="index-section-heading"><h3>{$t('当前任务')}</h3></div>
  {#each activeRoots as root (root.path)}
   {@const task=status.tasks.find(task=>task.path===root.path)}
   <section class="index-root">
    <div class="index-root-title">{@html icon('folder')}<strong>{root.name}</strong><span>{status.paused?$t(task&&task.files.length?'正在暂停…':'索引已暂停'):task?stageLabel(task.stage):$t('等待处理')}</span></div>
    <p class="index-root-path" title={displayPath(root.path)}>{displayPath(root.path)}</p>
    {#if task}
     {@const step=indexStages.findIndex(item=>item.stage===task.stage)}
     <ol class="index-pipeline" aria-label={$t('索引流程')}>
      {#each indexStages as item,index}<li class:current={step===index} class:done={step>index} aria-current={step===index?'step':undefined}><span>{step>index?'✓':index+1}</span>{$t(item.title)}</li>{/each}
     </ol>
     <p class="index-stage-description">{step>=0?$t(indexStages[step].description):$t('先索引文本，再转换其他格式，最后索引转换结果。')}</p>
     {#if task.total>0}<div class="index-count">{$t('已处理 {value0} / {value1} 个文件',{value0:task.completed,value1:task.total})}</div><progress aria-label={$t('{value0} 的索引进度',{value0:root.name})} value={task.completed} max={task.total}></progress>{/if}
     {#if task.files.length}<ul class="index-current-files" aria-label={$t('正在处理的文件')}>{#each task.files as file}<li>{@html icon('file')}<span>{displayPath(file)}</span></li>{/each}</ul>{/if}
    {/if}
   </section>
  {/each}
  </section>{/if}
  <section class="index-history" aria-label={$t('索引历史')}>
   <div class="index-history-heading"><h3>{$t('索引历史')}</h3><span>{$t('最近 100 次索引')}</span><Button.Root class="clear-index-history" aria-label={$t('清空索引记录')} title={$t('清空索引记录，仅删除历史，不影响已有索引和当前任务')} disabled={loading||clearing||!history.length} onclick={clearHistory}>{@html icon('delete')}{$t('清空')}</Button.Root></div>
   {#if latestHistory.length}<div class="index-history-list">
    {#each latestHistory as record (record.path)}<div class="index-root"><IndexHistoryEntry {record}/></div>{/each}
    {#if olderHistory.length}<details class="older-history"><summary>{@html icon('history')}{$t('查看更早记录（{value0}）',{value0:olderHistory.length})}{@html icon('down')}</summary><div class="older-history-list">{#each olderHistory as record (record.id)}<div class="index-root"><IndexHistoryEntry {record}/></div>{/each}</div></details>{/if}
   </div>{/if}
   {#if !loading&&!history.length}<div class="index-history-empty">{@html icon('history')}<p>{$t('暂无索引历史')}</p></div>{/if}
  </section>
  {#if issueRoots.length}<section class="index-issues" aria-label={$t('索引异常')}>
   <div class="index-section-heading"><h3>{$t('索引异常')}</h3></div>
   {#each issueRoots as root (root.path)}
    <div class="index-root">
     <div class="index-root-title">{@html icon('folder')}<strong>{root.name}</strong><span class:failed={root.status==='error'||root.status==='partial'}>{$t(root.status==='error'?'读取失败':root.status==='partial'?'部分失败':'已完成')}</span></div>
     <p class="index-root-path" title={displayPath(root.path)}>{displayPath(root.path)}</p>
     <p class="small muted">{$t('已索引 {value0} 个文件',{value0:root.count})}</p>
     {#if root.status==='partial'}<p class="small muted">{root.error?.includes('文件无法按')?$t('以下文件未进入索引，其他文件可正常检索。'):humanError(root.error)}</p>{/if}
     {#if root.error}<details class="index-failure"><summary>{$t('查看技术详情')}</summary><pre class="index-issue-details">{root.error.split('\n').map(line=>$t(displayPath(line))).join('\n')}</pre></details>{/if}
    </div>
   {/each}
  </section>{/if}
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 </div>
</Modal>

<style>
 :global(.dialog-panel.index-issues-dialog){width:min(620px,calc(100vw - 40px));max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head{padding:20px 24px 14px;flex-shrink:0}
 .dialog-head h2{font-size:17px;line-height:1.4;letter-spacing:-.2px}
 .index-detail-summary{display:flex;align-items:center;gap:12px;margin:0 24px 18px;padding:14px;border:1px solid var(--color-rule);border-radius:12px;background:var(--color-panel);flex-shrink:0}
 .index-summary-mark{display:grid;place-items:center;width:36px;height:36px;flex-shrink:0;border-radius:10px;background:var(--color-paper);border:1px solid var(--color-rule);color:var(--color-ink);box-shadow:0 1px 2px var(--color-shadow)}
 .index-summary-mark :global(.icon){width:17px;height:17px}
 [data-state=working] .index-summary-mark :global(.icon),[data-state=loading] .index-summary-mark :global(.icon){animation:index-spin 1.2s linear infinite}
 @keyframes index-spin{to{transform:rotate(360deg)}}
 @media(prefers-reduced-motion:reduce){.index-summary-mark :global(.icon){animation:none!important}}
 .index-summary-copy{display:flex;flex-direction:column;gap:2px;flex:1;min-width:0}
 .index-summary-state{font-size:14px;font-weight:600;line-height:1.45}
 .index-summary-copy>span{font-size:12px;line-height:1.5;color:var(--color-muted)}
 :global(.index-pause-button){display:flex;align-items:center;gap:6px;white-space:nowrap;font-size:12px;min-height:30px;padding:5px 11px;flex-shrink:0;border-radius:8px}
 :global(.index-pause-button .icon){width:13px;height:13px}
 .index-detail-body{min-height:0;overflow:auto;overscroll-behavior:contain;padding:0 24px 22px;gap:18px}
 .index-active,.index-issues{border:1px solid var(--color-rule);border-radius:12px;background:var(--color-paper);overflow:hidden;flex-shrink:0}
 .index-section-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:10px 14px;border-bottom:1px solid var(--color-rule)}
 .index-section-heading h3,.index-history-heading h3{margin:0;font-size:13px;font-weight:600;line-height:1.5;color:var(--color-ink)}
 .index-history{flex-shrink:0}
 .index-history-heading{display:flex;align-items:center;gap:8px;margin:0 2px 8px}
 .index-history-heading>span{flex:1;font-size:12px;color:var(--color-muted)}
 :global(.clear-index-history){display:inline-flex;align-items:center;gap:5px;min-height:26px;padding:3px 8px;border:0;border-radius:6px;background:transparent;color:var(--color-muted);font-size:12px}
 :global(.clear-index-history>.icon){width:13px;height:13px}
 :global(.clear-index-history:hover:not(:disabled)){color:var(--color-danger);background:var(--color-hover)}
 :global(.clear-index-history:disabled){opacity:.45}
 .index-history-list{border:1px solid var(--color-rule);border-radius:12px;background:var(--color-paper);overflow:hidden}
 .index-history-list .index-root{padding:0;border-bottom:1px solid var(--color-rule)}
 .index-history-list .index-root:last-of-type{border-bottom:0}
 .index-active .index-root,.index-issues .index-root{padding:14px;border-bottom:1px solid var(--color-rule)}
 .index-active .index-root:last-of-type,.index-issues .index-root:last-of-type{border-bottom:0}
 .older-history{border-top:1px solid var(--color-rule)}
 .older-history>summary{display:flex;align-items:center;justify-content:center;gap:6px;padding:10px 14px;color:var(--color-secondary);font-size:12px;cursor:pointer;list-style:none;transition:background-color .12s,color .12s}
 .older-history>summary::-webkit-details-marker{display:none}
 .older-history>summary:hover{background:var(--color-panel);color:var(--color-ink)}
 .older-history>summary :global(.icon){width:13px;height:13px}
 .older-history>summary :global(.icon:last-child){transition:transform .15s}
 .older-history[open]>summary{border-bottom:1px solid var(--color-rule)}
 .older-history[open]>summary :global(.icon:last-child){transform:rotate(180deg)}
 /* Earlier runs are reference material: one compact line each. */
 .older-history-list{background:var(--color-panel)}
 .older-history-list .index-root{border-bottom-color:var(--color-rule)}
 .older-history-list :global(.index-history-toggle){grid-template-columns:20px minmax(0,1fr) auto;gap:10px;padding:8px 14px}
 .older-history-list :global(.index-entry-icon){width:20px;height:20px;border:0;background:transparent}
 .older-history-list :global(.index-entry-icon .icon){width:14px;height:14px}
 .older-history-list :global(.index-entry-main){flex-direction:row;align-items:center;gap:10px}
 .older-history-list :global(.index-root-title){font-size:12.5px;flex-shrink:0;max-width:55%}
 .older-history-list :global(.index-root-title strong){font-weight:500}
 .older-history-list :global(.index-root-path),.older-history-list :global(.index-count::after){display:none}
 .older-history-list :global(.index-entry-notice),.older-history-list :global(.index-file-preview){margin-left:44px}
 .index-history-empty{display:flex;flex-direction:column;align-items:center;gap:8px;padding:28px 16px;border:1px dashed var(--color-rule);border-radius:12px;color:var(--color-muted);font-size:12px}
 .index-history-empty :global(.icon){width:20px;height:20px}
 .index-history-empty p{margin:0}
 .index-root-title{display:flex;align-items:center;gap:8px;font-size:13px;line-height:1.5}
 .index-root-title :global(.icon){width:16px;height:16px;flex-shrink:0;color:var(--color-secondary)}
 .index-root-title strong{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:550}
 .index-root-title>span{margin-left:auto;padding:1px 8px;border:1px solid transparent;border-radius:999px;background:var(--color-hover);white-space:nowrap;color:var(--color-secondary);font-size:11px;line-height:1.6;flex-shrink:0}
 .index-root-title>span.failed{border-color:var(--color-rule);background:var(--color-paper);color:var(--color-ink);font-weight:550}
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
</style>
