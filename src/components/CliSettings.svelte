<script lang="ts">
 import {onMount} from 'svelte';
 import {Button,Collapsible} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {CliAccess} from '../lib/types';

 export let access:CliAccess|undefined=undefined;
 export let pending=false;
 export let saving=false;
 type Installation={name:string;path:string;installed:boolean;version:string|null};
 let installations:Installation[]=[],loaded=false,error='',notice='',connection='';
 let operation:{target:string;kind:'install'|'uninstall'|'test'}|null=null;
 let copiedCommand:string|null=null;
 let savedAccess:CliAccess|undefined;
 const names:Record<string,string>={'claude-code':'Claude Code',codex:'Codex',opencode:'OpenCode'};
 $: disabled=pending||saving;
 $: unsaved=!!access&&!!savedAccess&&access.enabled!==savedAccess.enabled;
 $: integrations=installations.filter(item=>item.name in names);

 async function load(){
  const config=await api<{access:CliAccess;installations:Installation[];install_error?:string|null}>('get_cli_settings');
  savedAccess=config.access;
  access=access??config.access;
  installations=config.installations;error=config.install_error??'';loaded=true;
 }
 async function refresh(){pending=true;error='';try{await load();}catch(e){error=String(e);}finally{pending=false;}}
 onMount(()=>{void refresh();});
 async function manage(item:Installation){
  const uninstall=item.installed;
  pending=true;error='';notice='';connection='';operation={target:item.name,kind:uninstall?'uninstall':'install'};
  try{
   await api('install_cli_integration',{target:item.name,uninstall});
   await load();
   notice=$t(uninstall?'{value0} 已卸载。':'{value0} 已安装。',{value0:names[item.name]});
  }catch(e){error=String(e);}finally{pending=false;operation=null;}
 }
 async function copyCommand(name:string){
  error='';notice='';copiedCommand=null;
  try{
   await navigator.clipboard.writeText(`sgrep install ${name}`);
   copiedCommand=name;notice=$t('已复制 {value0} 的安装命令',{value0:names[name]});
  }catch(e){error=$t('复制失败：{value0}',{value0:String(e)});}
 }
 async function testConnection(){
  pending=true;error='';notice='';connection='';operation={target:'cli',kind:'test'};
  try{connection=await api<string>('test_cli_connection');}
  catch(e){error=String(e);}finally{pending=false;operation=null;}
 }
</script>

