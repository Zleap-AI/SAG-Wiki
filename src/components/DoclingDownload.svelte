<script lang="ts">
 import {onMount} from 'svelte';
 import {get} from 'svelte/store';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {doclingStatus,doclingDownloadOpen,doclingInstallRequest,openDoclingProgress,refreshDoclingStatus} from '../lib/docling';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {DoclingStatus} from '../lib/types';
 const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
 let error='',refreshError='',loading=true,cancelling=false,finishing=false,disposed=false,timer:ReturnType<typeof setTimeout>;
 let log='',logOpen=false,logLoading=false;
 $:status=$doclingStatus??initial;
 $:context=$doclingInstallRequest;
 $:complete=status.installed&&!status.running&&!status.error;
 $:steps=[$t('运行环境'),$t('Docling 与格式组件'),$t('文档识别模型')];
 $:step=['runtime','dependencies','models','done'].indexOf(status.stage);
 $:percent=status.total?status.downloaded/status.total*100:status.percent??null;
 $:label=status.running?(percent===null?$t('Docling 安装中'):$t('Docling 安装 {value0}%',{value0:Math.min(100,Math.round(percent))})):$t('Docling 安装失败');
 $:activity=status.running?status.message.split('\n').map(line=>line.trim()).filter(Boolean).at(-1)??'':'';
 async function openLog(){
  logLoading=true;error='';
  try{log=await api<string>('get_docling_install_log');logOpen=true;}catch(e){error=String(e)}finally{logLoading=false}
 }
 async function refresh(fresh=false){
  clearTimeout(timer);
  // A failed status read is reported separately so it never hides an action error.
  try{await refreshDoclingStatus(fresh);if(!disposed){loading=false;refreshError='';cancelling=false;}}
  catch(e){if(!disposed){loading=false;refreshError=String(e);}}
  finally{if(!disposed){clearTimeout(timer);timer=setTimeout(refresh,status.running?700:3000);}}
 }
 onMount(()=>{void refresh();return()=>{disposed=true;clearTimeout(timer);};});
 async function install(){
  const request=context;
  if(request?.pending||status.running)return;
  if(request){request.pending=true;doclingInstallRequest.set(request);}
  error='';logOpen=false;
  try{
   await api('install_docling');
   if(request)request.started=true;
   await refresh(true);
  }catch(e){if(!request||get(doclingInstallRequest)===request)error=String(e);}finally{if(request){request.pending=false;if(get(doclingInstallRequest)===request)doclingInstallRequest.set(request);}}
 }
 async function cancel(){
  if(context){context.valid=false;context.apply=undefined;doclingInstallRequest.set(context);}
  error='';cancelling=true;
  try{await api('cancel_docling_install');await refresh(true);}catch(e){error=String(e);cancelling=false;}
 }
 function close(background=false){
  if(!background&&context){context.valid=false;context.apply=undefined;doclingInstallRequest.set(context);}
  doclingDownloadOpen.set(false);
 }
 async function finish(){
  if(finishing||context?.pending)return;
  const request=context,apply=request?.apply;
  if(context){context.apply=undefined;doclingInstallRequest.set(context);}
  finishing=true;
  try{await apply?.(()=>!!request?.valid);if(!request||get(doclingInstallRequest)===request)doclingDownloadOpen.set(false);}catch(e){if(get(doclingInstallRequest)===request)error=String(e);}finally{finishing=false;}
 }
