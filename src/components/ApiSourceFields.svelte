<script lang="ts">
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import type {ApiSourceConfig} from '../lib/online-import';
 import type {Settings} from '../lib/types';

 export let config:ApiSourceConfig;
 export let id:string;
 export let connectionName:string;
 export let secret='';
 export let busy=false;
 export let valid=false;
 export let error='';

 let body=config.request.body??'';
 let bodyValid=true,tested=!!id;
 let response:unknown;
 let preview='',modelNames:string[]=[];
 const childPath=(path:string,key:string)=>path+'/'+key.replaceAll('~','~0').replaceAll('/','~1');
 const pathLabel=(path:string)=>path.slice(1).split('/').map(key=>key.replaceAll('~1','/').replaceAll('~0','~')).join(' › ');
 function valueAt(data:unknown,path:string):unknown{
  return path?path.slice(1).split('/').reduce<unknown>((node,key)=>{
   const name=key.replaceAll('~1','/').replaceAll('~0','~');
   return node!==null&&typeof node==='object'&&Object.hasOwn(node,name)?(node as Record<string,unknown>)[name]:undefined;
  },data):data;
 }
 function arrays(node:unknown,path=''):string[]{
  if(Array.isArray(node))return [path];
  if(node!==null&&typeof node==='object')return Object.entries(node).flatMap(([key,value])=>arrays(value,childPath(path,key)));
  return [];
 }
 function fields(node:unknown,path=''):string[]{
  if(node!==null&&typeof node==='object'&&!Array.isArray(node)){
   return Object.entries(node).flatMap(([key,value])=>fields(value,childPath(path,key)));
  }
  return [path];
 }
 function invalidate(){tested=false;response=undefined;preview='';}
 function updateBody(text:string){
  body=text;
  invalidate();
  try{if(text.trim())JSON.parse(text);config.request.body=text.trim()?text:null;bodyValid=true;}
  catch{bodyValid=false;}
 }
 function amend(kind:'query'|'headers',index:number,value:[string,string]|null){
  const pairs=[...config.request[kind]];
  if(value)pairs[index]=value;else pairs.splice(index,1);
  config.request[kind]=pairs;
  invalidate();
 }
 function toggleField(path:string,checked:boolean){
  config.fields=checked?[...config.fields,path]:config.fields.filter(field=>field!==path);
  preview='';
 }
 function chooseRecords(path:string){
  config.records_path=path;config.fields=[];config.dedup_field=null;preview='';
 }
 async function test(){
  error='';busy=true;invalidate();
  try{
   const data=await api<unknown>('online_import',{action:{action:'api_sample',id:id||null,request:copyConfig(config.request),secret:secret||null}});
   const choices=arrays(data);
   if(!id&&!config.fields.length&&choices.length)config.records_path=choices[0];
   if(config.records_path&&!choices.includes(config.records_path))chooseRecords(choices[0]??'');
   response=data;tested=true;
  }catch(e){error=String(e);}finally{busy=false;}
 }
 function copyConfig<T>(value:T):T{return JSON.parse(JSON.stringify(value));}
 async function loadModels(){
  if(modelNames.length)return;
  try{modelNames=(await api<Settings>('get_settings')).answer.models;}
  catch(e){error=String(e);}
 }
 function toggleCleanup(checked:boolean){
  config.cleanup=checked?{model:'',prompt:''}:null;
  preview='';
  if(checked)void loadModels();
 }
 async function showPreview(){
  error='';preview='';busy=true;
  try{preview=await api<string>('online_import',{action:{action:'api_preview',config:copyConfig(config),response}});}
  catch(e){error=String(e);}finally{busy=false;}
 }
 $: if(config.cleanup)void loadModels();
 $: listPaths=[...new Set(['',...arrays(response),config.records_path])];
 $: selectedRecords=valueAt(response,config.records_path);
 $: sampleRows=Array.isArray(selectedRecords)?selectedRecords:selectedRecords===undefined?[]:[selectedRecords];
 $: fieldPaths=[...new Set([...sampleRows.flatMap(row=>fields(row)),...config.fields,...(config.dedup_field===null?[]:[config.dedup_field])])];
 $: hasResponse=response!==undefined;
 $: canTest=!!config.request.url.trim()&&(config.request.method!=='POST'||bodyValid)
  && [...config.request.query,...config.request.headers].every(([name])=>!!name.trim())&&!busy;
 $: valid=tested&&!!config.fields.length&&(!config.cleanup||!!config.cleanup.model);
</script>

