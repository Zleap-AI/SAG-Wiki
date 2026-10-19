<script lang="ts">
 import {onMount} from 'svelte';
 import {listen} from '@tauri-apps/api/event';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import type {Root,IndexStatus} from '../lib/types';
 export let settingsChanged=false;
 let roots:Root[]=[],loading=true,submitting=false,started=false,error='';
 let progress:{path:string;count:number;total:number}|null=null;
 let status:IndexStatus={paused:false,tasks:[]};
 $:finishing=status.paused&&status.tasks.some(task=>task.files.length>0);
 $:indexing=roots.some(root=>root.status==='pending'||root.status==='indexing');
 $:failures=roots.filter(root=>root.status==='partial'||root.status==='error');
 $:current=roots.find(root=>root.path===progress?.path&&root.status==='indexing');
 $:count=roots.reduce((sum,root)=>sum+root.count,0);
 onMount(()=>{
  let disposed=false;
  let timer:ReturnType<typeof setTimeout>;
  let cleanup:(()=>void)[]=[];
  async function refresh(){const next=await api<Root[]>('list_roots');if(!disposed)roots=next;}
  async function refreshStatus(){
   try{const next=await api<IndexStatus>('get_index_status');if(!disposed)status=next;}
   catch(e){if(!disposed)error=String(e);}
   finally{if(!disposed)timer=setTimeout(refreshStatus,600);}
  }
  void Promise.all([
   listen<{path:string;count:number;total:number}>('index-progress',event=>{progress=event.payload;roots=roots.map(root=>root.path===progress!.path?{...root,status:'indexing'}:root)}),
   listen('data-changed',()=>{void refresh().catch(e=>error=String(e))})
  ]).then(async stops=>{if(disposed){stops.forEach(stop=>stop());return;}cleanup=stops;await Promise.all([refresh(),refreshStatus()]);}).catch(e=>error=String(e)).finally(()=>loading=false);
  return()=>{disposed=true;clearTimeout(timer);cleanup.forEach(stop=>stop());};
 });
 async function reindex(){
  submitting=true;error='';progress=null;
  try{roots=await api<Root[]>('reindex_all');started=true;}catch(e){error=String(e);}finally{submitting=false;}
 }
</script>

<div class="reindex-settings">
 <div class="settings-group-heading"><div><h3>{$t("导入文件类型")}</h3><p class="settings-help">{$t("默认启用常用文本类型。取消勾选后不再检索该类文件，原文件和文件浏览不受影响。")}</p></div><Button.Root type="button" class="outline" title={$t("按已保存的文件类型和排除目录检查全部文件，只更新变化的索引。")} disabled={loading||submitting||indexing||status.paused||!roots.length||settingsChanged} onclick={reindex}>{$t(finishing?'正在暂停…':status.paused?'索引已暂停':submitting||indexing?"正在索引…":"重新索引")}</Button.Root></div>
 {#if settingsChanged||submitting||indexing||status.paused||started||failures.length||error||!loading&&!roots.length}
 <div class="reindex-status" aria-live="polite">
  {#if settingsChanged}<p class="settings-help">{$t("请先保存文件类型或排除目录的修改，再重新索引。")}</p>{/if}
  {#if !loading&&!roots.length}<p class="settings-help">{$t("添加文件夹后可重新索引。")}</p>{/if}
  {#if status.paused}<p>{$t(finishing?'正在暂停…':'索引已暂停')}</p>{/if}
  {#if current&&progress}<p>{$t("{value0}：已处理 {value1} / {value2} 个文件",{value0:current.name,value1:progress.count,value2:progress.total})}</p><progress aria-label={$t("索引进度")} value={progress.count} max={Math.max(1,progress.total)}></progress>
  {:else if !status.paused&&(submitting||indexing)}<p class="reindex-running"><span aria-hidden="true">{@html icon('refresh')}</span>{$t("正在检查文件…")}</p>
  {:else if !status.paused&&started}<p>{$t(failures.length?"重新索引完成，部分文件未能索引。":"重新索引完成。")}</p>{/if}
  {#if !loading&&roots.length}<p class="settings-help">{$t("已索引 {value0} 个文件",{value0:count})}</p>{/if}
  {#each failures as root}<details class="reindex-error"><summary>{$t("查看 {value0} 的失败详情",{value0:root.name})}</summary><pre>{root.error?.split('\n').map(line=>$t(line)).join('\n')}</pre></details>{/each}
  {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
 {/if}
</div>
