<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t,language} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import type {FileInfo,Root,IndexStatus,IndexHistory} from '../lib/types';
 export let files:FileInfo[]=[];
 export let roots:Root[]=[];
 export let revision=0;
 export let onDetails:(()=>void)|undefined=undefined;
 export let onPauseChange:((paused:boolean)=>void)|undefined=undefined;
 let status:IndexStatus={paused:false,tasks:[]},history:IndexHistory[]=[];
 let loading=true,changing=false,error='',statusError=false,historyKey:string|undefined;
 let now=Date.now();
 let unit:'day'|'hour'|'minute'='minute';
 const colors:Record<string,string>={MD:'#4f8bff',HTML:'#485ee7',PDF:'#9851f1',PPTX:'#e6ad15',DOCX:'#288dce',XLSX:'#36a47b',TXT:'#25a6ab',JSON:'#cd794e'};
 const fallbackColors=['#8171c5','#b273a5','#578d9d','#a38c57','#858995'];
 $:counts=files.reduce((counts,file)=>{
  let name=file.name.toLowerCase();
  if(file.path.split(/[\\/]/).includes('.sag-converted')&&name.endsWith('.md'))name=name.slice(0,-3);
  const dot=name.lastIndexOf('.');
  let type=dot>0&&dot<name.length-1?name.slice(dot+1).toUpperCase():'无扩展名';
  if(type==='MARKDOWN')type='MD';
  if(type==='HTM')type='HTML';
  counts.set(type,(counts.get(type)??0)+1);return counts;
 },new Map<string,number>());
 $:allTypes=[...counts].sort((a,b)=>b[1]-a[1]||a[0].localeCompare(b[0]));
 $:types=(allTypes.length>6?[...allTypes.slice(0,5),['其他',allTypes.slice(5).reduce((sum,item)=>sum+item[1],0)] as [string,number]]:allTypes)
  .map(([name,count],index)=>({name,count,color:colors[name]??fallbackColors[index%fallbackColors.length]}));
 $:working=status.tasks.length>0||roots.some(root=>root.status==='indexing'||root.status==='pending');
 $:failed=roots.some(root=>root.status==='partial'||root.status==='error');
 $:finishing=status.paused&&status.tasks.some(task=>task.files.length>0);
 $:label=loading?'正在读取索引状态…':statusError?'无法读取索引状态':finishing?'正在暂停…':status.paused?'索引已暂停':working?'正在索引':failed?'部分文件索引失败':roots.length?'索引已完成':'等待添加文件夹';
 $:glyph=statusError?'info':status.paused?'pause':working?'refresh':failed?'info':roots.length?'check':'folder';
 $:updatedAt=history.reduce((latest,record)=>Math.max(latest,record.finished_at),0);
 $:elapsed=Math.max(0,now-updatedAt);
 $:unit=elapsed>=86400000?'day':elapsed>=3600000?'hour':'minute';
 $:age=new Intl.RelativeTimeFormat($language==='en'?'en':'zh-CN',{numeric:'auto'}).format(-Math.floor(elapsed/(unit==='day'?86400000:unit==='hour'?3600000:60000)),unit);
 $:processed=status.tasks.reduce((sum,task)=>sum+task.completed,0);
 $:total=status.tasks.reduce((sum,task)=>sum+task.total,0);

 onMount(()=>{
  let disposed=false,timer:ReturnType<typeof setTimeout>;
  async function refresh(){
   let readingStatus=true;
   try{
    const next=await api<IndexStatus>('get_index_status');
    if(disposed)return;
    status=next;statusError=false;readingStatus=false;onPauseChange?.(next.paused);now=Date.now();
    const key=JSON.stringify([revision,next.tasks.map(task=>task.path),roots.map(root=>[root.path,root.status,root.count])]);
    if(key!==historyKey){
     const records=await api<IndexHistory[]>('get_index_history');
     if(disposed)return;
     history=records;historyKey=key;
    }
    error='';
   }catch(e){if(!disposed){error=$t(String(e));statusError=readingStatus;}}
   finally{if(!disposed){loading=false;timer=setTimeout(refresh,1500);}}
  }
  void refresh();
  return()=>{disposed=true;clearTimeout(timer);};
 });
 async function togglePause(){
  changing=true;error='';
  try{status=await api<IndexStatus>('set_index_paused',{paused:!status.paused});onPauseChange?.(status.paused);}
  catch(e){error=$t(String(e));}
  finally{changing=false;}
 }
</script>

