<script lang="ts">
 import {onMount,onDestroy,tick} from 'svelte';
 import {Button} from 'bits-ui';
 import {open} from '@tauri-apps/plugin-dialog';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import ShareHelp from './ShareHelp.svelte';
 import ShareHistory from './ShareHistory.svelte';
 let historyOpen=false;
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {copyText} from '../lib/web-connection';
 export let embedded=false;
 export let busy=false;
 export let path='';
 export let initialCode='';
 export let initialDirectory='';
 export let onClose:()=>void;
 export let onReceived:(path:string)=>void;
 let codeInput:HTMLInputElement|null=null;
 type Preview={id:string;name:string;markdown_count:number;image_count:number;size:number};
 let preview:Preview|null=null,error='',code=initialCode,shared='',expires='',directory='',received='',copied=false,preparing=true;
 $: {const match=code.trim().match(/^(?:(?:分享码[：:]|share code:)\s*)?(?:SAG-)?([A-HJ-NP-Z2-9]{8})(?![A-Z0-9])/i);if(match)code='SAG-'+match[1].toUpperCase();}
 $: displaySize=preview ? (preview.size<1024 ? `${preview.size} B` : preview.size<1048576 ? `${(preview.size/1024).toFixed(1)} KB` : `${(preview.size/1048576).toFixed(2)} MB`) : '';
 const close=()=>{if(!busy)onClose()};
 onMount(async()=>{
  busy=true;
  try{directory=initialDirectory||(await api<{directory:string}>('get_share_settings')).directory;if(path)preview=await api<Preview>('prepare_share',{path})}
  catch(e){error=String(e)}finally{busy=false;preparing=false;await tick();if(!path)codeInput?.focus()}
 });
 onDestroy(()=>{if(preview)void api('close_share',{id:preview.id}).catch(console.error)});
 async function changeDirectory(){try{const value=await open({directory:true,multiple:false,title:$t('选择保存位置')});if(value)directory=value;error=''}catch(e){error=String(e)}}
 async function submit(){
  busy=true;error='';
  try{
   if(path&&preview){const result=await api<{code:string;expiresAt:string}>('upload_share',{id:preview.id});shared='SAG-'+result.code;expires=result.expiresAt;}
   else{const result=await api<{path:string}>('receive_share',{code,directory});received=result.path;onReceived(result.path);}
  }catch(e){error=String(e)}finally{busy=false}
 }
