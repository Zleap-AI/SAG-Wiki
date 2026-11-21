<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {changeWebAccessCode,copyText,web} from '../lib/web-connection';
 import {t,language,interfaceLocales} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import Switch from './ui/Switch.svelte';
 import Input from './ui/Input.svelte';
 import Select from './ui/Select.svelte';
 import Modal from './ui/Modal.svelte';
 import ApiKeyScope from './ApiKeyScope.svelte';
 export let pending=false;
 type Status={enabled:boolean;running:boolean;web_enabled:boolean;web_running:boolean;web_access_code_required?:boolean;host:string;port:number;local_url:string;lan_urls:string[];key_count:number;autostart:boolean;last_error:string|null};
 type Scope={type:'all'}|{type:'folders';paths:string[]};
 type Key={id:string;name:string;prefix:string;created_at:number;copyable:boolean;scope?:Scope};
 let scopeMode='all',scopePaths:string[]=[],editing:Key|null=null;
 let choosingMcp:{url:string;target:string}|null=null;
 const keyScope=(key:Key)=>key.scope?.type==='folders'?key.scope.paths.join(' · '):$t('全部知识库');
 let apiEnabled=false,webEnabled=false,autostartEnabled=false,copied='';
 let webAccessCodeRequired=true;
 let rotating=false;
 let copyTimer:ReturnType<typeof setTimeout>|undefined;
 onDestroy(()=>clearTimeout(copyTimer));
 let status:Status|undefined,keys:Key[]=[],host='',port:number|undefined=24681,error='',notice='',creating=false,keyName='',created:{key:string}|null=null,revoking:Key|null=null,dialogError='',refreshing=false;
 $: changed=!!status&&(host!==status.host||port!==status.port);
 $: mcpUrl=status?`${status.local_url}/mcp`:'';
 $: example=`curl ${status?.local_url??'http://127.0.0.1:24681'}/v1/search \\\n  -H "Authorization: Bearer <API_KEY>" \\\n  -H "Content-Type: application/json" \\\n  -d '{"query":"${$t('部署步骤')}","mode":"deep","limit":10}'`;
 const call=<T,>(action:Record<string,unknown>)=>api<T>('manage_api',{action});
 async function refresh(reset=false){
  const next=await call<Status>({action:'status'});
  if(reset||!status||!changed){host=next.host;port=next.port;}
  status=next;apiEnabled=next.enabled;webEnabled=next.web_enabled===true;autostartEnabled=next.autostart;keys=await call<Key[]>({action:'list_keys'});
  webAccessCodeRequired=next.web_access_code_required!==false;
 }
 onMount(()=>{
  let disposed=false;
  async function poll(){if(pending||refreshing)return;refreshing=true;try{await refresh();}catch(e){if(!disposed)error=String(e);}finally{refreshing=false;}}
  void poll();const timer=setInterval(()=>void poll(),3000);
  return()=>{disposed=true;clearInterval(timer);created=null;};
 });
 async function act(action:Record<string,unknown>,run=()=>call(action)){
  pending=true;error='';notice='';
  try{await run();await refresh(action.action==='configure'||action.action==='start'||action.action==='web_start');return true;}catch(e){error=String(e);return false;}finally{pending=false;}
 }
 async function toggleService(service:'api'|'web',enabled:boolean){
  if(!web&&enabled&&(!port||!Number.isInteger(port)||port<1||port>65535))error=$t('请输入有效端口（1–65535）');
  else{
   const action=service==='web'?(enabled?'web_start':'web_stop'):(enabled?'start':'stop');
   await act(enabled&&!web?{action,host,port}:{action});
  }
  if(service==='web')webEnabled=status!.web_enabled;else apiEnabled=status!.enabled;
 }
 async function toggleAutostart(enabled:boolean){await act({action:'autostart',enabled});autostartEnabled=status!.autostart;}
 async function toggleWebAccessCode(required:boolean){
  const action={action:'web_set_access_code' as const,required};
  await act(action,web?()=>changeWebAccessCode(action):undefined);
  webAccessCodeRequired=status!.web_access_code_required!==false;
 }
 async function rotateWebCode(){
  copied='';clearTimeout(copyTimer);
  const action={action:'web_rotate_code' as const};
  if(await act(action,web?()=>changeWebAccessCode(action):undefined)){rotating=false;notice=$t('访问码已更换');}
 }
 async function saveConnection(){if(await act({action:'configure',host,port}))notice=$t('连接配置已保存');}
 async function copy(value:string|Promise<string>,target:string){error='';dialogError='';try{
  if(web)await copyText(await value);
  else if(typeof value==='string')await navigator.clipboard.writeText(value);
  else await navigator.clipboard.write([new ClipboardItem({'text/plain':value.then(text=>new Blob([text],{type:'text/plain'}))})]);
  copied=target;notice=$t('已复制');clearTimeout(copyTimer);copyTimer=setTimeout(()=>{copied='';notice='';},2000);
 }catch(e){if(creating)dialogError=String(e);else error=String(e);}}
 async function copyCredential(source:{id:string}|{url:string;target:string;id?:string}){
  error='';notice='';copied='';clearTimeout(copyTimer);
  if('url' in source&&!keys.some(key=>key.copyable)){error='请先创建 API Key';return;}
  if('url' in source&&!source.id){
   const available=keys.filter(key=>key.copyable);
   if(available.length>1){choosingMcp=source;return;}
   source={...source,id:available[0].id};
  }
  const selected=source;
  await copy(call<{key:string}>({action:'copy_key',id:selected.id}).then(({key})=>'url' in selected?JSON.stringify({mcpServers:{'sag-wiki':{type:'http',url:selected.url,headers:{Authorization:`Bearer ${key}`}}}},null,2):key),'url' in selected?selected.target:selected.id);
  choosingMcp=null;
 }
 async function create(){
  if(scopeMode==='folders'&&!scopePaths.length){dialogError='请选择至少一个文件夹';return;}
  pending=true;dialogError='';
  try{created=await call<{key:string}>(scopeMode==='folders'?{action:'create_scoped_key',name:keyName.trim(),paths:scopePaths}:{action:'create_key',name:keyName.trim()});await refresh();}catch(e){dialogError=String(e);}finally{pending=false;}
 }
 async function revoke(){
  if(!revoking)return;pending=true;dialogError='';
  try{await call({action:'revoke_key',id:revoking.id});revoking=null;await refresh();}catch(e){dialogError=String(e);}finally{pending=false;}
 }
 function scopeValue():Scope{return scopeMode==='all'?{type:'all'}:{type:'folders',paths:scopePaths};}
 function editScope(key:Key){editing=key;scopeMode=key.scope?.type??'all';scopePaths=key.scope?.type==='folders'?[...key.scope.paths]:[];dialogError='';}
 async function saveScope(){
  if(!editing)return;
  if(scopeMode==='folders'&&!scopePaths.length){dialogError='请选择至少一个文件夹';return;}
  pending=true;dialogError='';
  try{await call({action:'update_key_scope',id:editing.id,scope:scopeValue()});editing=null;await refresh();}catch(e){dialogError=String(e);}finally{pending=false;}
 }
 function closeCreate(){if(!pending){creating=false;created=null;keyName='';dialogError='';copied='';notice='';clearTimeout(copyTimer);}}
