<script lang="ts">
 import SegmentProgress from './SegmentProgress.svelte';
 import {t} from '../lib/i18n';
 import {basename,displayPath} from '../lib/domain';
 import {icon} from '../lib/icons';
 import {isConversionStage} from '../lib/index-stages';
 import type {ConversionState,IndexFailure,IndexProgress,IndexTask,Root} from '../lib/types';
 export let active=false,paused=false,retrying=false,retryBusy=false;
 export let retryTotal=0;
 export let task:IndexTask|undefined=undefined;
 export let progress:IndexProgress|null=null;
 export let failures:IndexFailure[]=[];
 export let failureCount=0;
 export let roots:Root[]=[];
 export let conversionStates:ConversionState[]=[];
 export let recentFiles:string[]=[];
 export let retryError='';
 export let onRetry:()=>void;
 export let onHistory:()=>void;
 export let onDismiss:()=>void;
 export let onPause:()=>Promise<void>;
 let changingPause=false;
 async function togglePause(){changingPause=true;try{await onPause()}finally{changingPause=false}}
 let open=false;
 $: currentFiles=active?(task?.files??[]):[];
 $: rootErrors=roots.filter(root=>(root.status==='error'||root.status==='partial')&&root.error);
 $: failed=failureCount>0||failures.length>0||rootErrors.length>0;
 $: count=progress?.count??task?.completed??0;
 $: total=progress?.total??task?.total??0;
 $: stage=task?.stage??progress?.stage;
 $: activeTitle=isConversionStage(stage)?'正在解析文档':['indexing','indexing_native','indexing_converted','extracting_basic'].includes(stage??'')?'正在检查文档':'正在准备文档';
 $: title=paused?$t('文档处理已暂停'):retrying?$t('正在重试失败文件'):active?$t(activeTitle):failed?$t('部分文件处理失败'):$t('文档解析完成');
 $: counter=active&&total>0?`（${count}/${total}）`:retrying&&retryTotal>0?`（0/${retryTotal}）`:!active&&failureCount>0?`（${failureCount}）`:'';
 $: ariaCounter=active&&total>0?$t('已完成 {value0} 个文件，共 {value1} 个',{value0:count,value1:total}):title;
 $: if(retrying)open=true;
</script>