<fieldset class="api-section">
 <legend>{$t('请求')}</legend>
 <label>{$t('连接名称')}<input bind:value={connectionName} placeholder={$t('API 请求')} maxlength="100" /></label>
 <div class="request-line">
  <label>{$t('请求方法')}
   <span class="method-select">
    <select value={config.request.method} onchange={event=>{config.request.method=event.currentTarget.value==='POST'?'POST':'GET';invalidate();}}>
     <option value="GET">GET</option>
     <option value="POST">POST</option>
    </select>
    <span aria-hidden="true">{@html icon('down')}</span>
   </span>
  </label>
  <label>{$t('API 地址')}<input type="url" bind:value={config.request.url} oninput={invalidate} placeholder="https://…" autocomplete="off" /></label>
 </div>
 {#if config.request.method==='POST'}
  <label>{$t('JSON 请求体')}<textarea rows="4" value={body} oninput={event=>updateBody(event.currentTarget.value)} spellcheck={false} placeholder={'{ }'}></textarea></label>
  {#if !bodyValid}<p class="invalid" role="status">{$t('请输入有效的 JSON 请求体')}</p>{/if}
 {/if}
 <details class="advanced">
  <summary><span aria-hidden="true">{@html icon('chevron')}</span>{$t('参数与鉴权')}</summary>
  <div class="advanced-body">
   {#each ['query','headers'] as kind}
    {@const list=kind==='query'?'query':'headers'}
    <div class="pairs">
     <div class="section-heading">
      <strong>{$t(list==='query'?'查询参数':'请求头')}</strong>
      <Button.Root type="button" class="text-button add-pair" onclick={()=>amend(list,config.request[list].length,['',''])}>
       <span aria-hidden="true">{@html icon('plus')}</span>{$t(list==='query'?'添加查询参数':'添加请求头')}
      </Button.Root>
     </div>
     {#each config.request[list] as pair,index}
      <div class="pair">
       <input aria-label={$t('名称')} placeholder={$t('名称')} value={pair[0]} oninput={event=>amend(list,index,[event.currentTarget.value,pair[1]])} />
       <input aria-label={$t('参数值')} placeholder={$t('参数值')} value={pair[1]} oninput={event=>amend(list,index,[pair[0],event.currentTarget.value])} />
       <Button.Root type="button" class="icon-button remove-pair" aria-label={$t('移除')} title={$t('移除')} onclick={()=>amend(list,index,null)}>
        <span aria-hidden="true">{@html icon('close')}</span>
       </Button.Root>
      </div>
     {/each}
    </div>
   {/each}
   <div class="authorization">
    <label>{$t('鉴权请求头')}<input bind:value={config.request.secret_header} oninput={invalidate} placeholder="Authorization" autocomplete="off" /></label>
    {#if config.request.secret_header.trim()}
     <label>{$t('API 鉴权值')}<input type="password" bind:value={secret} oninput={invalidate} placeholder={id?$t('留空保留已有凭据'):'Bearer …'} autocomplete="off" /></label>
    {/if}
   </div>
  </div>
 </details>
 <div class="section-heading">
  <label class="check"><input type="checkbox" bind:checked={config.request.allow_lan} onchange={invalidate} />{$t('允许内网 API')}</label>
  <Button.Root type="button" class="outline" disabled={!canTest} onclick={test}>{$t('测试请求')}</Button.Root>
 </div>
</fieldset>

{#if hasResponse||fieldPaths.length}
 <fieldset class="api-section">
  <legend>{$t('保留字段')}</legend>
  {#if hasResponse}
   <div class="mapping-controls">
    <label class="field-setting">{$t('记录位置')}<select value={config.records_path} onchange={event=>chooseRecords(event.currentTarget.value)}>
     {#each listPaths as path}<option value={path}>{path?pathLabel(path):$t('整个响应')}</option>{/each}
    </select></label>
    {#if sampleRows.length}
     <Button.Root type="button" class="outline" disabled={!valid||busy} onclick={showPreview}>{$t('预览 Markdown')}</Button.Root>
    {/if}
   </div>
   <details class="sample">
    <summary><span aria-hidden="true">{@html icon('chevron')}</span>{$t('JSON 样例')}</summary>
    <pre>{JSON.stringify(response,null,2)}</pre>
   </details>
  {/if}
  {#if fieldPaths.length}
   <div class="fields">
    {#each fieldPaths as path}
     <label class="check field-option"><input type="checkbox" checked={config.fields.includes(path)} onchange={event=>toggleField(path,event.currentTarget.checked)} /><span>{path?pathLabel(path):$t('记录全文')}</span></label>
    {/each}
   </div>
   <label class="field-setting">{$t('按 ID 去重')}<select value={config.dedup_field??'none'} onchange={event=>config.dedup_field=event.currentTarget.value==='none'?null:event.currentTarget.value}>
    <option value="none">{$t('不去重')}</option>
    {#each fieldPaths as path}<option value={path}>{path?pathLabel(path):$t('记录全文')}</option>{/each}
   </select></label>
  {:else}<p class="muted">{$t('API 未返回可选字段')}</p>{/if}
  {#if preview}<pre>{preview}</pre>{/if}
 </fieldset>
{/if}

<fieldset class="api-section">
 <legend>{$t('AI 清洗')}</legend>
 <label class="check cleanup-toggle"><input type="checkbox" checked={!!config.cleanup} onchange={event=>toggleCleanup(event.currentTarget.checked)} />{$t('启用')}</label>
 {#if config.cleanup}
  <label>{$t('问答模型')}<select bind:value={config.cleanup.model} onchange={()=>preview=''}>
   <option value="">{$t('选择问答模型')}</option>
   {#each modelNames as model}<option value={model}>{model}</option>{/each}
  </select></label>
  <label>{$t('自定义提示词')}<textarea rows="4" bind:value={config.cleanup.prompt} oninput={()=>preview=''} placeholder={$t('去掉推广内容，保留事实，输出 Markdown。')}></textarea></label>
 {/if}
</fieldset>

<style>
 p{margin:0}
 label,.pairs,.authorization{display:grid;gap:4px;font-size:13px}
 .request-line,.pair{display:grid;gap:8px;align-items:center}
 .request-line{grid-template-columns:auto minmax(0,1fr);align-items:start}
 .method-select{display:grid;position:relative}
 .method-select select{appearance:none;height:36px;padding-inline-end:28px}
 .method-select span{position:absolute;inset-inline-end:8px;top:0;bottom:0;display:grid;place-items:center;pointer-events:none;color:var(--color-muted)}
 .method-select :global(svg){width:14px;height:14px}
 .pair{grid-template-columns:minmax(0,1fr) minmax(0,1fr) 32px}
 .check{display:flex;align-items:center;gap:8px;min-width:0;cursor:pointer}
 .check input{accent-color:var(--color-ink);margin:0;flex:none}
 .section-heading{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap}
 .advanced{border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel)}
 .advanced-body{display:grid;gap:12px;padding:0 12px 12px}
 .pairs{gap:8px}
 .pairs strong{font-weight:500}
 .pairs :global(.add-pair){display:inline-flex;gap:4px;align-items:center;min-height:28px;padding:4px;color:var(--color-muted)}
 .pair :global(.remove-pair){width:32px;height:32px;color:var(--color-muted)}
 .field-setting{grid-template-columns:minmax(max-content,108px) minmax(0,1fr);align-items:center;gap:8px}
 .mapping-controls{display:flex;align-items:end;gap:8px;flex-wrap:wrap}
 .mapping-controls>.field-setting{flex:1 1 232px}
 .fields{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,200px),1fr));gap:4px 8px;max-height:160px;overflow:auto}
 .field-option{min-height:28px;padding:4px 8px;border-radius:4px}
 .field-option:hover{background:var(--color-panel)}
 .field-option span{overflow-wrap:anywhere}
 .cleanup-toggle{min-height:24px;font-weight:500}
 summary{display:flex;align-items:center;gap:4px;cursor:pointer;list-style:none;color:var(--color-secondary);font-size:13px;min-height:32px}
 .advanced summary{padding-inline:8px}
 summary::-webkit-details-marker{display:none}
 summary span,.pairs :global(button span){display:grid;place-items:center}
 summary :global(svg),.pairs :global(button span svg){width:16px;height:16px;stroke-width:1.5}
 details[open]>summary span{transform:rotate(90deg)}
 input:not([type=checkbox]),textarea,select{font:inherit;color:var(--color-ink);background:var(--color-paper);border:1px solid var(--color-rule);border-radius:6px;min-width:0;min-height:36px;padding:4px 8px}
 textarea{resize:vertical;font-family:var(--font-mono,monospace)}
 pre{white-space:pre-wrap;overflow:auto;overflow-wrap:anywhere;max-height:200px;margin:0;padding:12px;background:var(--color-panel);border-radius:8px;font-size:12px}
 .muted{color:var(--color-muted)}
 .invalid{color:var(--color-danger,#ba3838);font-size:13px}
 @media(max-width:480px){
  .field-setting{grid-template-columns:1fr;gap:4px}
 }
</style>