</script>

<div class="api-settings" lang={$language} aria-busy={pending}>
 <header class="settings-page-heading api-page-heading">
  <h2>{$t('远程服务')}</h2>
  <div class="api-docs-link"><a href={`http://sag-wiki.zleap.com/${$language==='zh-CN'?'zh':'en'}/docs/api`} target="_blank" rel="noopener noreferrer" on:click|preventDefault={()=>{void api('open_external_link',{url:`http://sag-wiki.zleap.com/${$language==='zh-CN'?'zh':'en'}/docs/api`}).catch(e=>error=String(e));}}>{$t('API 文档')}{@html icon('externalLink')}</a></div>
 </header>
 {#if !status}<p role="status">{$t('正在读取 API 设置…')}</p>
 {:else}
 <section class="settings-group web-service-settings">
  <div class="setting-row">
   <div><strong>{$t('网页服务')}</strong><p class="api-run-state" data-state={status.web_running?'running':'stopped'} role="status">{$t(status.web_running?'运行中':status.web_enabled?'已启用，未运行':'已关闭')}</p></div>
   {#if !web}<Switch bind:checked={webEnabled} disabled={pending} label={$t('网页服务')} onCheckedChange={enabled=>toggleService('web',enabled)}/>{/if}
  </div>
  {#if status.web_running}
   {#if !web}<div class="api-address"><span>{$t('本机地址')}</span><code>{status.local_url}</code><Button.Root type="button" class="icon-button" aria-label={$t('打开网页')} onclick={()=>api('open_external_link',{url:status!.local_url}).catch(e=>error=String(e))}>{@html icon('externalLink')}</Button.Root></div>{/if}
   {#each status.lan_urls as url}
    <div class="api-address"><span>{$t('局域网地址')}</span><code>{url}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制局域网地址')} onclick={()=>copy(url,'web-'+url)}>{@html icon(copied==='web-'+url?'check':'copy')}</Button.Root></div>
   {/each}
   <div class="setting-row">
    <div><div class="web-code-label"><strong>{$t('网页访问码')}</strong><Switch bind:checked={webAccessCodeRequired} disabled={pending} label={$t('网页访问码')} onCheckedChange={toggleWebAccessCode}/></div>
     {#if !webAccessCodeRequired}<p>{$t('任何能打开网页地址的人都可以修改设置、管理 API Key 和清除应用数据。')}</p>{/if}
    </div>
    {#if webAccessCodeRequired}<div class="web-code-actions"><Button.Root type="button" class="outline" disabled={pending} onclick={()=>{rotating=true;error='';}}>{$t('更换访问码')}</Button.Root><Button.Root type="button" class="outline" disabled={pending} onclick={()=>copy(call<{code:string}>({action:'web_code'}).then(value=>value.code),'web-code')}>{$t(copied==='web-code'?'已复制':'复制访问码')}</Button.Root></div>{/if}
   </div>
  {/if}
 </section>
 <section class="settings-group api-endpoints">
  <div class="setting-row"><div><strong>{$t('API 和 MCP')}</strong><p class="api-run-state" data-state={status.running?'running':status.enabled?'enabled':'stopped'} role="status">{$t(status.running?'运行中':status.enabled?'已启用，未运行':'已关闭')}</p></div><Switch bind:checked={apiEnabled} disabled={pending} label={$t('API 和 MCP')} onCheckedChange={enabled=>toggleService('api',enabled)}/></div>
  {#if status.last_error}<p class="form-error" role="alert">{status.last_error}</p>{/if}
  {#if status.running}
  <div class="api-address"><span>{$t('本机地址')}</span><code>{status.local_url}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制本机地址')} onclick={()=>copy(status!.local_url,'local')}>{@html icon(copied==='local'?'check':'copy')}</Button.Root></div>
  {#each status.lan_urls as url}<div class="api-address"><span>{$t('局域网地址')}</span><code>{url}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制局域网地址')} onclick={()=>copy(url,url)}>{@html icon(copied===url?'check':'copy')}</Button.Root></div>{/each}
  <div class="api-address"><span>{$t('本机 MCP')}</span><code>{mcpUrl}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制本机 MCP 配置')} title={$t('复制本机 MCP 配置')} onclick={()=>copyCredential({url:mcpUrl,target:'mcp-local'})}>{@html icon(copied==='mcp-local'?'check':'copy')}</Button.Root></div>
  {#each status.lan_urls as url}
   {@const address = `${url}/mcp`}
   <div class="api-address"><span>{$t('局域网 MCP')}</span><code>{address}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制局域网 MCP 配置')} title={$t('复制局域网 MCP 配置')} onclick={()=>copyCredential({url:address,target:address})}>{@html icon(copied===address?'check':'copy')}</Button.Root></div>
  {/each}
  {#if error==='请先创建 API Key'}<p class="form-error api-key-prompt" role="alert">{$t(error)}</p>{/if}
  {/if}
  <details class="api-example"><summary>{@html icon('chevron')}{$t('调用示例')}</summary><pre><code>{example}</code></pre><Button.Root type="button" class="outline" onclick={()=>copy(example,'example')}>{copied==='example'?$t('已复制'):$t('复制示例')}</Button.Root></details>
 </section>
 {#if !web}<h3 class="settings-section-title" id="api-connection-heading">{$t('连接配置')}</h3>
 <section class="settings-group" aria-labelledby="api-connection-heading">
  <div class="api-connection">
   <label>{$t('连接范围')}<Select label={$t('连接范围')} placeholder={$t('选择连接范围')} bind:value={host} items={[{value:'127.0.0.1',label:$t('仅本机')},{value:'0.0.0.0',label:$t('本机与局域网')}]} disabled={pending||status.running||status.web_running}/></label>
   <label>{$t('端口')}<Input aria-label={$t('端口')} type="number" min={1} max={65535} bind:value={port} disabled={pending||status.running||status.web_running}/></label>
   {#if !status.running&&!status.web_running}<Button.Root type="button" class="outline" disabled={pending||!changed||!port} onclick={saveConnection}>{$t('保存连接配置')}</Button.Root>{/if}
  </div>
 </section>
 <section class="settings-group">
  <div class="setting-row"><div><strong>{$t('开机自启')}</strong><p>{$t('登录系统后恢复已启用的远程服务。')}</p></div><Switch bind:checked={autostartEnabled} disabled={pending} label={$t('开机自启')} onCheckedChange={toggleAutostart}/></div>
 </section>{/if}
 <header class="settings-group-heading"><h3 id="api-keys-heading">API Key</h3><Button.Root type="button" class="outline" disabled={pending} onclick={()=>{creating=true;dialogError='';scopeMode='all';scopePaths=[];}}>{$t('创建 Key')}</Button.Root></header>
 <section class="settings-group" aria-labelledby="api-keys-heading">
  {#if keys.length}
   <table class="api-key-table"><thead><tr><th>{$t('名称')}</th><th>Key</th><th>{$t('创建时间')}</th><th><span class="sr-only">{$t('操作')}</span></th></tr></thead><tbody>
    {#each keys as key}<tr><td>{key.name}<small class="api-scope-description" title={keyScope(key)}>{keyScope(key)}</small></td><td><div class="api-key-value"><code>{key.prefix}</code>{#if key.copyable}<Button.Root type="button" class="icon-button" aria-label={$t('复制 Key：{value0}',{value0:key.name})} onclick={()=>copyCredential({id:key.id})}>{@html icon(copied===key.id?'check':'copy')}</Button.Root>{/if}</div></td><td>{new Date(key.created_at).toLocaleDateString(interfaceLocales[$language].format)}</td><td><Button.Root type="button" class="api-revoke" disabled={pending} aria-label={$t('编辑 {value0} 的访问范围',{value0:key.name})} onclick={()=>editScope(key)}>{$t('编辑范围')}</Button.Root><Button.Root type="button" class="api-revoke" disabled={pending} onclick={()=>{revoking=key;dialogError='';}} aria-label={$t('撤销 {value0}',{value0:key.name})}>{$t('撤销 Key')}</Button.Root></td></tr>{/each}
   </tbody></table>
  {:else}<p class="settings-help api-key-empty">{$t('没有有效 Key，请求将返回未授权（401）。')}</p>{/if}
 </section>
 {/if}
 {#if error&&error!=='请先创建 API Key'}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 {#if notice}<p class="settings-status" role="status">{notice}</p>{/if}
</div>

{#if rotating}<Modal label={$t('更换访问码')} onClose={()=>{if(!pending)rotating=false;}}>
 <div class="dialog-head"><h2>{$t('更换访问码')}</h2></div>
 <div class="dialog-body"><p>{$t(web?'旧访问码将立即失效，其他已登录的浏览器需要重新输入新码。':'旧访问码将立即失效，已登录的浏览器需要重新输入新码。')}</p>{#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={pending} onclick={()=>rotating=false}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending} onclick={rotateWebCode}>{$t('确认更换')}</Button.Root></div>
</Modal>{/if}
{#if creating}<Modal label={$t('创建 Key')} onClose={closeCreate}>
 <div class="dialog-head"><h2>{$t('创建 Key')}</h2></div>
 <div class="dialog-body">
 {#if created}<code class="api-secret">{created.key}</code><Button.Root type="button" class="outline" onclick={()=>copy(created!.key,'key')}>{copied==='key'?$t('已复制'):$t('复制 Key')}</Button.Root>
 {:else}<label class="field-label">{$t('名称')}<Input data-autofocus aria-label={$t('Key 名称')} bind:value={keyName} maxlength={100} disabled={pending} onkeydown={e=>{if(e.key==='Enter'&&!e.isComposing){e.preventDefault();if(keyName.trim())void create();}}}/></label><ApiKeyScope bind:mode={scopeMode} bind:paths={scopePaths} disabled={pending}/>{/if}
 {#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}
 </div>
 <div class="dialog-footer">{#if created}<Button.Root type="button" class="primary" disabled={pending} onclick={closeCreate}>{$t('完成')}</Button.Root>{:else}<Button.Root type="button" disabled={pending} onclick={closeCreate}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending||!keyName.trim()} onclick={create}>{$t('创建 Key')}</Button.Root>{/if}</div>
</Modal>{/if}
{#if editing}<Modal label={$t('编辑访问范围')} onClose={()=>{if(!pending)editing=null;}}>
 <div class="dialog-head"><h2>{$t('编辑访问范围')} · {editing.name}</h2></div>
 <div class="dialog-body"><p class="settings-help">{$t('保存后立即生效，并取消此 Key 正在执行和排队的请求。其他 Key 不受影响。')}</p><ApiKeyScope bind:mode={scopeMode} bind:paths={scopePaths} disabled={pending}/>{#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={pending} onclick={()=>editing=null}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending} onclick={saveScope}>{$t('保存访问范围')}</Button.Root></div>
</Modal>{/if}
{#if choosingMcp}<Modal label={$t('选择 API Key')} onClose={()=>choosingMcp=null}>
 <div class="dialog-head"><h2>{$t('选择 API Key')}</h2></div><div class="dialog-body"><p class="settings-help">{$t('选择此项目使用的 Key，复制的 MCP 配置只使用该 Key 的访问范围。')}</p>{#each keys.filter(key=>key.copyable) as key}<Button.Root type="button" class="api-mcp-key" aria-label={$t('使用 {value0} 复制 MCP 配置',{value0:key.name})} onclick={()=>copyCredential({...choosingMcp!,id:key.id})}><strong>{key.name}</strong><small>{keyScope(key)}</small></Button.Root>{/each}{#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" onclick={()=>choosingMcp=null}>{$t('取消')}</Button.Root></div>
</Modal>{/if}
{#if revoking}<Modal label={$t('撤销 Key')} onClose={()=>{if(!pending)revoking=null;}}>
 <div class="dialog-head"><h2>{$t('撤销 Key')}</h2></div><div class="dialog-body"><p>{$t('撤销后将取消此 Key 的搜索，其他 Key 和 API 服务继续运行。')}</p><strong>{revoking.name}</strong>{#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={pending} onclick={()=>revoking=null}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending} onclick={revoke}>{$t('确认撤销')}</Button.Root></div>
</Modal>{/if}

<style>
 .api-page-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.api-page-heading h2{font-weight:600}
 .api-docs-link{flex-shrink:0;padding-top:4px}.api-docs-link a{display:inline-flex;align-items:center;gap:8px;font-size:13px;color:var(--color-secondary);text-underline-offset:3px}.api-docs-link :global(svg){width:16px;height:16px}
 .api-docs-link a:focus-visible,summary:focus-visible{outline:2px solid var(--color-focus);outline-offset:4px;border-radius:4px}
 .web-code-actions{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px}
 .web-code-label{display:flex;align-items:center;gap:12px}
 .setting-row{padding-left:16px;padding-right:16px}.api-run-state{display:flex;align-items:center;gap:6px}.api-run-state::before{content:'';width:6px;height:6px;flex-shrink:0;border-radius:50%;background:var(--color-muted)}.api-run-state[data-state=running]::before{background:var(--color-success)}
 .api-address{display:grid;grid-template-columns:76px minmax(0,1fr) 32px;align-items:center;gap:12px;padding:10px 16px;font-size:12px}.api-address>span{color:var(--color-secondary)}.api-address code{overflow-wrap:anywhere;font-family:var(--font-mono);font-size:12px;font-variant-numeric:tabular-nums;user-select:text;-webkit-user-select:text}.api-address :global(button){justify-self:end}
 .api-settings:lang(en) .api-address{grid-template-columns:96px minmax(0,1fr) 32px}
 details{border-top:1px solid var(--color-rule);padding:14px 16px}summary{display:flex;align-items:center;gap:8px;cursor:pointer;list-style:none;font-size:13px;font-weight:500}summary::-webkit-details-marker{display:none}summary :global(svg){width:16px;height:16px;color:var(--color-secondary);flex-shrink:0;transition:transform var(--dur-fast) var(--ease-out)}details[open]>summary :global(svg){transform:rotate(90deg)}.api-connection{display:grid;grid-template-columns:minmax(0,1fr) 100px;gap:14px;padding:16px}.api-connection label{display:grid;gap:8px;font-size:12px}.api-connection > :global(button){justify-self:start;grid-column:1/-1}
 .api-key-table{width:100%;border-collapse:collapse;font-size:12px;text-align:left}.api-key-table th{font-weight:400;color:var(--color-secondary)}.api-key-table th,.api-key-table td{padding:12px 6px;border-bottom:1px solid var(--color-rule)}.api-key-table th:first-child,.api-key-table td:first-child{padding-left:16px}.api-key-table th:last-child,.api-key-table td:last-child{padding-right:16px}.api-key-table td:first-child{overflow-wrap:anywhere;max-width:140px}.api-key-table td:last-child{text-align:right}:global(.api-revoke){color:var(--color-secondary);background:none;border:0;cursor:pointer}
 .api-scope-description{display:block;margin-top:5px;font-size:11px;color:var(--color-secondary);overflow-wrap:anywhere}:global(.api-mcp-key){display:flex;flex-direction:column;align-items:flex-start;gap:6px;width:100%;text-align:left;padding:12px;border:1px solid var(--color-rule);border-radius:8px;margin-top:10px}:global(.api-mcp-key small){font-size:12px;color:var(--color-secondary);overflow-wrap:anywhere}
 .api-key-empty{padding:16px}
 .setting-row + .api-example{border-top:0}
 .api-key-value{display:flex;align-items:center;gap:4px;flex-wrap:wrap}.api-key-prompt{margin:0;padding:0 16px 10px}
 .api-key-table thead{background:var(--color-panel)}.api-key-table tbody tr:last-child td{border-bottom:0}.api-key-table code{font-family:var(--font-mono);font-variant-numeric:tabular-nums;white-space:nowrap}
 .api-example pre{overflow:auto;padding:14px;background:var(--color-panel);border:1px solid var(--color-rule);border-radius:8px;font-size:12px}.api-secret{display:block;overflow-wrap:anywhere;user-select:all;padding:16px;margin:14px 0;border:1px solid var(--color-rule);border-radius:8px}
 @media(max-width:680px){.api-page-heading{flex-wrap:wrap;gap:12px}.api-docs-link{padding-top:0}.api-address,.api-settings:lang(en) .api-address{grid-template-columns:minmax(0,1fr) 32px;gap:8px 12px}.api-address>span{grid-column:1/-1}.api-key-table th,.api-key-table td{padding:10px 3px}.api-key-table code{font-size:10px}}
 @media(prefers-reduced-motion:reduce){summary :global(svg){transition:none}}
</style>