<!-- Hallmark · component: settings · existing monochrome tokens · P5 H4 E4 S5 R5 V4 -->
<div class="cli-settings" aria-busy={pending}>
 {#if !loaded}
  <div class="cli-loading" role="status">{$t('正在读取 CLI 设置…')}</div>
 {:else if access}
  <section class="cli-section" aria-labelledby="cli-access-heading">
   <header class="cli-section-heading"><h3 id="cli-access-heading">{$t('CLI 服务')}</h3><div class="cli-service-actions"><span>{$t('保存设置后生效')}</span><Button.Root type="button" class="outline cli-action" disabled={disabled||unsaved} onclick={testConnection}>{operation?.kind==='test'?$t('连接中…'):$t('测试连接')}</Button.Root></div></header>
   <div class="cli-panel">
    <div class="cli-access-row">
     <div><strong>{$t('启用 CLI')}</strong><p>{$t('首次启动自动安装并启用。搜索当前目录下已加入知识库的内容。')}</p></div>
     <Switch checked={access.enabled} onCheckedChange={enabled=>access={enabled}} disabled={disabled} label={$t('启用 CLI')}/>
    </div>
    <p class="cli-auto-scope">{$t('无需选择授权范围。编程工具可运行 sgrep add，将当前项目加入知识库后搜索。')}</p>
    {#if unsaved}<p class="cli-auto-scope">{$t('请先保存 CLI 开关，再测试连接。')}</p>{/if}
    {#if connection}<div class="cli-connection-result" role="status"><strong>{$t('连接状态')}</strong><pre>{connection}</pre></div>{/if}
   </div>
  </section>

  <section class="cli-section" aria-labelledby="cli-tools-heading">
   <header class="cli-section-heading"><h3 id="cli-tools-heading">{$t('编程工具')}</h3></header>
   <p class="cli-section-description">{$t('安装 SAG Skill，让编程工具知道如何通过 CLI 搜索已加入知识库的本地文件。')}</p>
   <div class="cli-panel cli-integrations">
    {#each integrations as item(item.name)}
     <div class="cli-install-row">
      <div class="cli-install-info"><strong>{names[item.name]}</strong><div class="cli-command-copy"><code class="cli-install-command" aria-label={$t('{value0} 的安装命令',{value0:names[item.name]})}>sgrep install {item.name}</code><Button.Root type="button" class="icon-button cli-copy" disabled={disabled} aria-label={$t('复制 {value0} 安装命令',{value0:names[item.name]})} title={copiedCommand===item.name?$t('已复制'):$t('复制安装命令')} onclick={()=>copyCommand(item.name)}>{@html icon(copiedCommand===item.name?'check':'copy')}</Button.Root></div></div>
      <Button.Root type="button" class={item.installed?'outline cli-action cli-uninstall':'outline cli-action'} disabled={disabled} aria-label={$t(item.installed?'卸载 {value0}':'安装 {value0}',{value0:names[item.name]})} onclick={()=>manage(item)}>{operation?.target===item.name?$t(operation.kind==='uninstall'?'卸载中…':'安装中…'):$t(item.installed?'卸载':'安装')}</Button.Root>
     </div>
    {/each}
    <div class="cli-skill-help"><p>{$t('也可在终端运行对应命令，或让编程工具执行安装。sgrep 随应用自动安装。')}</p><p>{$t('安装完成后，请在对应工具中新开会话以加载 Skill。')}</p></div>
    <Collapsible.Root class="cli-location">
     <Collapsible.Trigger class="cli-disclosure"><span aria-hidden="true">{@html icon('chevron')}</span>{$t('Skill 安装位置')}</Collapsible.Trigger>
     <Collapsible.Content class="cli-location-content">{#each integrations as item(item.name)}<div class="cli-skill-path"><strong>{names[item.name]}</strong><code>{item.path}</code></div>{/each}</Collapsible.Content>
    </Collapsible.Root>
   </div>
  </section>
  <aside class="cli-note"><span aria-hidden="true">{@html icon('info')}</span><div><p>{$t('CLI 可独立运行；桌面端开启时共用知识库。')}</p><p>{$t('检索问题和候选片段会发送到已配置的 JEV 服务。')}</p></div></aside>
 {/if}
 {#if error}<div class="cli-feedback cli-error" role="alert"><span aria-hidden="true">{@html icon('info')}</span><div><strong>{$t('操作未完成')}</strong><p>{error}</p></div>{#if !loaded}<Button.Root type="button" class="outline" disabled={pending} onclick={refresh}>{$t('重试')}</Button.Root>{/if}</div>{/if}
 {#if notice}<div class="cli-feedback" role="status"><span aria-hidden="true">{@html icon('check')}</span><p>{notice}</p></div>{/if}
</div>

<style>
 .cli-settings{display:flex;flex-direction:column;gap:24px;color:var(--color-ink)}
 .cli-section{min-width:0}
 .cli-section-heading{display:flex;align-items:baseline;justify-content:space-between;gap:12px;margin-bottom:10px}
 .cli-section-heading h3{margin:0;font:600 14px/1.5 var(--font-body)}
 .cli-section-description{font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .cli-service-actions{display:flex;align-items:center;gap:12px}
 .cli-service-actions>span{font-size:var(--text-caption);color:var(--color-secondary)}
 .cli-section-description{margin:-4px 0 10px}
 .cli-panel{border:1px solid var(--color-rule);border-radius:10px;background:var(--color-paper);overflow:hidden}
 .cli-access-row{display:flex;align-items:center;justify-content:space-between;gap:24px;padding:16px}
 .cli-access-row strong,.cli-install-info>strong{font-size:var(--text-ui);font-weight:500;line-height:1.5}
 .cli-access-row p{margin:4px 0 0;font-size:var(--text-caption);line-height:1.5;color:var(--color-secondary)}
 .cli-auto-scope{margin:0;padding:0 16px 16px;font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .cli-install-row{display:flex;align-items:center;gap:12px;padding:14px 16px;min-height:68px}
 .cli-install-row+.cli-install-row{border-top:1px solid var(--color-rule)}
 .cli-install-info{flex:1;min-width:0}
 .cli-settings :global(.cli-action){min-width:66px;min-height:32px;white-space:nowrap;flex-shrink:0}
 .cli-settings :global(.cli-action.cli-uninstall){background:var(--color-danger);border-color:var(--color-danger);color:var(--color-danger-ink)}
 .cli-settings :global(button:disabled){opacity:.5;cursor:default}
 .cli-settings :global(button:focus-visible){outline:2px solid var(--color-focus);outline-offset:3px}
 .cli-settings :global(.cli-location){border-top:1px solid var(--color-rule)}
 .cli-settings :global(.cli-disclosure){display:flex;align-items:center;gap:6px;width:100%;padding:10px 16px;text-align:left;background:var(--color-transparent);border:0;color:var(--color-secondary);font:400 var(--text-caption)/1.5 var(--font-body)}
 .cli-settings :global(.cli-disclosure>span){display:flex}
 .cli-settings :global(.cli-disclosure .icon){width:12px;height:12px}
 .cli-settings :global(.cli-disclosure[data-state=open] .icon){transform:rotate(90deg)}
 .cli-settings :global(.cli-location-content){padding:0 16px 14px;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.6}
 .cli-settings :global(.cli-location-content code){display:block;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono)}
 .cli-settings :global(.cli-location-content p){margin:6px 0 0}
 .cli-command-copy{display:flex;align-items:center;gap:6px;margin-top:3px}
 .cli-settings :global(.cli-copy){display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;flex-shrink:0;color:var(--color-secondary)}
 .cli-settings :global(.cli-copy .icon){width:14px;height:14px}
 .cli-install-command{display:block;user-select:text;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono);color:var(--color-secondary)}
 .cli-skill-help{padding:12px 16px;border-top:1px solid var(--color-rule);font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .cli-skill-help p{margin:0}
 .cli-skill-help p+p{margin-top:4px}
 .cli-skill-path+.cli-skill-path{margin-top:12px}
 .cli-skill-path strong{display:block;margin-bottom:4px;font-size:var(--text-caption);font-weight:500;color:var(--color-ink)}
 .cli-note,.cli-feedback{display:flex;align-items:flex-start;gap:8px;font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .cli-note>span,.cli-feedback>span{display:flex;padding-top:2px;flex-shrink:0}
 .cli-note :global(.icon),.cli-feedback :global(.icon){width:14px;height:14px}
 .cli-note p,.cli-feedback p{margin:0;overflow-wrap:anywhere}
 .cli-note p+p{margin-top:4px}
 .cli-feedback{padding:12px 14px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-paper);color:var(--color-ink)}
 .cli-error{color:var(--color-ink);border-color:var(--color-secondary)}
 .cli-error strong{display:block;font-size:var(--text-caption);font-weight:500;margin-bottom:3px}
 .cli-connection-result{padding:14px 16px;border-top:1px solid var(--color-rule)}
 .cli-connection-result>strong{font-size:var(--text-caption);font-weight:500}
 .cli-connection-result pre{margin:8px 0 0;white-space:pre-wrap;overflow-wrap:anywhere;font:11px/1.7 var(--font-mono);color:var(--color-secondary)}
 .cli-loading{padding:24px 0;font-size:var(--text-ui);color:var(--color-secondary)}
 @media(hover:hover){.cli-settings :global(.cli-action.cli-uninstall:not(:disabled):hover){background:var(--color-danger-hover);border-color:var(--color-danger-hover)}.cli-settings :global(.cli-disclosure:hover){background:var(--color-hover);color:var(--color-ink)}}
 @media(max-width:600px){.cli-section-heading{align-items:flex-start}.cli-install-row{gap:8px}.cli-access-row{gap:12px}}
</style>
