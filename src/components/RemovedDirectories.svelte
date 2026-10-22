<script lang="ts">
 import {onMount} from 'svelte';
 import {listen} from '@tauri-apps/api/event';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 let paths:string[]=[],loading=true,pending='',error='',notice='';
 onMount(()=>{
  let active=true,unlisten:(()=>void)|undefined;
  async function load(){const next=await api<string[]>('list_removed_directories');if(active)paths=next;}
  void listen('data-changed',()=>{void load().catch(e=>error=String(e))}).then(stop=>{if(active)unlisten=stop;else stop()}).catch(e=>error=String(e));
  void load().catch(e=>error=String(e)).finally(()=>loading=false);
  return()=>{active=false;unlisten?.();};
 });
 async function restore(path:string){
  pending=path;error='';notice='';
  try{await api('add_root',{path});paths=await api<string[]>('list_removed_directories');notice=$t('已恢复，正在重新索引。');}
  catch(e){error=String(e);}finally{pending='';}
 }
</script>
<section class="settings-group removed-directories" aria-label={$t('已移除的文件和文件夹')}>
 <div class="removed-heading"><h3>{$t('已移除的文件和文件夹')}</h3><p class="settings-help">{$t('这些文件和文件夹不显示在知识库中，也不参与搜索。本地原文件始终保留。')}</p></div>
 {#if loading}<p class="settings-help">{$t('正在读取…')}</p>
 {:else if !paths.length}<p class="settings-help removed-empty">{$t('暂无已移除的文件和文件夹。')}</p>
 {:else}{#each paths as path(path)}<div class="setting-row"><code>{path}</code><Button.Root type="button" class="outline" disabled={!!pending} onclick={()=>restore(path)}>{$t(pending===path?'正在恢复…':'恢复')}</Button.Root></div>{/each}{/if}
 {#if notice}<p class="settings-help" role="status">{notice}</p>{/if}
 {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
</section>
<style>
 .removed-heading{padding:16px 18px 0}.removed-heading h3{margin:0;font-size:var(--text-ui);font-weight:600}.removed-heading p{margin:6px 0 0}
 .removed-directories>p{margin:0;padding:12px 18px 16px}.removed-directories .setting-row{gap:16px}.setting-row code{overflow-wrap:anywhere;min-width:0;font:11px/1.6 var(--font-mono);color:var(--color-secondary)}
 .setting-row :global(button){flex-shrink:0}
</style>
