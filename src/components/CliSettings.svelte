<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import type {Root} from '../lib/types';
 type Access={enabled:boolean;roots:string[]};
 type Installation={name:string;path:string;installed:boolean;version:string|null};
 let access:Access={enabled:false,roots:[]},roots:Root[]=[],installations:Installation[]=[],busy=false,loaded=false,error='',status='';
 const names:Record<string,string>={cli:'SAG CLI','claude-code':'Claude Code',codex:'Codex',opencode:'OpenCode','factory-droid':'Factory Droid'};
 async function load(){const [config,added]=await Promise.all([api<{access:Access;installations:Installation[]}>('get_cli_settings'),api<Root[]>('list_roots')]);access={...config.access,roots:config.access.roots.filter(path=>added.some(root=>root.path===path))};installations=config.installations;roots=added;loaded=true;}
 onMount(()=>{load().catch(e=>error=String(e));});
 async function action(work:()=>Promise<string>){busy=true;error='';status='';try{status=await work();await load();}catch(e){error=String(e);}finally{busy=false;}}
 function toggle(path:string,checked:boolean){access={...access,roots:checked?[...access.roots,path]:access.roots.filter(root=>root!==path)};}
</script>
<div class="settings-info-box">{$t('关闭窗口后仍可使用 CLI；退出 SAG 后不可用。深入检索会把问题和候选片段发送到已配置的 JEV 服务。')}</div>
{#if loaded}
 <section class="settings-group settings-fields">
  <div class="setting-row"><div><strong>{$t('启用 CLI 接入')}</strong><p>{$t('仅允许搜索下方授权的已添加目录。')}</p></div><Switch bind:checked={access.enabled} disabled={busy} label={$t('启用 CLI 接入')}/></div>
  {#each roots as root(root.path)}
   <label class="cli-root"><input type="checkbox" checked={access.roots.includes(root.path)} disabled={busy} onchange={e=>toggle(root.path,e.currentTarget.checked)}/><span>{root.name}<small>{root.path}</small></span></label>
  {:else}<p class="settings-help">{$t('请先在文件区域添加项目目录。')}</p>{/each}
  <Button.Root type="button" class="outline" disabled={busy} onclick={()=>action(async()=>{await api('save_cli_settings',{access});return $t('CLI 设置已应用。');})}>{$t('应用 CLI 设置')}</Button.Root>
 </section>
 {#each installations as item(item.name)}
  <section class="settings-group settings-fields">
   <div class="setting-row"><div><strong>{names[item.name]}</strong><p>{item.installed?$t('已安装'):$t('未安装')}{item.version?' · '+item.version:''}</p></div><Button.Root type="button" class="outline" disabled={busy} onclick={()=>action(()=>api<string>('install_cli_integration',{target:item.name,uninstall:false}))}>{item.installed?$t('更新'):$t('安装')}</Button.Root></div>
   <p class="settings-help cli-path">{item.path}</p>
   <div class="settings-card-actions">
    <Button.Root type="button" class="settings-text-button" disabled={busy||!item.installed} onclick={()=>action(()=>api<string>('install_cli_integration',{target:item.name,uninstall:true}))}>{$t('卸载')}</Button.Root>
    <Button.Root type="button" class="settings-text-button" disabled={busy||!item.installed||!access.roots.length} onclick={()=>action(()=>api<string>('test_cli_connection',{path:access.roots[0]}))}>{$t('测试 CLI 连接')}</Button.Root>
   </div>
   {#if item.name!=='cli'}<p class="settings-help">{$t('安装后重新启动 Harness。测试按钮检查 CLI 连接；实际任务调用需在 Harness 中验证。')}</p>{/if}
  </section>
 {/each}
{/if}
{#if error}<p class="form-error" role="alert">{error}</p>{/if}
{#if status}<pre class="cli-feedback" role="status">{status}</pre>{/if}
<style>
 .cli-root{display:flex;align-items:center;gap:10px;margin:8px 0;min-width:0}.cli-root span{min-width:0}.cli-root small{display:block;overflow-wrap:anywhere;color:var(--text-secondary);margin-top:4px}.cli-path{overflow-wrap:anywhere}.cli-feedback{white-space:pre-wrap;overflow-wrap:anywhere;font:inherit}
</style>
