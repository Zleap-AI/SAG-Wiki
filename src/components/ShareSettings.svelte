<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {open} from '@tauri-apps/plugin-dialog';
 import Switch from './ui/Switch.svelte';
 import ShareHelp from './ShareHelp.svelte';
 import ShareHistory from './ShareHistory.svelte';
 let historyOpen=false;
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 export let onReceive:(()=>void)|undefined=undefined;
 let value:{directory:string;clipboard:boolean}|null=null,error='',busy=false;
 onMount(async()=>{try{const settings=await api<{directory:string;clipboard:boolean}>('get_share_settings');if(typeof settings?.directory!=='string'||typeof settings?.clipboard!=='boolean')throw new Error($t('无法读取分享设置'));value=settings}catch(e){error=String(e)}});
 async function save(next:{directory:string;clipboard:boolean}){
  busy=true;error='';
  try{await api('save_share_settings',{settings:next});value=next}catch(e){error=String(e)}finally{busy=false}
 }
 async function pick(){
  try{const path=await open({directory:true,multiple:false,title:$t('选择保存位置')});if(path&&value)await save({...value,directory:path})}catch(e){error=String(e)}
 }
</script>
<h3 class="settings-section-title">{$t('分享')}</h3>
<section class="settings-group share-settings">
 <div class="setting-row"><div class="share-label-heading"><strong>{$t('接收分享')}</strong><ShareHelp onError={message=>error=message}/></div><Button.Root type="button" class="outline" onclick={onReceive}>{$t('填写分享码')}</Button.Root></div>
 {#if value}
 <div class="setting-row"><div class="share-location"><strong>{$t('默认保存位置')}</strong><p>{value.directory}</p></div><Button.Root type="button" class="outline" disabled={busy} onclick={pick}>{$t('更改')}</Button.Root></div>
 <div class="setting-row"><strong>{$t('识别剪贴板分享码')}</strong><Switch label={$t('识别剪贴板分享码')} checked={value.clipboard} disabled={busy} onCheckedChange={clipboard=>{if(value)void save({...value,clipboard})}}/></div>
 {/if}
 <div class="setting-row"><strong>{$t('分享记录')}</strong><Button.Root type="button" class="outline" onclick={()=>historyOpen=true}>{$t('查看')}</Button.Root></div>
</section>
{#if historyOpen}<ShareHistory onClose={()=>historyOpen=false}/>{/if}
{#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
<style>.share-label-heading{display:flex;align-items:center;gap:4px}.share-location{min-width:0}.share-location p{overflow-wrap:anywhere}</style>