<div class="library-stats">
 <section class="library-index-card" class:paused={status.paused} aria-label={$t('索引状态')}>
  <Button.Root class="library-index-details" aria-label={$t('查看索引记录')} title={error||$t('查看索引记录')} onclick={()=>onDetails?.()}>
   <span class="index-copy">
    <span class="index-state" aria-live="polite">{@html icon(glyph)}<strong>{$t(label)}</strong></span>
    {#if error}<span class="index-updated" role="alert">{$t('暂时无法更新，点击查看详情')}</span>
    {:else if working&&total>0}<span class="index-updated">{$t('已处理 {value0} / {value1} 个文件',{value0:processed,value1:total})}</span>
    {:else if updatedAt}<span class="index-updated"><time datetime={new Date(updatedAt).toISOString()} title={new Date(updatedAt).toLocaleString($language==='en'?'en-US':'zh-CN')}>{$t('最近更新 {value0}',{value0:age})}</time></span>{/if}
   </span>
   <span class="index-chevron" aria-hidden="true">{@html icon('chevron')}</span>
  </Button.Root>
  {#if !loading&&(working||status.paused)}<Button.Root class="library-index-pause" aria-label={$t(status.paused?'继续索引':'暂停索引')} title={$t(status.paused?'继续索引':'暂停索引')} disabled={changing} onclick={togglePause}>{@html icon(status.paused?'play':'pause')}</Button.Root>{/if}
 </section>
 <section class="library-file-stats" aria-label={$t('文件类型统计')}>
  <div class="file-stats-heading"><span><strong>{files.length.toLocaleString($language==='en'?'en-US':'zh-CN')}</strong>{$t('个文件')}</span></div>
  <div class="file-type-bar" role="img" aria-label={types.length?types.map(type=>`${$t(type.name)} ${type.count}`).join('，'):$t('暂无已索引文件')}>
   {#each types as type}<span style:width={`${type.count/files.length*100}%`} style:background={type.color} title={`${$t(type.name)} · ${type.count} · ${(type.count/files.length*100).toFixed(1)}%`}></span>{/each}
  </div>
  <ul class="file-type-legend">{#each types as type}<li><i style:background={type.color}></i><span>{$t(type.name)}</span><span>{type.count.toLocaleString($language==='en'?'en-US':'zh-CN')}</span></li>{:else}<li>{$t('暂无已索引文件')}</li>{/each}</ul>
 </section>
</div>

<style>
 .library-stats{display:grid;grid-template-columns:minmax(230px,.75fr) minmax(0,1.65fr);gap:16px;margin:0 0 24px}
 .library-index-card,.library-file-stats{border:1px solid var(--color-rule);border-radius:16px;background:var(--color-paper);min-width:0;padding:18px 20px;min-height:124px}
 .library-index-card{position:relative;display:flex;align-items:center;gap:8px;overflow:hidden}
 .library-index-card.paused{background-image:repeating-linear-gradient(135deg,transparent,transparent 13px,color-mix(in srgb,var(--color-rule) 25%,transparent) 13px,color-mix(in srgb,var(--color-rule) 25%,transparent) 14px)}
 .library-index-card :global(.library-index-details){display:flex;gap:8px;align-items:center;flex:1;min-width:0;padding:0;text-align:left;background:transparent}
 .index-copy{display:flex;flex-direction:column;gap:8px;flex:1;min-width:0}
 .index-state{display:flex;align-items:center;gap:8px;line-height:1.5}
 .index-state strong{font-size:15px;font-weight:600}
 .index-state :global(.icon){width:15px;height:15px;flex-shrink:0;color:var(--color-secondary)}
 .index-updated{font-size:12px;color:var(--color-secondary);line-height:1.5}
 .index-chevron{display:flex;color:var(--color-muted)}
 .index-chevron :global(.icon){width:13px;height:13px}
 .library-index-card :global(.library-index-pause){display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;flex-shrink:0;padding:0;border:1px solid var(--color-rule);border-radius:7px;background:var(--color-paper)}
 .library-index-card :global(.library-index-pause .icon){width:13px;height:13px}
 .library-index-card :global(.library-index-pause:disabled){opacity:.45}
 .library-file-stats{display:flex;flex-direction:column;justify-content:center;gap:14px}
 .file-stats-heading{display:flex;align-items:baseline;justify-content:space-between;gap:12px;line-height:1.2}
 .file-stats-heading>span{display:flex;align-items:baseline;gap:6px;font-size:12px;color:var(--color-secondary)}
 .file-stats-heading strong{font-size:26px;font-weight:600;color:var(--color-ink);font-variant-numeric:tabular-nums;letter-spacing:-.5px}
 .file-type-bar{display:flex;width:100%;height:23px;overflow:hidden;border-radius:6px;background:var(--color-panel)}
 .file-type-bar>span{height:100%;flex-shrink:0}
 .file-type-legend{list-style:none;display:flex;align-items:center;gap:6px 14px;flex-wrap:wrap;margin:0;padding:0;font-size:11px;line-height:1.5;color:var(--color-secondary)}
 .file-type-legend li{display:flex;align-items:center;gap:4px}
 .file-type-legend i{width:8px;height:8px;border-radius:2px;flex-shrink:0}
 .file-type-legend li>span:last-child{font-variant-numeric:tabular-nums}
 @media(max-width:1050px){.library-stats{grid-template-columns:minmax(205px,.8fr) minmax(0,1.2fr);gap:12px}.library-index-card,.library-file-stats{padding:16px}}
 @media(max-width:760px){.library-stats{grid-template-columns:minmax(0,1fr)}.library-index-card,.library-file-stats{min-height:110px}}
</style>
