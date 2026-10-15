<script lang="ts">
 import {t} from '../lib/i18n';
 import {Button,Collapsible} from 'bits-ui';
 import {icon} from '../lib/icons';
 import Input from './ui/Input.svelte';import Textarea from './ui/Textarea.svelte';import Select from './ui/Select.svelte';import Switch from './ui/Switch.svelte';
 import {parseMcpImport} from '../lib/mcp-config';import type {McpServer} from '../lib/types';
 import SagTools from './SagTools.svelte';
 export let servers:McpServer[];export let drafts:Record<string,string>;export let onRemove:(id:string)=>void;
 export let onTest:(server:McpServer)=>Promise<void>;export let testing='';export let errors:Record<string,string>={};
 let active='',importOpen=false,importText='',importError='',importStatus='';
 $:for(const server of servers)if(!(server.id in drafts))drafts[server.id]='';
 function add(example=false){const server:McpServer={id:crypto.randomUUID(),name:example?'DeepWiki':'',transport:'http',url:example?'https://mcp.deepwiki.com/mcp':'',command:'',args:[],require_confirmation:!example,has_credentials:false,tools:null};servers=[...servers,server];active=server.id;}
 function importConfig(){
  importError='';importStatus='';
  try {
   const imported=parseMcpImport(importText);
   const names=new Set(servers.map(server=>server.name.trim().toLowerCase()));
   for(const {server} of imported){const name=server.name.toLowerCase();if(names.has(name))throw Error($t("已存在同名服务“{value0}”，请修改名称后导入。",{value0:server.name}));names.add(name);}
   for(const {server,credentials} of imported){const values=server.transport==='http'?credentials.headers:credentials.env;if(Object.keys(values).length)drafts[server.id]=JSON.stringify(values,null,2);}
   servers=[...servers,...imported.map(item=>item.server)];active=imported[0].server.id;
   importText='';importOpen=false;importStatus=$t("已导入 {value0} 个服务，请测试连接。",{value0:imported.length});
  }catch(error){importError=$t(String(error))}
 }
 function connectionChanged(id:string,fields:Partial<McpServer>={}){servers=servers.map(server=>server.id===id?{...server,...fields,tools:null}:server);}
