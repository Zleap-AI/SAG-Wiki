<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {parseMcpCredentials,parseMcpImport} from '../lib/mcp-config';
 import type {McpServer} from '../lib/types';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import Textarea from './ui/Textarea.svelte';
 import Select from './ui/Select.svelte';
 import Switch from './ui/Switch.svelte';
 export let initial:McpServer;
 export let onClose:()=>void;
 export let onAdd:(server:McpServer,credentials:string)=>void;
 let server=structuredClone(initial),args=initial.args.join('\n'),credentials='',error='';
 function add(){
  error='';
  try{
   if(!server.name.trim())throw Error($t('请填写服务名称。'));
   const config=server.transport==='http'?{url:server.url.trim()}:{command:server.command.trim(),args:args.split('\n').filter(Boolean)};
   const parsed=parseMcpImport(JSON.stringify({name:server.name.trim(),...config}))[0].server;
   const next={...parsed,id:server.id,require_confirmation:server.require_confirmation};
   parseMcpCredentials(next,credentials);
   onAdd(next,credentials);
  }catch(e){error=$t(String(e))}
 }
</script>
<Modal label={$t('添加 MCP 服务')} class="mcp-dialog" overlayClass="mcp-dialog-backdrop" {onClose}>
 <div class="dialog-head"><h2>{$t('添加 MCP 服务')}</h2></div>
 <form onsubmit={event=>{event.preventDefault();event.stopPropagation();add()}}>
  <div class="dialog-body settings-fields">
   <label class="field-label">{$t('服务名称')}<Input data-autofocus required bind:value={server.name} aria-label={$t('MCP 服务名称')}/></label>
   <label class="field-label">{$t('连接方式')}<Select label={$t('MCP 连接方式')} value={server.transport} onValueChange={value=>{server.transport=value as 'http'|'stdio';credentials=''}} items={[{value:'http',label:$t('远程 HTTP')},{value:'stdio',label:$t('本地命令（stdio）')}]}/></label>
   {#if server.transport==='http'}
    <label class="field-label">{$t('服务地址')}<Input type="url" required bind:value={server.url} aria-label={$t('MCP 服务地址')} placeholder="https://example.com/mcp" spellcheck="false"/></label>
   {:else}
    <label class="field-label">{$t('启动命令')}<Input required bind:value={server.command} aria-label={$t('MCP 启动命令')} spellcheck="false"/></label>
    <label class="field-label">{$t('启动参数 · 每行一个')}<Textarea bind:value={args} rows={3} aria-label={$t('MCP 启动参数')} spellcheck="false"/></label>
   {/if}
   <details class="mcp-advanced"><summary>{$t('高级设置')}</summary>
    <label class="field-label">{$t('{value0} · JSON，可留空',{value0:server.transport==='http'?$t('请求头'):$t('环境变量')})}<Textarea bind:value={credentials} rows={3} aria-label={$t('MCP 凭据')} spellcheck="false"/></label>
    <div class="setting-row"><span>{$t('调用前确认')}</span><Switch bind:checked={server.require_confirmation} label={$t('MCP 调用前确认')}/></div>
   </details>
   {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  </div>
  <div class="dialog-footer"><Button.Root type="button" class="outline" onclick={onClose}>{$t('取消')}</Button.Root><Button.Root type="submit" class="primary">{$t('添加服务')}</Button.Root></div>
 </form>
</Modal>
