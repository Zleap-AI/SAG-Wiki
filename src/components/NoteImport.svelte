<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Switch from './ui/Switch.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {language,t} from '../lib/i18n';
 import type {NoteSource} from '../lib/note-import';
 import {noteSources} from '../lib/note-import';
 export let source:NoteSource;
 export let onClose:()=>void;
 export let onImported:()=>Promise<void>;
 let frame:HTMLIFrameElement;
 let picker:HTMLInputElement;
 let ready=false,busy=false,loading=true,error='',destination='',base:string|null=null,applePath:string|null=null;
 let session:number|null=null,folders:{ID:number;NAME:string}[]=[],selected:number[]=[];
 let files:File[]=[],includeArchived=true,current=0,total=0,disposed=false;
 let converted:typeof result=null;
 let needsPermission=false,waitingForPermission=false;
 let result:{notes:number;attachments:number;log:{outcome:string;name:string;reason:string}[]}|null=null;
 $:meta=noteSources[source];
 $:title=$t('从 {value0} 导入',{value0:meta.name});
 $:issues=result?.log.filter(item=>item.outcome!=='message')??[];
 $:allFailed=!!result&&result.notes===0&&result.log.some(item=>item.outcome==='failed');
 function post(data:object){frame.contentWindow!.postMessage({channel:'sag-note-import',...data},location.origin);}
 async function prepare(){
  loading=true;error='';needsPermission=false;folders=[];selected=[];
  try{
   if(session!==null){await api('close_note_import',{session});session=null;}
   const data=await api<{session:number;destination:string;folders:typeof folders}>('prepare_note_import',{source,destination:base,applePath});
   if(disposed){await api('close_note_import',{session:data.session});return;}
   session=data.session;destination=data.destination;folders=data.folders;selected=folders.map(folder=>folder.ID);
  }catch(e){if(source==='apple'&&String(e)==='APPLE_NOTES_PERMISSION_REQUIRED')needsPermission=true;else error=String(e)}finally{loading=false}
 }
 async function openPermissionSettings(){
  error='';waitingForPermission=true;
  try{await api('open_full_disk_access');}catch(e){waitingForPermission=false;error=String(e)}
 }
 function checkOnReturn(){if(waitingForPermission&&!loading&&!busy){waitingForPermission=false;void prepare();}}
 async function chooseDestination(){
  try{const chosen=await api<string|null>('choose_note_import_folder');if(chosen){base=chosen;await prepare();}}catch(e){error=String(e)}
 }
 async function chooseAppleSource(){
  try{const chosen=await api<string|null>('choose_note_import_folder');if(chosen){applePath=chosen;await prepare();}}catch(e){error=String(e)}
 }
 async function finish(){
  try{destination=await api<string>('finish_note_import',{session});session=null;result=converted;await onImported();}
  catch(e){error=String(e)}finally{busy=false}
 }
 function run(){
  if(converted){busy=true;error='';void finish();return;}
  busy=true;error='';current=0;total=0;
  post({type:'run',source,files,session,folders:selected,includeArchived,language:$language});
 }
 async function receive(event:MessageEvent){
  if(event.source!==frame?.contentWindow||event.origin!==location.origin||event.data?.channel!=='sag-note-import')return;
  const data=event.data;
  if(data.type==='ready'){ready=true;return;}
  if(data.type==='native'){
   // The importer frame only receives this run's narrow file/database capabilities.
   if(!['query_apple_notes','read_apple_attachment','write_note_import_file','read_note_import_files'].includes(data.command)||data.args?.session!==session)return;
   try{post({type:'native-result',request:data.request,result:await api(data.command,data.args)});}
   catch(e){post({type:'native-result',request:data.request,error:String(e)});}return;
  }
  if(data.type==='progress'){if(data.total){current=data.current;total=data.total;}return;}
  if(data.type==='error'){error=data.message;busy=false;return;}
  if(data.type==='complete'){
   converted=data;await finish();
  }
 }
 onMount(()=>{
  window.addEventListener('message',receive);window.addEventListener('focus',checkOnReturn);void prepare();
  return()=>{disposed=true;window.removeEventListener('message',receive);window.removeEventListener('focus',checkOnReturn);if(session!==null)void api('close_note_import',{session}).catch(e=>console.error('Could not close note import',e));};
 });
