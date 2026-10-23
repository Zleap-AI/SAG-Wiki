<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import FolderIndexTypes from './FolderIndexTypes.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {basename,displayPath} from '../lib/domain';
 import {t} from '../lib/i18n';
 import type {FolderIndexOptions} from '../lib/types';
 export let path:string;
 export let onClose:()=>void;
 export let onSaved:(rebuild:boolean)=>void;
 let options:FolderIndexOptions|null=null,loading=true,saving=false,error='';
 onMount(()=>{let disposed=false;void api<FolderIndexOptions>('get_folder_index_options',{path}).then(value=>{if(!disposed)options=value}).catch(e=>{if(!disposed)error=String(e)}).finally(()=>{if(!disposed)loading=false});return()=>{disposed=true}});
 async function save(rebuild:boolean){
  saving=true;error='';
  try{await api('save_folder_index_options',{path,options,rebuild});onSaved(rebuild);onClose();}
  catch(e){error=String(e)}finally{saving=false}
 }
</script>
<Modal label={$t('索引设置')} class="folder-index-settings" onClose={()=>{if(!saving)onClose()}}>
 <div class="dialog-head"><h2>{$t('索引设置')}</h2><Button.Root class="icon-button" disabled={saving} aria-label={$t('关闭索引设置')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="dialog-body">
  <section class="folder-summary"><div class="folder-summary-title">{@html icon('folder')}<div><h3>{basename(path)}</h3><p class="folder-path">{displayPath(path)}</p></div></div><p class="folder-index-intro">{$t('应用于当前文件夹及未单独设置的子文件夹。取消勾选的类型不再索引，原文件仍可浏览。')}</p></section>
  <section class="folder-types-section" aria-label={$t('文件类型')}><div class="folder-section-heading"><h3>{$t('文件类型')}</h3></div><div class="folder-types-body">{#if loading}<p role="status">{$t('正在读取索引设置…')}</p>{:else if options}<FolderIndexTypes bind:value={options} disabled={saving}/>{/if}</div></section>
  <section class="folder-index-notes" aria-label={$t('规则说明')}><h3>{$t('规则说明')}</h3><p>{$t('选项保存在此文件夹的 .sagignore 中，已有手写规则会保留并继续生效。')}</p><p class="rebuild-note">{$t('重新索引会清除当前文件夹的旧索引，再按当前选项建立索引；原文件和其他文件夹不受影响。')}</p></section>
  {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
 <div class="dialog-footer"><Button.Root class="outline rebuild" disabled={loading||saving||!options} onclick={()=>save(true)}>{@html icon('refresh')}{$t('重新索引')}</Button.Root><Button.Root disabled={saving} onclick={onClose}>{$t('取消')}</Button.Root><Button.Root class="primary" disabled={loading||saving||!options} onclick={()=>save(false)}>{$t(saving?'正在保存…':'保存')}</Button.Root></div>
</Modal>
<style>
 :global(.dialog-panel.folder-index-settings){width:560px;max-width:calc(100vw - 40px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head{padding:20px 24px 16px;flex-shrink:0}
 .dialog-head h2{font-size:18px;line-height:1.4;letter-spacing:0}
 .dialog-body{min-height:0;overflow:auto;overscroll-behavior:contain;padding:0 24px 20px;display:flex;flex-direction:column;gap:16px}
 .dialog-body>section{flex-shrink:0;border:1px solid var(--color-rule);border-radius:10px;overflow:hidden}
 .folder-summary{padding:14px 16px;background:var(--color-panel)}
 .folder-summary-title{display:flex;align-items:flex-start;gap:10px}
 .folder-summary-title>div{min-width:0}
 .folder-summary-title :global(.icon){width:18px;height:18px;margin-top:1px;color:var(--color-secondary)}
 h3{font-size:13px;font-weight:600;line-height:1.5;margin:0}
 .folder-path{color:var(--color-muted);font-size:12px;line-height:1.6;overflow-wrap:anywhere;margin:0}
 .folder-summary .folder-path{margin-top:4px}
 .folder-index-intro{font-size:12px;line-height:1.6;color:var(--color-secondary);margin:10px 0 0}
 .folder-section-heading{padding:10px 16px;background:var(--color-sidebar);border-bottom:1px solid var(--color-rule)}
 .folder-types-body{padding:0 16px;background:var(--color-paper)}
 .folder-index-notes{display:grid;gap:8px;padding:12px 16px;background:var(--color-panel)}
 .folder-index-notes p{font-size:12px;line-height:1.6;color:var(--color-muted);margin:0;overflow-wrap:anywhere}
 .dialog-footer{flex-shrink:0;padding:12px 24px}
 :global(.folder-index-settings .dialog-footer .rebuild){display:flex;align-items:center;gap:7px;margin-right:auto}
 :global(.folder-index-settings .rebuild .icon){width:14px;height:14px}
</style>