<section class="index-status-card" class:has-failures={!active&&failed} aria-label={$t('文档处理状态')}>
 <div class="card-heading">
  <div class="card-title-row"><span class="index-status-primary" role="status" aria-live="polite" title={ariaCounter}>{title}{counter}</span><div class="card-actions">
   {#if active||paused||retrying||retryBusy}<button class="pause-button" aria-label={$t(paused?'继续索引':'暂停索引')} title={$t(paused?'继续索引':'暂停索引')} disabled={changingPause} onclick={togglePause}>{@html icon(paused?'play':'pause')}</button>{/if}
   <button class="dismiss-button" aria-label={$t("隐藏索引状态")} title={$t("隐藏索引状态")} onclick={onDismiss}>{@html icon("close")}</button>
  </div></div>
  {#if currentFiles.length}<p class="index-status-current-file" title={currentFiles.map(displayPath).join('\n')}>{basename(currentFiles[0])}{#if currentFiles.length>1}<span> · {$t('等 {value0} 个文件',{value0:currentFiles.length})}</span>{/if}</p>{/if}
 </div>
 <details bind:open>
  <summary><span class="chevron" aria-hidden="true">{@html icon('down')}</span><span>{open?$t('收起详情'):$t('查看详情')}</span></summary>
  <div class="card-details">
   {#if paused}<p class="status-note">{$t('已完成的内容仍可搜索，继续后恢复文档处理。')}</p>{/if}
   {#each currentFiles as path}
    {@const conversion=task?.conversions?.find(item=>item.path===path)}
    {@const state=conversionStates.find(item=>item.source===path)}
    <div class="document-detail">
     {#if currentFiles.length>1}<p class="detail-filename">{basename(path)}</p>{/if}
     {#if state}<p class="availability" class:searchable={state.searchable}>{state.searchable?$t('已可搜索部分内容'):$t('解析完成后可搜索')}</p>{/if}
     {#if isConversionStage(task?.stage)&&conversion?.segment_progress}
      <p class="detail-heading">{$t('当前文档进度')}</p>
      <SegmentProgress progress={conversion.segment_progress}/>
     {:else if isConversionStage(task?.stage)&&conversion?.segment}
      <p class="detail-heading">{$t('当前文档进度')}</p>
      <div class="index-status-segment"><span>{$t('正在处理第 {value0}/{value1} 段',{value0:conversion.segment.current,value1:conversion.segment.total})}</span><span>{$t('第 {value0}–{value1} 页',{value0:conversion.segment.start_page,value1:conversion.segment.end_page})}</span></div>
      <progress value={Math.max(0,conversion.segment.current-1)} max={conversion.segment.total} aria-label={$t('当前文档已完成分段')}></progress>
      <p class="segment-completed">{$t('已完成 {value0}/{value1} 段',{value0:Math.max(0,conversion.segment.current-1),value1:conversion.segment.total})}</p>
     {/if}
     {#if isConversionStage(task?.stage)&&conversion?.engine&&!conversion.segment_progress}<p class="index-status-parser">{$t('解析方式：{value0}',{value0:conversion.engine})}</p>{/if}
    </div>
   {/each}
   {#if retrying}<p class="status-note" role="status">{$t('仅重试失败文件，已成功的文件不重复处理。')}</p>{/if}
   {#if failures.length}
    <p class="detail-heading">{$t('失败文件')}</p>
    <ul class="failed-files">{#each failures as file (`${file.root}:${file.path}`)}<li><p title={displayPath(file.path)}>{basename(file.path)}</p><p class="failure-reason">{$t(file.error)}</p></li>{/each}</ul>
    <p class="status-note">{$t('其他已完成文档可正常搜索。')}</p>
   {/if}
   {#if rootErrors.length}
    {#each rootErrors as root}<p class="failure-reason">{root.name}：{$t(root.error??'')}</p>{/each}
   {/if}
   {#if failureCount>0||retrying}
    <button class="retry-button index-retry-button" disabled={active||paused||retryBusy||retrying} aria-busy={retryBusy||retrying} onclick={onRetry}>{retrying||retryBusy?$t('正在重试…'):$t('重试失败文件')}</button>
    {#if active&&!retrying}<p class="status-note">{$t('当前任务结束后可重试失败文件。')}</p>{/if}
   {/if}
   {#if retryError}<p class="failure-reason" role="alert">{$t(retryError)}</p>{/if}
   {#if active&&recentFiles.length}<div class="recent-files"><p class="availability searchable">{$t('新增文件已可搜索')}</p>{#each recentFiles as file}<p title={displayPath(file)}>{basename(file)}</p>{/each}</div>{/if}
   {#if !active&&!failed}<p class="availability searchable">{$t('文档已可搜索')}</p>{/if}
   <button class="history-button index-status-link" aria-label={$t('查看索引详情')} onclick={onHistory}>{$t('索引历史')}</button>
  </div>
 </details>
</section>

<style>
 .index-status-card{border:1px solid var(--color-rule);border-radius:12px;background:var(--color-paper);overflow:hidden;color:var(--color-ink);font-size:12px}
 .card-heading{padding:10px 10px 14px 14px}.card-title-row{display:flex;align-items:flex-start;gap:4px}.card-title-row>.index-status-primary{flex:1;min-width:0;padding-top:4px}.card-actions{display:flex;flex-shrink:0}.card-actions button{width:26px;height:28px;padding:6px;display:flex;align-items:center;justify-content:center;color:var(--color-secondary);border-radius:5px}.card-actions button:hover{background:var(--color-panel)}.card-actions button:disabled{opacity:.5}.card-actions :global(svg){width:14px;height:14px}.index-status-primary{font-size:13px;font-weight:600;line-height:1.5;font-variant-numeric:tabular-nums}.has-failures .index-status-primary{color:var(--color-danger)}
 .index-status-current-file{margin:7px 0 0;font-size:12px;color:var(--color-secondary);overflow-wrap:anywhere;display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
 summary{display:flex;align-items:center;gap:6px;padding:9px 14px;min-height:36px;border-top:1px solid var(--color-rule);list-style:none;cursor:pointer;color:var(--color-secondary);font-size:11px}summary::-webkit-details-marker{display:none}summary:hover{background:var(--color-panel)}summary:focus-visible,button:focus-visible{outline:2px solid var(--color-focus);outline-offset:-3px}
 .chevron{display:flex;transform:rotate(-90deg)}details[open] .chevron{transform:none}.chevron :global(svg){width:12px;height:12px}
 .card-details{padding:0 14px 14px;max-height:min(480px,50dvh);overflow:auto;overscroll-behavior:contain}.document-detail+.document-detail{border-top:1px solid var(--color-rule);padding-top:12px;margin-top:12px}.detail-filename{overflow-wrap:anywhere;font-size:12px}.availability{margin:10px 0 16px;color:var(--color-secondary);font-size:12px}.searchable{color:var(--color-success)}
 .detail-heading{font-weight:600;margin:16px 0 8px}.index-status-segment{display:flex;justify-content:space-between;flex-wrap:wrap;gap:4px;margin:0 0 8px;font-size:11px}.index-status-segment>span:last-child{color:var(--color-secondary)}
 progress{appearance:none;border:0 solid;border-radius:4px;overflow:clip;width:100%;height:4px;background:var(--color-rule);display:block}progress::-webkit-progress-bar{background:transparent}progress::-webkit-progress-value{background:var(--color-ink);border-radius:4px}progress::-moz-progress-bar{background:var(--color-ink);border-radius:4px}
 .segment-completed{margin:6px 0 14px;color:var(--color-secondary);font-size:11px}.index-status-parser{font-size:11px;color:var(--color-secondary);margin:12px 0}.status-note{font-size:11px;color:var(--color-secondary);line-height:1.7;margin:10px 0;overflow-wrap:anywhere}
 .failed-files{list-style:none;padding:0;margin:0;max-height:220px;overflow:auto;overscroll-behavior:contain}.failed-files li{padding:8px 0;border-bottom:1px solid var(--color-rule)}.failed-files li:last-child{border-bottom:0}.failed-files p{margin:0;overflow-wrap:anywhere}.failure-reason{font-size:11px;color:var(--color-danger);white-space:pre-wrap;overflow-wrap:anywhere}.failed-files .failure-reason{margin-top:5px}
 .retry-button{min-height:32px;border:0;border-radius:6px;background:var(--color-accent);color:var(--color-accent-ink);font-size:12px;padding:6px 10px;margin-top:12px}.retry-button:disabled{opacity:.5;cursor:default}.recent-files{border-top:1px solid var(--color-rule);margin-top:14px;padding-top:4px;font-size:11px}.recent-files p{overflow-wrap:anywhere}
 .history-button{display:block;background:none;border:0;color:var(--color-secondary);text-decoration:underline;text-underline-offset:3px;font-size:11px;padding:4px 0;margin-top:14px}
</style>
