<script lang="ts">
 import {t} from '../lib/i18n';
 import {onDestroy,tick} from 'svelte';
 import {fade} from 'svelte/transition';
 import {Button,Collapsible} from 'bits-ui';
 import {icon} from '../lib/icons';
 import Input from './ui/Input.svelte';import Textarea from './ui/Textarea.svelte';import Select from './ui/Select.svelte';import Switch from './ui/Switch.svelte';
 import {parseMcpImport} from '../lib/mcp-config';import type {McpServer,ModelProfile} from '../lib/types';
 import SagTools from './SagTools.svelte';
 import Modal from './ui/Modal.svelte';import AddMcpServer from './AddMcpServer.svelte';
 export let model:string;export let profile:ModelProfile;
 export let servers:McpServer[];export let drafts:Record<string,string>;export let onRemove:(id:string)=>void|(()=>void);
 export let onTest:(server:McpServer)=>Promise<void>;export let testing='';export let errors:Record<string,string>={};
 function bindServer(server:McpServer,enabled:boolean){
  profile.mcp_servers=enabled?[...profile.mcp_servers,server.id]:profile.mcp_servers.filter(id=>id!==server.id);
  if(enabled&&server.tools){profile.mcp_tools??={};profile.mcp_tools[server.id]??=server.tools.map(tool=>tool.name);}
  profile={...profile};
 }
 function selectTool(server:McpServer,name:string,enabled:boolean){
  const selected=profile.mcp_tools?.[server.id]??server.tools!.map(tool=>tool.name);
  profile.mcp_tools={...profile.mcp_tools,[server.id]:enabled?[...selected,name]:selected.filter(tool=>tool!==name)};
  profile={...profile};
 }
 let active='',importOpen=false,importText='',importError='',status='',statusError=false;
 let addDraft:McpServer|null=null,removing:McpServer|null=null;
 let removed:{server:McpServer;index:number;draft:string;restoreBindings:void|(()=>void)}|null=null;
 let removalNoticeVisible=false,removalTimer:ReturnType<typeof setTimeout>|undefined;
 let addedIds=new Set<string>();
 let statusTimer:ReturnType<typeof setTimeout>|undefined;
 const addedTimers=new Map<string,ReturnType<typeof setTimeout>>();
 const feedbackFade={duration:window.matchMedia('(prefers-reduced-motion: reduce)').matches?0:400};
 function showStatus(message:string,error=false,temporary=false){
  clearTimeout(statusTimer);statusError=error;status=message;
  if(temporary)statusTimer=setTimeout(()=>status='',4000);
 }
 function markAdded(added:McpServer[]){
  addedIds=new Set([...addedIds,...added.map(server=>server.id)]);
  for(const {id} of added){
   clearTimeout(addedTimers.get(id));
   addedTimers.set(id,setTimeout(()=>{addedIds=new Set([...addedIds].filter(value=>value!==id));addedTimers.delete(id)},4000));
  }
 }
 onDestroy(()=>{clearTimeout(statusTimer);clearTimeout(removalTimer);for(const timer of addedTimers.values())clearTimeout(timer)});
 const cards=new Map<string,HTMLElement>();
 function registerCard(node:HTMLElement,id:string){cards.set(id,node);return{destroy(){cards.delete(id)}}}
 async function reveal(id:string){await tick();cards.get(id)?.scrollIntoView?.({block:'nearest',behavior:window.matchMedia('(prefers-reduced-motion: reduce)').matches?'instant':'smooth'});cards.get(id)?.querySelector<HTMLButtonElement>('.model-tool-refresh')?.focus({preventScroll:true});}
 function checkName(name:string){if(servers.some(server=>server.name.trim().toLowerCase()===name.trim().toLowerCase()))throw Error($t('已存在同名服务“{value0}”，请修改名称后导入。',{value0:name}));}
 $:for(const server of servers)if(!(server.id in drafts))drafts[server.id]='';
 function add(example=false){addDraft={id:crypto.randomUUID(),name:example?'DeepWiki':'',transport:'http',url:example?'https://mcp.deepwiki.com/mcp':'',command:'',args:[],require_confirmation:!example,has_credentials:false,tools:null};}
 function confirmAdd(server:McpServer,credentials:string){checkName(server.name);drafts={...drafts,[server.id]:credentials};servers=[...servers,server];addDraft=null;active='';markAdded([server]);showStatus($t('{value0} 添加成功，请测试连接。',{value0:server.name}),false,true);void reveal(server.id);}
 function confirmRemove(){
  if(!removing||testing===removing.id)return;
  const server=removing;
  removed={server,index:servers.findIndex(item=>item.id===server.id),draft:drafts[server.id]??'',restoreBindings:onRemove(server.id)};
  clearTimeout(removalTimer);removalNoticeVisible=true;removalTimer=setTimeout(()=>removalNoticeVisible=false,4000);
  servers=servers.filter(item=>item.id!==server.id);delete drafts[server.id];drafts={...drafts};
  if(active===server.id)active='';removing=null;showStatus('');
 }
 function undoRemove(){
  if(!removed)return;
  try{checkName(removed.server.name)}catch(e){showStatus($t(String(e)),true);return;}
  clearTimeout(removalTimer);removalNoticeVisible=false;
  const item=removed;servers=[...servers.slice(0,item.index),item.server,...servers.slice(item.index)];drafts={...drafts,[item.server.id]:item.draft};item.restoreBindings?.();removed=null;showStatus($t('已恢复 {value0}。',{value0:item.server.name}),false,true);void reveal(item.server.id);
 }
 function importConfig(){
  importError='';
  try {
   const imported=parseMcpImport(importText);
   const names=new Set(servers.map(server=>server.name.trim().toLowerCase()));
   for(const {server} of imported){const name=server.name.toLowerCase();if(names.has(name))throw Error($t("已存在同名服务“{value0}”，请修改名称后导入。",{value0:server.name}));names.add(name);}
   for(const {server,credentials} of imported){const values=server.transport==='http'?credentials.headers:credentials.env;if(Object.keys(values).length)drafts[server.id]=JSON.stringify(values,null,2);}
   const added=imported.map(item=>item.server);servers=[...servers,...added];active='';markAdded(added);
   importText='';importOpen=false;showStatus($t("已导入 {value0} 个服务，请测试连接。",{value0:imported.length}),false,true);void reveal(added[0].id);
  }catch(error){importError=$t(String(error))}
 }
 function connectionChanged(id:string,fields:Partial<McpServer>={}){servers=servers.map(server=>server.id===id?{...server,...fields,tools:null}:server);}
