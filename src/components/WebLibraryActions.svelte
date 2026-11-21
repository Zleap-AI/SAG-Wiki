<script lang="ts">
 import {onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {uploadWebFile} from '../lib/web-connection';
 import Input from './ui/Input.svelte';
 import Modal from './ui/Modal.svelte';

 export let folder:string;
 export let onCreated:(path:string)=>Promise<void>;
 export let onUploaded:()=>Promise<void>;
 export let onCreateDocument:(()=>void)|undefined=undefined;
 export let creating:boolean;
 export let canUpload:boolean;

 let filePicker:HTMLInputElement,folderPicker:HTMLInputElement;
 let name='',createError='',saving=false;
 let upload:AbortController|null=null;
 let finished=0,total=0,uploaded=0,failures:{path:string;message:string}[]=[];
 onDestroy(()=>upload?.abort());

 function open(){
  name='';createError='';creating=true;
 }
 function close(){if(!saving)creating=false;}
 async function create(){
  saving=true;createError='';
  try{
   const path=await api<string>('create_web_folder',{name:name.trim(),parent:folder||null});
   creating=false;
   await onCreated(path);
  }catch(error){
   creating=true;createError=error instanceof Error?error.message:String(error);
  }finally{saving=false;}
 }
 async function uploadSelection(input:HTMLInputElement){
  const files=Array.from(input.files??[]);
  input.value='';
  if(!files.length)return;
  const batch=new AbortController();
  upload=batch;finished=0;uploaded=0;total=files.length;failures=[];
  try{
   for(const file of files){
    const path=file.webkitRelativePath||file.name;
    try{
     if(path.split('/').some(part=>part.startsWith('.')))throw new Error('隐藏文件不支持上传');
     await uploadWebFile(folder,path,file,batch.signal);
     uploaded++;
    }catch(error){
     if(batch.signal.aborted)return;
     failures=[...failures,{path,message:error instanceof Error?error.message:String(error)}];
    }
    finished++;
   }
   if(uploaded)await onUploaded();
  }finally{upload=null;}
 }
</script>

{#if !folder||canUpload||onCreateDocument}
 <div class="web-library-action-group">
  {#if !folder||canUpload}<Button.Root type="button" class="outline library-folder-action" disabled={!!upload} onclick={open}>{@html icon('plus')}{$t('创建文件夹')}</Button.Root>{/if}
  {#if onCreateDocument}<Button.Root type="button" class="outline library-folder-action" onclick={onCreateDocument}>{@html icon('plus')}{$t('新建文档')}</Button.Root>{/if}
 </div>
{/if}
{#if canUpload}
 <input type="file" hidden bind:this={filePicker} onchange={e=>uploadSelection(e.currentTarget)}/>
 <input type="file" hidden webkitdirectory bind:this={folderPicker} onchange={e=>uploadSelection(e.currentTarget)}/>
 <div class="web-library-action-group">
  <Button.Root type="button" class="outline library-folder-action" disabled={!!upload} onclick={()=>filePicker.click()}>{@html icon('arrow')}{$t('上传文件')}</Button.Root>
  <Button.Root type="button" class="outline library-folder-action" disabled={!!upload} onclick={()=>folderPicker.click()}>{@html icon('arrow')}{$t('上传文件夹')}</Button.Root>
 </div>
 <div class="web-upload-result" role="status">
  {#if upload}{$t('正在上传 {value0} / {value1}',{value0:Math.min(finished+1,total),value1:total})}
  {:else if uploaded}{$t('已上传 {value0} 个文件',{value0:uploaded})}{/if}
 </div>
 {#if failures.length}
  <ul class="web-upload-errors" aria-label={$t('上传失败')}>
   {#each failures as failure}
    <li>{failure.path} · {$t(failure.message)}</li>
   {/each}
  </ul>
 {/if}
{/if}
{#if creating}
 <Modal label={$t('创建文件夹')} onClose={close}>
  <form onsubmit={event=>{event.preventDefault();void create()}}>
   <div class="dialog-head"><h2>{$t('创建文件夹')}</h2></div>
   <div class="dialog-body">
    <label class="field-label">{$t('文件夹名称')}<Input bind:value={name} required data-autofocus disabled={saving}/></label>
    {#if createError}<p class="form-error" role="alert">{$t(createError)}</p>{/if}
   </div>
   <div class="dialog-footer">
    <Button.Root type="button" disabled={saving} onclick={close}>{$t('取消')}</Button.Root>
    <Button.Root type="submit" class="primary" disabled={saving||!name.trim()}>{$t('创建')}</Button.Root>
   </div>
  </form>
 </Modal>
{/if}

<style>
 .web-library-action-group{display:flex;flex-wrap:wrap;align-items:center;gap:8px}
 :global(.real-client) .web-library-action-group > :global(.library-folder-action){margin-left:0}
 .web-upload-result:empty{display:none}
 .web-upload-result,.web-upload-errors{flex:0 0 100%;font-size:12px;overflow-wrap:anywhere}
 .web-upload-errors{color:var(--color-danger);max-height:110px;overflow:auto;margin:0;padding-inline-start:20px}
</style>
