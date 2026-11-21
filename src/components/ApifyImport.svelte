<script lang="ts">
 import {onDestroy,onMount,tick} from 'svelte';
 import {Button} from 'bits-ui';
 import {open} from '@tauri-apps/plugin-dialog';
 import Modal from './ui/Modal.svelte';
 import {api} from '../lib/api';
 import {apifyRuns,apifyRunPending,apifyRequest,startApifyRun,checkApifyRun,cancelApifyRun,previewApifyRun,importApifyRun,forgetApifyRun} from '../lib/apify-runs';
 import {language,t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {web} from '../lib/web-connection';
 import {renderMarkdown,splitFrontmatter} from '../lib/markdown';
 import {stripDocsScreenshots} from '../lib/markdown-source';
 import {externalLinkClick} from '../lib/external-links';
 import {onlineSchedules,type OnlineSchedule,type OnlineReport} from '../lib/online-import';
 import guideZh from '../../docs-site/content/docs/data-source-apify.mdx?raw';
 import guideEn from '../../docs-site/content/docs/data-source-apify.en.mdx?raw';

 export let onClose:()=>void;
 export let onImported:(()=>Promise<void>)|undefined=undefined;

 const crawler='apify/website-content-crawler';
 const runLabels={
  starting:'正在运行 Apify…',running:'正在运行 Apify…',cancelling:'正在取消…',
  'loading-preview':'正在读取…',preview:'结果预览','preview-error':'',
  importing:'正在导入…','import-error':'',done:'导入完成',failed:'',aborted:'已取消',
 };
 const syncLabels:Record<string,string>={running:'正在同步',success:'同步完成',failed:'同步失败',partial:'部分完成',canceled:'已取消',interrupted:'已中断'};
 type InputField={type?:string; title?:string; description?:string; enum?:unknown[]; enumTitles?:string[]; default?:unknown; isSecret?:boolean; editor?:string; minimum?:number; maximum?:number};
 type Actor={id:string;slug:string;title:string;preset:boolean;schema:null|{properties?:Record<string,InputField>;required?:string[]}};
 type Preview={fields:[string,string][];samples:string[];total:number|null};
 type Connection={id:string;name:string;directory:string;connection:{app_id:string;address:string};schedule:OnlineSchedule;daily_time:string;paused:boolean};
 type ConnectionRow={source:Connection;next_at:number|null;last:OnlineReport|null;running:boolean};
 type ConnectionDetails={input:Record<string,unknown>;has_mapping:boolean};
 type StoreActor={id:string;username:string;name:string;title:string;description:string|null};
 type StorePage={items:StoreActor[];offset:number;limit:number;next:number|null};

 let reference='', token='', destination='', error='', website='', pageLimit=20, includePages=false;
 let connections:ConnectionRow[]=[], documents='', editing=false, loading=true, guideOpen=false, removeId='';
 let connectionId='', connectionName='', savedDirectory='';
 let schedule:OnlineSchedule='manual', dailyTime='09:00', paused=false, hasMapping=false;
 let actor:Actor|null=null, values:Record<string,unknown>={}, jsonInput='{}', useJson=false;
 let busy=false, runId='', preview:Preview|null=null, sampleIndex=0;
 let title='', url='', identity='', body:string[]=[];
 let disposed=false,refreshing=false;
 let storeOpen=false,storeBusy=false,storeSearch='',storeQuery='',storeError='',storeRequest=0;
 let storePage:StorePage|null=null,storeInput:HTMLInputElement;

 $: run=$apifyRuns.get(runId);
 $: active=apifyRunPending(run);
 $: result=run?.phase==='done'?run.progress:undefined;
 $: if(run?.preview&&run.preview!==preview)setPreview(run.preview,run.mapping);
 $: fields=Object.entries(actor?.schema?.properties??{});
 $: validSchedule=schedule!=='daily'||!!dailyTime;
 $: canRun=!!actor&&(!!token.trim()||!!connectionId)&&(!actor.preset||!!website.trim()&&(!includePages||Number.isInteger(pageLimit)&&pageLimit>0));
 $: guideHtml=guideOpen?renderMarkdown(stripDocsScreenshots(splitFrontmatter($language==='zh-CN'?guideZh:guideEn).body)):'';

 function call<T>(action:string,args:Record<string,unknown>={},id=connectionId) {
  return apifyRequest<T>(id,action,args,token);
 }
 function showError(reason:unknown) {
  error=String(reason).replace(/^Error: /,'');
 }
 function resetRun() {
  runId='';preview=null;sampleIndex=0;
  title='';url='';identity='';body=[];error='';
 }
 async function refresh(initial=false) {
  if(refreshing)return;
  refreshing=true;
  try{
   const list=await api<{sources:ConnectionRow[];documents:string|null}>('online_import',{action:{action:'list',platform:'apify'}});
   if(disposed)return;
   const changed=!initial&&list.sources.some(({source,last})=>last?.finished
    && connections.find(row=>row.source.id===source.id)?.last?.finished!==last.finished
    && (last.result.imported||last.result.updated));
   connections=list.sources;
   documents=list.documents??'';
   if(initial&&!connections.length)newConnection();
   if(changed)await onImported?.();
  }catch(reason){showError(reason)}
  finally{loading=false;refreshing=false}
 }
 function newConnection() {
  resetRun();
  reference='';token='';destination='';website='';pageLimit=20;includePages=false;
  schedule='manual';dailyTime='09:00';paused=false;hasMapping=false;
  actor=null;connectionId='';connectionName='';savedDirectory='';editing=true;removeId='';
 }
 function setActor(loaded:Actor,input?:Record<string,unknown>) {
  actor=loaded;
  const properties=loaded.schema?.properties??{};
  values=input??Object.fromEntries(Object.entries(properties)
   .filter(([,field])=>field.default!==undefined&&!field.isSecret&&field.editor!=='password')
   .map(([key,field])=>[key,field.default]));
  jsonInput=JSON.stringify(values,null,2);
  useJson=!Object.keys(properties).length||Object.keys(values).some(key=>!Object.hasOwn(properties,key));
  if(loaded.preset&&input){
   const starts=input.startUrls as {url?:string}[]|undefined;
   website=starts?.[0]?.url??'';
   includePages=Number(input.maxCrawlDepth)>0;
   if(includePages)pageLimit=Number(input.maxCrawlPages);
  }
 }
 async function editConnection(saved:Connection,runNow=false) {
  newConnection();
  connectionId=saved.id;connectionName=saved.name;savedDirectory=saved.directory;reference=saved.connection.address;
  schedule=saved.schedule;dailyTime=saved.daily_time||'09:00';paused=saved.paused;
  busy=true;
  try{
   const [loaded,details]=await Promise.all([
    call<Actor>('describe',{actor:saved.connection.app_id}),
    call<ConnectionDetails>('get_connection'),
   ]);
   if(disposed)return;
   setActor(loaded,details.input);
   hasMapping=details.has_mapping;
  }catch(reason){showError(reason)}
  finally{busy=false}
  if(runNow&&actor&&!disposed)void start(false);
 }
 async function operateSync(id:string,action:'run'|'cancel') {
  busy=true;error='';
  try{
   await api('online_import',{action:action==='run'?{action,id,retry:false}:{action,id}});
   await refresh();
  }catch(reason){showError(reason)}
  finally{busy=false}
 }
 async function showConnections() {
  resetRun();editing=false;token='';connectionId='';removeId='';
  await refresh();
 }
 function cancelEdit() {
  if(!connections.length&&!connectionId)onClose();
  else void showConnections();
 }
 async function removeConnection(id:string) {
  busy=true;error='';
  try{await call('remove_connection',{},id);forgetApifyRun(id);removeId='';await refresh()}
  catch(reason){showError(reason)}
  finally{busy=false}
 }
 async function load(value=reference) {
  if(busy||active||!value.trim())return;
  reference=value.trim();actor=null;resetRun();busy=true;
  try{
   const [loaded,details]=await Promise.all([
    call<Actor>('describe',{actor:reference}),
    connectionId?call<ConnectionDetails>('get_connection'):undefined,
   ]);
   if(disposed)return;
   setActor(loaded,details?.input);
   hasMapping=details?.has_mapping??false;
   if(!loaded.preset&&!hasMapping){schedule='manual';paused=false}
  }catch(reason){showError(reason)}
  finally{busy=false}
 }
 async function browseStore(search:string,offset=0) {
  const request=++storeRequest;
  storeQuery=search.trim();storePage=null;storeBusy=true;storeError='';
  try{
   const page=await apifyRequest<StorePage>('','store',{search:storeQuery,offset});
   if(request===storeRequest&&!disposed)storePage=page;
  }catch(reason){if(request===storeRequest&&!disposed)storeError=String(reason).replace(/^Error: /,'')}
  finally{if(request===storeRequest&&!disposed)storeBusy=false}
 }
 async function openStore() {
  storeOpen=true;storeSearch='';
  void browseStore('');
  await tick();
  storeInput?.focus();
 }
 function leaveStore() {
  storeOpen=false;storeRequest++;
 }
 function selectStoreActor(id:string) {
  leaveStore();
  void load(id);
 }
 function pasteReference(event:ClipboardEvent) {
  const value=event.clipboardData?.getData('text').trim();
  if(!value)return;
  event.preventDefault();
  void load(value);
 }
 function editReference() {
  actor=null;resetRun();
 }
 function referenceKeydown(event:KeyboardEvent) {
  if(connectionId||event.isComposing||event.altKey||event.ctrlKey||event.metaKey)return;
  if(event.key==='Enter'){
   event.preventDefault();
   if(reference.trim()){void load();return}
  }else if(event.key!=='Tab'||event.shiftKey)return;
  if(!reference.trim()){
   reference=crawler;
   editReference();
  }
 }
 function fieldValue(value:unknown) {
  return value===undefined?'':typeof value==='string'?value:JSON.stringify(value,null,2);
 }
 function setField(key:string,value:unknown) {
  values={...values,[key]:value};
 }
 function inputData(current:Actor) {
  if(current.preset){
   const address=new URL(website.trim());
   if(!['http:','https:'].includes(address.protocol)||address.username||address.password)throw new Error('请输入有效的网页地址');
   return {startUrls:[{url:address.href}],saveMarkdown:true,saveHtml:false,saveScreenshots:false,
    respectRobotsTxtFile:true,maxCrawlDepth:includePages?20:0,maxCrawlPages:includePages?pageLimit:1};
  }
  if(useJson){
   const input=JSON.parse(jsonInput);
   if(!input||typeof input!=='object'||Array.isArray(input))throw new Error('Actor 参数必须是 JSON 对象');
   return input;
  }
  return Object.fromEntries(Object.entries(values)
   .filter(([,value])=>value!==undefined&&value!=='')
   .map(([key,value])=>{
    const field=current.schema?.properties?.[key];
    if(typeof value==='string'&&(field?.type==='object'||field?.type==='array')){
     return [key,JSON.parse(value)];
    }
    return [key,value];
   }));
 }
 function setMode(json:boolean) {
  if(!actor)return;
  try{
   if(json)jsonInput=JSON.stringify(inputData(actor),null,2);
   else values=inputData(actor);
   useJson=json;error='';
  }catch(reason){showError(reason)}
 }
 async function saveConnection(current:Actor,input:unknown) {
  const saved=await call<Connection>('save_connection',{
   name:connectionName.trim()||current.title,actor:current.id,reference:current.slug,input,directory:destination,
   schedule,daily_time:dailyTime,paused:schedule!=='manual'&&paused,
  });
  connectionId=saved.id;connectionName=saved.name;savedDirectory=saved.directory;token='';
  if(schedule!=='manual')forgetApifyRun(saved.id);
 }
 async function saveOnly() {
  if(!actor||busy)return;
  busy=true;error='';
  try{await saveConnection(actor,inputData(actor));await showConnections()}
  catch(reason){showError(reason)}
  finally{busy=false}
 }
 async function start(save=true) {
  if(!actor||busy||apifyRunPending($apifyRuns.get(connectionId)))return;
  const current=actor;
  resetRun();busy=true;
  try{
   const input=inputData(current);
   if(save)await saveConnection(current,input);
   if(schedule!=='manual'){
    await api('online_import',{action:{action:'run',id:connectionId,retry:false}});
    await showConnections();
    return;
   }
   runId=connectionId;
   void startApifyRun(runId,current.id,input,current.preset,onImported);
  }catch(reason){showError(reason)}
  finally{busy=false}
 }
 function showRun(saved:Connection) {
  newConnection();
  connectionId=saved.id;connectionName=saved.name;savedDirectory=saved.directory;runId=saved.id;
 }
 function setPreview(saved:Preview,mapping?:{title:string|null;url:string|null;id:string|null;content:string[]}) {
  preview=saved;
  const has=(key:string)=>saved.fields.some(([field])=>field===key);
  title=(mapping?mapping.title:['/title','/name','/metadata/title'].find(has))??'';
  url=(mapping?mapping.url:['/url','/link','/sourceUrl'].find(has))??'';
  identity=(mapping?mapping.id:['/id','/placeId'].find(has))??'';
  const suggested=['/markdown','/text','/content','/description'].find(has);
  body=mapping?.content??(suggested?[suggested]:[]);
 }
 function toggleBody(pointer:string,checked:boolean) {
  body=checked?(pointer===''?['']:[...body.filter(Boolean),pointer]):body.filter(value=>value!==pointer);
 }
 async function chooseDestination() {
  try{
   const path=await open({directory:true,multiple:false});
   if(typeof path==='string')destination=path;
  }catch(reason){showError(reason)}
 }
 function importResults() {
  void importApifyRun(runId,{title:title||null,url:url||null,id:identity||null,content:body});
 }
 function close(){if(guideOpen)guideOpen=false;else if(storeOpen)leaveStore();else if(!busy)onClose()}
 onMount(()=>{
  void refresh(true);
  const timer=setInterval(()=>{if(!busy&&!editing&&!guideOpen)void refresh()},1500);
  return ()=>clearInterval(timer);
 });
 onDestroy(()=>{disposed=true});
</script>

<Modal label={guideOpen?$t('配置教程'):storeOpen?$t('查看 Actor'):'Apify'} class="apify-import" onClose={close}>
 <header>
  <div class="heading-row">
   <h2>{guideOpen?$t('配置教程'):storeOpen?$t('查看 Actor'):'Apify'}</h2>
   {#if !guideOpen&&!storeOpen}<Button.Root type="button" class="text-button guide-button" disabled={busy} onclick={()=>guideOpen=true}>{@html icon('help')}{$t('配置教程')}</Button.Root>{/if}
  </div>
  <Button.Root type="button" class="icon-button" aria-label={$t('关闭')} disabled={busy} onclick={close}>{@html icon('close')}</Button.Root>
 </header>
 <div class="apify-body">
  {#if guideOpen}
   <article class="guide" role="presentation" onclick={externalLinkClick}>{@html guideHtml}</article>
  {:else if storeOpen}
   <form class="store-search" onsubmit={event=>{event.preventDefault();void browseStore(storeSearch)}}>
    <input type="search" aria-label={$t('搜索 Actor')} placeholder={$t('搜索 Actor')} maxlength="200" bind:this={storeInput} bind:value={storeSearch} />
    <Button.Root type="submit" class="outline">{$t('搜索')}</Button.Root>
   </form>
   {#if storeBusy}
    <p class="secondary" role="status">{$t('正在读取…')}</p>
   {:else if storeError}
    <p class="error" role="alert">{$t(storeError)}</p>
   {:else if storePage?.items.length}
    {#if !storeQuery}<h3>{$t('热门 Actor')}</h3>{/if}
    <ul class="store-results">
     {#each storePage.items as item (item.id)}
      <li><button type="button" class="store-actor" onclick={()=>selectStoreActor(item.id)}>
       <strong>{item.title}</strong>
       <span class="secondary">{item.username}/{item.name}</span>
       {#if item.description}<span class="secondary actor-description">{item.description}</span>{/if}
      </button></li>
     {/each}
    </ul>
   {:else}
    <p class="secondary" role="status">{$t('没有找到 Actor')}</p>
   {/if}
  {:else if !editing}
   {#if loading}
    <p class="secondary" role="status">{$t('正在读取…')}</p>
   {:else if !connections.length}
    <p class="secondary">{$t('尚未添加在线连接')}</p>
   {:else}
    {#each connections as row (row.source.id)}
     {@const saved=row.source}
     {@const scheduled=saved.schedule!=='manual'}
     {@const job=scheduled?undefined:$apifyRuns.get(saved.id)}
     {@const pending=row.running||apifyRunPending(job)}
     <section class="connection-card">
      <strong>{saved.name}</strong>
      <p class="secondary">{saved.connection.address}</p>
      <p class="path" title={saved.directory}>{saved.directory}</p>
      {#if job}
       {#if runLabels[job.phase]&&job.phase!=='preview'}<p class="secondary" role="status">{$t(runLabels[job.phase])}</p>{/if}
       {#if job.error}<p class="error" role="alert">{$t(job.error)}</p>{/if}
       {#if job.finalStatus}<p class="error" role="alert">{$t('Apify 运行结束（{value0}）',{value0:job.finalStatus})}</p>{/if}
      {/if}
      {#if row.last||row.next_at}
       <p class="secondary" role={row.running?'status':undefined}>
        {#if row.last}{$t(syncLabels[row.last.result.status]??row.last.result.status)}{/if}
        {#if row.next_at&&!row.running}{row.last?' · ':''}{$t('下次同步')} {new Intl.DateTimeFormat($language,{dateStyle:'short',timeStyle:'short'}).format(row.next_at*1000)}{/if}
       </p>
       {#each row.last?.result.issues??[] as issue}<p class="error" role="alert">{$t(issue.reason)}</p>{/each}
      {/if}
      {#if removeId===saved.id&&!pending}
       <p class="secondary">{$t('移除连接后，本地文件会保留。')}</p>
       <div class="actions">
        <Button.Root type="button" class="outline" disabled={busy} onclick={()=>removeId=''}>{$t('取消')}</Button.Root>
        <Button.Root type="button" class="outline" disabled={busy} onclick={()=>void removeConnection(saved.id)}>{$t('移除连接')}</Button.Root>
       </div>
      {:else}
       <div class="actions">
        {#if row.running}
         <Button.Root type="button" class="outline" disabled={busy} onclick={()=>void operateSync(saved.id,'cancel')}>{$t('停止同步')}</Button.Root>
        {:else if job?.phase==='running'||job?.phase==='cancelling'}
         {#if job.error}<Button.Root type="button" class="outline" disabled={job.checking} onclick={()=>void checkApifyRun(saved.id)}>{$t('刷新状态')}</Button.Root>{/if}
         <Button.Root type="button" class="text-button" disabled={job.phase==='cancelling'} onclick={()=>void cancelApifyRun(saved.id)}>{$t('取消运行')}</Button.Root>
        {:else if job?.phase==='preview'||job?.phase==='preview-error'||job?.phase==='import-error'}
         <Button.Root type="button" class="outline" disabled={busy} onclick={()=>showRun(saved)}>{$t(job.phase==='import-error'?'继续导入':'结果预览')}</Button.Root>
        {/if}
        {#if !pending}
         {#if !job||job.phase==='done'||job.phase==='failed'||job.phase==='aborted'}
          <Button.Root type="button" class="outline" disabled={busy} onclick={()=>scheduled?void operateSync(saved.id,'run'):void editConnection(saved,true)}>{$t('立即同步')}</Button.Root>
         {/if}
         <Button.Root type="button" class="text-button" disabled={busy} onclick={()=>void editConnection(saved)}>{$t('配置')}</Button.Root>
         <Button.Root type="button" class="text-button" disabled={busy} onclick={()=>removeId=saved.id}>{$t('移除连接')}</Button.Root>
        {/if}
       </div>
      {/if}
     </section>
    {/each}
   {/if}
  {:else if result}
   <div class="connection-card" role="status">
    <h3>{$t('导入完成')}</h3>
    <p>{$t('新增 {value0} · 更新 {value1} · 跳过 {value2} · 冲突 {value3} · 无正文 {value4}',{
     value0:result.imported,value1:result.updated,value2:result.skipped,value3:result.conflicts,value4:result.empty,
    })}</p>
    {#if result.imported+result.updated+result.skipped+result.conflicts>0}
     <p class="path">{result.directory}</p>
    {/if}
   </div>
  {:else if preview}
   <fieldset>
   <legend>{$t('结果预览')}</legend>
   {#if preview.samples.length}
    <div class="sample-bar">
     <label class="sample">{$t('预览记录')}
      <select bind:value={sampleIndex}>
       {#each preview.samples as _,index}<option value={index}>{index+1}</option>{/each}
      </select>
     </label>
     {#if preview.total!==null}<span class="secondary">{$t('共 {value0} 条结果',{value0:preview.total})}</span>{/if}
    </div>
    <pre>{preview.samples[sampleIndex]}</pre>
   {:else}
    <p role="status">{$t('本次运行没有可导入的结果')}</p>
   {/if}
   </fieldset>
   {#if preview.samples.length}
    <fieldset disabled={busy||!!run?.mapping}>
     <legend>{$t('导入字段')}</legend>
     <div class="mapping">
      <label>{$t('标题')}<select bind:value={title}><option value="">{$t('自动')}</option>{#each preview.fields as [key,label]}<option value={key}>{label}</option>{/each}</select></label>
      <label>{$t('原文链接')}<select bind:value={url}><option value="">{$t('无')}</option>{#each preview.fields as [key,label]}<option value={key}>{label}</option>{/each}</select></label>
      <label>{$t('唯一标识')}<select bind:value={identity}><option value="">{$t('自动')}</option>{#each preview.fields as [key,label]}<option value={key}>{label}</option>{/each}</select></label>
     </div>
     <p class="field-heading">{$t('正文字段')}</p>
     <div class="field-options">
      {#each [['',$t('整条记录')],...preview.fields] as [key,label]}
       <label class="check"><input type="checkbox" checked={body.includes(key)} onchange={event=>toggleBody(key,event.currentTarget.checked)} />{label}</label>
      {/each}
     </div>
    </fieldset>
   {/if}
  {:else if !run||actor}
   <fieldset disabled={busy||active||run?.phase==='import-error'}>
    <legend>{$t('连接与范围')}</legend>
    <label>{$t('连接名称')}<input maxlength="100" bind:value={connectionName} placeholder={actor?.title||'Apify'} /></label>
    <div class="actor-input">
     <label>{$t('Store 链接或 Actor ID')}
      <input bind:value={reference} readonly={!!connectionId} oninput={editReference} onpaste={connectionId?undefined:pasteReference}
       onkeydown={referenceKeydown}
       placeholder={crawler} data-autofocus autocomplete="off" />
     </label>
     <div class="actions">
      {#if !connectionId||!actor}<Button.Root type="button" class="outline" disabled={!reference.trim()} onclick={()=>void load()}>{$t('加载参数')}</Button.Root>{/if}
      {#if !connectionId}<Button.Root type="button" class="outline" onclick={openStore}>{$t('查看 Actor')}</Button.Root>{/if}
     </div>
    </div>
    <label>Apify API Token<input type="password" bind:value={token} placeholder={connectionId?$t('留空保留已有凭据'):''} autocomplete="off" /></label>
   </fieldset>
   {#if actor}
     <fieldset disabled={busy||active||run?.phase==='import-error'}>
      <legend>{$t('运行参数')}</legend>
      {#if actor.preset}
       <label>{$t('网站起始地址')}<input type="url" bind:value={website} placeholder="https://example.org/" /></label>
       <label class="check"><input type="checkbox" bind:checked={includePages} />{$t('包含子页面')}</label>
       {#if includePages}<label>{$t('最多抓取页数')}<input type="number" min="1" step="1" bind:value={pageLimit} /></label>{/if}
      {:else}
       {#if fields.length}
        <label class="check"><input type="checkbox" checked={useJson} onchange={event=>setMode(event.currentTarget.checked)} />{$t('编辑 JSON 参数')}</label>
       {/if}
       {#if useJson}
        <label>{$t('Actor 参数（JSON）')}<textarea rows="6" bind:value={jsonInput} spellcheck="false"></textarea></label>
       {:else}
        {#each fields as [key,field] (key)}
         {@const label=field.title||key}
         {@const required=actor.schema?.required?.includes(key)}
         {#if field.type==='boolean'}
          <label class="check"><input type="checkbox" checked={values[key]===true} onchange={event=>setField(key,event.currentTarget.checked)} />{label}</label>
         {:else}
          <label title={field.description}>{label}{required?' *':''}
           {#if field.enum?.length}
            <select value={field.enum.findIndex(value=>Object.is(value,values[key]))} onchange={event=>setField(key,field.enum?.[Number(event.currentTarget.value)])}>
             <option value="-1">{$t('选择')}</option>
             {#each field.enum as option,index}<option value={index}>{field.enumTitles?.[index]??String(option)}</option>{/each}
            </select>
           {:else if field.type==='array'||field.type==='object'}
            <textarea rows="3" value={fieldValue(values[key])} placeholder={field.type==='array'?'[]':'{}'}
             oninput={event=>setField(key,event.currentTarget.value)}></textarea>
           {:else if field.type==='number'||field.type==='integer'}
            <input type="number" step={field.type==='integer'?1:'any'} min={field.minimum} max={field.maximum}
             value={fieldValue(values[key])} oninput={event=>setField(key,event.currentTarget.value===''?undefined:event.currentTarget.valueAsNumber)} />
           {:else}
            <input type={field.isSecret||field.editor==='password'?'password':'text'} value={fieldValue(values[key])}
             oninput={event=>setField(key,event.currentTarget.value)} autocomplete="off" />
           {/if}
          </label>
         {/if}
        {/each}
       {/if}
      {/if}
      {#if !active&&!run?.mapping}<p class="secondary">{$t('运行 Actor 可能由 Apify 计费。')}</p>{/if}
     </fieldset>
   {/if}
  {/if}
  {#if editing&&!guideOpen&&!storeOpen&&!result&&!preview&&(!run||actor)}
   <fieldset disabled={busy||active}>
    <legend>{$t('保存位置')}</legend>
    <div class="destination">
     <p class="path">{savedDirectory||destination||(documents?`${documents}/sag-wiki/data-sources/apify/`:$t('文档/sag-wiki/data-sources/apify/'))}</p>
     {#if !web&&!connectionId}<Button.Root type="button" class="text-button" onclick={chooseDestination}>{$t('更改')}</Button.Root>{/if}
    </div>
   </fieldset>
   <fieldset disabled={busy||active}>
    <legend>{$t('同步计划')}</legend>
    <div class="schedule">
     <select aria-label={$t('同步频率')} value={schedule} onchange={event=>schedule=event.currentTarget.value as OnlineSchedule} disabled={!actor?.preset&&!hasMapping}>
      {#each Object.entries(onlineSchedules) as [value,label]}<option {value}>{$t(label)}</option>{/each}
     </select>
     {#if schedule==='daily'}<input type="time" aria-label={$t('每日同步时间')} bind:value={dailyTime} required />{/if}
    </div>
    {#if schedule!=='manual'}
     <label class="check"><input type="checkbox" bind:checked={paused} />{$t('暂停定时同步')}</label>
     <p class="secondary">{$t('后台服务运行时按本机时间同步。')}</p>
    {:else if actor&&!actor.preset&&!hasMapping}
     <p class="secondary">{$t('先选择字段并完成一次导入，再设置定时同步。')}</p>
    {/if}
   </fieldset>
  {/if}
  {#if !guideOpen&&!storeOpen&&run&&!result&&runLabels[run.phase]&&run.phase!=='preview'}<p class="secondary" role="status">{$t(runLabels[run.phase])}</p>{/if}
  {#if !guideOpen&&!storeOpen&&(error||run?.error)}<p class="error" role="alert">{$t(error||run?.error||'')}</p>{/if}
  {#if !guideOpen&&!storeOpen&&run?.finalStatus}<p class="error" role="alert">{$t('Apify 运行结束（{value0}）',{value0:run.finalStatus})}</p>{/if}
 </div>
 <footer>
  {#if guideOpen}
   <Button.Root type="button" class="outline" onclick={()=>guideOpen=false}>{$t('返回')}</Button.Root>
  {:else if storeOpen}
   <Button.Root type="button" class={storePage?'outline store-back':'outline'} onclick={leaveStore}>{$t('返回')}</Button.Root>
   {#if storePage}
    {@const {offset,limit,next}=storePage}
    <Button.Root type="button" class="outline" disabled={offset===0} onclick={()=>void browseStore(storeQuery,Math.max(0,offset-limit))}>{$t('上一页')}</Button.Root>
    <Button.Root type="button" class="outline" disabled={next===null} onclick={()=>{if(next!==null)void browseStore(storeQuery,next)}}>{$t('下一页')}</Button.Root>
   {/if}
  {:else if !editing}
   <Button.Root type="button" class="outline" disabled={busy} onclick={close}>{$t('完成')}</Button.Root>
   <Button.Root type="button" class="primary" disabled={busy||loading} onclick={newConnection}>{$t('添加连接')}</Button.Root>
  {:else if result}
   <Button.Root type="button" class="outline" onclick={showConnections}>{$t('完成')}</Button.Root>
  {:else if active}
   {#if run?.phase==='running'||run?.phase==='cancelling'}
    {#if run.error}<Button.Root type="button" class="outline" disabled={run.checking} onclick={()=>void checkApifyRun(runId)}>{$t('刷新状态')}</Button.Root>{/if}
    <Button.Root type="button" class="outline" disabled={run.phase==='cancelling'} onclick={()=>void cancelApifyRun(runId)}>{$t('取消运行')}</Button.Root>
   {/if}
   <Button.Root type="button" class="primary" onclick={close}>{$t('后台运行')}</Button.Root>
  {:else}
   <Button.Root type="button" class="outline" disabled={busy} onclick={cancelEdit}>{$t(run?'返回':'取消')}</Button.Root>
   {#if run?.phase==='import-error'}
    <Button.Root type="button" class="primary" onclick={()=>void importApifyRun(runId)}>{$t('继续导入')}</Button.Root>
   {:else if run?.phase==='preview'}
    {#if preview?.samples.length}
     <Button.Root type="button" class="primary" disabled={!body.length} onclick={importResults}>{$t('导入')}</Button.Root>
    {/if}
   {:else if run?.phase==='preview-error'}
    <Button.Root type="button" class="primary" onclick={()=>void previewApifyRun(runId)}>{$t('加载预览')}</Button.Root>
   {:else if actor||!run}
    <Button.Root type="button" class="outline" disabled={!canRun||!validSchedule||busy} onclick={saveOnly}>{$t('保存')}</Button.Root>
    <Button.Root type="button" class="primary" disabled={!canRun||!validSchedule||busy} onclick={()=>void start()}>{$t(actor?.preset||schedule!=='manual'?'保存并导入':'保存并运行')}</Button.Root>
   {/if}
  {/if}
 </footer>
</Modal>

<style>
 :global(.dialog-panel.apify-import){width:590px;max-width:calc(100vw - 32px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden;background:var(--color-paper)}
 header,footer{padding:16px 20px;display:flex;align-items:center;gap:10px}
 header{justify-content:space-between;border-bottom:1px solid var(--color-rule)}
 footer{justify-content:flex-end;border-top:1px solid var(--color-rule)}
 h2,h3,p{margin:0}
 h2{font-size:18px;font-weight:600}
 h3{font-size:16px;font-weight:600}
 .apify-body{display:grid;align-content:start;gap:12px;padding:18px 20px;overflow:auto}
 fieldset,.connection-card{margin:0;padding:12px 14px 14px;display:grid;gap:10px;border:1px solid var(--color-rule);border-radius:9px;min-width:0}
 .connection-card{gap:5px}
 legend{padding:0 5px;font-weight:600}
 label{display:grid;gap:5px;min-width:0;font-size:13px}
 input:not([type=checkbox]),select{min-width:0;color:var(--color-ink);background:var(--color-paper);font:inherit;border:1px solid var(--color-rule);border-radius:6px;padding:7px 9px}
 .heading-row,.actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
 .schedule{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
 .schedule select{min-width:120px}
 .actions{margin-top:4px}
 .heading-row :global(.guide-button){display:inline-flex;align-items:center;gap:5px;color:var(--color-muted);font-size:13px}
 .heading-row :global(.guide-button svg){width:14px;height:14px}
 .guide{font-size:14px;line-height:1.7;overflow-wrap:anywhere}
 .guide :global(h2){font-size:16px}
 .guide :global(a){color:var(--color-ink);text-decoration:underline}
 .actor-input,.destination{display:flex;align-items:end;justify-content:space-between;gap:10px}
 .actor-input label{flex:1}
 .actor-input .actions{margin-top:0}
 .store-search{display:flex;align-items:center;gap:8px}
 .store-search input{flex:1}
 .store-results{list-style:none;margin:0;padding:0;border:1px solid var(--color-rule);border-radius:9px}
 .store-results li+li{border-top:1px solid var(--color-rule)}
 .store-actor{width:100%;display:grid;gap:4px;text-align:start;background:transparent;color:inherit;border:0;padding:10px 12px;cursor:pointer;font:inherit}
 .store-actor:hover{background:var(--color-hover)}
 .actor-description{display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;line-clamp:2;overflow:hidden}
 :global(.store-back){margin-inline-end:auto}
 .destination{align-items:center}
 .field-heading{font-size:var(--text-ui);font-weight:500}
 .check{display:flex;align-items:center;gap:8px;overflow-wrap:anywhere}
 .mapping{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px}
 .field-options{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;max-height:158px;overflow:auto}
 .sample{display:flex;align-items:center;gap:8px}
 .sample-bar{display:flex;align-items:center;justify-content:space-between;gap:8px}
 .sample select{width:auto}
 pre{margin:0;padding:12px;max-height:220px;overflow:auto;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-bg);font:12px/1.5 monospace;white-space:pre-wrap;overflow-wrap:anywhere}
 textarea{min-height:70px;resize:vertical;border:1px solid var(--color-rule);border-radius:7px;padding:8px 10px;background:var(--color-paper);color:inherit;font:13px/1.5 monospace}
 .path,.secondary{color:var(--color-muted);font-size:13px;overflow-wrap:anywhere}
 .error{white-space:pre-wrap;overflow-wrap:anywhere}
 @media(max-width:520px){.mapping,.field-options{grid-template-columns:1fr}.actor-input{align-items:stretch;flex-direction:column}}
</style>