</script>
{#snippet content()}
 {#if !embedded}<div class="dialog-head"><h2>{$t(path?'分享':'接收分享')}</h2><Button.Root class="icon-button" aria-label={$t('关闭')} disabled={busy} onclick={close}>{@html icon('close')}</Button.Root></div>{/if}
 <div class="share-body">
  {#if received}<p class="share-complete">{$t('下载完成')}</p><p class="share-path">{received}</p>
  {:else if shared}<div class="share-result"><div class="share-code-row"><p class="share-code">{shared}</p><ShareHelp onError={message=>error=message}/></div><p class="share-instructions">{$t('将分享码发给对方。对方复制后打开 SAG 即可接收，也可在“设置 → 数据源”中输入分享码。')}</p><Button.Root class="outline" onclick={async()=>{try{await copyText($t('分享码：{value0}\n复制此分享码，打开SAG-Wiki即可下载对应文件内容',{value0:shared}));copied=true;error=''}catch(e){error=String(e)}}}>{@html icon('copy')}{$t(copied?'已复制':'复制分享码')}</Button.Root><p class="share-hint share-expiry"><span aria-hidden="true">{@html icon('clock')}</span>{$t('有效期至 {value0}',{value0:new Date(expires).toLocaleString()})}</p></div>
  {:else if path}
   {#if preview}
    <div class="share-package">
     <span class="share-package-icon" aria-hidden="true">{@html icon(path.toLowerCase().endsWith('.md')?'file':'folder')}</span>
     <div class="share-package-details">
      <h3>{preview.name}</h3>
      <p class="share-metadata">{$t(preview.markdown_count===1?'1 篇文档':'{value0} 篇文档',{value0:preview.markdown_count})} · {$t(preview.image_count===1?'1 张图片':'{value0} 张图片',{value0:preview.image_count})}</p>
     </div>
     <span class="share-size">{displaySize}</span>
    </div>
    <p class="share-hint share-expiry"><span aria-hidden="true">{@html icon('clock')}</span>{$t('分享有效期为 1 小时')}</p>{:else if busy}<p role="status">{$t('正在准备分享…')}</p>{/if}
  {:else}
   <div class="share-label"><div class="share-label-heading"><label for="share-code-input">{$t('分享码')}</label><ShareHelp onError={message=>error=message}/></div><Input id="share-code-input" bind:ref={codeInput} data-autofocus bind:value={code} placeholder="SAG-XXXXXXXX" disabled={busy}/></div>
   <div class="share-destination"><div class="share-location"><span class="share-label">{$t('保存位置')}</span><span class="share-path">{directory}</span></div><Button.Root class="text-button" disabled={busy} onclick={changeDirectory}>{$t('更改')}</Button.Root></div>
  {/if}
  {#if error}<p class="share-error" role="alert">{$t(error)}</p>{/if}
 </div>
 <div class="dialog-footer">{#if path}<Button.Root type="button" class="share-history-button" disabled={busy} onclick={()=>historyOpen=true}>{$t('分享记录')}</Button.Root>{/if}<Button.Root class="outline" disabled={busy} onclick={close}>{$t(shared||received?'关闭':'取消')}</Button.Root>{#if !shared&&!received}<Button.Root class="primary" disabled={busy||(path?!preview:!code.trim()||!directory)} onclick={submit}><span aria-hidden="true">{@html icon(path?'share':'download')}</span>{$t(preparing?'正在准备…':busy?(path?'正在上传…':'正在下载…'):(path?'分享':'下载'))}</Button.Root>{/if}</div>
{/snippet}
{#if embedded}<div class="share-embedded">{@render content()}</div>{:else}<Modal label={$t(path?'分享':'接收分享')} class="share-dialog" onClose={close}>{@render content()}</Modal>{/if}
{#if historyOpen}<ShareHistory onClose={()=>historyOpen=false}/>{/if}
<style>
 .dialog-footer :global(.share-history-button){margin-right:auto;color:var(--color-secondary)}
 .share-label-heading,.share-code-row{display:flex;align-items:center;gap:4px}
 .share-code-row{justify-content:center;position:relative}
 .share-code-row :global(.share-help){position:absolute;left:calc(100% + 4px)}
 .share-label-heading{min-height:24px}
 .share-embedded{width:100%}
 .share-embedded .share-body{padding:4px 24px 32px}
 .share-package{display:flex;align-items:center;gap:12px;padding:16px;min-width:0;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-panel)}
 .share-package-icon{display:grid;place-items:center;flex-shrink:0;width:32px;height:40px;color:var(--color-secondary)}
 .share-package-icon :global(.icon){width:28px;height:28px;stroke-width:1.25}
 .share-package-details{min-width:0;flex:1;display:flex;flex-direction:column;gap:4px}
 .share-package h3{font-size:15px;font-weight:500;line-height:1.5;overflow-wrap:anywhere}
 .share-metadata{font-size:12px;line-height:18px;color:var(--color-muted);font-variant-numeric:tabular-nums}
 .share-size{flex-shrink:0;align-self:center;font-size:12px;line-height:20px;color:var(--color-secondary);font-variant-numeric:tabular-nums}
 .share-expiry{display:flex;align-items:center;gap:8px;line-height:20px}
 .share-expiry :global(.icon){display:block;width:14px;height:14px;stroke-width:1.5}
 .dialog-head{padding:20px 24px 4px}
 .dialog-footer :global(.primary){display:inline-flex;align-items:center;justify-content:center;gap:8px}
 .dialog-footer :global(.primary .icon){display:block;width:15px;height:15px;stroke-width:1.5}
 :global(.share-dialog){width:min(480px,calc(100vw - 32px))}.share-body{padding:20px 24px 24px;display:flex;flex-direction:column;gap:16px;min-width:0}.share-body h3,.share-body p{margin:0}.share-label{display:flex;flex-direction:column;gap:8px}.share-result{display:flex;flex-direction:column;align-items:center;gap:20px;padding:8px 0;text-align:center}.share-result :global(button){display:inline-flex;align-items:center;justify-content:center;gap:8px}.share-instructions{max-width:360px;font-size:13px;line-height:1.7;color:var(--color-secondary)}.share-code{font-size:28px;font-weight:600;letter-spacing:.05em;font-variant-numeric:tabular-nums}.share-hint{font-size:13px;color:var(--color-muted)}.share-path{overflow-wrap:anywhere;font-size:13px}.share-location{display:flex;flex-direction:column;gap:8px;min-width:0}.share-location>.share-label{font-size:13px}.share-destination{display:flex;align-items:center;gap:16px;justify-content:space-between}.share-error{color:var(--color-danger,#b42318);font-size:13px;white-space:pre-wrap}.share-complete{font-weight:600}
</style>
