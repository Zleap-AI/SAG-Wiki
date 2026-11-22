<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import Switch from './ui/Switch.svelte';
 import Select from './ui/Select.svelte';
 import './ui/controls.css';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {basename} from '../lib/domain';
 import {interfaceLocales,language,t} from '../lib/i18n';
 import {chatSources} from '../lib/types';
 import type {ChatSource,ChatCatalog,ChatSession,ChatPreview,ChatImportItem,ChatImportSchedule,ChatImportScheduleStatus} from '../lib/types';
 export let source:ChatSource;
 export let onClose:()=>void;
 export let onImported:()=>Promise<void>;
 export let onDestinationChange:((folder:string)=>void)|undefined=undefined;
 let catalog:ChatCatalog|null=null,sourceRoot='',query='',project='',selectedSessions:Record<string,ChatSession>={},active:ChatSession|null=null,preview:ChatPreview|null=null,previewHtml='';
 let destination='',choosingFolder=false;
 let includeTools=false,loading=true,previewLoading=false,importing=false,error='',previewError='',results:ChatImportItem[]=[];
 let loadedRoot:string|null=null,changingSource=false;
 let schedule:ChatImportSchedule|null=null,lastRun:ChatImportScheduleStatus|null=null;
 let scheduleEnabled=false,scheduleFrequency='daily',scheduleTime='',scheduleSaving=false,scheduleError='';
 const scheduleOptions=[
  {value:'30',label:'每 30 分钟'},
  {value:'60',label:'每小时'},
  {value:'360',label:'每 6 小时'},
  {value:'720',label:'每 12 小时'},
  {value:'daily',label:'每天'},
 ];
 let listRequest=0,previewRequest=0,page=0;
 let previewPane:HTMLElement;
 let searchTimer:ReturnType<typeof setTimeout>;
 function filterChanged(){clearTimeout(searchTimer);searchTimer=setTimeout(()=>void load(false,0),250);}
 $:title=$t('导入 {value0} 聊天记录',{value0:chatSources[source].name});
 $:destinationFolder=destination?`${destination.replace(/[\\/]$/, '')}${destination.includes('\\')?'\\':'/'}${chatSources[source].folder}`:'';
 $:projects=catalog?.projects??[];
 $:projectOptions=[{value:'',label:$t('全部项目')},...projects.map(value=>({value,label:value?basename(value):$t('未知项目')}))];
 $:visible=catalog?.sessions??[];
 $:chosen=Object.keys(selectedSessions);
 $:selected=Object.values(selectedSessions);
 $:succeeded=results.filter(item=>item.status!=='failed').length;
 $:hasLoadedSource=!!catalog&&sourceRoot.trim()===loadedRoot;
 $:lastRunSummary=lastRun?.last_run_at&&lastRun.last_run?[
  $t('上次导入：{value0}',{value0:new Date(lastRun.last_run_at).toLocaleString(interfaceLocales[$language].format)}),
  $t('新增或更新 {value0} 条，失败 {value1} 条',{value0:lastRun.last_run.changed,value1:lastRun.last_run.failed}),
  lastRun.last_run.error?$t(lastRun.last_run.error):'',
 ].filter(Boolean).join(' · '):undefined;
 function select(session:ChatSession){if(selectedSessions[session.id]){delete selectedSessions[session.id];selectedSessions={...selectedSessions};}else selectedSessions={...selectedSessions,[session.id]:session};results=[];}
 function selectPage(checked:boolean){for(const session of visible){if(checked)selectedSessions[session.id]=session;else delete selectedSessions[session.id];}selectedSessions={...selectedSessions};results=[];}
 async function load(custom=false,nextPage=0){
  clearTimeout(searchTimer);if(custom){query='';project='';selectedSessions={};changingSource=true;}page=nextPage;
  const requestedRoot=custom?sourceRoot.trim():loadedRoot;
  const request=++listRequest;++previewRequest;loading=true;previewLoading=false;catalog=null;active=null;preview=null;previewHtml='';results=[];error='';previewError='';
  try{
   const result=await api<ChatCatalog>('list_chat_sessions',{source,sourceRoot:requestedRoot,query:{page,search:query,project}});
   if(request!==listRequest)return;
   if(custom&&schedule?.enabled&&result.source_root!==loadedRoot){
    const saved=await saveSchedule({...schedule,source_root:result.source_root,include_tools:includeTools});
    if(request!==listRequest)return;
    if(!saved){sourceRoot=loadedRoot!;void load();return;}
   }
   catalog=result;page=result.page;loadedRoot=result.source_root;
   if(custom||!sourceRoot||sourceRoot===requestedRoot)sourceRoot=loadedRoot;
   if(result.sessions.length)void show(result.sessions[0]);
  }
  catch(e){if(request===listRequest)error=String(e)}finally{if(request===listRequest){loading=false;changingSource=false;}}
 }
 async function saveSchedule(next:ChatImportSchedule){
  scheduleSaving=true;scheduleError='';
  try{
   await api('save_chat_import_schedule',{source,schedule:next});
   schedule=next;
   return true;
  }catch(e){scheduleError=String(e);return false;}
  finally{
   scheduleEnabled=schedule!.enabled;
   scheduleFrequency=String(schedule!.interval_minutes??'daily');
   scheduleTime=schedule!.time;
   scheduleSaving=false;
  }
 }
 function toggleSchedule(enabled:boolean){
  const current=schedule!;
  void saveSchedule({...current,enabled,source_root:enabled?loadedRoot:current.source_root,include_tools:enabled?includeTools:current.include_tools});
 }
 function changeFrequency(frequency:string){
  void saveSchedule({...schedule!,interval_minutes:frequency==='daily'?null:Number(frequency)});
 }
 async function setIncludeTools(checked:boolean){
  includeTools=checked;results=[];
  if(schedule?.enabled&&!(await saveSchedule({...schedule,include_tools:checked})))includeTools=schedule.include_tools;
  if(active)void show(active);
 }
 async function show(session:ChatSession){
  const request=++previewRequest;active=session;preview=null;previewHtml='';previewError='';previewLoading=true;
  try{
   const result=await api<ChatPreview>('preview_chat_import',{source,sourceRoot:catalog!.source_root,paths:session.paths,includeTools});
   const {renderMarkdown}=await import('../lib/markdown');
   if(request!==previewRequest)return;
   previewHtml=renderMarkdown(result.body);preview=result;previewLoading=false;
   await tick();
   if(request===previewRequest&&previewPane)previewPane.scrollTop=previewPane.scrollHeight;
  }
  catch(e){if(request===previewRequest)previewError=String(e)}finally{if(request===previewRequest)previewLoading=false}
 }
 async function importSelected(){
  importing=true;error='';results=[];
  try{results=await api<ChatImportItem[]>('import_chat_sessions',{source,sourceRoot:catalog!.source_root,sessions:selected,includeTools});if(results.some(item=>item.status!=='failed'))await onImported();}
  catch(e){error=String(e)}finally{importing=false}
 }
 async function changeDestination(){
  choosingFolder=true;error='';
  try{const folder=await api<string|null>('choose_chat_import_folder');if(folder){destination=folder;results=[];onDestinationChange?.(folder);}}
  catch(e){error=String(e)}finally{choosingFolder=false}
 }
 onMount(()=>{
  void api<ChatImportScheduleStatus>('get_chat_import_schedule',{source}).then(status=>{
   lastRun=status;schedule=status.schedule;
   scheduleEnabled=schedule.enabled;scheduleFrequency=String(schedule.interval_minutes??'daily');
   scheduleTime=schedule.time;
   loadedRoot=schedule.source_root;sourceRoot=loadedRoot??'';includeTools=schedule.include_tools;
  }).catch(e=>scheduleError=String(e)).finally(()=>void load());
  void api<string>('get_chat_import_folder').then(folder=>destination=folder).catch(e=>error=String(e));
  return()=>{clearTimeout(searchTimer);++listRequest;++previewRequest;};
 });
