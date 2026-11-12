<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t,language} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import Switch from './ui/Switch.svelte';
 import Input from './ui/Input.svelte';
 import Modal from './ui/Modal.svelte';
 export let pending=false;
 type Status={enabled:boolean;running:boolean;host:string;port:number;local_url:string;lan_urls:string[];key_count:number;autostart:boolean;last_error:string|null};
 type Key={id:string;name:string;prefix:string;created_at:number};
 let apiEnabled=false,autostartEnabled=false,copied='';
 let copyTimer:ReturnType<typeof setTimeout>|undefined;
 onDestroy(()=>clearTimeout(copyTimer));
 let status:Status|undefined,keys:Key[]=[],host='',port:number|undefined=24681,error='',notice='',creating=false,keyName='',created:{key:string}|null=null,revoking:Key|null=null,dialogError='',refreshing=false;
 $: changed=!!status&&(host!==status.host||port!==status.port);
 $: example=`curl ${status?.local_url??'http://127.0.0.1:24681'}/v1/search \\\n  -H "Authorization: Bearer <API_KEY>" \\\n  -H "Content-Type: application/json" \\\n  -d '{"query":"${$language==='en'?'Deployment steps':'部署步骤'}","mode":"deep","limit":10}'`;
 const call=<T,>(action:Record<string,unknown>)=>api<T>('manage_api',{action});
 async function refresh(reset=false){
  const next=await call<Status>({action:'status'});
  if(reset||!status||!changed){host=next.host;port=next.port;}
  status=next;apiEnabled=next.enabled;autostartEnabled=next.autostart;keys=await call<Key[]>({action:'list_keys'});
 }
 onMount(()=>{
  let disposed=false;
  async function poll(){if(pending||refreshing)return;refreshing=true;try{await refresh();}catch(e){if(!disposed)error=String(e);}finally{refreshing=false;}}
  void poll();const timer=setInterval(()=>void poll(),3000);
  return()=>{disposed=true;clearInterval(timer);created=null;};
 });
 async function act(action:Record<string,unknown>){
  pending=true;error='';notice='';
  try{await call(action);await refresh(action.action==='configure'||action.action==='start');return true;}catch(e){error=String(e);return false;}finally{pending=false;}
 }
 async function toggle(enabled:boolean){
  if(enabled&&(!host.trim()||!port||!Number.isInteger(port)||port<1||port>65535)){error=$t('请填写监听地址和有效端口（1–65535）');apiEnabled=status!.enabled;return;}
  await act(enabled?{action:'start',host:host.trim(),port}:{action:'stop'});apiEnabled=status!.enabled;
 }
 async function toggleAutostart(enabled:boolean){await act({action:'autostart',enabled});autostartEnabled=status!.autostart;}
 async function saveConnection(){if(await act({action:'configure',host:host.trim(),port}))notice=$t('连接配置已保存');}
 async function copy(value:string,target:string){error='';dialogError='';try{await navigator.clipboard.writeText(value);copied=target;notice=$t('已复制');clearTimeout(copyTimer);copyTimer=setTimeout(()=>{copied='';notice='';},2000);}catch(e){if(creating)dialogError=String(e);else error=String(e);}}
 async function create(){
  pending=true;dialogError='';
  try{created=await call<{key:string}>({action:'create_key',name:keyName.trim()});await refresh();}catch(e){dialogError=String(e);}finally{pending=false;}
 }
 async function revoke(){
  if(!revoking)return;pending=true;dialogError='';
  try{await call({action:'revoke_key',id:revoking.id});revoking=null;await refresh();}catch(e){dialogError=String(e);}finally{pending=false;}
 }
 function closeCreate(){if(!pending){creating=false;created=null;keyName='';dialogError='';copied='';notice='';clearTimeout(copyTimer);}}
</script>

