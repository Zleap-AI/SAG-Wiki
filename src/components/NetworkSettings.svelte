<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import type {ModelProxy,ProxyMode} from '../lib/types';
 import Input from './ui/Input.svelte';
 import Select from './ui/Select.svelte';
 import Switch from './ui/Switch.svelte';
 import Textarea from './ui/Textarea.svelte';
 /** The settings draft; `password` holds a value typed this session, null keeps the stored one. */
 export let value:ModelProxy;
 export let testUrl='';
 export let saving=false;
 let authEnabled=!!value.username||value.has_password;
 let showPassword=false,testing=false,result='',error='',revision=0;
 $: modes=[
  {value:'system',label:$t('跟随系统代理')},
  {value:'custom',label:$t('自定义代理')},
  {value:'none',label:$t('不使用代理')},
 ];
 $: summary=value.mode==='custom'?$t('模型请求、MinerU 解析、MCP 服务、组件下载和软件更新都经过下面的代理；桌面端、CLI 和后台服务共用。')
  :value.mode==='none'?$t('所有请求直接连接外网，适合不需要代理的网络。')
  :$t('读取 Windows「代理」设置或 macOS「网络 → 代理」中的 HTTP/HTTPS 代理，以及 HTTP_PROXY、HTTPS_PROXY、NO_PROXY 环境变量。暂不支持 PAC 脚本。');
 $: urlHint=(()=>{const text=value.url.trim();if(!text)return '';if(/^socks5h?:\/\//i.test(text))return $t('SOCKS5 代理：由代理服务器连接目标地址。');if(/^https:\/\//i.test(text))return $t('HTTPS 代理：与代理服务器之间的连接加密。');return $t('HTTP 代理：HTTPS 请求通过 CONNECT 隧道转发。');})();
 /** Any edit outdates a test that is still running, so its result never describes stale input. */
 function invalidate(){revision++;result='';error='';testing=false;}
 function changeMode(next:string){value={...value,mode:next as ProxyMode};invalidate();}
 function changeUrl(raw:string){
  invalidate();
  // Pasting user:pass@host moves the credentials into the authentication fields.
  const match=raw.trim().match(/^([a-z0-9+.-]+:\/\/)?([^/@\s]+)@([^/@\s]+)$/i);
  if(match){
   const [user,pass='']=match[2].split(':');
   let username=user,secret=pass;try{username=decodeURIComponent(user);secret=decodeURIComponent(pass);}catch{/* keep raw */}
   value={...value,url:`${match[1]??''}${match[3]}`,username,password:secret};
   authEnabled=true;return;
  }
  value={...value,url:raw};
 }
 function toggleAuth(enabled:boolean){authEnabled=enabled;invalidate();if(!enabled)value={...value,username:'',password:''};else if(value.password==='')value={...value,password:null};}
 function validate():string{
  if(value.mode==='custom'&&!value.url.trim())return $t('请填写代理地址，例如 http://127.0.0.1:7890');
  if(value.mode==='custom'&&authEnabled&&!value.username.trim())return $t('请填写代理用户名');
  return '';
 }
 async function test(){
  error=validate();result='';if(error)return;
  const current=++revision;testing=true;
  try{
   const message=await api<string>('test_proxy_connection',{proxy:{...value,url:value.url.trim(),username:value.username.trim(),bypass:value.bypass.trim()}});
   if(current===revision)result=$t(message);
  }catch(reason){if(current===revision)error=$t(String(reason));}
  finally{if(current===revision)testing=false;}
 }
</script>

<div class="network-settings">
 <section class="settings-group">
  <div class="setting-row network-mode-row">
   <div><strong>{$t('代理模式')}</strong><p>{summary}</p></div>
   <Select label={$t('代理模式')} class="network-mode-select" value={value.mode} items={modes} onValueChange={changeMode}/>
  </div>
 </section>
 {#if value.mode!=='none'}
  <section class="settings-group settings-fields network-proxy-fields">
   {#if value.mode==='custom'}
    <label class="field-label">{$t('代理地址')}
     <Input id="network-proxy-url" type="text" inputmode="url" required value={value.url} oninput={event=>changeUrl(event.currentTarget.value)} placeholder="http://127.0.0.1:7890" autocomplete="off" spellcheck="false" aria-describedby="network-proxy-url-help"/>
    </label>
    <p class="settings-help" id="network-proxy-url-help">{urlHint||$t('支持 http://、https://、socks5:// 和 socks5h:// 地址；只写主机和端口时按 HTTP 代理处理。')}</p>
    <div class="setting-row network-auth-row">
     <div><strong>{$t('代理需要认证')}</strong><p>{$t('以 Basic 方式把用户名和密码发给代理服务器；密码只保存在本机。')}</p></div>
     <Switch checked={authEnabled} label={$t('代理需要认证')} onCheckedChange={toggleAuth}/>
    </div>
    {#if authEnabled}
     <div class="network-auth-fields">
      <label class="field-label">{$t('用户名')}<Input id="network-proxy-username" type="text" required value={value.username} oninput={event=>{value={...value,username:event.currentTarget.value};invalidate();}} autocomplete="off" spellcheck="false"/></label>
      <label class="field-label">{$t('密码')}
       <div class="key-field">
        <Input id="network-proxy-password" type={showPassword?'text':'password'} value={value.password??''} oninput={event=>{value={...value,password:event.currentTarget.value};invalidate();}} placeholder={value.has_password&&value.password===null?$t('已保存在本机；留空保持原密码'):$t('填写代理密码')} autocomplete="off" spellcheck="false"/>
        <Button.Root type="button" onclick={()=>showPassword=!showPassword}>{showPassword?$t('隐藏'):$t('显示')}</Button.Root>
       </div>
      </label>
     </div>
    {/if}
   {/if}
   <label class="field-label">{$t('直连地址')}
    <Textarea id="network-proxy-bypass" rows={2} value={value.bypass} oninput={event=>{value={...value,bypass:event.currentTarget.value};invalidate();}} placeholder=".corp.example.com, 10.0.0.0/8, *.internal" spellcheck="false" aria-describedby="network-proxy-bypass-help"/>
   </label>
   <p class="settings-help" id="network-proxy-bypass-help">{$t('用逗号分隔。localhost 和回环地址始终直接连接；可填写域名、以点开头的域名后缀、IP、网段或带 * 的匹配规则，例如内网的模型服务。')}</p>
  </section>
 {/if}
 <section class="settings-group network-test">
  <div class="setting-row">
   <div><strong>{$t('测试连接')}</strong><p>{testUrl?$t('按当前填写的设置请求 {value0}，不会保存。',{value0:testUrl}):$t('按当前填写的设置请求模型服务地址，不会保存。')}</p></div>
   <Button.Root type="button" class="outline network-test-button" disabled={testing||saving} onclick={test}>{testing?$t('正在测试…'):$t('测试连接')}</Button.Root>
  </div>
  <div role="status" aria-live="polite">{#if result||error}<div class="network-test-result" class:failed={!!error}>{@html icon(error?'alert':'check')}<span>{error||result}</span></div>{/if}</div>
 </section>
 <p class="settings-help network-settings-note">{$t('保存后立即生效，无需重启。正在进行的请求保持原连接，下一次请求使用新设置。')}</p>
</div>

<style>
 .network-settings{display:flex;flex-direction:column;min-width:0}
 .network-settings .setting-row{padding:16px}
 .network-settings .setting-row:last-child{border-bottom:0}
 .network-settings .setting-row p{margin:4px 0 0;font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .network-mode-row{align-items:flex-start}
 .network-mode-row>div:first-child{flex:1;min-width:0}
 .network-settings :global(.network-mode-select){flex-shrink:0;min-width:168px}
 .network-proxy-fields .field-label:first-child{margin-top:0}
 .network-proxy-fields .settings-help{margin:-6px 0 0}
 .network-settings .network-auth-row{margin-top:8px;padding-left:0;padding-right:0}
 .network-auth-fields{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.3fr);gap:0 14px}
 .network-auth-fields .field-label{margin-top:0}
 .network-auth-fields :global(.key-field){margin-top:8px}
 .network-test .setting-row{border-bottom:0}
 .network-test-result{display:flex;align-items:flex-start;gap:8px;margin:0 16px 16px;padding:10px 12px;border-radius:8px;font-size:var(--text-caption);line-height:1.6;color:var(--color-ink);background:var(--color-panel);overflow-wrap:anywhere}
 .network-test-result :global(.icon){flex-shrink:0;width:14px;height:14px;margin-top:3px;color:var(--color-success,#1a7f37)}
 .network-test-result.failed{color:var(--color-danger,#c0392b)}
 .network-test-result.failed :global(.icon){color:currentColor}
 .network-settings-note{margin-top:-6px}
 @media (max-width:640px){.network-auth-fields{grid-template-columns:minmax(0,1fr)}.network-mode-row{flex-direction:column}.network-settings :global(.network-mode-select){width:100%}}
</style>
