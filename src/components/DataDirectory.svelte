<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {open} from '@tauri-apps/plugin-dialog';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {displayPath} from '../lib/domain';
 import Modal from './ui/Modal.svelte';
 export let pending=false;
 export let disabled=false;
 export let onBeforeRestart:(()=>Promise<boolean>)|undefined=undefined;
 let location:{directory:string;environment_override:boolean}|undefined;
 let loading=true,error='',selected='';
 async function load(){
  loading=true;error='';
  try{location=await api('get_data_directory')}catch(e){error=String(e)}finally{loading=false}
 }
 onMount(load);
 async function choose(){
  pending=true;error='';
  try{
   const path=await open({directory:true,multiple:false,title:$t('选择新的应用数据目录（空文件夹）')});
   if(typeof path==='string')selected=path;
  }catch(e){error=String(e)}finally{pending=false}
 }
 async function migrate(){
  pending=true;error='';
  try{
   if(onBeforeRestart&&!await onBeforeRestart()){pending=false;return}
   await api('migrate_data_directory',{directory:selected});
   // Remain locked until the native client restarts.
  }catch(e){error=String(e);pending=false}
 }
</script>
<section class="settings-group data-directory-settings">
 <div class="setting-row"><div><strong>{$t('应用数据位置')}</strong><p>{$t('SQLite 索引、配置与问答历史')}</p></div></div>
 <div class="data-directory-details">
 {#if loading}<p role="status">{$t('正在读取数据位置…')}</p>
 {:else if location}
  <p class="data-directory-path" title={location.directory}>{displayPath(location.directory)}</p>
  <div class="data-directory-actions">
   <Button.Root type="button" class="outline" disabled={pending||disabled} onclick={()=>api('open_data_directory').catch(e=>error=String(e))}>{$t('打开目录')}</Button.Root>
   <Button.Root type="button" class="outline" disabled={pending||disabled||location.environment_override} onclick={choose}>{$t('更改位置')}</Button.Root>
  </div>
  {#if location.environment_override}<p>{$t('数据位置由环境变量指定，请移除 SAG_DATA_DIR / SAG_DESKTOP_DATA_DIR 后重启再调整')}</p>{/if}
 {:else}<Button.Root type="button" class="outline" onclick={load}>{$t('重试')}</Button.Root>{/if}
 {#if error&&!selected}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
</section>
{#if selected&&location}
 <Modal label={$t('迁移应用数据')} onClose={()=>{if(!pending){selected='';error=''}}}>
  <div class="dialog-head"><h2>{$t('迁移应用数据')}</h2></div>
  <div class="dialog-body data-directory-confirm">
   <p>{$t('将索引、配置、密钥、问答历史和应用内转换结果复制到新位置，完成后自动重启。原始资料文件不移动，旧数据目录保留为备份。')}</p>
   <dl><dt>{$t('当前位置')}</dt><dd>{displayPath(location.directory)}</dd><dt>{$t('新位置')}</dt><dd>{displayPath(selected)}</dd></dl>
   <p>{$t('请选择空文件夹，并预留足够空间。迁移期间请勿退出应用。本页未保存的设置不会保留。')}</p>
   {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
   {#if pending}<p role="status">{$t('正在迁移，请稍候…')}</p>{/if}
  </div>
  <div class="dialog-footer">
   <Button.Root type="button" disabled={pending} onclick={()=>{selected='';error=''}}>{$t('取消')}</Button.Root>
   <Button.Root type="button" class="primary" disabled={pending} onclick={migrate}>{$t('迁移并重启')}</Button.Root>
  </div>
 </Modal>
{/if}
<style>
 .data-directory-details{padding:12px 16px 16px}
 .data-directory-path,.data-directory-confirm dd{overflow-wrap:anywhere;font-size:13px;line-height:1.6}
 .data-directory-path{margin:0 0 12px}
 .data-directory-actions{display:flex;gap:8px;flex-wrap:wrap}
 .data-directory-confirm dt{font-weight:600;margin-top:12px}
 .data-directory-confirm dd{margin:4px 0 12px}
 .data-directory-confirm p{line-height:1.6}
</style>