</script>
{#if status.running||status.error}<Button.Root type="button" class="docling-progress" aria-label={$t('查看 Docling 安装进度')} title={error||refreshError?$t(error||refreshError):$t(status.message)} onclick={()=>{openDoclingProgress();void refresh();}}><span>{label}</span></Button.Root>{/if}

{#if $doclingDownloadOpen}<Modal label={$t('下载文档解析组件')} onClose={()=>close()} class="docling-dialog">
 <div class="dialog-head"><h2>{$t('下载文档解析组件')}</h2><Button.Root type="button" class="icon-button" aria-label={$t('关闭下载详情')} onclick={()=>close()}>{@html icon('close')}</Button.Root></div>
 <div class="dialog-body">
  {#if loading}<p role="status">{$t('正在读取下载状态…')}</p>{/if}
  <p>{$t('将在 NAS 上下载 Docling、运行环境和识别模型，用于图片识别、扫描件 OCR 和音视频转写。文件在 NAS 转换，原文件保持不变。')}</p>
  {#if context?.formats.length}<p class="selected-formats"><strong>{$t('所选格式')}</strong> {context.formats.map(extension=>extension.toUpperCase()).join('、')}</p>{/if}
  <p class="settings-help">{$t('组件可能占用数 GB 磁盘空间，实际大小取决于系统和所用格式。生成的 Markdown 保存在 .sag-converted，保存设置后开始转换和索引。')}</p>
  {#if status.running||status.stage==='done'||status.stage==='cancelled'||status.error}
   <ol class="download-steps">{#each steps as label,index}<li class:current={step===index} class:complete={step>index}>{#if step>index}{@html icon('check')}{:else}<span>{index+1}</span>{/if}{label}</li>{/each}</ol>
   <div class="download-state" role="status" aria-live="polite">
    {#if status.running}
     {#if status.total}<progress aria-label={$t('下载进度')} value={status.downloaded} max={status.total}></progress>
     {:else if status.percent!=null}<progress aria-label={$t('下载进度')} value={status.percent} max={100}></progress><p>{status.percent.toFixed(0)}%</p>
     {:else}<div class="download-indeterminate" role="progressbar" aria-label={$t('正在下载或安装组件…')}><span></span></div>{/if}
    {/if}
    {#if status.total}<p>{(status.downloaded/1048576).toFixed(1)} / {(status.total/1048576).toFixed(1)} MB{#if status.packages_total}　{$t('已完成 {value0} / {value1} 个包',{value0:status.packages_done??0,value1:status.packages_total})}{/if}</p>{/if}
    <p class="download-log">{status.running?(steps[Math.min(2,Math.max(0,step))]):complete?$t('Docling 已准备就绪'):$t('安装未完成，可重试。')}</p>
    {#if activity}<p class="download-activity" aria-label={$t('最近活动')}>{activity}</p>{/if}
    {#if status.running||status.error}<Button.Root type="button" class="download-log-link" disabled={logLoading} onclick={openLog}>{$t('查看安装日志')}</Button.Root>{/if}
    {#if logOpen}<pre class="install-log" aria-label={$t('安装日志')}>{log}</pre>{/if}
   </div>
  {/if}
  {#if error||refreshError||status.error}<p class="form-error" role="alert">{$t(error||refreshError||status.error||"")}</p>{/if}
 </div>
 <div class="dialog-footer">
  {#if status.running}<Button.Root type="button" disabled={cancelling} onclick={cancel}>{$t(cancelling?'正在取消…':'取消下载')}</Button.Root><Button.Root type="button" class="primary" onclick={()=>close(true)}>{$t('后台下载')}</Button.Root>
  {:else if complete}<Button.Root type="button" class="primary" disabled={finishing||context?.pending} onclick={finish}>{$t(context?.apply?'应用所选格式':'完成')}</Button.Root>
  {:else}<Button.Root type="button" onclick={()=>close()}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={context?.pending} onclick={install}>{$t(context?.pending?'正在安装…':status.error||status.stage==='cancelled'||error||refreshError?'重试下载':'下载并启用')}</Button.Root>{/if}
 </div>
</Modal>{/if}

<style>
 .selected-formats{font-size:13px;line-height:1.7;overflow-wrap:anywhere}.selected-formats strong{font-weight:600;margin-right:8px}
 :global(.docling-dialog:not(.docling-formats-dialog)){width:560px;max-width:calc(100vw - 40px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head,.dialog-footer{flex-shrink:0}.dialog-head h2{font-size:18px;line-height:1.4}
 .dialog-body{overflow:auto;min-height:0}
 .download-indeterminate{height:6px;width:100%;border-radius:3px;background:var(--color-rule);overflow:hidden}.download-indeterminate span{display:block;width:35%;height:100%;border-radius:3px;background:var(--color-secondary);animation:download-progress 1.1s ease-in-out infinite alternate}@keyframes download-progress{from{transform:translateX(-100%)}to{transform:translateX(285%)}}@media(prefers-reduced-motion:reduce){.download-indeterminate span{animation:none;transform:translateX(90%)}}
 .download-steps{display:flex;gap:14px;list-style:none;padding:18px 0 8px;margin:0;flex-wrap:wrap}.download-steps li{display:flex;align-items:center;gap:6px;font-size:12px;color:var(--color-muted)}.download-steps li.current,.download-steps li.complete{color:var(--color-ink)}.download-steps li span{display:grid;place-items:center;border:1px solid var(--color-rule);width:20px;height:20px;border-radius:50%}.download-steps :global(svg){width:16px;height:16px}.download-state progress{width:100%;height:6px;accent-color:var(--color-ink)}.download-state p{font-size:12px;color:var(--color-muted)}.download-log{overflow-wrap:anywhere;max-height:96px;overflow:auto;white-space:pre-wrap}.download-activity{font-family:var(--font-mono);font-size:11px;color:var(--color-muted);overflow-wrap:anywhere;white-space:pre-wrap;max-height:48px;overflow:hidden}:global(.download-log-link){font-size:12px;padding:4px 0;color:var(--color-secondary);text-decoration:underline}.install-log{font-family:var(--font-mono);font-size:11px;line-height:1.5;color:var(--color-secondary);background:var(--color-panel);border:1px solid var(--color-rule);border-radius:7px;padding:8px 10px;max-height:160px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;margin-top:8px}
</style>
