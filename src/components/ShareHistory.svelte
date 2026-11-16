<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {copyText} from '../lib/web-connection';
 import {icon} from '../lib/icons';
 export let onClose:()=>void;
 type Record={code:string;name:string;shared_at:number;expires_at:string};
 let records:Record[]=[],loading=true,busy='',error='',confirmCancel:Record|null=null;
 let refreshing=false;
 async function load(){if(refreshing||busy)return;refreshing=true;try{records=await api<Record[]>('get_share_history')}catch(e){error=String(e)}finally{refreshing=false;loading=false}}
 const timer=setInterval(()=>{if(records.some(record=>Date.parse(record.expires_at)<=Date.now()))void load()},1000);
 onDestroy(()=>clearInterval(timer));
 onMount(()=>{void load()});
 async function cancel(){
  const record=confirmCancel!;confirmCancel=null;busy=record.code;error='';
  try{await api('cancel_share',{code:record.code});records=records.filter(item=>item.code!==record.code)}catch(e){error=String(e)}finally{busy=''}
 }
 async function copy(record:Record){try{await copyText($t('分享码：{value0}\n复制此分享码，打开SAG-Wiki即可下载对应文件内容',{value0:'SAG-'+record.code}))}catch(e){error=String(e)}}
</script>
<Modal label={$t('分享记录')} class="share-history" onClose={()=>{if(!busy)onClose()}}>
 <div class="dialog-head"><h2>{$t('分享记录')}</h2><Button.Root class="icon-button" aria-label={$t('关闭')} disabled={!!busy} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="history-body">
 {#if loading}<p role="status">{$t('正在读取…')}</p>{:else if !records.length}<p class="empty-history">{$t('暂无分享记录')}</p>{:else}
  <ul>{#each records as record (record.code)}<li>
   <div class="record-heading"><strong>{record.name}</strong></div>
   <code>SAG-{record.code}</code>
   <div class="record-dates"><span>{$t('分享时间')} {new Date(record.shared_at*1000).toLocaleString()}</span><span>{$t('到期时间')} {new Date(record.expires_at).toLocaleString()}</span></div>
   <div class="record-actions"><Button.Root class="outline small" disabled={!!busy} onclick={()=>copy(record)}>{$t('复制分享码')}</Button.Root><Button.Root class="outline small" disabled={!!busy} onclick={()=>confirmCancel=record}>{$t(busy===record.code?'正在取消…':'取消分享')}</Button.Root></div>
  </li>{/each}</ul>
 {/if}
 {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
 <div class="dialog-footer"><Button.Root class="primary" disabled={!!busy} onclick={onClose}>{$t('关闭')}</Button.Root></div>
</Modal>
{#if confirmCancel}<Modal label={$t('取消分享')} onClose={()=>confirmCancel=null}><div class="dialog-head"><h2>{$t('取消分享')}</h2></div><div class="dialog-body"><p>{$t('取消后分享码立即失效，服务器文件将被删除。已下载的副本不受影响。')}</p></div><div class="dialog-footer"><Button.Root class="outline" onclick={()=>confirmCancel=null}>{$t('返回')}</Button.Root><Button.Root class="primary" onclick={cancel}>{$t('确认取消')}</Button.Root></div></Modal>{/if}
<style>
 :global(.share-history){width:min(620px,calc(100vw - 32px));max-height:calc(100dvh - 64px);display:flex;flex-direction:column}
 .history-body{padding:0 24px 24px;overflow:auto;min-height:100px}
 ul{list-style:none;padding:0;margin:0}li{padding:18px 0;border-bottom:1px solid var(--color-rule)}li:last-child{border:0}
 .record-heading{display:flex;justify-content:space-between;gap:16px;align-items:baseline}.record-heading strong{font-size:15px;overflow-wrap:anywhere}
 code{display:block;margin:8px 0;font-size:13px}.record-dates{display:flex;flex-wrap:wrap;gap:6px 20px;font-size:12px;color:var(--color-secondary)}.record-actions{display:flex;gap:8px;margin-top:12px}.empty-history{color:var(--color-secondary);text-align:center;padding:32px 0}
</style>