</script>
<Modal label={title} class="note-import" onClose={()=>{if(!busy)onClose()}}>
 <header><h2>{title}</h2><Button.Root class="icon-button" aria-label={$t('关闭')} disabled={busy} onclick={onClose}>{@html icon('close')}</Button.Root></header>
 <div class="note-import-content">
  {#if result}
   {#if allFailed}<div class="import-failure" role="alert"><h3>{$t('导入失败')}</h3><p>{$t('没有笔记成功导入，请查看下方原因。')}</p></div>{:else}<div class="import-result" role="status">{@html icon('check')}<h3>{$t('已导入 {value0} 篇笔记',{value0:result.notes})}</h3><p>{$t('{value0} 个附件',{value0:result.attachments})}</p></div>{/if}
   {#if issues.length}<details open={allFailed}><summary>{$t('{value0} 项未导入',{value0:issues.length})}</summary>{#each issues as item}<p class="import-issue">{item.name}：{item.reason}</p>{/each}</details>{/if}
  {:else if source==='apple'}
   <div class="source-heading"><h3>{$t('备忘录文件夹')}</h3><Button.Root class="text-button" disabled={busy||loading} onclick={chooseAppleSource}>{$t('选择数据文件夹')}</Button.Root></div>
   {#if loading}<p class="muted" role="status">{$t('正在读取文件夹…')}</p>
   {:else if needsPermission}
    <section class="permission-guide" aria-label={$t('授权读取备忘录')}>
     <h3>{$t('允许读取 Apple Notes')}</h3>
     <p>{$t('在系统设置的“完全磁盘访问权限”中开启当前应用，然后返回这里重新检查。')}</p>
     <div class="permission-actions"><Button.Root class="primary" onclick={openPermissionSettings}>{$t('打开系统设置')}</Button.Root><Button.Root class="outline" onclick={()=>prepare()}>{$t('重新检查')}</Button.Root></div>
    </section>
   {:else if folders.length}<div class="note-folders">{#each folders as folder}<label><input type="checkbox" value={folder.ID} bind:group={selected} disabled={busy}/><span>{folder.NAME}</span></label>{/each}</div>
   {:else if !error}<p class="muted">{$t('没有找到备忘录文件夹')}</p>{/if}
   {#if !needsPermission}<p class="source-hint">{$t('仅导入本机已下载的备忘录。锁定笔记需先在备忘录中解锁。')}</p>{/if}
  {:else}
   <p class="source-hint">{$t(meta.hint)}</p>
   <input bind:this={picker} hidden type="file" accept={meta.accept} multiple={source==='evernote'} disabled={busy} on:change={()=>files=Array.from(picker.files??[])}/>
   <div class="file-choice"><span>{@html icon('file')}{files.length?$t('已选择 {value0} 个文件',{value0:files.length}):$t('尚未选择文件')}</span><Button.Root class="outline" data-autofocus disabled={busy} onclick={()=>picker.click()}>{$t(files.length?'重新选择':'选择文件')}</Button.Root></div>
   {#if files.length}<ul class="file-list">{#each files as file}<li>{file.name}</li>{/each}</ul>{/if}
   {#if source==='keep'}<label class="archived"><Switch label={$t('包含已归档笔记')} bind:checked={includeArchived} disabled={busy}/><span>{$t('包含已归档笔记')}</span></label>{/if}
  {/if}
  {#if error}<p class="import-error" role="alert">{$t(error)}</p>{/if}
  {#if busy}<div class="import-progress" role="status"><span>{$t('正在导入…')}</span>{#if total}<span>{current} / {total}</span>{/if}</div>{/if}
 </div>
 <div class="note-destination"><span>{$t('保存位置')}</span><div title={destination}>{@html icon('folder')}<span>{destination||'—'}</span></div>{#if !result}<Button.Root class="text-button" disabled={busy||loading} onclick={chooseDestination}>{$t('更改')}</Button.Root>{/if}</div>
 <footer>{#if !result}<Button.Root class="outline" disabled={busy} onclick={onClose}>{$t('取消')}</Button.Root><Button.Root class="primary" disabled={busy||loading||!ready||session===null||(source==='apple'?!selected.length:!files.length)} onclick={run}>{$t(converted?'重试':'导入')}</Button.Root>{:else}<Button.Root class="primary" onclick={onClose}>{$t('完成')}</Button.Root>{/if}</footer>
 <iframe bind:this={frame} src="/importer/frame.html" title="Note converter" hidden></iframe>
</Modal>
<style>
 :global(.dialog-panel.note-import){width:640px;max-width:calc(100vw - 48px);max-height:calc(100dvh - 80px);display:flex;flex-direction:column;overflow:hidden;background:var(--color-paper);font:400 14px/1.5 var(--font-body)}
 header{display:flex;align-items:center;justify-content:space-between;padding:20px 24px;border-bottom:1px solid var(--color-rule)}
 h2{font-size:18px;line-height:24px;font-weight:500;margin:0}h3{font-size:14px;font-weight:500;margin:0}
 header :global(.icon-button){width:32px;height:32px;padding:8px;color:var(--color-secondary)}
 :global(.note-import .icon){width:16px;height:16px;flex-shrink:0}
 .note-import-content{padding:24px;overflow:auto;min-height:160px}
 .source-hint{margin:0 0 20px;color:var(--color-secondary);line-height:1.7}
 .file-choice,.source-heading{display:flex;align-items:center;justify-content:space-between;gap:16px}
 .file-choice>span{display:flex;align-items:center;gap:8px;color:var(--color-secondary)}
 .file-list{list-style:none;padding:0;margin:16px 0 0;color:var(--color-secondary);font-size:13px;max-height:120px;overflow:auto;overflow-wrap:anywhere}
 .archived{display:flex;align-items:center;gap:10px;margin-top:24px}
 .note-folders{display:grid;gap:4px;max-height:260px;overflow:auto;margin:16px 0}
 .note-folders label{display:flex;gap:10px;align-items:center;padding:8px;border-radius:6px;background:var(--color-panel)}
 .note-folders input{accent-color:var(--color-ink)}
 .source-heading+.muted{margin:20px 0}.source-heading~.source-hint{font-size:12px;margin-bottom:0}
 .permission-guide{margin-top:20px;padding:20px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel)}
 .permission-guide p{margin:8px 0 16px;color:var(--color-secondary);line-height:1.7}
 .permission-actions{display:flex;flex-wrap:wrap;gap:8px}
 .muted{color:var(--color-muted)}
 .note-destination{display:flex;align-items:center;gap:12px;padding:16px 24px;border-block:1px solid var(--color-rule);background:var(--color-panel);font-size:12px;color:var(--color-secondary)}
 .note-destination>span{flex-shrink:0}.note-destination>div{display:flex;align-items:center;gap:8px;min-width:0;flex:1}.note-destination>div>span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 :global(.note-import .text-button){padding:4px;font-size:12px;color:var(--color-secondary);flex-shrink:0}
 footer{display:flex;justify-content:flex-end;gap:8px;padding:16px 24px}
 .import-error{color:var(--color-error,#b42318);overflow-wrap:anywhere;margin:20px 0 0;font-size:13px}
 .import-progress{display:flex;justify-content:space-between;margin-top:20px;color:var(--color-secondary);font-variant-numeric:tabular-nums}
 .import-failure{padding:12px 0}.import-failure h3{font-size:18px}.import-failure p{margin:8px 0 0;color:var(--color-secondary)}
 .import-result{text-align:center;padding:12px 0}.import-result :global(.icon){width:24px;height:24px}.import-result h3{font-size:18px;margin-top:12px}.import-result p{color:var(--color-secondary);margin:6px 0}
 details{margin-top:20px;font-size:12px}.import-issue{overflow-wrap:anywhere;color:var(--color-secondary)}
 @media(max-width:600px){.note-destination{flex-wrap:wrap}.note-destination>div{flex-basis:70%}}
</style>