</script>
<p class="settings-help">{$t("添加或导入 MCP 服务，测试连接后为当前模型启用并选择工具。")}</p>
<div class="mcp-toolbar"><Button.Root type="button" class="outline" onclick={()=>add()}>{$t("添加服务")}</Button.Root><Button.Root type="button" class="outline" onclick={()=>{importOpen=true;importError=''}}>{$t("导入 JSON")}</Button.Root><Button.Root type="button" class="outline" onclick={()=>add(true)}>{$t("添加 DeepWiki 示例")}</Button.Root></div>
{#if status}<div class="mcp-feedback" class:mcp-feedback-error={statusError} role="status" out:fade={feedbackFade}><span>{#if !statusError}{@html icon('check')}{/if}{status}</span></div>{/if}
{#if removed&&removalNoticeVisible}<div class="mcp-feedback" role="status" out:fade={feedbackFade}><span>{@html icon('check')}{$t('已移除 {value0}，保存设置后生效。',{value0:removed.server.name})}</span><Button.Root type="button" class="settings-text-button" onclick={undoRemove}>{$t('撤销移除')}</Button.Root></div>{/if}
{#key model}<SagTools {profile} onChange={value=>profile=value}/>{/key}
{#each servers as server(server.id)}
 {@const bound=profile.mcp_servers.includes(server.id)}
 <div class="model-mcp-server mcp-server" use:registerCard={server.id}>
  <div class="setting-row mcp-card-heading"><div class="mcp-card-name"><strong>{server.name||$t("新 MCP 服务")}</strong>{#if addedIds.has(server.id)}<span class="mcp-added" out:fade={feedbackFade}>{@html icon('check')}{$t('已添加')}</span>{/if}</div><Button.Root type="button" class="settings-text-button mcp-remove" disabled={testing===server.id} aria-label={$t('移除 {value0} 服务',{value0:server.name})} onclick={()=>removing=server}>{@html icon('delete')}{$t('移除服务')}</Button.Root></div>
  <div class="mcp-binding-row"><span class="mcp-state" class:mcp-state-ready={!!server.tools?.length}>{testing===server.id?$t('正在连接…'):errors[server.id]?$t('连接失败'):server.tools?.length?(bound?$t('当前模型已启用'):$t('已连接 · 未启用')):server.tools?$t('未发现可用工具'):$t('待测试')}</span><label class="mcp-binding-label"><span>{$t('为当前模型启用')}</span><Switch label={$t("绑定 {value0}",{value0:server.name})} checked={bound} disabled={!bound&&!server.tools?.length} onCheckedChange={checked=>bindServer(server,checked)}/></label></div>
  {#if server.tools&&bound}
   {#key model}<Collapsible.Root class="model-tool-disclosure">
    <div class="model-tool-toolbar">
     <Collapsible.Trigger class="model-tool-toggle" aria-label={$t("{value0} 的工具列表",{value0:server.name})}>
      <span class="model-tool-chevron" aria-hidden="true">{@html icon('chevron')}</span><span>{$t("工具列表")}</span>
      <small>{$t("共 {value0} 个工具 · 已启用 {value1} 个",{value0:server.tools.length,value1:(profile.mcp_tools?.[server.id]??server.tools.map(tool=>tool.name)).filter(name=>server.tools?.some(tool=>tool.name===name)).length})}</small>
     </Collapsible.Trigger>
     <Button.Root type="button" class="settings-text-button model-tool-refresh" disabled={!!testing} onclick={()=>onTest(server)}>{testing===server.id?$t("正在连接…"):$t("刷新工具列表")}</Button.Root>
    </div>
    <Collapsible.Content class="model-tool-content"><div class="model-mcp-tools">{#each server.tools as tool(tool.name)}<div class="setting-row"><div><strong>{tool.name}</strong><p>{tool.description}</p></div><Switch label={$t("启用 {value0} / {value1}",{value0:server.name,value1:tool.name})} checked={(profile.mcp_tools?.[server.id]??server.tools.map(item=>item.name)).includes(tool.name)} onCheckedChange={checked=>selectTool(server,tool.name,checked)}/></div>{/each}</div></Collapsible.Content>
   </Collapsible.Root>{/key}
  {:else}
   <div class="model-tool-toolbar"><p class="settings-help">{#if server.tools}{$t("共 {value0} 个工具",{value0:server.tools.length})}{:else}{$t("先测试连接以获取工具列表。")}{/if}</p><Button.Root type="button" class="settings-text-button model-tool-refresh" disabled={!!testing} onclick={()=>onTest(server)}>{testing===server.id?$t("正在连接…"):server.tools?$t("刷新工具列表"):$t("测试连接")}</Button.Root></div>
  {/if}
  {#if errors[server.id]}<p class="form-error model-tool-error" role="alert">{$t(errors[server.id])}</p>{/if}
 <details class="mcp-connection settings-fields" open={active===server.id} ontoggle={event=>{if(event.currentTarget.open)active=server.id;else if(active===server.id)active='';}}>
  <summary aria-label={$t("{value0} 的连接设置",{value0:server.name||$t("新 MCP 服务")})}><span class="mcp-connection-title"><span aria-hidden="true">{@html icon('chevron')}</span><strong>{$t("连接设置")}</strong></span><span>{server.transport==='http'?'HTTP':$t("本地命令")}</span></summary>
  <label class="field-label">{$t("服务名称")}<Input required bind:value={server.name} aria-label={$t("MCP 服务名称")}/></label>
  <label class="field-label">{$t("连接方式")}<Select label={$t("MCP 连接方式")} value={server.transport} disabled={testing===server.id} onValueChange={value=>connectionChanged(server.id,{transport:value as 'http'|'stdio'})} items={[{value:'http',label:$t("远程 HTTP")},{value:'stdio',label:$t("本地命令（stdio）")}]}/></label>
  {#if server.transport==='http'}<label class="field-label">{$t("服务地址")}<Input type="url" required value={server.url} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{url:event.currentTarget.value})} placeholder="https://example.com/mcp" aria-label={$t("MCP 服务地址")} spellcheck="false"/></label>
  {:else}<label class="field-label">{$t("启动命令")}<Input required value={server.command} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{command:event.currentTarget.value})} placeholder={$t("/完整路径/可执行文件")} aria-label={$t("MCP 启动命令")} spellcheck="false"/></label><label class="field-label">{$t("启动参数 · 每行一个")}<Textarea value={server.args.join('\n')} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{args:event.currentTarget.value.split('\n').filter(Boolean)})} rows={3} aria-label={$t("MCP 启动参数")} spellcheck="false"/></label>{/if}
  <label class="field-label">{$t("{value0} · JSON，可留空",{value0:server.transport==='http'?$t("请求头"):$t("环境变量")})}<Textarea bind:value={drafts[server.id]} disabled={testing===server.id} oninput={()=>connectionChanged(server.id)} rows={3} aria-label={$t("MCP 凭据")} placeholder={server.has_credentials?$t("已保存在本机，留空保持原值"):server.transport==='http'?'{"Authorization":"Bearer …"}':'{"API_KEY":"…"}'} spellcheck="false"/></label>
  <p class="settings-help">{$t("留空保留已保存的凭据，输入 {} 清除凭据。凭据仅保存在本机。")}</p>
  <div class="setting-row"><div><strong>{$t("调用前确认")}</strong><p>{$t("显示工具名称和参数，由你决定是否执行")}</p></div><Switch bind:checked={server.require_confirmation} label={$t("MCP 调用前确认")}/></div>
 </details>
 </div>
{:else}<p class="settings-help">{$t("尚未添加 MCP 服务。")}</p>{/each}

{#if addDraft}<AddMcpServer initial={addDraft} onClose={()=>addDraft=null} onAdd={confirmAdd}/>{/if}
{#if importOpen}
 <Modal label={$t('导入 MCP 配置')} class="mcp-dialog mcp-import" overlayClass="mcp-dialog-backdrop" onClose={()=>{importOpen=false;importText=''}}>
  <div class="dialog-head"><h2>{$t('导入 MCP 配置')}</h2></div>
   <form onsubmit={event=>{event.preventDefault();event.stopPropagation();importConfig()}}>
   <div class="dialog-body settings-fields"><label class="field-label">{$t('JSON 配置')}<Textarea data-autofocus aria-label={$t('MCP JSON 配置')} bind:value={importText} rows={8} spellcheck="false" placeholder={'{\n  "mcpServers": {\n    "DeepWiki": {"url": "https://mcp.deepwiki.com/mcp"}\n  }\n}'}/></label><p class="settings-help">{$t('支持 mcpServers、servers、单个服务配置。可包含 HTTP 请求头或本地环境变量；导入后不会自动连接。')}</p>{#if importError}<p class="form-error" role="alert">{importError}</p>{/if}</div>
   <div class="dialog-footer"><Button.Root type="button" class="outline" onclick={()=>{importText='';importOpen=false;importError=''}}>{$t('取消导入')}</Button.Root><Button.Root type="submit" class="primary" disabled={!importText.trim()||!!testing}>{$t('导入配置')}</Button.Root></div>
  </form>
 </Modal>
{/if}
{#if removing}
 <Modal label={$t('确认移除服务')} class="mcp-dialog mcp-remove-dialog" overlayClass="mcp-dialog-backdrop" onClose={()=>removing=null}>
  <div class="dialog-head"><h2>{$t('确认移除服务')}</h2></div>
  <div class="dialog-body"><p>{$t('确定移除“{value0}”吗？',{value0:removing.name})}</p><p class="settings-help">{$t('保存后，所有已绑定模型将无法再调用该服务的工具。')}</p></div>
  <div class="dialog-footer"><Button.Root data-autofocus type="button" class="outline" onclick={()=>removing=null}>{$t('取消')}</Button.Root><Button.Root type="button" class="mcp-confirm-remove" disabled={testing===removing.id} onclick={confirmRemove}>{$t('确认移除')}</Button.Root></div>
 </Modal>
{/if}
