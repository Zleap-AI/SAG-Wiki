<script lang="ts">
 import {onMount} from 'svelte';
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {pickDirectory} from '../lib/platform';
 import {icon} from '../lib/icons';
 import {normalizeScope} from '../lib/domain';
 import {t,language} from '../lib/i18n';
 import type {FolderIndexOptions} from '../lib/types';
 import FolderIndexTypes from './FolderIndexTypes.svelte';
 export let roots:string[];
 export let onClose:()=>void;
 export let defaults:FolderIndexOptions={file_types:{markdown:true,text:true,code:false,config:false},convert_other_formats:false};
 export let onAdd:(paths:string[],options?:FolderIndexOptions)=>Promise<{path:string;error:string}[]>;
 let folders:string[]=[],labels:Record<string,string>={},selected:string[]=[],loading=false,adding=false,error='';
 let failures:{path:string;error:string}[]=[];
 let optionsOpen=false,options=structuredClone(defaults);
 $:optionsChanged=JSON.stringify(options)!==JSON.stringify(defaults);
 $:chosen=normalizeScope(selected);
 function select(path:string){selected=selected.includes(path)?selected.filter(item=>item!==path):[...selected,path]}
 async function refresh(){
  loading=true;error='';
  try{
   folders=await api<string[]>('fnos_accessible_folders');
   if(folders.length){
    const display=await api<{status:number;result:{path:string;semanticPath:string}[]}>('fnos_convert_paths',{paths:folders,language:$language==='en'?'en-US':'zh-CN'});
    if(display.status!==0)throw new Error($t('无法读取文件夹显示路径'));
    labels=Object.fromEntries(display.result.map(item=>[item.path,item.semanticPath]));
   }
  }catch(value){error=String(value)}finally{loading=false}
 }
 async function authorize(){
  error='';
  try{const paths=await pickDirectory();await refresh();selected=paths.filter(path=>!roots.includes(path));}
  catch(value){error=String(value)}
 }
 async function add(){
  adding=true;error='';failures=[];
  try{
   failures=optionsChanged?await onAdd(chosen,options):await onAdd(chosen);
   if(!failures.length)onClose();else selected=failures.map(item=>item.path);
  }catch(value){error=String(value)}finally{adding=false}
 }
 onMount(()=>{
  const result=sessionStorage.getItem('fnos.auth-result');
  sessionStorage.removeItem('fnos.auth-result');
  if(result){const value=JSON.parse(result);if(value.status==='success')selected=(value.path??[]).filter((path:string)=>!roots.includes(path));else if(value.status==='error')error=$t('文件授权失败，请重新选择。');}
  void refresh();
 });
</script>
<Modal label={$t('添加文件夹')} class="folder-picker" onClose={()=>{if(!adding)onClose()}}>
 <div class="folder-picker-heading"><div><h2>{$t('添加文件夹')}</h2><p>{$t('通过飞牛选择并授权目录，添加后开始索引。')}</p></div><Button.Root class="icon-button" aria-label={$t('关闭')} disabled={adding} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="folder-picker-navigation"><Button.Root type="button" class="primary" disabled={adding} onclick={authorize}>{$t('选择并授权文件夹')}</Button.Root><Button.Root type="button" class="outline" disabled={adding||loading} onclick={refresh}>{$t('刷新授权状态')}</Button.Root></div>
 <div class="folder-picker-content"><div class="fnos-folder-list">
  {#each folders as path}<label class="fnos-folder-row"><Checkbox.Root class="ui-checkbox" aria-label={labels[path]??$t('正在读取显示路径…')} checked={selected.includes(path)} disabled={adding||roots.includes(path)} onCheckedChange={()=>select(path)}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>{labels[path]??$t('正在读取显示路径…')}</span>{#if roots.includes(path)}<small>{$t('已添加')}</small>{/if}</label>{:else}<div class="fnos-folder-empty">{@html icon('folder')}<p>{loading?$t('正在读取授权目录…'):$t('尚未授权文件夹。请先选择并授权。')}</p></div>{/each}
 </div>
 <Button.Root type="button" class="folder-picker-options" aria-expanded={optionsOpen} onclick={()=>optionsOpen=!optionsOpen}>{@html icon('chevron')}{$t('索引选项')}</Button.Root>
 {#if optionsOpen}<FolderIndexTypes bind:value={options} disabled={adding}/>{/if}
 {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 {#each failures as failure}<p class="form-error" role="alert">{labels[failure.path]??$t('添加失败')}：{failure.error}</p>{/each}
 </div>
 <div class="dialog-footer"><Button.Root type="button" class="outline" disabled={adding} onclick={onClose}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={adding||!chosen.length} onclick={add}>{adding?$t('正在添加…'):$t('添加文件夹')}</Button.Root></div>
</Modal>
<style>
 :global(.dialog-panel.folder-picker){width:600px;max-width:calc(100vw - 32px);max-height:calc(100dvh - 32px);display:flex;flex-direction:column;overflow:hidden}
 .folder-picker-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px;padding:24px 24px 20px;flex-shrink:0}
 .folder-picker-heading h2{font-size:18px;font-weight:600;line-height:1.5}
 .folder-picker-heading p{margin-top:6px;font-size:13px;line-height:1.7;color:var(--color-secondary)}
 .folder-picker-heading :global(.icon-button){flex-shrink:0;width:28px;height:28px;margin-top:-2px;margin-right:-4px}
 .folder-picker-navigation{display:flex;flex-wrap:wrap;gap:8px;padding:0 24px 20px;flex-shrink:0}
 .folder-picker-navigation :global(button){min-height:34px;padding:7px 12px;font-size:13px}
 .folder-picker-content{padding:0 24px 20px;overflow-y:auto;min-height:0;overscroll-behavior:contain}
 .fnos-folder-list{display:grid;gap:8px;margin-bottom:16px}
 .fnos-folder-row{display:flex;align-items:center;gap:10px;padding:12px;border:1px solid var(--color-rule);border-radius:8px;font-size:13px}
 .fnos-folder-row span{flex:1;min-width:0;overflow-wrap:anywhere}
 .fnos-folder-row small{color:var(--color-muted);flex-shrink:0}
 .fnos-folder-empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:10px;min-height:120px;padding:20px;border:1px dashed var(--color-rule);border-radius:8px;background:var(--color-panel);color:var(--color-muted);text-align:center}
 .fnos-folder-empty :global(.icon){width:24px;height:24px}
 .fnos-folder-empty p{font-size:13px;line-height:1.7}
 :global(.folder-picker-options){display:flex;align-items:center;gap:6px;padding:6px 0;font-size:13px;color:var(--color-secondary)}
 :global(.folder-picker-options>.icon){width:15px;height:15px}
 :global(.folder-picker-options[aria-expanded=true]>.icon){transform:rotate(90deg)}
 .form-error{margin-top:12px;font-size:13px;line-height:1.7;overflow-wrap:anywhere}
 .dialog-footer{flex-shrink:0;border-top:1px solid var(--color-rule);padding:14px 24px}
 @media(max-width:480px){.folder-picker-heading{padding:20px 20px 16px}.folder-picker-navigation{padding:0 20px 16px}.folder-picker-content{padding:0 20px 16px}.dialog-footer{padding:12px 20px}}
</style>
