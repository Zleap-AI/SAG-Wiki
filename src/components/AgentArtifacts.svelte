<script lang="ts">
 import {onMount} from 'svelte';
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Switch from './ui/Switch.svelte';
 import './ui/controls.css';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {web} from '../lib/web-connection';
 import type {ArtifactSource,Root} from '../lib/types';

 export let agent:ArtifactSource;
 export let name:string;
 export let onClose:()=>void;
 export let onImported:(()=>Promise<void>)|undefined=undefined;

 const formats={
  html:'HTML',slides:'PPT',word:'Word',spreadsheet:'Excel / CSV',pdf:'PDF',markdown:'Markdown',image:'图片',
 } as const;
 type Format=keyof typeof formats;
 type Source={agent:ArtifactSource; enabled:boolean; directory:string|null};
 type Options={sources:Source[]; formats:Format[]; destination:string};
 type Item={id:string; path:string; name:string};
 type Report={changed:number; failed:number; errors:string[]};
 type Overview={running:boolean; options:Options; detected:{directories:string[]}[]; default_destination:string; report:Report};
 type Catalog={scan:string; total:number; page:number; page_size:number; candidates:Item[]; report:Report};

 let options:Options|null=null, paths:string[]=[], catalog:Catalog|null=null, report:Report|null=null;
 let webRoots:Root[]=[], defaultDestination='';
 let busy=true, running=false, initialError='', error='', excludeAll=false, exceptions=new Set<string>();
 $: chosen=catalog?(excludeAll?exceptions.size:catalog.total-exceptions.size):0;
 $: title=$t('导入 {value0} 产物',{value0:name});
 $: fromChat=agent==='claude-code'&&!web;
 $: customDirectory=options?.sources[0].directory;
 $: hasDirectory=!!customDirectory||!!paths.length;
 $: canScan=!!options?.formats.length&&(fromChat||hasDirectory);

 function setSelected(id:string,selected:boolean){
  const next=new Set(exceptions);
  if(selected===excludeAll)next.add(id);else next.delete(id);
  exceptions=next;
 }
 function selectAll(checked:boolean){excludeAll=!checked;exceptions=new Set();}

 async function read(next:Options){
  options=next;
  catalog=null;
  excludeAll=false;
  exceptions=new Set();
  error='';
  if(!next.formats.length||(!fromChat&&!next.sources[0].directory&&!paths.length))return;
  busy=true;
  try{catalog=await api<Catalog>('agent_artifacts',{action:{type:'scan',options:next}})}
  catch(e){error=String(e)}
  finally{busy=false}
 }
 async function load(){
  busy=true;
  initialError='';
  try{
   const [state,roots]=await Promise.all([
    api<Overview>('agent_artifacts',{action:{type:'status',agent}}),
    web?api<Root[]>('list_roots'):[],
   ]);
   webRoots=roots;
   defaultDestination=state.default_destination;
   running=state.running;
   paths=state.detected[0].directories;
   report=state.report;
   const source=state.options.sources[0];
   await read({...state.options,sources:[{...source,enabled:true,directory:fromChat?null:source.directory}]});
  }catch(e){initialError=String(e)}
  finally{busy=false}
 }
 onMount(()=>{void load()});

 async function changeDirectory(directory:string|null){
  if(options)await read({...options,sources:[{...options.sources[0],directory}]});
 }
 function changeFormat(format:Format,selected:boolean){
  if(options)void read({...options,formats:selected?[...options.formats,format]:options.formats.filter(item=>item!==format)});
 }
 async function choose(target:'source'|'destination'){
  busy=true;
  error='';
  try{
   const directory=await api<string|null>('choose_artifact_directory',{title:$t(target==='source'?'选择交付目录':'选择保存位置')});
   if(!directory||!options)return;
   if(target==='source')await changeDirectory(directory);
   else await read({...options,destination:directory});
  }catch(e){error=String(e)}
  finally{busy=false}
 }
 async function page(page:number){
  if(!catalog)return;
  busy=true;
  error='';
  try{catalog=await api<Catalog>('agent_artifacts',{action:{type:'page',scan:catalog.scan,page}})}
  catch(e){error=String(e)}
  finally{busy=false}
 }
 async function importArtifacts(autoSync:boolean){
  if(!catalog&&options)await read(options);
  if(!catalog)return false;
  busy=true;
  error='';
  try{
   report=await api<Report>('agent_artifacts',{action:{type:'start',scan:catalog.scan,exclude_all:excludeAll,exceptions:[...exceptions],auto_sync:autoSync}});
   catalog=null;
  }catch(e){error=String(e);catalog=null;return false}
  finally{busy=false}
  if(report.changed)await onImported?.().catch(e=>{error=String(e)});
  return true;
 }
 async function changeSync(checked:boolean){
  if(checked){
   if(!await importArtifacts(true))running=false;
   return;
  }
  busy=true;
  error='';
  try{
   await api('agent_artifacts',{action:{type:'stop',agent}});
  }catch(e){error=String(e);running=true}
  finally{busy=false}
 }
