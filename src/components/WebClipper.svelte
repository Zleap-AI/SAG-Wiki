<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 let packages:string[]=[],loaded=false,busy=false,error='',notice='';
 onMount(()=>{void api<{packages:string[]}>('get_web_clipper').then(value=>{packages=value.packages;loaded=true;}).catch(e=>error=String(e));});
 async function download(browser:string){busy=true;error='';notice='';try{const path=await api<string|null>('export_web_clipper',{browser});if(path)notice=$t('扩展安装包已保存到 {value0}',{value0:path});}catch(e){error=String(e);}finally{busy=false;}}
 async function addFolder(){busy=true;error='';notice='';try{const path=await api<string|null>('add_web_clipper_folder');if(path)notice=$t('已添加剪藏目录：{value0}',{value0:path});}catch(e){error=String(e);}finally{busy=false;}}
</script>
<div class="settings-info-box"><strong>{$t('独立保存网页')}</strong><p>{$t('扩展直接保存 Markdown，无需连接码，也无需运行 SAG Wiki。')}</p></div>
{#if loaded}
 <section class="settings-group settings-fields">
  <div class="settings-group-heading"><h3>{$t('安装浏览器扩展')}</h3></div>
  <div class="clipper-downloads">
   <Button.Root type="button" class="outline" disabled={busy||!packages.includes('chrome')} onclick={()=>download('chrome')}>{$t('下载 Chrome / Edge 扩展')}</Button.Root>
   <Button.Root type="button" class="outline" disabled={busy||!packages.includes('safari')} onclick={()=>download('safari')}>{$t('下载 Safari 扩展')}</Button.Root>
  </div>
  <p class="settings-help">{$t('Chrome / Edge：解压安装包，在扩展管理中打开开发者模式，选择“加载已解压的扩展程序”。')}</p>
  <p class="settings-help">{$t('Safari：打开 DMG，将 SAG Web Clipper 拖入“应用程序”后打开，再到 Safari 设置的“扩展”中启用。')}</p>
  {#if packages.length===0}<p class="settings-help">{$t('当前构建尚未包含扩展安装包。')}</p>{/if}
 </section>
 <section class="settings-group settings-fields">
  <div class="settings-group-heading"><h3>{$t('保存位置')}</h3></div>
  <p class="settings-help">{$t('首次安装会创建文稿/sag-wiki/clippings，并将文稿/sag-wiki 加入文件夹管理。')}</p>
  <p class="settings-help">{$t('在扩展设置中选择并授权 clippings 文件夹一次，即可直接保存到该目录；也可以选择其他文件夹。')}</p>
  <p class="settings-help">{$t('在网页上右键选择“保存为 Markdown”。图片保留原网页链接。')}</p>
 </section>
 <section class="settings-group settings-fields">
  <div class="settings-group-heading"><h3>{$t('加入知识库（可选）')}</h3></div>
  <p class="settings-help">{$t('选择实际保存剪藏的文件夹，SAG Wiki 会按照文件设置监听并索引其中的 Markdown。扩展保存不依赖此操作。')}</p>
  <div class="settings-card-actions"><Button.Root type="button" class="outline" disabled={busy} onclick={addFolder}>{$t('添加剪藏文件夹')}</Button.Root></div>
 </section>
{:else if !error}<p role="status">{$t('正在读取设置…')}</p>{/if}
{#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
{#if notice}<p class="settings-help" role="status">{notice}</p>{/if}
<style>
 .clipper-downloads{display:flex;flex-wrap:wrap;gap:8px;margin-bottom:16px}.settings-help+.settings-help{margin-top:10px}
</style>
