<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {checkUpdates,downloadUpdate,installUpdate,updateDownload,updateKey,updateState} from '../lib/updates';
 import type {UpdateSettings} from '../lib/types';
 export let config:UpdateSettings;
 export let onBeforeInstall:()=>Promise<boolean>=async()=>true;
 let confirming=false;
 $: state=$updateState.key===updateKey(config)?$updateState:null;
 $: transfer=$updateDownload.key===updateKey(config)?$updateDownload:null;
 $: transferring=$updateDownload.stage==='downloading'||$updateDownload.stage==='verifying'||$updateDownload.stage==='installing';
 $: percent=transfer?.total?Math.min(100,Math.floor(transfer.downloaded/transfer.total*100)):null;
 async function action(){
  if(transfer?.stage==='ready'){
   if(confirming)return;
   confirming=true;
   try{if(await onBeforeInstall())await installUpdate()}finally{confirming=false}
  }
  else if(state?.info?.available)await downloadUpdate(config,state.info);
  else await checkUpdates(config);
 }
</script>

<section class="settings-group app-updates" aria-label={$t('软件更新')}>
 <div class="setting-row">
  <div><strong>{$t('软件更新')}</strong><p>{$t('启动时及运行期间每隔 8 小时检查，发现新版后可下载并重启安装。')}</p></div>
  <Button.Root type="button" class={state?.info?.available?'primary':'outline'} disabled={state?.checking||transferring||confirming} onclick={action}>{state?.checking?$t('正在检查更新…'):transfer?.stage==='installing'?$t('正在安装更新…'):transfer?.stage==='verifying'?$t('正在校验更新…'):transfer?.stage==='downloading'?$t('正在下载更新…'):transfer?.stage==='ready'?$t('重启并更新'):state?.info?.available?$t('下载更新'):$t('检查更新')}</Button.Root>
 </div>
 <div class="update-content">
  {#if state?.info}
   <div class="update-result" role="status">
    {#if state.info.available}
     <strong>{$t('发现新版本 {value0}',{value0:state.info.latest_version})}</strong>
    {:else}<span>{$t('当前已是最新版本（{value0}）',{value0:state.info.current_version})}</span>{/if}
   </div>
   {#if state.info.available&&state.info.notes}<details class="update-notes"><summary>{$t('更新说明')}</summary><pre>{state.info.notes}</pre></details>{/if}
  {/if}
  {#if transfer?.stage==='downloading'}
   <div class="update-progress" role="status">
    <progress aria-label={$t('更新下载进度')} max="100" value={percent??undefined}></progress>
    <span>{percent===null?$t('已下载 {value0} MB',{value0:(transfer.downloaded/1048576).toFixed(1)}):`${percent}%`}</span>
    <p>{$t('下载期间可继续使用，关闭设置不会中断下载。')}</p>
   </div>
  {:else if transfer?.stage==='verifying'}<p class="update-status" role="status">{$t('正在校验更新…')}</p>
  {:else if transfer?.stage==='ready'}<p class="update-status" role="status">{$t('更新已下载并通过校验，重启后完成安装。')}</p>{/if}
  {#if state?.error||transfer?.error}<p class="form-error" role="alert">{$t('更新操作失败，请重试。')} {$t(transfer?.error||state?.error||'')}</p>{/if}
  {#if transfer?.error&&transfer.stage==='idle'}<Button.Root type="button" class="outline update-recheck" disabled={state?.checking} onclick={()=>checkUpdates(config)}>{$t('重新检查更新')}</Button.Root>{/if}
 </div>
</section>

<style>
 .update-content{padding:0 18px 18px}
 .update-result{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-top:20px;font-size:var(--text-ui);line-height:1.6}
 .update-result strong{font-weight:600}
 .update-notes{margin-top:16px;font-size:var(--text-caption);color:var(--color-secondary)}
 .update-notes summary{cursor:pointer}
 .update-notes pre{font:inherit;white-space:pre-wrap;overflow-wrap:anywhere;max-height:200px;overflow:auto;margin:12px 0 0}
 .update-progress{display:grid;grid-template-columns:1fr auto;align-items:center;gap:8px 12px;margin-top:16px;font-size:var(--text-caption);color:var(--color-secondary)}
 .update-progress progress{width:100%;height:6px;accent-color:var(--color-ink)}
 .update-progress p{grid-column:1/-1;margin:0}
 .update-status{margin:16px 0 0;font-size:var(--text-caption);color:var(--color-secondary)}
 .form-error{margin:16px 0 0;overflow-wrap:anywhere;font-size:var(--text-caption)}
 .update-content :global(.update-recheck){margin-top:12px}
</style>
