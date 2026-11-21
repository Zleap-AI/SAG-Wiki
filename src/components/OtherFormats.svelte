<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {defaultConversionWorkers,doclingStatus,openDoclingDownload,refreshDoclingStatus,updateDoclingStatus,profileForExtension,profileInstalled} from '../lib/docling';
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {web} from '../lib/web-connection';
 import type {DoclingStatus,MineruConfig,Settings} from '../lib/types';
 export let enabled=false;
 export let remoteParsing:boolean|undefined=undefined;
 export let remoteProvider:MineruConfig['provider']|undefined=undefined;
 export let visionParsing:boolean|undefined=undefined;
 let savedRemoteParsing=false;
 let savedVisionParsing=false;
 let savedRemoteProvider:MineruConfig['provider']|undefined;
 $:usingRemote=remoteParsing??savedRemoteParsing;
 $:usingVision=visionParsing??savedVisionParsing;
 $:pdfOnly=(remoteProvider??savedRemoteProvider)==='302';
 const remoteFormats=new Set(['pdf','png','jpg','jpeg','jp2','webp','gif','bmp','doc','docx','ppt','pptx','xls','xlsx']);
 const visionFormats=new Set(['pdf','png','jpg','jpeg']);
 export let formats:string[]|null|undefined=undefined;
 export let disabled=false;
 export let workers:number|undefined=undefined;
 let draftWorkers=defaultConversionWorkers,savedWorkers=defaultConversionWorkers,workersLoading=false,workersReady=false,applying=false,selectionError='';
 let selected:string[]=[];
 let legacyAll=false,selectionChanged=false;
 let revokeInstall=()=>{},selectionVersion=0,disposed=false;
 let details=false,configuringDocling=false,error='',loading=true,confirmUninstall=false,uninstalling=false,uninstallError='';
 let status:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
 $:if($doclingStatus){
  status=$doclingStatus;loading=false;error='';
 }
 onMount(()=>{void refreshDoclingStatus().catch(e=>{loading=false;error=String(e);});});
 onDestroy(()=>{disposed=true;revokeInstall();});
 function closeDetails(){selectionVersion++;revokeInstall();details=false;}
 function invalidateInstall(){selectionVersion++;revokeInstall();}
 function changeSelection(next:string[]){selectionVersion++;revokeInstall();selectionChanged=true;selected=next;}
 $:preserveLegacyAll=legacyAll&&!selectionChanged;
 $:requiredProfiles=[...new Set(selected.filter(extension=>!(usingVision&&visionFormats.has(extension))&&(!usingRemote||!(pdfOnly?extension==='pdf':remoteFormats.has(extension)))).map(extension=>profileForExtension(status,extension)).filter(profile=>profile!==undefined))];
 $:missingProfiles=requiredProfiles.filter(profile=>!profileInstalled(status,profile));
 $:needsInstall=!preserveLegacyAll&&missingProfiles.length>0;
 export async function showDetails(configure=false){
  configuringDocling=configure;
  selectionVersion++;revokeInstall();legacyAll=enabled&&formats==null;selectionChanged=false;
  // Nothing is preselected: a ticked component format would install gigabytes on one click.
  // A legacy "all formats" preference stays in force until the user changes the selection.
  selected=formats==null?[]:[...formats];
  selectionError='';draftWorkers=workers??defaultConversionWorkers;savedWorkers=draftWorkers;workersReady=workers!==undefined;details=true;
  if(workers===undefined||remoteParsing===undefined){
   workersLoading=true;
   try{const settings=await api<Settings>('get_settings');if(workers===undefined){draftWorkers=settings.conversion_workers??defaultConversionWorkers;savedWorkers=draftWorkers;}savedRemoteParsing=settings.mineru?.enabled??false;savedRemoteProvider=settings.mineru?.provider;savedVisionParsing=settings.document_parser==='vision';workersReady=true;}
   catch(e){selectionError=String(e);}
   finally{workersLoading=false;}
  }
 }
 async function openDocling(event:MouseEvent){
  event.preventDefault();uninstallError='';
  try{await api('open_about_link',{target:'docling'});}catch(e){uninstallError=String(e);}
 }
 async function uninstall(){
  uninstalling=true;uninstallError='';
  try{
   const next=await api<DoclingStatus>('uninstall_docling');
   updateDoclingStatus(next);enabled=false;confirmUninstall=false;await showDetails(configuringDocling);
  }catch(e){uninstallError=String(e);}finally{uninstalling=false;}
 }
 async function applyFormats(){
  if(!Number.isInteger(draftWorkers)||draftWorkers<1||draftWorkers>16){selectionError='转换并行数应为 1–16';return;}
  if(!preserveLegacyAll&&selected.some(extension=>!profileForExtension(status,extension))){selectionError='当前服务不支持这些转换格式，请更新后重试';return;}
  const required=requiredProfiles;
  const missing=required.filter(profile=>!profileInstalled(status,profile));
  if(!preserveLegacyAll&&missing.includes('media')&&!status.profiles?.some(profile=>profile.id==='media')){selectionError='当前服务不支持按需安装音视频组件，请更新后台服务后重试';return;}
  if(!preserveLegacyAll&&missing.length){
   revokeInstall();const version=selectionVersion,chosen=[...selected],chosenWorkers=draftWorkers;
   revokeInstall=openDoclingDownload(missing,chosen,async installValid=>{
    if(!installValid()||disposed||!details||version!==selectionVersion)return;
    if(required.some(profile=>!profileInstalled(status,profile))){selectionError='所选文件格式尚未准备就绪，请重试安装。';return;}
    await commitFormats(chosen,chosenWorkers,false,()=>installValid()&&!disposed&&details&&version===selectionVersion);
   });return;
  }
  await commitFormats([...selected],draftWorkers,preserveLegacyAll);
 }
 async function commitFormats(chosen:string[],chosenWorkers:number,preserveAll:boolean,valid=()=>true){
  if(!valid())return;
  applying=true;selectionError='';
  try{
   if(workers===undefined&&chosenWorkers!==savedWorkers)await api('set_conversion_workers',{workers:chosenWorkers});
   if(!valid())return;
   if(workers!==undefined)workers=chosenWorkers;
  const all=status.formats.flatMap(([,extensions])=>extensions);
  if(!preserveAll){formats=all.filter(extension=>chosen.includes(extension));enabled=chosen.length>0;}
  closeDetails();
  }catch(e){if(valid())selectionError=String(e);}finally{applying=false;}
 }

