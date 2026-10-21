<script lang="ts">
 import {onMount} from 'svelte';
 import {doclingStatus,doclingDownloadOpen,refreshDoclingStatus} from '../lib/docling';
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import type {DoclingStatus,Settings} from '../lib/types';
 export let enabled=false;
 export let formats:string[]|null|undefined=undefined;
 export let disabled=false;
 export let workers:number|undefined=undefined;
 let draftWorkers=4,savedWorkers=4,workersLoading=false,workersReady=false,applying=false,selectionError='';
 let selected:string[]=[];
 let details=false,error='',loading=true,waiting=false,confirmUninstall=false,uninstalling=false,uninstallError='';
 let status:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
 $:if($doclingStatus){
  status=$doclingStatus;loading=false;error='';
  if(status.stage==='uninstalled')enabled=false;
  if(status.running)waiting=true;
  else if(waiting){waiting=false;if(status.installed&&!status.error&&status.stage!=='cancelled')enabled=true;}
 }
 onMount(()=>{void refreshDoclingStatus().catch(e=>{loading=false;error=String(e);});});
 async function showDetails(){
  selected=formats==null?status.formats.flatMap(([,extensions])=>extensions):[...formats];
  selectionError='';draftWorkers=workers??4;savedWorkers=draftWorkers;workersReady=workers!==undefined;details=true;
  if(workers===undefined){
   workersLoading=true;
   try{const settings=await api<Settings>('get_settings');draftWorkers=settings.conversion_workers??4;savedWorkers=draftWorkers;workersReady=true;}
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
   doclingStatus.set(next);enabled=false;details=false;confirmUninstall=false;
  }catch(e){uninstallError=String(e);}finally{uninstalling=false;}
 }
 async function applyFormats(){
  if(!Number.isInteger(draftWorkers)||draftWorkers<1||draftWorkers>16){selectionError='转换并行数应为 1–16';return;}
  applying=true;selectionError='';
  try{
   if(workers===undefined&&draftWorkers!==savedWorkers)await api('set_conversion_workers',{workers:draftWorkers});
   if(workers!==undefined)workers=draftWorkers;
  const all=status.formats.flatMap(([,extensions])=>extensions);
  formats=all.every(extension=>selected.includes(extension))?null:all.filter(extension=>selected.includes(extension));
  enabled=selected.length>0;details=false;
  }catch(e){selectionError=String(e);}finally{applying=false;}
 }

</script>