</script>

<Modal label={title} class="agent-artifacts" onClose={()=>{if(!busy)onClose()}}>
 <header class="artifact-heading">
  <h2>{title}</h2>
  <Button.Root class="icon-button" aria-label={$t('关闭')} disabled={busy} onclick={onClose}>{@html icon('close')}</Button.Root>
 </header>
 <div class="artifact-body" aria-busy={busy}>
  {#if initialError}
   <p role="alert" class="form-error">{$t(initialError)}</p>
   <Button.Root class="outline" onclick={load} disabled={busy}>{$t('重试')}</Button.Root>
  {:else if options}
   <aside class="artifact-options">
   <section class="artifact-sources">
    {#if web}
    <h3>{$t('交付目录')}</h3>
    <div class="artifact-source">
     <select class="artifact-folder-select" aria-label={$t('交付目录')} value={customDirectory??''} disabled={busy||!webRoots.length} onchange={event=>changeDirectory(event.currentTarget.value||null)}>
      <option value="">{$t('请选择一个已接入的文件夹')}</option>
      {#each webRoots as root (root.path)}<option value={root.path}>{root.path}</option>{/each}
     </select>
    </div>
    {:else if fromChat}
    <h3>{$t('聊天记录')}</h3>
    <div class="artifact-source"><small>{$t('从最终回复识别交付文件')}</small></div>
    {:else}
    <h3>{$t('交付目录')}</h3>
    <div class="artifact-source">
     <small title={customDirectory??paths.join('\n')}>{paths.length>1&&!customDirectory?$t('已检测到 {value0} 个交付目录',{value0:paths.length}):customDirectory??paths[0]??$t('未检测到交付目录')}</small>
     <Button.Root class="artifact-link" aria-label={$t('选择 {value0} 的交付目录',{value0:name})} disabled={busy} onclick={()=>choose('source')}>{$t(hasDirectory?'更改':'选择目录')}</Button.Root>
    </div>
    {#if customDirectory}
     <Button.Root class="artifact-link" disabled={busy} onclick={()=>changeDirectory(null)}>{$t('恢复默认')}</Button.Root>
    {/if}
    {/if}
   </section>
   <section class="artifact-formats" aria-label={$t('文件类型')}>
    <h3>{$t('文件类型')}</h3>
    <div class="artifact-format-list">
     {#each Object.entries(formats) as [key, label] (key)}
      {@const format=key as Format}
      <label class="artifact-checkbox artifact-format">
       <Checkbox.Root class="ui-checkbox" aria-label={$t(label)} checked={options.formats.includes(format)} disabled={busy} onCheckedChange={checked=>changeFormat(format,checked)}>
        {#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}
       </Checkbox.Root>
       <span>{$t(label)}</span>
      </label>
     {/each}
    </div>
   </section>
   <div class="artifact-sync">
    <span>{$t('自动同步')}</span>
    <Switch bind:checked={running} label={$t('自动同步')} disabled={busy||(!running&&!canScan)} onCheckedChange={changeSync}/>
   </div>
   </aside>
   <section class="artifact-history" aria-label={$t('已有产物')}>
    <div class="artifact-list-head">
     <h3>{$t('已有产物')}</h3>
     <Button.Root class="artifact-link artifact-rescan" disabled={!canScan||busy} onclick={()=>options&&read(options)}>{$t('重新扫描')}{@html icon('refresh')}</Button.Root>
    </div>
    {#if catalog?.total}
     <div class="artifact-list-tools">
      <label class="artifact-checkbox">
       <Checkbox.Root class="ui-checkbox" aria-label={$t('全选')} checked={chosen===catalog.total} indeterminate={chosen>0&&chosen<catalog.total} disabled={busy} onCheckedChange={selectAll}>
        {#snippet children({checked,indeterminate})}{#if indeterminate}{@html icon('minimize')}{:else if checked}{@html icon('check')}{/if}{/snippet}
       </Checkbox.Root>
       <span>{$t('全选')}</span>
      </label>
      <span class="artifact-count">{catalog.total}</span>
     </div>
     <div class="artifact-list">
      {#each catalog.candidates as item (item.id)}
       <label class="artifact-checkbox artifact-item">
        <Checkbox.Root class="ui-checkbox" aria-label={item.path} checked={excludeAll===exceptions.has(item.id)} disabled={busy} onCheckedChange={checked=>setSelected(item.id,checked)}>
         {#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}
        </Checkbox.Root>
        <span><strong title={item.path}>{item.name}</strong><small title={item.path}>{item.path}</small></span>
       </label>
      {/each}
     </div>
     {#if catalog.total>catalog.page_size}
      <div class="artifact-pages">
       <Button.Root class="icon-button" aria-label={$t('上一页')} title={$t('上一页')} disabled={busy||catalog.page===0} onclick={()=>catalog&&page(catalog.page-1)}>{@html icon('back')}</Button.Root>
       <span>{catalog.page+1} / {Math.ceil(catalog.total/catalog.page_size)}</span>
       <Button.Root class="icon-button" aria-label={$t('下一页')} title={$t('下一页')} disabled={busy||(catalog.page+1)*catalog.page_size>=catalog.total} onclick={()=>catalog&&page(catalog.page+1)}>{@html icon('forward')}</Button.Root>
      </div>
     {/if}
    {:else if busy}
     <p class="artifact-status" role="status">{$t('正在读取产物…')}</p>
    {:else if !fromChat&&!hasDirectory}
     <p class="artifact-status">{$t('请选择交付目录。')}</p>
    {:else if !canScan}
     <p class="artifact-status">{$t('请选择文件类型')}</p>
    {:else if catalog}
     <p class="artifact-status">{$t(fromChat?'没有找到可导入的交付文件':'没有待导入的产物')}</p>
    {:else if report?.changed}
     <p class="artifact-status" role="status">{$t('已归档 {value0} 个产物',{value0:report.changed})}</p>
    {/if}
   {#if catalog?.report.failed||report?.failed}
    {@const issues=catalog?.report.failed?catalog.report:report!}
    <details class="artifact-errors">
     <summary>{$t('{value0} 个产物或目录处理失败',{value0:issues.failed})}</summary>
     {#each issues.errors as message}<p>{$t(message)}</p>{/each}
    </details>
   {/if}
   {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
   </section>
  {:else}
   <p class="artifact-status" role="status">{$t('正在读取产物…')}</p>
  {/if}
 </div>
 {#if options}
  <div class="artifact-destination">
   <span>{$t('保存位置')}</span>
   {@html icon('folder')}
   {#if web}
    <select class="artifact-folder-select" aria-label={$t('保存位置')} value={options.destination} disabled={busy} onchange={event=>options&&read({...options,destination:event.currentTarget.value})}>
     <option value={defaultDestination}>{defaultDestination}</option>
     {#each webRoots.filter(root=>root.path!==defaultDestination) as root (root.path)}<option value={root.path}>{root.path}</option>{/each}
    </select>
   {:else}
    <strong title={options.destination}>{options.destination}</strong>
    <Button.Root class="artifact-link" disabled={busy} onclick={()=>choose('destination')}>{$t('更改')}</Button.Root>
   {/if}
  </div>
 {/if}
 <footer class="artifact-footer">
  {#if catalog}<span class="artifact-selection" role="status">{$t('已选 {value0} 个文件',{value0:chosen})}</span>{/if}
  <Button.Root class="outline" disabled={busy} onclick={onClose}>{$t(running||(!catalog&&report?.changed)?'完成':'取消')}</Button.Root>
  <Button.Root class="primary" disabled={busy||!catalog||!canScan||(!chosen&&!running)} onclick={()=>importArtifacts(running)}>
   {$t(chosen||!running?'导入':'保存设置')}
  </Button.Root>
 </footer>
</Modal>

<style>
 /* Refined with unslop by Damir Sokolovsky (@heydamir), CC BY 4.0. */
 :global(.dialog-panel.agent-artifacts){width:960px;max-width:calc(100vw - 48px);height:min(680px,calc(100dvh - var(--window-bar-height,0px) - 64px));max-height:calc(100dvh - var(--window-bar-height,0px) - 64px);display:flex;flex-direction:column;overflow:hidden;background:var(--color-paper);font:400 13px/20px var(--font-body)}
 .artifact-heading{display:flex;align-items:center;justify-content:space-between;height:72px;padding:20px 24px;flex-shrink:0}
 .artifact-heading h2{margin:0;font-size:18px;font-weight:500;line-height:24px}
 .artifact-heading :global(.icon-button){width:32px;height:32px;padding:8px;color:var(--color-secondary)}
 .artifact-heading :global(.icon),.artifact-pages :global(.icon){width:16px;height:16px}
 .artifact-body{display:grid;grid-template-columns:312px minmax(0,1fr);flex:1;min-height:0;overflow:hidden;border-top:1px solid var(--color-rule)}
 .artifact-options{min-height:0;padding:16px;overflow:auto;background:var(--color-panel);border-inline-end:1px solid var(--color-rule)}
 h3{margin:0;font-size:13px;font-weight:500;line-height:20px}
 .artifact-source,.artifact-item,.artifact-list-head,.artifact-list-tools,.artifact-pages,.artifact-destination{display:flex;align-items:center;gap:8px}
 .artifact-source{min-height:36px;justify-content:space-between}
 .artifact-source small{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .artifact-folder-select{width:100%;min-width:0;max-width:360px}
 .artifact-checkbox{display:flex;align-items:center;gap:8px;min-width:0;flex:1;cursor:pointer}
 .artifact-checkbox>span{min-width:0}
 .artifact-item>span{display:grid;flex:1}
 .artifact-checkbox strong,.artifact-checkbox small,.artifact-destination strong{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .artifact-checkbox strong{font-weight:500;line-height:20px}
 small,.artifact-list-tools,.artifact-pages,.artifact-destination{color:var(--color-secondary);font-size:12px}
 .artifact-checkbox small{font-size:11px;line-height:16px;color:var(--color-muted)}
 .artifact-checkbox :global(.ui-checkbox:disabled){opacity:.5;cursor:default}
 :global(.agent-artifacts .artifact-link){flex-shrink:0;height:28px;padding:0 4px;border:0;border-radius:4px;background:transparent;color:var(--color-secondary);font-size:12px;box-shadow:none}
 :global(.agent-artifacts .artifact-link:hover:not(:disabled)){background:var(--color-hover);color:var(--color-ink)}
 .artifact-formats{margin-top:12px;padding-top:12px;border-top:1px solid var(--color-rule)}
 .artifact-format-list{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:0 8px;margin-top:4px}
 .artifact-format{min-height:32px;color:var(--color-secondary)}
 .artifact-sync{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:12px;padding:12px 0;border-top:1px solid var(--color-rule);font-weight:500}
 .artifact-history{display:flex;flex-direction:column;min-width:0;min-height:0;overflow:hidden}
 .artifact-list-head{min-height:48px;padding:8px 24px;justify-content:space-between;flex-shrink:0}
 .artifact-list-head :global(.artifact-rescan){display:flex;align-items:center;gap:8px;padding:0 8px}
 .artifact-list-head :global(.artifact-rescan .icon){width:14px;height:14px}
 .artifact-list-tools{min-height:36px;padding:0 24px;justify-content:space-between;flex-shrink:0}
 .artifact-count{color:var(--color-muted);font-variant-numeric:tabular-nums}
 .artifact-list{flex:1;min-height:0;overflow:auto;overscroll-behavior:contain;padding:4px 8px 12px}
 .artifact-item{gap:12px;min-height:56px;padding:8px 16px;margin-bottom:4px;border-radius:8px}
 .artifact-item:hover{background:var(--color-hover)}
 .artifact-pages{flex-shrink:0;padding:4px 16px;border-top:1px solid var(--color-rule);justify-content:flex-end;font-variant-numeric:tabular-nums}
 .artifact-pages :global(.icon-button){width:28px;height:28px;padding:6px}
 .artifact-status,.artifact-body>.form-error{padding:16px 20px;margin:0;color:var(--color-secondary);grid-column:1/-1}
 .artifact-body>.form-error{color:var(--color-danger)}
 .artifact-body>:global(.outline){justify-self:start;align-self:start;margin-inline-start:20px}
 .artifact-destination{min-height:56px;padding:12px 24px;justify-content:flex-end;flex-shrink:0;border-block:1px solid var(--color-rule);background:var(--color-panel)}
 .artifact-destination>span,.artifact-destination :global(.icon){flex-shrink:0;color:var(--color-muted)}
 .artifact-destination :global(.icon){width:14px;height:14px}
 .artifact-destination strong{min-width:0;font-weight:400;color:var(--color-ink)}
 .artifact-history .form-error,.artifact-errors{margin:0;padding:8px 20px;max-height:84px;overflow:auto;overflow-wrap:anywhere;font-size:12px}
 .artifact-errors p{margin:4px 0}
 .artifact-footer{display:flex;align-items:center;justify-content:flex-end;gap:8px;min-height:64px;flex-shrink:0;padding:12px 24px}
 .artifact-selection{margin-inline-end:auto;color:var(--color-secondary);font-size:12px;font-variant-numeric:tabular-nums}
 .artifact-footer :global(button){min-height:36px;min-width:80px;padding:0 16px;border-radius:8px;font-size:13px;font-weight:500;box-shadow:none}
 @media(max-width:700px){
  :global(.dialog-panel.agent-artifacts){max-width:calc(100vw - 24px);height:calc(100dvh - var(--window-bar-height,0px) - 32px);max-height:calc(100dvh - var(--window-bar-height,0px) - 32px)}
  .artifact-heading,.artifact-destination,.artifact-footer{padding-inline:16px}
  .artifact-heading{height:64px}
  .artifact-heading h2{font-size:16px}
  .artifact-destination{justify-content:flex-start}
  .artifact-destination strong{flex:1}
  .artifact-body{grid-template-columns:minmax(0,1fr);grid-template-rows:auto minmax(240px,1fr);overflow:auto}
  .artifact-options{border-inline-end:0;border-bottom:1px solid var(--color-rule);overflow:visible}
 }
</style>