</script>

<div class="setting-row other-formats">
 <div class="description">
  <strong>{$t('更多文件格式')}</strong>
  {#if !usingVision}<p>{$t('扫描 PDF、图片、音视频等可按需安装 Docling，普通文档无需下载。')}</p>{/if}
  {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
 <div class="other-formats-actions"><Button.Root type="button" class="outline other-formats-action" aria-label={$t('选择更多文件格式')} disabled={loading||disabled} onclick={()=>showDetails()}>{$t('选择')}</Button.Root></div>
</div>

{#if details}{@const title=$t(configuringDocling?'配置 Docling':status.installed?'选择更多文件格式':'全部支持格式')}<Modal label={title} onClose={()=>{if(!uninstalling&&!applying)closeDetails()}} class="docling-dialog docling-formats-dialog">
 <div class="dialog-head"><h2>{title}</h2><Button.Root type="button" class="icon-button" disabled={uninstalling||applying} aria-label={$t('关闭格式选择')} onclick={closeDetails}>{@html icon('close')}</Button.Root></div>
 <div class="dialog-body formats-list">
  <div class="docling-component">
   <a href="https://docling.ai/" target="_blank" rel="noreferrer" on:click={openDocling}>Docling {@html icon('externalLink')}</a>
   <div class="docling-component-actions">
    {#if !profileInstalled(status,'documents')}
     {#if status.running}
      <Button.Root type="button" class="outline" onclick={()=>openDoclingDownload(['documents'],[])}>{$t('查看安装进度')}</Button.Root>
     {:else}
      <Button.Root type="button" class="primary" aria-label={$t('安装 Docling')} onclick={()=>openDoclingDownload(['documents'],[],undefined,'resource')}>{$t('安装')}</Button.Root>
     {/if}
    {/if}
    {#if status.installed||status.profiles?.some(profile=>profile.installed)}
     <Button.Root type="button" class="outline uninstall-docling" disabled={status.running||uninstalling} onclick={()=>{uninstallError='';confirmUninstall=true;}}>{$t('卸载相关组件')}</Button.Root>
    {/if}
   </div>
  </div>
  {#if status.builtin_formats?.length}<section class="builtin-formats" aria-label={$t('内置转换格式')}>
   <div class="formats-heading">
    <h3>{$t('内置转换格式')}</h3>
    <span class="builtin-note">{$t('无需下载 Docling')}</span>
   </div>
   <p>{status.builtin_formats.map(([label,extensions])=>`${$t(label)}（${extensions.map(extension=>`.${extension}`).join(' ')}）`).join('；')}</p>
  </section>{/if}
  {#if status.formats.length}
   <section class="optional-formats">
    <div class="formats-heading">
     <h3>{$t('可选格式')}</h3>
     <div class="format-selection-actions">
      <Button.Root type="button" onclick={()=>changeSelection(status.formats.flatMap(([,extensions])=>extensions))}>{$t('全选')}</Button.Root>
      <Button.Root type="button" onclick={()=>changeSelection([])}>{$t('清空选择')}</Button.Root>
     </div>
    </div>
    {#if legacyAll&&!selectionChanged}
     <p>{$t('沿用原有全部格式范围；仅调整并行数不会安装组件。修改格式选择后，才会确认所需组件。')}</p>
    {/if}
    {#each status.formats as [label,extensions]}
     <div class="format-group">
      <h4>{$t(label)}</h4>
      <div class="format-extensions">{#each extensions as extension}<label class="format-extension"><Checkbox.Root class="ui-checkbox" aria-label={$t('转换 .{value0}',{value0:extension})} checked={selected.includes(extension)} onCheckedChange={checked=>changeSelection(checked?[...selected,extension]:selected.filter(value=>value!==extension))}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>.{extension}</span></label>{/each}</div>
     </div>
    {/each}
   </section>
  {/if}
  <div class="conversion-workers">
   <label for="conversion-workers"><strong>{$t('转换并行数')}</strong><p>{$t('默认 {value0}，可设为 1–16。受共享资源预算限制，下次索引生效。',{value0:defaultConversionWorkers})}</p></label>
   <Input id="conversion-workers" aria-label={$t('转换并行数')} type="number" min="1" max="16" required disabled={workersLoading||applying} oninput={invalidateInstall} bind:value={draftWorkers}/>
  </div>
 </div>
 {#if selectionError||uninstallError&&!confirmUninstall}<p class="format-dialog-error form-error" role="alert">{$t(selectionError||uninstallError)}</p>{/if}
 <div class="dialog-footer formats-footer">
  {#if !profileInstalled(status,'documents')&&!status.running}
   <div class="format-install-options">
    {#if !web}<Button.Root type="button" class="docling-component-link" disabled={uninstalling||applying} onclick={()=>openDoclingDownload(['documents'],[],undefined,'command')}>{$t('通过命令行安装')}</Button.Root>
    <Button.Root type="button" class="docling-component-link" disabled={uninstalling||applying} onclick={()=>openDoclingDownload(['documents'],[],undefined,'import')}>{$t('导入离线组件')}</Button.Root>{/if}
   </div>
  {/if}
  <div class="formats-footer-actions">
   <Button.Root type="button" disabled={applying} onclick={closeDetails}>{$t('取消')}</Button.Root>
   <Button.Root type="button" class="primary" disabled={workersLoading||applying||!workersReady||(!preserveLegacyAll&&selected.length===0&&!enabled)} onclick={applyFormats}>{$t(needsInstall?'安装并应用':'应用')}</Button.Root>
  </div>
 </div>
</Modal>{/if}

{#if confirmUninstall}<Modal label={$t('卸载 Docling 相关组件')} class="docling-uninstall-dialog" onClose={()=>{if(!uninstalling)confirmUninstall=false}}>
 <div class="dialog-head"><h2>{$t('卸载 Docling 相关组件')}</h2></div>
 <div class="dialog-body"><p>{$t('将删除 SAG Wiki 安装的文档与音视频组件、运行环境、模型、FFmpeg 和下载缓存。原文件、转换文件及已有索引都会保留。')}</p><p>{$t('卸载后普通文档仍可解析；扫描 PDF 和其他格式需要重新安装 Docling。')}</p>{#if uninstallError}<p class="form-error" role="alert">{$t(uninstallError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={uninstalling} onclick={()=>confirmUninstall=false}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" aria-label={$t('确认卸载 Docling')} disabled={uninstalling} onclick={uninstall}>{$t(uninstalling?'正在卸载…':'卸载')}</Button.Root></div>
</Modal>{/if}


<style>
 .other-formats-actions{display:flex;gap:8px;flex-shrink:0}
 :global(.other-formats-action){flex-shrink:0;font-size:13px;line-height:1.4;padding:6px 12px;min-height:30px}
 :global(.docling-formats-dialog){width:600px;max-width:calc(100vw - 32px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head,.dialog-footer{flex-shrink:0}
 .formats-list{min-height:0;overflow:auto;padding:4px 24px 20px;scrollbar-gutter:stable}
 .docling-component{display:flex;align-items:center;justify-content:space-between;gap:12px;min-height:48px;padding:8px 12px;background:var(--color-panel);border-radius:8px}
 .docling-component a{display:inline-flex;align-items:center;gap:4px;font-size:14px;line-height:20px;font-weight:600;color:var(--color-ink);text-decoration:none}
 .docling-component a:hover{text-decoration:underline;text-underline-offset:3px}
 .docling-component a :global(.icon){width:14px;height:14px;color:var(--color-secondary)}
 .docling-component-actions{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px;max-width:62%}
 .docling-component-actions :global(button){min-height:32px;font-size:13px;padding:4px 12px}
 :global(.uninstall-docling){font-size:12px;white-space:nowrap;padding:6px 10px}
 .builtin-formats,.optional-formats,.conversion-workers{padding:12px 0;border-bottom:1px solid var(--color-rule)}
 .formats-heading{display:flex;flex-wrap:wrap;align-items:center;column-gap:8px;row-gap:4px}
 .formats-heading h3,.conversion-workers strong{margin:0;color:var(--color-ink);font-size:14px;line-height:20px;font-weight:600}
 .builtin-note{color:var(--color-secondary);font-size:12px;line-height:16px}
 .builtin-formats p,.optional-formats>p,.conversion-workers p{font-size:12px;line-height:20px;color:var(--color-secondary);margin:4px 0 0;overflow-wrap:anywhere}
 .format-selection-actions{display:flex;flex-wrap:wrap;gap:4px;margin-left:auto}
 .format-selection-actions :global(button){min-height:28px;font-size:12px;padding:4px 8px;white-space:nowrap}
 .format-group{padding:8px 0;border-top:1px solid var(--color-rule)}
 .optional-formats>p{margin-bottom:8px}
 .format-group h4{margin:0;font-size:13px;line-height:20px;font-weight:500}
 .format-extensions{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,92px),1fr));gap:4px 12px;margin-top:4px}
 .format-extensions .format-extension{display:flex;align-items:center;gap:8px;margin:0;min-height:28px;font:12px/16px var(--font-mono);cursor:pointer;min-width:0}
 .format-extension>span{overflow-wrap:anywhere}
 .conversion-workers{display:flex;align-items:center;justify-content:space-between;gap:12px}
 .conversion-workers>label{min-width:0;margin:0}
 .conversion-workers :global(input){width:68px;min-height:32px;flex-shrink:0;font-variant-numeric:tabular-nums}
 .format-dialog-error{flex-shrink:0;margin:0;padding:8px 24px}
 .formats-footer{display:flex;justify-content:flex-end;align-items:center;flex-wrap:wrap;gap:8px 16px;padding:8px 24px}
 .format-install-options,.formats-footer-actions{display:flex;flex-wrap:wrap;align-items:center;gap:4px 12px}
 .format-install-options{margin-right:auto}
 .formats-footer-actions{justify-content:flex-end;margin-left:auto;gap:8px}
 :global(.docling-component-link){font-size:12px;color:var(--color-secondary);min-height:32px;padding:4px 0;text-decoration:underline;text-underline-offset:3px}
 :global(.docling-component-link:hover:not(:disabled)){color:var(--color-ink);background:transparent}
 .formats-footer-actions :global(button){min-height:32px;font-size:13px;padding:4px 12px}
 :global(.docling-uninstall-dialog){width:500px;max-width:calc(100vw - 40px)}
 .description{min-width:0}
 @media(max-width:520px){
  .formats-list,.formats-footer,.format-dialog-error{padding-inline:16px}
  .format-install-options{width:100%}
 }

</style>