<div class="setting-row other-formats">
 <div class="description">
  <strong>{$t('其他格式')}</strong>
  <p>{$t('本地转换为 Markdown，原文件保留。')}</p>
  {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 </div>
 <Button.Root type="button" class="outline other-formats-action" aria-label={$t(status.installed?'选择其他格式':'安装 Docling')} disabled={loading||disabled} onclick={()=>{if(status.installed)showDetails();else doclingDownloadOpen.set(true);}}>{$t(status.installed?'选择':'安装')}</Button.Root>
</div>

{#if details}<Modal label={$t(status.installed?'选择其他格式':'全部支持格式')} onClose={()=>{if(!uninstalling&&!applying)details=false}} class="docling-dialog docling-formats-dialog">
 <div class="dialog-head"><h2>{$t(status.installed?'选择其他格式':'全部支持格式')}</h2><Button.Root type="button" class="icon-button" disabled={uninstalling||applying} aria-label={$t('关闭格式选择')} onclick={()=>details=false}>{@html icon('close')}</Button.Root></div>
 <div class="dialog-body formats-list">
  <div class="docling-component"><div><a href="https://docling.ai/" target="_blank" rel="noreferrer" on:click={openDocling}>Docling ↗</a><p>{$t('本地文档转换组件')}</p></div><Button.Root type="button" class="outline uninstall-docling" disabled={status.running||uninstalling} onclick={()=>{uninstallError='';confirmUninstall=true;}}>{$t('卸载相关组件')}</Button.Root></div>
  <section class="conversion-info" aria-label={$t('转换与保存方式')}>
   <h3>{$t('转换与保存方式')}</h3>
   <p>{$t('先索引 Markdown、文本等直接支持的文件，再并行转换其他格式，最后索引转换结果。已完成索引的文件可立即搜索。')}</p>
   <p>{$t('转换文件保存在已添加文件夹根目录的 .sag-converted 文件夹内，保留原有子目录结构，原文件不变。')}</p>
   <code>{$t('资料/报告.pdf → .sag-converted/资料/报告.pdf.md')}</code>
   <p class="conversion-note">{$t('上例路径相对于已添加的文件夹；可在文件设置中显示隐藏文件夹。')}</p>
  </section>
  <div class="conversion-workers"><label for="conversion-workers"><strong>{$t('转换并行数')}</strong><p>{$t('默认 4，可设为 1–16；所有文件夹共用，下次索引任务生效。')}</p></label><Input id="conversion-workers" aria-label={$t('转换并行数')} type="number" min="1" max="16" required disabled={workersLoading||applying} bind:value={draftWorkers}/></div>
  {#if selectionError}<p class="form-error" role="alert">{$t(selectionError)}</p>{/if}
  {#if uninstallError&&!confirmUninstall}<p class="form-error" role="alert">{$t(uninstallError)}</p>{/if}
  {#if status.installed}<div class="format-selection-actions"><Button.Root type="button" class="outline" onclick={()=>selected=status.formats.flatMap(([,extensions])=>extensions)}>{$t('全选')}</Button.Root><Button.Root type="button" onclick={()=>selected=[]}>{$t('清空选择')}</Button.Root></div>{/if}
  {#each status.formats as [label,extensions]}
   <div class="format-group"><strong>{$t(label)}</strong>
    {#if status.installed}<div class="format-extensions">{#each extensions as extension}<label class="format-extension"><Checkbox.Root class="ui-checkbox" aria-label={$t('转换 .{value0}',{value0:extension})} checked={selected.includes(extension)} onCheckedChange={checked=>selected=checked?[...selected,extension]:selected.filter(value=>value!==extension)}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>.{extension}</span></label>{/each}</div>
    {:else}<p>{extensions.map(ext=>'.'+ext).join(' · ')}</p>{/if}
   </div>
  {/each}
  <p class="settings-help">{$t('扫描件和音视频需要识别模型，首次使用相关格式时可能继续下载所需模型。')}</p>
 </div>
 <div class="dialog-footer">{#if status.installed}<Button.Root type="button" disabled={applying} onclick={()=>details=false}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={workersLoading||applying||!workersReady} onclick={applyFormats}>{$t('应用')}</Button.Root>{:else}<Button.Root type="button" class="primary" onclick={()=>details=false}>{$t('知道了')}</Button.Root>{/if}</div>
</Modal>{/if}

{#if confirmUninstall}<Modal label={$t('卸载 Docling 相关组件')} class="docling-uninstall-dialog" onClose={()=>{if(!uninstalling)confirmUninstall=false}}>
 <div class="dialog-head"><h2>{$t('卸载 Docling 相关组件')}</h2></div>
 <div class="dialog-body"><p>{$t('将删除 SAG Wiki 安装的 Docling、运行环境、识别模型和下载缓存。原文件、.sag-converted 中的转换文件及已有索引都会保留。')}</p><p>{$t('卸载后需重新安装，才能转换新的其他格式文件。')}</p>{#if uninstallError}<p class="form-error" role="alert">{$t(uninstallError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={uninstalling} onclick={()=>confirmUninstall=false}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" aria-label={$t('确认卸载 Docling')} disabled={uninstalling} onclick={uninstall}>{$t(uninstalling?'正在卸载…':'卸载')}</Button.Root></div>
</Modal>{/if}


<style>
 :global(.other-formats-action){flex-shrink:0;font-size:13px;line-height:1.4;padding:6px 12px;min-height:30px}
 :global(.docling-formats-dialog){width:560px;max-width:calc(100vw - 40px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden}
 .dialog-head,.dialog-footer{flex-shrink:0}
 .formats-list{min-height:0}
 .docling-component{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:16px}
 .docling-component a{font-size:14px;font-weight:600;color:var(--color-ink);text-underline-offset:3px}
 .docling-component p{font-size:12px;color:var(--color-muted);margin:4px 0 0}
 :global(.uninstall-docling){font-size:12px;white-space:nowrap;padding:6px 10px}
 .conversion-info{padding:12px 14px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-panel)}
 .conversion-info h3{font-size:13px;line-height:1.5;margin:0;font-weight:600}
 .conversion-info p{font-size:12px;line-height:1.7;color:var(--color-secondary);margin:8px 0 0}
 .conversion-info code{display:block;font-size:11px;line-height:1.7;overflow-wrap:anywhere;white-space:normal;margin-top:8px}
 .conversion-info .conversion-note{color:var(--color-muted);font-size:11px}
 .conversion-workers{display:flex;align-items:center;justify-content:space-between;gap:20px;padding:16px 0;border-bottom:1px solid var(--color-rule)}
 .conversion-workers strong{font-size:13px;font-weight:600}
 .conversion-workers p{margin:5px 0 0;font-size:12px;line-height:1.6;color:var(--color-muted)}
 .conversion-workers :global(input){width:76px;flex-shrink:0}
 :global(.docling-uninstall-dialog){width:500px;max-width:calc(100vw - 40px)}

 .format-selection-actions{display:flex;gap:8px;margin-top:14px}
 .format-selection-actions :global(button){font-size:12px;padding:5px 10px}
 .format-group>strong{font-size:13px;font-weight:600}
 .format-extensions{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px 12px;margin-top:12px}
 .format-extensions .format-extension{display:flex;align-items:center;gap:7px;margin:0;font-size:12px;line-height:1.5;cursor:pointer;min-width:0}
 .format-extension>span{overflow-wrap:anywhere}
 @media(max-width:560px){.format-extensions{grid-template-columns:repeat(3,minmax(0,1fr))}}
 .description{min-width:0}
 .formats-list{max-height:60vh;overflow:auto}.format-group{padding:12px 0;border-bottom:1px solid var(--color-rule)}.format-group p{margin:6px 0 0;font-size:13px;line-height:1.6;color:var(--color-muted);overflow-wrap:anywhere}

</style>