<div class="api-settings" lang={$language} aria-busy={pending}>
 <header class="settings-page-heading api-page-heading">
  <div><h2>{$t('API 服务')}</h2><p>{$t('供本机与局域网程序搜索知识库。')}</p></div>
  <div class="api-docs-link"><a href={`http://sag-wiki.zleap.com/${$language==='en'?'en':'zh'}/docs/api`} target="_blank" rel="noopener noreferrer" on:click|preventDefault={()=>{void api('open_external_link',{url:`http://sag-wiki.zleap.com/${$language==='en'?'en':'zh'}/docs/api`}).catch(e=>error=String(e));}}>{$t('API 文档')}{@html icon('externalLink')}</a></div>
 </header>
 {#if !status}<p role="status">{$t('正在读取 API 设置…')}</p>
 {:else}
 <section class="settings-group">
  <div class="setting-row"><div><strong>{$t('启用 API')}</strong><p class="api-run-state" data-state={status.running?'running':status.enabled?'enabled':'stopped'} role="status">{$t(status.running?'运行中':status.enabled?'已启用，未运行':'已关闭')}</p></div><Switch bind:checked={apiEnabled} disabled={pending} label={$t('启用 API')} onCheckedChange={toggle}/></div>
  {#if status.last_error}<p class="form-error" role="alert">{status.last_error}</p>{/if}
  <div class="api-address"><span>{$t('本机地址')}</span><code>{status.local_url}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制本机地址')} onclick={()=>copy(status!.local_url,'local')}>{@html icon(copied==='local'?'check':'copy')}</Button.Root></div>
  {#each status.lan_urls as url}<div class="api-address"><span>{$t('局域网地址')}</span><code>{url}</code><Button.Root type="button" class="icon-button" aria-label={$t('复制局域网地址')} onclick={()=>copy(url,url)}>{@html icon(copied===url?'check':'copy')}</Button.Root></div>{/each}
  <details><summary>{@html icon('chevron')}{$t('连接配置')}</summary><div class="api-connection">
   <label>{$t('监听地址')}<Input aria-label={$t('监听地址')} bind:value={host} disabled={pending||status.running}/></label>
   <label>{$t('端口')}<Input aria-label={$t('端口')} type="number" min={1} max={65535} bind:value={port} disabled={pending||status.running}/></label>
   {#if !status.running}<Button.Root type="button" class="outline" disabled={pending||!changed||!host.trim()||!port} onclick={saveConnection}>{$t('保存连接配置')}</Button.Root>{/if}
  </div></details>
  <div class="setting-row"><div><strong>{$t('开机自启')}</strong><p>{$t('登录系统后启动后台，API 按保存的启用状态恢复。')}</p></div><Switch bind:checked={autostartEnabled} disabled={pending} label={$t('开机自启')} onCheckedChange={toggleAutostart}/></div>
 </section>
 <section class="settings-group">
  <div class="api-section-heading"><h3>API Key</h3><Button.Root type="button" class="outline" disabled={pending} onclick={()=>{creating=true;dialogError='';}}>{$t('创建 Key')}</Button.Root></div>
  {#if keys.length}
   <table class="api-key-table"><thead><tr><th>{$t('名称')}</th><th>Key</th><th>{$t('创建时间')}</th><th><span class="sr-only">{$t('操作')}</span></th></tr></thead><tbody>
    {#each keys as key}<tr><td>{key.name}</td><td><code>{key.prefix}</code></td><td>{new Date(key.created_at*1000).toLocaleDateString($language==='en'?'en-US':'zh-CN')}</td><td><Button.Root type="button" class="api-revoke" disabled={pending} onclick={()=>{revoking=key;dialogError='';}} aria-label={$t('撤销 {value0}',{value0:key.name})}>{$t('撤销 Key')}</Button.Root></td></tr>{/each}
   </tbody></table>
  {:else}<p class="settings-help api-key-empty">{$t('没有有效 Key，请求将返回未授权（401）。')}</p>{/if}
 </section>
 <details class="api-example"><summary>{@html icon('chevron')}{$t('调用示例')}</summary><pre><code>{example}</code></pre><Button.Root type="button" class="outline" onclick={()=>copy(example,'example')}>{copied==='example'?$t('已复制'):$t('复制示例')}</Button.Root></details>
 {/if}
 {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
 {#if notice}<p class="settings-status" role="status">{notice}</p>{/if}
</div>

{#if creating}<Modal label={$t('创建 Key')} onClose={closeCreate}>
 <div class="dialog-head"><h2>{$t('创建 Key')}</h2></div>
 <div class="dialog-body">
 {#if created}<p>{$t('请保存此 Key，关闭后无法再次查看。')}</p><code class="api-secret">{created.key}</code><Button.Root type="button" class="outline" onclick={()=>copy(created!.key,'key')}>{copied==='key'?$t('已复制'):$t('复制 Key')}</Button.Root>
 {:else}<label class="field-label">{$t('名称')}<Input data-autofocus aria-label={$t('Key 名称')} bind:value={keyName} maxlength={100} disabled={pending} onkeydown={e=>{if(e.key==='Enter'){e.preventDefault();if(keyName.trim())void create();}}}/></label>{/if}
 {#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}
 </div>
 <div class="dialog-footer">{#if created}<Button.Root type="button" class="primary" disabled={pending} onclick={closeCreate}>{$t('我已保存')}</Button.Root>{:else}<Button.Root type="button" disabled={pending} onclick={closeCreate}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending||!keyName.trim()} onclick={create}>{$t('创建 Key')}</Button.Root>{/if}</div>
</Modal>{/if}
{#if revoking}<Modal label={$t('撤销 Key')} onClose={()=>{if(!pending)revoking=null;}}>
 <div class="dialog-head"><h2>{$t('撤销 Key')}</h2></div><div class="dialog-body"><p>{$t('撤销后将取消此 Key 的搜索，其他 Key 和 API 服务继续运行。')}</p><strong>{revoking.name}</strong>{#if dialogError}<p class="form-error" role="alert">{$t(dialogError)}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" disabled={pending} onclick={()=>revoking=null}>{$t('取消')}</Button.Root><Button.Root type="button" class="primary" disabled={pending} onclick={revoke}>{$t('确认撤销')}</Button.Root></div>
</Modal>{/if}

<style>
 .api-page-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.api-page-heading>div:first-child{min-width:0}.api-page-heading h2{font-weight:600}.api-page-heading p{margin:0}
 .api-docs-link{flex-shrink:0;padding-top:4px}.api-docs-link a{display:inline-flex;align-items:center;gap:8px;font-size:13px;color:var(--color-secondary);text-underline-offset:3px}.api-docs-link :global(svg){width:16px;height:16px}
 .api-docs-link a:focus-visible,summary:focus-visible{outline:2px solid var(--color-focus);outline-offset:4px;border-radius:4px}
 .setting-row{padding-left:16px;padding-right:16px}.api-run-state{display:flex;align-items:center;gap:6px}.api-run-state::before{content:'';width:6px;height:6px;flex-shrink:0;border-radius:50%;background:var(--color-muted)}.api-run-state[data-state=running]::before{background:var(--color-success)}
 .api-address{display:grid;grid-template-columns:76px minmax(0,1fr) 32px;align-items:center;gap:12px;padding:10px 16px;font-size:12px}.api-address>span{color:var(--color-secondary)}.api-address code{overflow-wrap:anywhere;font-family:var(--font-mono);font-size:12px;font-variant-numeric:tabular-nums;user-select:text;-webkit-user-select:text}.api-address :global(button){justify-self:end}
 .api-settings:lang(en) .api-address{grid-template-columns:96px minmax(0,1fr) 32px}
 details{border-top:1px solid var(--color-rule);padding:14px 16px}summary{display:flex;align-items:center;gap:8px;cursor:pointer;list-style:none;font-size:13px;font-weight:500}summary::-webkit-details-marker{display:none}summary :global(svg){width:16px;height:16px;color:var(--color-secondary);flex-shrink:0;transition:transform var(--dur-fast) var(--ease-out)}details[open]>summary :global(svg){transform:rotate(90deg)}.api-connection{display:grid;grid-template-columns:minmax(0,1fr) 100px;gap:14px;padding-top:16px}.api-connection label{display:grid;gap:8px;font-size:12px}.api-connection :global(button){justify-self:start;grid-column:1/-1}
 .api-section-heading{display:flex;justify-content:space-between;align-items:center;margin-bottom:0;padding:14px 16px}.api-section-heading h3{margin:0;font-size:14px}.api-key-table{width:100%;border-collapse:collapse;font-size:12px;text-align:left}.api-key-table th{font-weight:400;color:var(--color-secondary)}.api-key-table th,.api-key-table td{padding:12px 6px;border-bottom:1px solid var(--color-rule)}.api-key-table th:first-child,.api-key-table td:first-child{padding-left:16px}.api-key-table th:last-child,.api-key-table td:last-child{padding-right:16px}.api-key-table td:first-child{overflow-wrap:anywhere;max-width:140px}.api-key-table td:last-child{text-align:right}:global(.api-revoke){color:var(--color-secondary);background:none;border:0;cursor:pointer}
 .api-key-empty{padding:0 16px 16px}
 .api-key-table thead{background:var(--color-panel)}.api-key-table tbody tr:last-child td{border-bottom:0}.api-key-table code{font-family:var(--font-mono);font-variant-numeric:tabular-nums;white-space:nowrap}
 .api-example pre{overflow:auto;padding:14px;background:var(--color-panel);border:1px solid var(--color-rule);border-radius:8px;font-size:12px}.api-secret{display:block;overflow-wrap:anywhere;user-select:all;padding:16px;margin:14px 0;border:1px solid var(--color-rule);border-radius:8px}
 @media(max-width:640px){.api-page-heading{flex-wrap:wrap;gap:12px}.api-docs-link{padding-top:0}.api-address,.api-settings:lang(en) .api-address{grid-template-columns:minmax(0,1fr) 32px;gap:8px 12px}.api-address>span{grid-column:1/-1}.api-key-table th,.api-key-table td{padding:10px 3px}.api-key-table code{font-size:10px}}
 @media(prefers-reduced-motion:reduce){summary :global(svg){transition:none}}
</style>
