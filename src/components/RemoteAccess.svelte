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
<div class="remote-sections">
<section class="settings-group" aria-busy={loading||busy}>
 <div class="setting-row"><div><strong>{$t('1. 连接知识库')}</strong><p>{$t('让电脑上的 Codex 直接搜索飞牛知识库，无需 SSH。仅能读取当前飞牛用户已授权的文件。')}</p></div><span class="state">{configured?(enabled?$t('已启用'):$t('已暂停')):$t('未启用')}</span></div>
 <div class="settings-fields remote-body">
  <ol class="steps"><li>{$t('填写电脑可访问的 NAS 地址，生成访问密钥。')}</li><li>{$t('复制下方 Codex 配置，加入电脑的 ~/.codex/config.toml，保留已有设置。')}</li><li>{$t('继续安装下方 Skill，再新开 Codex 会话。')}</li></ol>
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
  <details class="remote-guide" open>
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
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('2. 安装远程 Skill')}</strong><p>{$t('让 Agent 使用 sag_fnos 搜索、读取 NAS 文件并引用来源。Skill 中不包含访问密钥。')}</p></div></div>
 <div class="settings-fields remote-body">
  <a class="outline download" href={`${import.meta.env.BASE_URL}skills/sag-fnos-skill.zip`} download>{$t('下载远程 Skill')}</a>
  <p class="help">{$t('解压得到 sag-fnos 文件夹，将整个文件夹放入电脑上所用 Agent 的 Skills 目录。')}</p>
  <dl class="skill-paths"><div><dt>Codex</dt><dd><code>~/.agents/skills/sag-fnos</code></dd></div><div><dt>Claude Code</dt><dd><code>~/.claude/skills/sag-fnos</code></dd></div><div><dt>OpenCode</dt><dd><code>~/.config/opencode/skills/sag-fnos</code></dd></div></dl>
  <p class="help">{$t('上方配置用于 Codex。其他 Agent 请先在各自的 MCP 设置中添加相同地址和访问密钥，再安装 Skill。')}</p>
 </div>
</section>
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('3. 开始使用')}</strong><p>{$t('新开 Agent 会话加载 Skill，然后用完整问题提问。')}</p></div></div>
 <div class="settings-fields remote-body"><blockquote>{$t('使用 sag_fnos 搜索飞牛知识库，整理资料中的验收要求并注明来源。')}</blockquote><p class="help">{$t('保持 NAS 上的 SAG Wiki 和电脑的局域网或 VPN 连接。网页可以关闭。')}</p></div>
</section>
</div>
<style>
 .remote-sections{display:grid;gap:18px}
 .remote-body{padding:17px;display:grid;gap:14px}
 .field-label{display:grid;gap:7px}
 .state,.help{font-size:var(--text-caption);color:var(--color-secondary)}
 .state{white-space:nowrap}
 .help{line-height:1.6;margin:0}
 .steps{margin:0;padding-left:20px;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.8}
 .download{justify-self:start;text-decoration:none;padding:8px 12px;font-size:var(--text-ui)}
 .skill-paths{margin:0;display:grid;gap:8px;font-size:var(--text-caption)}
 .skill-paths div{display:grid;grid-template-columns:100px 1fr;gap:10px}.skill-paths dd{margin:0;overflow-wrap:anywhere}.skill-paths code{font-family:var(--font-mono)}
 blockquote{margin:0;border-left:3px solid var(--color-rule);padding:8px 12px;font-size:var(--text-ui);line-height:1.7}
 .actions{display:flex;flex-wrap:wrap;gap:8px}
 summary{cursor:pointer;font-size:var(--text-ui);font-weight:500}
 .remote-guide[open]{display:grid;gap:12px}
 pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.65 var(--font-mono,monospace);background:var(--color-panel);border:1px solid var(--color-rule);border-radius:7px;padding:12px;margin:0}
 textarea{box-sizing:border-box;width:100%;min-height:140px;resize:vertical;font:12px/1.65 var(--font-mono,monospace);padding:12px;border:1px solid var(--color-rule);border-radius:7px;background:var(--color-panel);color:inherit}
 .form-error{margin:0}
</style>