</script>
<div class="settings-info-box">{$t("粘贴 JSON 配置或手动添加服务，测试连接后可在“模型设置”中逐项选择工具。")}</div>
<SagTools/>
<div class="mcp-toolbar"><Button.Root type="button" class="outline" onclick={()=>{importOpen=!importOpen;importError='';importStatus=''}}>{$t("导入 JSON")}</Button.Root><Button.Root type="button" class="outline" onclick={()=>add()}>{$t("添加服务")}</Button.Root><Button.Root type="button" class="outline" onclick={()=>add(true)}>{$t("添加 DeepWiki 示例")}</Button.Root></div>
{#if importOpen}
 <section class="settings-group settings-fields mcp-import">
  <label class="field-label">{$t("JSON 配置")}<Textarea aria-label={$t("MCP JSON 配置")} bind:value={importText} rows={8} spellcheck="false" placeholder={'{\n  "mcpServers": {\n    "DeepWiki": {"url": "https://mcp.deepwiki.com/mcp"}\n  }\n}'}/></label>
  <p class="settings-help">{$t("支持 mcpServers、servers、单个服务配置。可包含 HTTP 请求头或本地环境变量；导入后不会自动连接。")}</p>
  {#if importError}<p class="form-error" role="alert">{$t(importError)}</p>{/if}
  <div class="settings-card-actions"><Button.Root type="button" class="primary" disabled={!importText.trim()||!!testing} onclick={importConfig}>{$t("导入配置")}</Button.Root><Button.Root type="button" class="settings-text-button" onclick={()=>{importText='';importOpen=false;importError=''}}>{$t("取消导入")}</Button.Root></div>
 </section>
{/if}
{#if importStatus}<p class="settings-help" role="status">{$t(importStatus)}</p>{/if}
{#each servers as server(server.id)}
 <details class="settings-group settings-fields mcp-server" open={active===server.id} ontoggle={event=>{if(event.currentTarget.open)active=server.id;else if(active===server.id)active='';}}>
  <summary><strong>{server.name||$t("新 MCP 服务")}</strong><span>{server.transport==='http'?'HTTP':$t("本地命令")}</span></summary>
  <label class="field-label">{$t("服务名称")}<Input required bind:value={server.name} aria-label={$t("MCP 服务名称")}/></label>
  <label class="field-label">{$t("连接方式")}<Select label={$t("MCP 连接方式")} value={server.transport} disabled={testing===server.id} onValueChange={value=>connectionChanged(server.id,{transport:value as 'http'|'stdio'})} items={[{value:'http',label:$t("远程 HTTP")},{value:'stdio',label:$t("本地命令（stdio）")}]}/></label>
  {#if server.transport==='http'}<label class="field-label">{$t("服务地址")}<Input type="url" required value={server.url} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{url:event.currentTarget.value})} placeholder="https://example.com/mcp" aria-label={$t("MCP 服务地址")} spellcheck="false"/></label>
  {:else}<label class="field-label">{$t("启动命令")}<Input required value={server.command} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{command:event.currentTarget.value})} placeholder={$t("/完整路径/可执行文件")} aria-label={$t("MCP 启动命令")} spellcheck="false"/></label><label class="field-label">{$t("启动参数 · 每行一个")}<Textarea value={server.args.join('\n')} disabled={testing===server.id} oninput={event=>connectionChanged(server.id,{args:event.currentTarget.value.split('\n').filter(Boolean)})} rows={3} aria-label={$t("MCP 启动参数")} spellcheck="false"/></label>{/if}
  <label class="field-label">{$t("{value0} · JSON，可留空",{value0:server.transport==='http'?$t("请求头"):$t("环境变量")})}<Textarea bind:value={drafts[server.id]} disabled={testing===server.id} oninput={()=>connectionChanged(server.id)} rows={3} aria-label={$t("MCP 凭据")} placeholder={server.has_credentials?$t("已保存在本机，留空保持原值"):server.transport==='http'?'{"Authorization":"Bearer …"}':'{"API_KEY":"…"}'} spellcheck="false"/></label>
  <p class="settings-help">{$t("留空保留已保存的凭据，输入 {} 清除凭据。凭据仅保存在本机。")}</p>
  <div class="setting-row"><div><strong>{$t("调用前确认")}</strong><p>{$t("显示工具名称和参数，由你决定是否执行")}</p></div><Switch bind:checked={server.require_confirmation} label={$t("MCP 调用前确认")}/></div>
  <div class="settings-card-actions"><Button.Root type="button" class="outline" disabled={!!testing} onclick={()=>onTest(server)}>{testing===server.id?$t("正在连接…"):$t("测试连接")}</Button.Root><Button.Root type="button" class="settings-text-button" disabled={testing===server.id} onclick={()=>{onRemove(server.id);servers=servers.filter(s=>s.id!==server.id)}}>{$t("移除服务")}</Button.Root></div>
  {#if errors[server.id]}<p class="form-error" role="alert">{$t(errors[server.id])}</p>{/if}
  {#if server.tools}<Collapsible.Root class="mcp-tool-catalog">
   <Collapsible.Trigger class="model-tool-toggle" aria-label={$t("{value0} 的工具列表",{value0:server.name})}><span class="model-tool-chevron" aria-hidden="true">{@html icon('chevron')}</span><span>{$t("工具列表")}</span><small>{$t("已获取 {value0} 个工具",{value0:server.tools.length})}</small></Collapsible.Trigger>
   <Collapsible.Content class="model-tool-content"><ul class="mcp-tool-list">{#each server.tools as tool(tool.name)}<li><strong>{tool.name}</strong><p>{tool.description}</p></li>{/each}</ul></Collapsible.Content>
  </Collapsible.Root>{/if}
 </details>
{:else}<p class="settings-help">{$t("尚未添加 MCP 服务。")}</p>{/each}