</script>
<Modal label={title} class="chat-import" onClose={()=>{if(!importing&&!scheduleSaving)onClose()}}>
 <div class="chat-import-heading">
  <h2>{title}</h2>
  <Button.Root class="icon-button" aria-label={$t('关闭')} disabled={importing||scheduleSaving} onclick={onClose}>{@html icon('close')}</Button.Root>
 </div>
 <form class="chat-source-path" onsubmit={event=>{event.preventDefault();void load(true)}}>
  <div class="chat-path-input">{@html icon('folder')}<Input aria-label={$t('聊天记录文件夹')} bind:value={sourceRoot} disabled={importing||loading||scheduleSaving}/></div>
  <Button.Root type="submit" class="chat-reload" disabled={importing||loading||scheduleSaving||!sourceRoot.trim()}>{$t('读取')}{@html icon('refresh')}</Button.Root>
 </form>
 {#if error}<p class="chat-import-error" role="alert">{$t(error)}</p>{/if}
 <div class="chat-import-body">
  <section class="chat-session-pane" aria-label={$t('聊天记录')} aria-busy={loading}>
   <div class="chat-import-filters">
    <div class="chat-search-input">{@html icon('search')}<Input data-autofocus aria-label={$t('搜索聊天')} placeholder={$t('搜索聊天')} bind:value={query} oninput={filterChanged} disabled={importing||changingSource}/></div>
    <Select label={$t('项目')} items={projectOptions} bind:value={project} onValueChange={()=>void load(false,0)} disabled={importing||changingSource}/>
   </div>
   <div class="chat-list-toolbar">
    <label class="chat-select-all"><Checkbox.Root class="ui-checkbox" aria-label={$t('全选本页')} checked={visible.length>0&&visible.every(session=>chosen.includes(session.id))} indeterminate={visible.some(session=>chosen.includes(session.id))&&!visible.every(session=>chosen.includes(session.id))} disabled={importing||loading||!visible.length} onCheckedChange={selectPage}>{#snippet children({checked,indeterminate})}{#if indeterminate}{@html icon('minimize')}{:else if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>{$t('全选本页')}</span></label>
    <span class="chat-list-count">{catalog?.total??0}</span>
   </div>
   <div class="chat-session-list">
    {#if loading}<p class="chat-empty" role="status">{$t('正在读取聊天记录…')}</p>
    {:else if catalog}
     {#each visible as session(session.id)}
      <div class="chat-session" class:active={active?.id===session.id} class:selected={chosen.includes(session.id)}>
       <Checkbox.Root class="ui-checkbox" aria-label={$t('选择 {value0}',{value0:session.title})} checked={chosen.includes(session.id)} disabled={importing} onCheckedChange={()=>select(session)}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root>
       <Button.Root type="button" class="chat-session-open" aria-current={active?.id===session.id?'true':undefined} disabled={importing} onclick={()=>show(session)}>
        <strong>{session.title}</strong>
        <span class="chat-session-meta"><span class="chat-session-project" title={session.project}>{session.project?basename(session.project):$t('未知项目')}</span>{#if session.updated}<time datetime={session.updated}>{session.updated.slice(0,10)}</time>{/if}</span>
       </Button.Root>
      </div>
     {:else}<p class="chat-empty">{$t(query||project?'没有符合条件的聊天':'没有找到聊天记录，可指定聊天记录文件夹。')}</p>{/each}
    {/if}
   </div>
   {#if catalog&&catalog.total>catalog.page_size}<nav class="chat-pagination" aria-label={$t('聊天分页')}><Button.Root class="icon-button" aria-label={$t('上一页')} disabled={loading||importing||page===0} onclick={()=>load(false,page-1)}>{@html icon('back')}</Button.Root><span>{page+1} / {Math.ceil(catalog.total/catalog.page_size)}</span><Button.Root class="icon-button" aria-label={$t('下一页')} disabled={loading||importing||(page+1)*catalog.page_size>=catalog.total} onclick={()=>load(false,page+1)}>{@html icon('forward')}</Button.Root></nav>{/if}
  </section>
  <section bind:this={previewPane} class="chat-preview" aria-label={$t('导入预览')} aria-busy={previewLoading}>
   {#if previewLoading}<p class="chat-empty" role="status">{$t('正在生成预览…')}</p>
   {:else if previewError}<p class="chat-import-error" role="alert">{$t(previewError)}</p>
   {:else if preview}<article class="chat-preview-document tiptap">{@html previewHtml}</article>
   {:else}<div class="chat-preview-empty">{@html icon('file')}<p>{$t('选择聊天查看导入内容')}</p></div>{/if}
  </section>
 </div>
 {#if catalog?.issues.length}<details class="chat-import-issues"><summary>{$t('{value0} 个记录读取失败',{value0:catalog.issues.length})}</summary>{#each catalog.issues as issue}<p>{basename(issue.path)}：{$t(issue.error)}</p>{/each}</details>{/if}
 <div class="chat-import-settings">
  <div class="chat-import-options"><Switch label={$t('展示工具调用过程')} bind:checked={includeTools} disabled={importing||loading||scheduleSaving} onCheckedChange={setIncludeTools}/><span>{$t('展示工具调用过程')}</span></div>
  <div class="chat-import-destination"><span>{$t('保存位置')}</span><span class="chat-destination-folder" title={destinationFolder}>{@html icon('folder')}<strong>{destinationFolder}</strong><Button.Root class="chat-destination-change" disabled={importing||choosingFolder} onclick={changeDestination}>{$t('更改')}</Button.Root></span></div>
 </div>
 <div class="chat-import-schedule" role="group" aria-label={$t('定时同步')} title={lastRunSummary} aria-busy={scheduleSaving}>
  <div class="chat-import-options"><Switch label={$t('定时同步')} bind:checked={scheduleEnabled} disabled={!schedule||importing||loading||scheduleSaving||(!schedule.enabled&&!hasLoadedSource)} onCheckedChange={toggleSchedule}/><span>{$t('定时同步')}</span></div>
  {#if scheduleEnabled}
   <select aria-label={$t('同步频率')} bind:value={scheduleFrequency} disabled={importing||loading||scheduleSaving} onchange={event=>changeFrequency(event.currentTarget.value)}>
    {#each scheduleOptions as option}<option value={option.value}>{$t(option.label)}</option>{/each}
   </select>
   {#if scheduleFrequency==='daily'}
    <label class="chat-schedule-time"><span>{$t('每天导入时间')}</span><Input type="time" bind:value={scheduleTime} disabled={importing||loading||scheduleSaving} onchange={event=>{
     const time=event.currentTarget.value;
     if(!time){scheduleTime=schedule!.time;return;}
     if(time!==schedule!.time)void saveSchedule({...schedule!,time});
    }}/></label>
   {/if}
  {/if}
 </div>
 {#if scheduleError}<p class="chat-import-error" role="alert">{$t(scheduleError)}</p>{/if}
 {#if results.length}<div class="chat-import-results" role="status"><p>{$t('已处理 {value0} 个聊天',{value0:succeeded})}</p>{#each results as item}<p class:form-error={item.status==='failed'}>{basename(item.path)} · {$t(item.status==='imported'?'已导入':item.status==='updated'?'已更新':item.status==='skipped'?'未变化': '导入失败')}{item.error?`：${$t(item.error)}`:''}</p>{/each}</div>{/if}
 <div class="chat-import-footer">
  <span class="chat-selection-count" role="status">{$t('已选 {value0} 个聊天',{value0:selected.length})}</span>
  <Button.Root class="outline" disabled={importing||scheduleSaving} onclick={onClose}>{$t(results.length?'完成':'取消')}</Button.Root>
  <Button.Root class="primary" disabled={importing||loading||choosingFolder||scheduleSaving||!selected.length||!destination} onclick={importSelected}>{importing?$t('正在导入…'):$t('导入')}</Button.Root>
 </div>
</Modal>
<style>
 /* Refined with unslop by Damir Sokolovsky (@heydamir), CC BY 4.0. */
 :global(.dialog-panel.chat-import){width:960px;max-width:calc(100vw - 48px);height:min(680px,calc(100dvh - var(--window-bar-height,0px) - 64px));max-height:calc(100dvh - var(--window-bar-height,0px) - 64px);display:flex;flex-direction:column;overflow:hidden;background:var(--color-paper);font:400 14px/1.5 var(--font-body)}
 .chat-import-heading{display:flex;align-items:center;gap:24px;height:72px;padding:20px 24px;flex-shrink:0}
 .chat-import-heading h2{margin:0;font-size:18px;line-height:24px;font-weight:500;letter-spacing:0}
 .chat-import-heading :global(.icon-button){width:32px;height:32px;padding:8px;margin-left:auto;border-radius:8px;color:var(--color-secondary)}
 .chat-import-heading :global(.icon){width:16px;height:16px}
 .chat-source-path{display:flex;align-items:center;gap:12px;padding:0 24px 16px;min-width:0;flex-shrink:0}
 .chat-path-input{display:flex;align-items:center;gap:8px;min-width:0;flex:1;height:32px}
 .chat-path-input :global(.icon){width:16px;height:16px;flex-shrink:0;color:var(--color-muted)}
 .chat-path-input :global(input){width:100%;min-width:0;height:32px;padding:0 4px;border:1px solid transparent;border-radius:4px;background:transparent;color:var(--color-secondary);box-shadow:none;font:400 12px/20px var(--font-mono)}
 .chat-path-input :global(input:hover){border-color:var(--color-rule)}
 .chat-path-input :global(input:focus){border-color:var(--color-focus);outline:2px solid var(--color-focus);outline-offset:2px;color:var(--color-ink)}
 .chat-source-path :global(.chat-reload){height:32px;display:flex;align-items:center;gap:8px;padding:0 8px;border-radius:6px;font-size:12px;color:var(--color-secondary);flex-shrink:0}
 .chat-source-path :global(.chat-reload .icon){width:14px;height:14px}
 .chat-import-body{display:grid;grid-template-columns:312px minmax(0,1fr);flex:1;min-height:0;border-block:1px solid var(--color-rule);overflow:hidden}
 .chat-session-pane{display:flex;flex-direction:column;min-width:0;min-height:0;border-right:1px solid var(--color-rule);background:var(--color-panel)}
 .chat-import-filters{display:flex;flex-direction:column;gap:8px;padding:16px 16px 8px;flex-shrink:0}
 .chat-search-input{display:flex;align-items:center;gap:8px;min-width:0;height:32px;padding:0 8px;border:1px solid var(--color-rule);border-radius:6px;background:var(--color-paper)}
 .chat-search-input :global(.icon){width:16px;height:16px;flex-shrink:0;color:var(--color-muted)}
 .chat-search-input :global(input){width:100%;min-width:0;height:28px;padding:0;border:0;border-radius:0;background:transparent;box-shadow:none;outline:0;font:400 13px/20px var(--font-body)}
 .chat-search-input:focus-within{outline:2px solid var(--color-focus);outline-offset:2px}
 .chat-import-filters :global(.ui-select){width:100%;height:28px;padding:0 8px;border:0;border-radius:4px;background:transparent;font-size:12px;color:var(--color-secondary)}
 .chat-list-toolbar{display:flex;align-items:center;justify-content:space-between;height:36px;padding:0 24px;flex-shrink:0}
 .chat-select-all{display:flex;gap:12px;align-items:center;font-size:12px;color:var(--color-secondary);cursor:pointer}
 .chat-list-count{color:var(--color-muted);font-size:12px;font-variant-numeric:tabular-nums}
 .chat-select-all :global(.ui-checkbox[data-state=indeterminate]){background:var(--color-ink);border-color:var(--color-ink)}
 .chat-pagination{display:flex;align-items:center;justify-content:space-between;padding:8px 16px;border-top:1px solid var(--color-rule);font-size:12px;color:var(--color-secondary);flex-shrink:0}
 .chat-pagination :global(.icon-button){height:28px;width:28px;padding:6px}
 .chat-pagination :global(.icon){width:16px;height:16px}
 .chat-session-list{flex:1;min-height:0;overflow:auto;overscroll-behavior:contain;padding:4px 8px 12px}
 .chat-session{position:relative;display:flex;align-items:center;gap:12px;min-height:64px;margin-bottom:4px;padding:0 16px;border-radius:8px;border:1px solid transparent}
 .chat-session:hover{background:var(--color-hover)}
 .chat-session.active{background:var(--color-paper);border-color:var(--color-rule)}
 .chat-session.selected{background:var(--color-selected)}
 .chat-session :global(.chat-session-open){display:flex;flex-direction:column;align-items:stretch;justify-content:center;gap:4px;min-width:0;flex:1;min-height:64px;padding:12px 0;border-radius:4px;background:transparent;text-align:left;white-space:normal;box-shadow:none}
 .chat-session strong{font-size:13px;font-weight:500;line-height:20px;display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;overflow-wrap:anywhere}
 .chat-session-meta{display:flex;align-items:center;gap:8px;color:var(--color-muted);font-size:11px;line-height:16px;min-width:0}
 .chat-session-project{min-width:0;flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 time{flex-shrink:0;font-variant-numeric:tabular-nums}
 .chat-preview{min-width:0;min-height:0;overflow:auto;overscroll-behavior:contain;background:var(--color-paper)}
 .chat-preview-document{padding:28px 36px;font-size:14px;line-height:1.8;overflow-wrap:anywhere;min-height:0}
 .chat-preview-document :global(h1){font-size:22px;line-height:1.4;font-weight:500;margin:0 0 24px;letter-spacing:0}
 .chat-preview-document :global(h2){font-size:12px;line-height:20px;font-weight:500;color:var(--color-secondary);margin:24px 0 8px;padding-top:16px;border-top:1px solid var(--color-rule)}
 .chat-preview-document :global(p){margin:0 0 12px}
 .chat-preview-document :global(ul),.chat-preview-document :global(ol){padding-left:24px;margin:0 0 16px}
 .chat-preview-document :global(pre){max-width:100%;overflow:auto;padding:16px;border-radius:8px;background:var(--color-panel);font:12px/1.7 var(--font-mono);white-space:pre;overflow-wrap:normal}
 .chat-preview-document :global(table){display:block;max-width:100%;overflow:auto}
 .chat-preview-document :global(img){max-width:100%}
 .chat-preview-empty{height:100%;min-height:120px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:24px;color:var(--color-muted);text-align:center}
 .chat-preview-empty :global(.icon){width:24px;height:24px;stroke-width:1.5}
 .chat-preview-empty p,.chat-empty{font-size:13px;line-height:20px;margin:0}
 .chat-empty{padding:24px;color:var(--color-muted);text-align:center}
 .chat-import-settings{display:flex;align-items:center;justify-content:space-between;gap:24px;min-height:56px;padding:12px 24px;flex-shrink:0;background:var(--color-panel)}
 .chat-import-schedule{display:flex;align-items:center;flex-wrap:wrap;gap:8px 20px;min-height:48px;padding:8px 24px;border-top:1px solid var(--color-rule);flex-shrink:0;background:var(--color-panel)}
 .chat-import-schedule select{min-width:120px;max-width:100%;height:32px;padding:0 8px;border:1px solid var(--color-rule);border-radius:6px;background:var(--color-paper);color:var(--color-ink);font:400 12px var(--font-body)}
 .chat-schedule-time{display:flex;align-items:center;gap:8px;font-size:12px;color:var(--color-secondary)}
 .chat-schedule-time :global(input){width:120px;height:32px;font-size:12px}
 .chat-import-options{display:flex;align-items:center;gap:8px;flex-shrink:0;font-size:12px;line-height:20px;color:var(--color-secondary)}
 .chat-import-destination{display:flex;align-items:center;gap:12px;min-width:0;font-size:12px;line-height:20px}
 .chat-import-destination>span:first-child{color:var(--color-muted);flex-shrink:0}
 .chat-destination-folder{display:flex;align-items:center;gap:8px;min-width:0}
 .chat-destination-folder :global(.icon){width:14px;height:14px;color:var(--color-muted);flex-shrink:0}
 .chat-destination-folder :global(.chat-destination-change){flex-shrink:0;font-size:12px;height:28px;padding:0 4px;color:var(--color-secondary);border-radius:4px}
 .chat-destination-folder strong{font-weight:400;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .chat-import-footer{display:flex;align-items:center;gap:8px;height:64px;flex-shrink:0;padding:12px 24px;border-top:1px solid var(--color-rule)}
 .chat-selection-count{margin-right:auto;color:var(--color-secondary);font-size:12px;font-variant-numeric:tabular-nums}
 .chat-import-footer :global(button){height:36px;min-width:80px;padding:0 16px;border-radius:8px;font-size:13px;font-weight:500;box-shadow:none}
 .chat-import-footer :global(button:disabled){opacity:.45;cursor:default}
 :global(.chat-import button:focus-visible){outline:2px solid var(--color-focus);outline-offset:2px}
 .chat-session :global(.chat-session-open:focus-visible){outline-offset:2px}
 .chat-import-issues,.chat-import-results{padding:12px 24px;border-top:1px solid var(--color-rule);font-size:12px;max-height:96px;overflow:auto;flex-shrink:0}
 .chat-import-issues p,.chat-import-results p{margin:4px 0;overflow-wrap:anywhere}
 .chat-import-error{margin:0;padding:12px 24px;color:var(--color-danger);font-size:13px;overflow-wrap:anywhere}
 .chat-preview .chat-import-error{padding:24px}
 @media(prefers-reduced-motion:no-preference){.chat-session{transition:background-color 140ms var(--ease-out),border-color 140ms var(--ease-out)}}
 @media(max-width:700px){
  .chat-import-schedule{padding-inline:16px}
  :global(.dialog-panel.chat-import){max-width:calc(100vw - 24px);height:calc(100dvh - var(--window-bar-height,0px) - 32px);max-height:calc(100dvh - var(--window-bar-height,0px) - 32px)}
  .chat-import-heading{gap:16px;height:64px;padding:16px}.chat-import-heading h2{font-size:16px}
  .chat-source-path{padding:0 16px 12px;gap:8px}
  .chat-import-body{grid-template-columns:1fr;grid-template-rows:minmax(0,2fr) minmax(0,3fr)}
  .chat-session-pane{border-right:0;border-bottom:1px solid var(--color-rule)}
  .chat-import-filters{flex-direction:row;padding:12px 16px 0;gap:8px}.chat-search-input{flex:1}.chat-import-filters :global(.ui-select){width:112px;height:32px}
  .chat-list-toolbar{height:32px}.chat-session-list{padding-top:0}
  .chat-session{min-height:56px}.chat-session :global(.chat-session-open){min-height:56px;padding:8px 0}
  .chat-preview-document{padding:20px 24px}.chat-preview-document :global(h1){font-size:20px;margin-bottom:16px}
  .chat-import-settings{flex-wrap:wrap;gap:4px;padding:8px 16px;min-height:64px}.chat-import-destination{width:100%;justify-content:space-between}.chat-import-footer{padding:12px 16px;height:60px}
 }
</style>
