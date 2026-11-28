<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {Button,Checkbox,DropdownMenu,IsUsingKeyboard} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {displayPath,normalizeScope} from '../lib/domain';
 import {t} from '../lib/i18n';
 import {surface} from '../lib/motion';
 import type {FolderIndexOptions} from '../lib/types';
 import FolderIndexTypes from './FolderIndexTypes.svelte';
 import DataImport from './DataImport.svelte';
 import ShareDialog from './ShareDialog.svelte';
 export let initialTab='folders';
 export let initialDirectory='';
 export let onShareReceived:((path:string)=>void)|undefined=undefined;
 let receiving=false;
 const tabs=[['folders','添加文件夹'],['imports','导入数据'],['receive','接收分享']];
 export let roots:string[];
 export let showHidden=false;
 export let onClose:()=>void;
 export let onImported:(()=>Promise<void>)|undefined=undefined;
 export let onOpenImportedFolder:((path:string)=>Promise<void>)|undefined=undefined;
 export let onDestinationChange:((folder:string)=>void)|undefined=undefined;
 let tab=initialTab;
 function tabKey(event:KeyboardEvent){if(!receiving&&['ArrowLeft','ArrowRight','Home','End'].includes(event.key)){event.preventDefault();const index=tabs.findIndex(([id])=>id===tab);tab=tabs[event.key==='Home'?0:event.key==='End'?2:(index+(event.key==='ArrowRight'?1:2))%3][0];document.getElementById(`add-data-tab-${tab}`)?.focus();}}
 export let defaults:FolderIndexOptions={file_types:{markdown:true,text:true,code:false,config:false},convert_other_formats:false};
 export let onAdd:(paths:string[],options?:FolderIndexOptions)=>Promise<{path:string;error:string}[]>;
 type Listing={path:string;parent:string|null;home:string;folders:{path:string;name:string}[]};
 let listing:Listing|null=null,pathInput='',selected:string[]=[],loading=false,adding=false,error='',request=0;
 let failures:{path:string;error:string}[]=[];
 type Location={kind:string;path:string;name?:string};
 let locations:{places:Location[];volumes:Location[]}={places:[],volumes:[]},drives=false;
 const keyboard=new IsUsingKeyboard();
 const placeText:Record<string,string>={home:'用户目录',desktop:'folderPicker.desktop',documents:'folderPicker.documents',downloads:'folderPicker.downloads'};
 const placeGlyph:Record<string,string>={home:'home',desktop:'desktop',documents:'file',downloads:'download'};
 const volumeText:Record<string,string>={system:'系统磁盘',fixed:'本地磁盘',removable:'可移动磁盘',network:'网络驱动器',optical:'光驱'};
 const volumeGlyph:Record<string,string>={removable:'usb',network:'server',optical:'disc'};
 const driveLetter=(path:string)=>/^[A-Za-z]:\\$/.test(path)?path.slice(0,2):'';
 function volumeName(volume:Location){
  const letter=driveLetter(volume.path),name=volume.name||$t(volumeText[volume.kind]??'磁盘');
  return letter?`${name} (${letter})`:name;
 }
 // Drive letters compare case-insensitively; the deepest volume containing the folder wins.
 function inside(path:string,root:string){
  const fold=driveLetter(root)!=='';
  const folder=fold?path.toLowerCase():path,base=fold?root.toLowerCase():root;
  return folder===base||folder.startsWith(/[\\/]$/.test(base)?base:base+(fold?'\\':'/'));
 }
 // "Copy as path" in Windows Explorer wraps the path in quotes.
 const typedPath=(value:string)=>value.trim().replace(/^(["'])(.+)\1$/,'$2').trim();
 $:places=locations.places.length?locations.places:listing?[{kind:'home',path:listing.home}]:[];
 $:current=listing&&!drives?locations.volumes.filter(volume=>inside(displayPath(listing!.path),volume.path)).sort((a,b)=>b.path.length-a.path.length)[0]:undefined;
 $:windows=locations.volumes.some(volume=>driveLetter(volume.path))||/^[A-Za-z]:\\/.test(displayPath(listing?.home??''));
 // A drive or filesystem root has no parent; going up from it lists the other drives.
 $:canGoUp=!drives&&!!listing&&(!!listing.parent||locations.volumes.length>1);
 async function loadLocations(){
  // Without the list (older service or a failed probe) the menu still offers the home folder.
  try{
   const result=await api<typeof locations|undefined>('folder_locations');
   if(Array.isArray(result?.places)&&Array.isArray(result?.volumes))locations=result;
  }catch{}
 }
 function showDrives(){request++;loading=false;error='';drives=true;pathInput=''}
 function goUp(){if(listing?.parent)void browse(listing.parent);else showDrives()}
 let optionsOpen=false,options=structuredClone(defaults);
 let content:HTMLDivElement,indexOptions:HTMLDivElement;
 async function toggleOptions(){optionsOpen=!optionsOpen;await tick();content.scrollTop=optionsOpen?indexOptions.offsetTop:0;}
 $:optionsChanged=JSON.stringify(options)!==JSON.stringify(defaults);
 $:chosen=normalizeScope(selected);
 function select(path:string){selected=selected.includes(path)?selected.filter(item=>item!==path):[...selected,path]}
 async function browse(path:string|null=null){
  const current=++request;loading=true;error='';
  try{
   const result=await api<Listing>('browse_folders',{path,showHidden});
   if(current!==request)return;
   listing=result;drives=false;pathInput=displayPath(result.path);
  }catch(e){if(current===request)error=$t(String(e))}
  finally{if(current===request)loading=false}
 }
 async function add(){
  adding=true;error='';failures=[];
  try{
   failures=optionsChanged?await onAdd(chosen,options):await onAdd(chosen);
   if(!failures.length)onClose();
   else selected=failures.map(item=>item.path);
  }catch(e){error=$t(String(e))}
  finally{adding=false}
 }
 onMount(()=>{void browse();void loadLocations()});
</script>
<Modal label={$t(tabs.find(([id])=>id===tab)![1])} class={`folder-picker ${tab==='receive'?'folder-picker-receive':''}`} onClose={()=>{if(!adding&&!receiving)onClose()}}>
 <div class="folder-picker-heading">
  <div class="folder-picker-tabs" role="tablist" aria-label={$t('添加到知识库')}>
   {#each tabs as [id,label]}
    <Button.Root role="tab" id={`add-data-tab-${id}`} aria-controls={`add-data-panel-${id}`} aria-selected={tab===id} tabindex={tab===id?0:-1} disabled={adding||receiving} onclick={()=>tab=id} onkeydown={tabKey}>{$t(label)}</Button.Root>
   {/each}
  </div>
  <Button.Root class="icon-button" aria-label={$t('关闭')} disabled={adding||receiving} onclick={onClose}>{@html icon('close')}</Button.Root>
 </div>
 <div id="add-data-panel-folders" class="folder-picker-panel" role="tabpanel" aria-labelledby="add-data-tab-folders" hidden={tab!=='folders'}>
 <p class="folder-picker-description">{$t('可跨目录多选，添加后开始索引。也可在路径栏输入或粘贴路径。')}</p>
 <form class="folder-picker-navigation" on:submit|preventDefault={()=>browse(typedPath(pathInput))}>
  <DropdownMenu.Root>
   <DropdownMenu.Trigger class="folder-navigation-button folder-location" aria-label={$t('切换位置')} title={$t('切换位置')} disabled={!listing||adding}>{@html icon(drives?'computer':current?volumeGlyph[current.kind]??'drive':'drive')}<span>{drives?$t('所有磁盘'):current?volumeName(current):$t('位置')}</span>{@html icon('down')}</DropdownMenu.Trigger>
   <DropdownMenu.Portal>
    <DropdownMenu.Content align="start" sideOffset={6} collisionPadding={12} forceMount>
     {#snippet child({wrapperProps,props,open})}
      {#if open}<div {...wrapperProps}><div {...props} class="folder-location-menu" transition:surface|global={{keyboard:keyboard.current}}>
       <DropdownMenu.Group>
        <DropdownMenu.GroupHeading class="folder-location-heading">{$t('常用')}</DropdownMenu.GroupHeading>
        {#each places as place(place.path)}<DropdownMenu.Item class="folder-location-item" textValue={$t(placeText[place.kind]??'用户目录')} onSelect={()=>browse(place.path)}>{@html icon(placeGlyph[place.kind]??'folder')}<span>{$t(placeText[place.kind]??'用户目录')}</span></DropdownMenu.Item>{/each}
       </DropdownMenu.Group>
       {#if locations.volumes.length}
        <DropdownMenu.Separator class="folder-location-separator"/>
        <DropdownMenu.Group>
         <DropdownMenu.GroupHeading class="folder-location-heading">{$t('磁盘')}</DropdownMenu.GroupHeading>
         {#each locations.volumes as volume(volume.path)}<DropdownMenu.Item class="folder-location-item" textValue={volumeName(volume)} onSelect={()=>browse(volume.path)}>{@html icon(volumeGlyph[volume.kind]??'drive')}<span title={displayPath(volume.path)}>{volumeName(volume)}</span>{#if volume===current}{@html icon('check')}{/if}</DropdownMenu.Item>{/each}
        </DropdownMenu.Group>
       {/if}
      </div></div>{/if}
     {/snippet}
    </DropdownMenu.Content>
   </DropdownMenu.Portal>
  </DropdownMenu.Root>
  <div class="folder-path-field"><Input aria-label={$t('文件夹路径')} bind:value={pathInput} disabled={adding} placeholder={windows?$t('输入或粘贴路径，例如 D:\\资料'):$t('输入或粘贴文件夹路径')}/>
  <Button.Root type="submit" class="folder-go" aria-label={$t('前往文件夹')} title={$t('前往文件夹')} disabled={adding||!typedPath(pathInput)}>{@html icon('forward')}</Button.Root></div>
 </form>
 <div class="folder-picker-options"><label><Checkbox.Root class="ui-checkbox" checked={showHidden} disabled={adding} onCheckedChange={value=>{showHidden=value;if(!drives)void browse(listing?.path??null)}}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>{$t('显示隐藏文件夹')}</span></label>{#if listing&&!drives}<label><Checkbox.Root class="ui-checkbox" aria-label={$t('选择当前文件夹')} checked={selected.includes(listing.path)} disabled={adding||roots.includes(listing.path)} onCheckedChange={()=>select(listing!.path)}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>{$t('选择当前文件夹')}</span></label>{/if}</div>
 <div class="folder-picker-content" bind:this={content}>
 {#if error}<p class="folder-picker-error" role="alert">{error}</p>{/if}
 <div class="folder-picker-list" aria-busy={loading}>
  {#if canGoUp}<Button.Root type="button" class="folder-picker-parent" aria-label={$t('返回上级文件夹')} disabled={adding||loading} onclick={goUp}>{@html icon('back')}<span>{$t('返回上层')}</span></Button.Root>{/if}
  {#if loading}<p class="folder-picker-empty" role="status">{$t('正在读取文件夹…')}</p>
  {:else if drives}{#each locations.volumes as volume(volume.path)}<div class="folder-picker-row"><Button.Root class="folder-picker-folder" disabled={adding} title={displayPath(volume.path)} onclick={()=>browse(volume.path)}>{@html icon(volumeGlyph[volume.kind]??'drive')}<span>{volumeName(volume)}</span>{@html icon('chevron')}</Button.Root></div>{/each}
  {:else if listing}{#each listing.folders as folder(folder.path)}<div class="folder-picker-row" class:selected={selected.includes(folder.path)}><Checkbox.Root class="ui-checkbox" aria-label={$t('选择 {value0}',{value0:folder.name})} checked={selected.includes(folder.path)} disabled={adding||roots.includes(folder.path)} onCheckedChange={()=>select(folder.path)}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><Button.Root class="folder-picker-folder" disabled={adding} title={displayPath(folder.path)} onclick={()=>browse(folder.path)}>{@html icon('folder')}<span>{folder.name}</span>{#if roots.includes(folder.path)}<small>{$t('已加入知识库')}</small>{/if}{@html icon('chevron')}</Button.Root></div>{:else}<p class="folder-picker-empty">{$t('此目录没有子文件夹，可选择当前文件夹。')}</p>{/each}{/if}
 </div>
 {#if chosen.length}<div class="folder-picker-selection" aria-label={$t('已选文件夹')}>{#each chosen as path(path)}<div><span title={displayPath(path)}>{displayPath(path)}</span><Button.Root class="icon-button" disabled={adding} aria-label={$t('取消选择 {value0}',{value0:displayPath(path)})} onclick={()=>selected=selected.filter(item=>item!==path)}>{@html icon('close')}</Button.Root></div>{/each}</div>{/if}
 {#if failures.length}<div class="folder-picker-error" role="alert">{#each failures as failure}<p>{displayPath(failure.path)}：{failure.error}</p>{/each}</div>{/if}
 {#if optionsOpen}<div id="folder-index-options" class="folder-picker-index-options" bind:this={indexOptions}><p>{$t('应用于本次添加的文件夹。已有手写忽略规则会保留。')}</p><FolderIndexTypes bind:value={options} disabled={adding}/></div>{/if}
 </div>
 <div class="folder-picker-footer"><Button.Root class="folder-index-toggle" aria-expanded={optionsOpen} aria-controls="folder-index-options" disabled={adding} onclick={toggleOptions}>{@html icon('settings')}{$t('索引选项')}{@html icon(optionsOpen?'down':'chevron')}</Button.Root><Button.Root class="outline" disabled={adding||receiving} onclick={onClose}>{$t('取消')}</Button.Root><Button.Root class="primary" disabled={adding||loading||!chosen.length} onclick={add}>{adding?$t('正在添加…'):chosen.length?$t('添加 {value0} 个文件夹',{value0:chosen.length}):$t('添加到知识库')}</Button.Root></div>
 </div>
 <div id="add-data-panel-imports" class="folder-picker-imports" role="tabpanel" aria-labelledby="add-data-tab-imports" hidden={tab!=='imports'}>
  <DataImport showSharing={false} {onImported} {onOpenImportedFolder} {onDestinationChange}/>
 </div>
 {#if tab==='receive'}<div id="add-data-panel-receive" role="tabpanel" aria-labelledby="add-data-tab-receive"><ShareDialog embedded {initialDirectory} bind:busy={receiving} {onClose} onReceived={path=>onShareReceived?.(path)}/></div>{/if}
</Modal>
<style>
 :global(.dialog-panel.folder-picker){width:720px;max-width:calc(100vw - 48px);height:720px;max-height:calc(100dvh - 64px);display:flex;flex-direction:column;overflow:hidden}
 :global(.dialog-panel.folder-picker-receive){height:auto}
 .folder-picker-heading,.folder-picker-navigation,.folder-picker-description,.folder-picker-options,.folder-picker-footer{flex-shrink:0}
 .folder-picker-panel>.folder-picker-content{position:relative;flex:1 1 auto;min-height:0;overflow-y:auto;overscroll-behavior:contain}
 .folder-picker-heading{display:flex;justify-content:space-between;align-items:flex-start;padding:22px 24px 20px}
 .folder-picker-panel{display:flex;flex-direction:column;flex:1;min-height:0}
 .folder-picker-panel[hidden],.folder-picker-imports[hidden]{display:none}
 .folder-picker-tabs{display:flex;gap:24px}
 @media(max-width:520px){.folder-picker-tabs{gap:16px}.folder-picker-tabs :global(button){font-size:14px;white-space:nowrap}}
 .folder-picker-tabs :global(button){padding:0 0 10px;border-radius:0;font-size:18px;line-height:26px;font-weight:500;color:var(--color-secondary);border-bottom:2px solid transparent}
 .folder-picker-tabs :global(button[aria-selected=true]){color:var(--color-ink);border-bottom-color:var(--color-ink)}
 .folder-picker-tabs :global(button:focus-visible){outline:2px solid var(--color-focus);outline-offset:4px}
 .folder-picker-description{padding:0 24px 20px;margin:0}
 .folder-picker-imports{flex:1;min-height:0;overflow:auto;padding:0 24px 24px}
 p{font-size:var(--text-caption);color:var(--color-secondary);margin-top:6px}
 .folder-picker-navigation{display:flex;gap:8px;align-items:center;padding:0 24px 16px}
 .folder-picker-navigation :global(.folder-navigation-button){display:inline-flex;align-items:center;justify-content:center;gap:5px;flex-shrink:0;height:36px;padding:0 10px;border:1px solid var(--color-rule);border-radius:8px;font-size:var(--text-caption);background:var(--color-paper)}
 .folder-picker-navigation :global(.folder-location){max-width:220px;gap:6px}
 .folder-picker-navigation :global(.folder-location>span){min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .folder-picker-navigation :global(.folder-location>.icon){width:15px;height:15px;flex-shrink:0;color:var(--color-secondary)}
 .folder-picker-navigation :global(.folder-location>.icon:last-child){width:12px;height:12px}
 :global(.folder-location-menu){z-index:70;min-width:220px;max-width:min(320px,calc(100vw - 24px));max-height:min(420px,var(--bits-floating-available-height));overflow-y:auto;overscroll-behavior:contain;padding:5px;border:1px solid var(--color-window-border);border-radius:10px;background:var(--color-paper);color:var(--color-ink);box-shadow:0 10px 32px var(--color-control-shadow);outline:none;font:400 var(--text-ui)/1.5 var(--font-body);transform-origin:var(--bits-dropdown-menu-content-transform-origin)}
 :global(.folder-location-heading){margin:0;padding:6px 10px 2px;font-size:var(--text-caption);color:var(--color-muted)}
 :global(.folder-location-item){display:flex;align-items:center;gap:10px;min-height:32px;padding:6px 10px;border-radius:6px;cursor:pointer;outline:none;user-select:none}
 :global(.folder-location-item[data-highlighted]){background:var(--color-hover)}
 :global(.folder-location-item>span){flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 :global(.folder-location-item>.icon){width:16px;height:16px;flex-shrink:0;color:var(--color-secondary)}
 :global(.folder-location-item>.icon:last-child:not(:first-child)){width:14px;height:14px;color:var(--color-ink)}
 :global(.folder-location-separator){height:1px;margin:5px 4px;background:var(--color-rule)}
 .folder-path-field{display:flex;align-items:center;flex:1;min-width:0;height:36px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel);overflow:hidden}
 .folder-path-field:focus-within{border-color:var(--color-focus)}
 .folder-path-field :global(input){min-width:0;flex:1;width:100%;height:34px;font-size:var(--text-caption);padding:0 10px;border:0;border-radius:0;background:transparent;outline:none}
 .folder-path-field :global(.folder-go){display:flex;align-items:center;justify-content:center;width:28px;height:28px;flex-shrink:0;padding:0;margin-right:4px;border-radius:5px;font-size:var(--text-caption);color:var(--color-secondary)}
 .folder-path-field :global(.folder-go .icon){width:16px;height:16px}
 .folder-picker-options{display:flex;justify-content:space-between;gap:16px;padding:0 24px 12px;font-size:var(--text-caption);color:var(--color-secondary)}
 label{display:flex;align-items:center;gap:8px;cursor:pointer}
 .folder-picker-list{min-height:160px;padding:0 16px;border-block:1px solid var(--color-rule)}
 :global(.folder-picker-parent){display:flex;align-items:center;gap:10px;width:100%;min-height:42px;padding:8px 16px;text-align:left;font-size:var(--text-ui);color:var(--color-secondary);border-bottom:1px solid var(--color-rule);border-radius:0}
 :global(.folder-picker-parent>.icon){width:17px;height:17px}
 .folder-picker-row{display:flex;align-items:center;gap:10px;min-height:42px;padding-left:8px}
 .folder-picker-row{border-radius:7px}
 .folder-picker-row:hover,.folder-picker-row.selected{background:var(--color-hover)}
 :global(.folder-picker-folder){display:flex;align-items:center;gap:10px;flex:1;min-width:0;min-height:42px;text-align:left;font-size:var(--text-ui);padding:8px;background:transparent}
 :global(.folder-picker-folder>span){flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 :global(.folder-picker-folder>.icon){width:17px;height:17px;color:var(--color-secondary)}
 :global(.folder-picker-folder>small){font-size:var(--text-caption);color:var(--color-muted)}
 .folder-picker-empty{padding:30px 8px;text-align:center}
 .folder-picker-selection{padding:10px 24px;border-bottom:1px solid var(--color-rule)}
 .folder-picker-selection>div{display:flex;align-items:center;gap:8px;font-size:var(--text-caption);color:var(--color-secondary)}
 .folder-picker-selection span{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .folder-picker-selection :global(.icon-button){width:24px;height:24px;flex-shrink:0}
 .folder-picker-error{color:var(--color-danger);font-size:var(--text-caption);padding:8px 24px;overflow-wrap:anywhere}
 .folder-picker-error p{color:inherit}
 .folder-picker-footer{border-top:1px solid var(--color-rule);display:flex;align-items:center;gap:8px;padding:16px 24px;font-size:var(--text-caption)}
 .folder-picker-index-options{padding:8px 24px 12px;border-top:1px solid var(--color-rule)}
 :global(.folder-picker-footer .folder-index-toggle){display:flex;align-items:center;gap:7px;margin-right:auto;padding-left:0;color:var(--color-secondary)}
 :global(.folder-index-toggle .icon){width:14px;height:14px}
 .folder-picker-footer :global(button){height:36px;min-width:64px;padding:0 14px;border-radius:8px;font-size:var(--text-ui)}
</style>
