<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import type {Root,Entry} from '../lib/types';
 export let mode='all';
 export let paths:string[]=[];
 export let disabled=false;
 let roots:{path:string;name:string}[]=[],items:{path:string;name:string}[]=[],current='',parents:string[]=[],loading=false,error='';
 onMount(()=>{void browse('')});
 async function browse(path:string,back=false){
  loading=true;error='';
  try{
   if(!path){roots=await api<Root[]>('list_roots')??[];items=roots;parents=[];}
   else{
    const entries=await api<Entry[]>('list_entries',{path});
    items=entries.filter(entry=>entry.directory&&!entry.symlink&&entry.name!=='.sag-converted');
    parents=back?parents.slice(0,-1):[...parents,current];
   }
   current=path;
  }catch(e){error=String(e)}finally{loading=false}
 }
 function toggle(path:string){paths=paths.includes(path)?paths.filter(item=>item!==path):[...paths,path]}
</script>
<fieldset class="api-key-scope" {disabled}>
 <legend>{$t('访问范围')}</legend>
 <label><input type="radio" name="api-key-scope" value="all" bind:group={mode}/><span>{$t('全部知识库')}</span></label>
 <label><input type="radio" name="api-key-scope" value="folders" bind:group={mode}/><span>{$t('指定文件夹')}</span></label>
 <p class="settings-help">{$t('全部知识库包含以后新增的目录；指定文件夹仅允许访问所选目录及其子目录。')}</p>
 {#if mode==='folders'}
  <div class="scope-browser" aria-busy={loading}>
   <div class="scope-navigation"><Button.Root type="button" class="outline" disabled={disabled||loading||!current} onclick={()=>browse(parents.at(-1)??'',true)}>{$t('返回上层')}</Button.Root><span title={current}>{current||$t('已加入知识库的文件夹')}</span></div>
   {#if loading}<p role="status">{$t('正在读取文件夹…')}</p>
   {:else}{#each items as folder(folder.path)}<div class="scope-folder"><label><input type="checkbox" aria-label={$t('选择 {value0}',{value0:folder.name})} checked={paths.includes(folder.path)} onchange={()=>toggle(folder.path)}/><span title={folder.path}>{folder.name}</span></label><Button.Root type="button" class="outline" aria-label={$t('浏览 {value0}',{value0:folder.name})} disabled={disabled} onclick={()=>browse(folder.path)}>{$t('浏览')}</Button.Root></div>{:else}<p>{$t('此目录没有子文件夹。')}</p>{/each}{/if}
  </div>
  {#if paths.length}<ul class="scope-selected" aria-label={$t('已选文件夹')}>{#each paths as path(path)}<li><span>{path}</span><Button.Root type="button" class="outline" aria-label={$t('取消选择 {value0}',{value0:path})} disabled={disabled} onclick={()=>toggle(path)}>{$t('移除')}</Button.Root></li>{/each}</ul>{/if}
 {/if}
 {#if error}<p class="form-error" role="alert">{error}</p>{/if}
</fieldset>
<style>
 .api-key-scope{border:0;padding:0;margin:20px 0 0;min-width:0}.api-key-scope legend{font-size:13px;font-weight:500;margin-bottom:8px}.api-key-scope>label{display:inline-flex;align-items:center;gap:8px;margin:0 20px 8px 0;font-size:13px}.api-key-scope input{width:14px;height:14px;min-width:14px;flex:0 0 14px;padding:0;margin:0;accent-color:var(--color-ink)}.api-key-scope>label span{white-space:nowrap}.api-key-scope input:focus-visible{outline:2px solid var(--color-focus);outline-offset:3px}.settings-help{font-size:12px;line-height:1.6;color:var(--color-secondary)}.scope-browser{border:1px solid var(--color-rule);border-radius:8px;padding:10px;max-height:240px;overflow:auto}.scope-navigation,.scope-folder,.scope-selected li{display:flex;align-items:center;gap:10px}.scope-navigation{margin-bottom:8px;font-size:12px}.scope-navigation span{overflow-wrap:anywhere}.scope-folder{justify-content:space-between;min-height:44px}.scope-folder label{display:flex;align-items:center;gap:8px;min-width:0;font-size:13px}.scope-folder label span{overflow-wrap:anywhere}.scope-selected{padding:0;list-style:none;font-size:12px}.scope-selected li{justify-content:space-between;margin-top:8px}.scope-selected span{overflow-wrap:anywhere}.scope-browser p{font-size:12px;color:var(--color-secondary)}
</style>
