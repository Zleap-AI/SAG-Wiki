<script lang="ts">
 import {onMount, tick} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 export let enabled=false;
 let configured=false,loading=true,busy=false,token='',endpoint='',error='',copied=false,manual=false,port=15679;
 let configBox:HTMLTextAreaElement;
 onMount(()=>{void load()});
 async function load(){
  try{
   const status=await api<{enabled:boolean;port:number}>('fnos_remote_status');
   configured=status.enabled;port=status.port;
   endpoint=localStorage.getItem('sag.fnos.remote.endpoint')??'';
   if(!endpoint&&!location.hostname.endsWith('fnos.net'))endpoint=`http://${location.hostname}:${port}/mcp`;
  }catch(e){error=String(e)}finally{loading=false}
 }
 function validEndpoint(value:string){
  try{const url=new URL(value);return ['http:','https:'].includes(url.protocol)&&!url.username&&!url.password&&!url.hash&&!url.search}catch{return false}
 }
 $: valid=validEndpoint(endpoint);
 $: config=`[mcp_servers.sag_fnos]\nurl = ${JSON.stringify(endpoint.trim())}\nhttp_headers = { Authorization = ${JSON.stringify(`Bearer ${token||'<ACCESS_TOKEN>'}`)} }\nstartup_timeout_sec = 30\ntool_timeout_sec = 600`;
 async function create(){
  busy=true;error='';copied=false;manual=false;
  try{const result=await api<{token:string}>('fnos_remote_rotate');token=result.token;configured=true;if(valid)localStorage.setItem('sag.fnos.remote.endpoint',endpoint.trim())}catch(e){error=String(e)}finally{busy=false}
 }
 async function revoke(){
  busy=true;error='';
  try{await api('fnos_remote_revoke');token='';configured=false;copied=false;manual=false}catch(e){error=String(e)}finally{busy=false}
 }
 async function copy(){
  localStorage.setItem('sag.fnos.remote.endpoint',endpoint.trim());
  if(!navigator.clipboard){manual=true;await tick();configBox.focus();configBox.select();return}
  try{await navigator.clipboard.writeText(config);copied=true;error=''}catch(e){error=String(e)}
 }
</script>
<section class="settings-group" aria-busy={loading||busy}>
 <div class="setting-row"><div><strong>{$t('电脑远程接入')}</strong><p>{$t('让电脑上的 Codex 直接搜索飞牛知识库，无需 SSH。仅能读取当前飞牛用户已授权的文件。')}</p></div><span class="state">{configured?(enabled?$t('已启用'):$t('已暂停')):$t('未启用')}</span></div>
 <div class="settings-fields remote-body">
  <label class="field-label"><span>{$t('连接地址')}</span><Input bind:value={endpoint} placeholder={`http://NAS_IP:${port}/mcp`} disabled={busy} spellcheck={false}/></label>
  <p class="help">{$t('填写电脑可访问的 NAS 局域网或 VPN 地址。公网连接请使用 HTTPS 反向代理；飞牛网页登录地址不能直接作为此地址。')}</p>
  {#if endpoint.trim()&&!valid}<p class="form-error">{$t('请输入有效的 HTTP 或 HTTPS 连接地址。')}</p>{/if}
  <div class="actions">
   <Button.Root type="button" class="outline" disabled={loading||busy||!enabled} onclick={create}>{configured?$t('重新生成密钥'):$t('生成访问密钥')}</Button.Root>
   {#if configured}<Button.Root type="button" class="outline" disabled={busy} onclick={revoke}>{$t('关闭远程接入')}</Button.Root>{/if}
  </div>
  {#if !enabled}<p class="help">{$t('请先启用上方的 CLI 接入。')}</p>{/if}
  {#if token}
   <p class="help">{$t('密钥仅在本次可复制，已包含在复制的配置中。重新生成后旧密钥立即失效，请勿分享配置。')}</p>
  {:else if configured}
   <p class="help">{$t('已有访问密钥。要配置另一台电脑，请重新生成密钥，并更新原电脑的配置。')}</p>
  {/if}
  <details class="remote-guide" open={!!token}>
   <summary>{$t('配置 Codex')}</summary>
   <p class="help">{$t('在电脑的 ~/.codex/config.toml 中加入以下配置，然后重新开启 Codex 会话。')}</p>
   <pre><code>{config.replace(token||'<ACCESS_TOKEN>','<ACCESS_TOKEN>')}</code></pre>
   <Button.Root type="button" class="outline" disabled={!token||!valid||busy} onclick={copy}>{copied?$t('已复制'):$t('复制 Codex 配置')}</Button.Root>
   {#if manual}<p class="help">{$t('当前连接不支持自动复制。配置已选中，请按 Ctrl+C 或 ⌘C 复制。')}</p><textarea bind:this={configBox} aria-label={$t('Codex 连接配置')} readonly value={config}></textarea>{/if}
   <p class="help">{$t('连接后可直接提问：使用 sag_fnos 搜索知识库，整理资料中的验收要求并注明来源。')}</p>
  </details>
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 </div>
</section>
<style>
 .remote-body{padding:17px;display:grid;gap:14px}
 .field-label{display:grid;gap:7px}
 .state,.help{font-size:var(--text-caption);color:var(--color-secondary)}
 .state{white-space:nowrap}
 .help{line-height:1.6;margin:0}
 .actions{display:flex;flex-wrap:wrap;gap:8px}
 summary{cursor:pointer;font-size:var(--text-ui);font-weight:500}
 .remote-guide[open]{display:grid;gap:12px}
 pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.65 var(--font-mono,monospace);background:var(--color-panel);border:1px solid var(--color-rule);border-radius:7px;padding:12px;margin:0}
 textarea{box-sizing:border-box;width:100%;min-height:140px;resize:vertical;font:12px/1.65 var(--font-mono,monospace);padding:12px;border:1px solid var(--color-rule);border-radius:7px;background:var(--color-panel);color:inherit}
 .form-error{margin:0}
</style>
