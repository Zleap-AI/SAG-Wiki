<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {doclingStatus,doclingDownloadOpen,refreshDoclingStatus} from '../lib/docling';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {DoclingStatus} from '../lib/types';
 const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
 let error='',loading=true,cancelling=false,disposed=false,timer:ReturnType<typeof setTimeout>;
 $:status=$doclingStatus??initial;
 $:steps=[$t('运行环境'),$t('Docling 与格式组件'),$t('文档识别模型')];
 $:step=['runtime','dependencies','models','done'].indexOf(status.stage);
 $:percent=status.total?status.downloaded/status.total*100:status.percent??null;
 $:label=status.running?(percent===null?$t('Docling 安装中'):$t('Docling 安装 {value0}%',{value0:Math.min(100,Math.round(percent))})):$t('Docling 安装失败');
 async function refresh(){
  clearTimeout(timer);
  try{await refreshDoclingStatus();if(!disposed){loading=false;error='';cancelling=false;}}
  catch(e){if(!disposed){loading=false;error=String(e);}}
  finally{if(!disposed){clearTimeout(timer);timer=setTimeout(refresh,status.running?700:3000);}}
 }
 onMount(()=>{void refresh();return()=>{disposed=true;clearTimeout(timer);};});
 async function install(){
  error='';
  try{await api('install_docling');await refresh();}catch(e){error=String(e);}
 }
 async function cancel(){
  error='';cancelling=true;
  try{await api('cancel_docling_install');await refresh();}catch(e){error=String(e);cancelling=false;}
 }
</script>
{#if status.running||status.error}<Button.Root type="button" class="docling-progress" aria-label={$t('查看 Docling 安装进度')} title={error?$t(error):$t(status.message)} onclick={()=>{doclingDownloadOpen.set(true);void refresh();}}><span>{label}</span></Button.Root>{/if}

{#if $doclingDownloadOpen}<Modal label={$t('下载文档解析组件')} onClose={()=>doclingDownloadOpen.set(false)} class="docling-dialog">
 <div class="dialog-head"><h2>{$t('下载文档解析组件')}</h2><Button.Root type="button" class="icon-button" aria-label={$t('关闭下载详情')} onclick={()=>doclingDownloadOpen.set(false)}>{@html icon('close')}</Button.Root></div>
 <div class="dialog-body">
  {#if loading}<p role="status">{$t('正在读取下载状态…')}</p>{/if}
  <p>{$t('将下载 Docling、运行环境和识别模型。文件在本机转换，原文件保持不变。')}</p>
  <p class="settings-help">{$t('组件可能占用数 GB 磁盘空间，实际大小取决于系统和所用格式。生成的 Markdown 保存在 .sag-converted，保存设置后开始转换和索引。')}</p>
  {#if status.running||status.stage==='done'||status.stage==='cancelled'||status.error}
   <ol class="download-steps">{#each steps as label,index}<li class:current={step===index} class:complete={step>index}>{#if step>index}{@html icon('check')}{:else}<span>{index+1}</span>{/if}{label}</li>{/each}</ol>
   <div class="download-state" role="status" aria-live="polite">
    {#if status.running}
     {#if status.total}<progress aria-label={$t('下载进度')} value={status.downloaded} max={status.total}></progress>
     {:else if status.percent!=null}<progress aria-label={$t('下载进度')} value={status.percent} max={100}></progress><p>{status.percent.toFixed(0)}%</p>
     {:else}<div class="download-indeterminate" role="progressbar" aria-label={$t('正在下载或安装组件…')}><span></span></div>{/if}
    {/if}
    {#if status.total}<p>{(status.downloaded/1048576).toFixed(1)} / {(status.total/1048576).toFixed(1)} MB</p>{/if}
    <p class="download-log">{$t(status.message)}</p>
   </div>
  {/if}
  {#if status.error||error}<p class="form-error" role="alert">{$t(status.error||error)}</p>{/if}
 </div>
 <div class="dialog-footer">
  {#if status.running}<Button.Root type="button" disabled={cancelling} onclick={cancel}>{$t(cancelling?'正在取消…':'取消下载')}</Button.Root><Button.Root type="button" class="primary" onclick={()=>doclingDownloadOpen.set(false)}>{$t('后台下载')}</Button.Root>
  {:else if status.installed&&!status.error}<Button.Root type="button" class="primary" onclick={()=>doclingDownloadOpen.set(false)}>{$t('完成')}</Button.Root>
  {:else}<Button.Root type="button" onclick={()=>doclingDownloadOpen.set(false)}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" onclick={install}>{$t(status.error||status.stage==='cancelled'?'重试下载':'下载并启用')}</Button.Root>{/if}
 </div>
</Modal>{/if}

<style>
 :global(.docling-dialog:not(.docling-formats-dialog)){width:560px;max-width:calc(100vw - 40px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head,.dialog-footer{flex-shrink:0}.dialog-head h2{font-size:18px;line-height:1.4}
 .dialog-body{overflow:auto;min-height:0}
 .download-indeterminate{height:6px;width:100%;border-radius:3px;background:var(--color-rule);overflow:hidden}.download-indeterminate span{display:block;width:35%;height:100%;border-radius:3px;background:var(--color-secondary);animation:download-progress 1.1s ease-in-out infinite alternate}@keyframes download-progress{from{transform:translateX(-100%)}to{transform:translateX(285%)}}@media(prefers-reduced-motion:reduce){.download-indeterminate span{animation:none;transform:translateX(90%)}}
 .download-steps{display:flex;gap:14px;list-style:none;padding:18px 0 8px;margin:0;flex-wrap:wrap}.download-steps li{display:flex;align-items:center;gap:6px;font-size:12px;color:var(--color-muted)}.download-steps li.current,.download-steps li.complete{color:var(--color-ink)}.download-steps li span{display:grid;place-items:center;border:1px solid var(--color-rule);width:20px;height:20px;border-radius:50%}.download-steps :global(svg){width:16px;height:16px}.download-state progress{width:100%;height:6px;accent-color:var(--color-ink)}.download-state p{font-size:12px;color:var(--color-muted)}.download-log{overflow-wrap:anywhere;max-height:96px;overflow:auto;white-space:pre-wrap}
</style>
