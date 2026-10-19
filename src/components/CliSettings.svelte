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
 let operation:{target:string;kind:'install'|'uninstall'|'test'|'download'}|null=null;
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
   await navigator.clipboard.writeText(`sag install ${name}`);
   copiedCommand=name;notice=$t('已复制 {value0} 的安装命令',{value0:names[name]});
  }catch(e){error=$t('复制失败：{value0}',{value0:String(e)});}
 }
 async function testConnection(){
  pending=true;error='';notice='';connection='';operation={target:'cli',kind:'test'};
  try{connection=await api<string>('test_cli_connection');}
  catch(e){error=String(e);}finally{pending=false;operation=null;}
 }
 async function downloadSkill(){
  pending=true;error='';notice='';connection='';operation={target:'other',kind:'download'};
  try{
   const path=await api<string|null>('download_cli_skill',{title:$t('选择 Skill 保存位置')});
   if(path)notice=$t('Skill 已下载到 {value0}。请将 sag 文件夹添加到所用工具的 Skills 目录。',{value0:path});
  }catch(e){error=String(e);}finally{pending=false;operation=null;}
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
    <p class="cli-auto-scope">{$t('无需选择授权范围。Agent 可运行 sag add，将当前项目加入知识库后搜索。')}</p>
    {#if unsaved}<p class="cli-auto-scope">{$t('请先保存 CLI 开关，再测试连接。')}</p>{/if}
    {#if connection}<div class="cli-connection-result" role="status"><strong>{$t('连接状态')}</strong><pre>{connection}</pre></div>{/if}
   </div>
  </section>

  <section class="cli-section" aria-labelledby="cli-tools-heading">
   <header class="cli-section-heading"><h3 id="cli-tools-heading">{$t('接入Agent')}</h3></header>
   <p class="cli-section-description">{$t('安装 SAG Skill，让 Agent 知道如何通过 CLI 搜索已加入知识库的本地文件。')}</p>
   <div class="cli-panel cli-integrations">
    {#each integrations as item(item.name)}
     <div class="cli-install-row">
      <div class="cli-install-info"><strong>{names[item.name]}</strong><div class="cli-command-copy"><code class="cli-install-command" aria-label={$t('{value0} 的安装命令',{value0:names[item.name]})}>sag install {item.name}</code><Button.Root type="button" class="icon-button cli-copy" disabled={disabled} aria-label={$t('复制 {value0} 安装命令',{value0:names[item.name]})} title={copiedCommand===item.name?$t('已复制'):$t('复制安装命令')} onclick={()=>copyCommand(item.name)}>{@html icon(copiedCommand===item.name?'check':'copy')}</Button.Root></div></div>
      <Button.Root type="button" class={item.installed?'outline cli-action cli-uninstall':'outline cli-action'} disabled={disabled} aria-label={$t(item.installed?'卸载 {value0}':'安装 {value0}',{value0:names[item.name]})} onclick={()=>manage(item)}>{operation?.target===item.name?$t(operation.kind==='uninstall'?'卸载中…':'安装中…'):$t(item.installed?'卸载':'安装')}</Button.Root>
     </div>
    {/each}
    <div class="cli-install-row">
     <div class="cli-install-info"><strong>{$t('其他')}</strong><p class="cli-download-description">{$t('下载完整 Skill 文件夹，接入到其他Agent。')}</p></div>
     <Button.Root type="button" class="outline cli-action" disabled={disabled} aria-label={$t('下载 SAG Skill')} onclick={downloadSkill}>{operation?.kind==='download'?$t('下载中…'):$t('下载')}</Button.Root>
    </div>
   </div>
  </section>
  <aside class="cli-note"><span aria-hidden="true">{@html icon('info')}</span><div><p>{$t('CLI 和桌面端共用后台服务，退出桌面后仍可使用。')}</p><p>{$t('检索问题和候选片段会发送到已配置的 JEV 服务。')}</p></div></aside>
 {/if}
 {#if error}<div class="cli-feedback cli-error" role="alert"><span aria-hidden="true">{@html icon('info')}</span><div><strong>{$t('操作未完成')}</strong><p>{error}</p></div>{#if !loaded}<Button.Root type="button" class="outline" disabled={pending} onclick={refresh}>{$t('重试')}</Button.Root>{/if}</div>{/if}
 {#if notice}<div class="cli-feedback" role="status"><span aria-hidden="true">{@html icon('check')}</span><p>{notice}</p></div>{/if}
 <div class="cli-panel cli-guides">
  <Collapsible.Root class="cli-location" open={false}>
   <Collapsible.Trigger type="button" class="cli-disclosure cli-guide-trigger"><span aria-hidden="true">{@html icon('chevron')}</span>{$t('Skill 安装指南')}</Collapsible.Trigger>
   <Collapsible.Content class="cli-location-content">
    <div class="cli-skill-help"><p>{$t('点击上方对应 Agent 的“安装”按钮即可完成安装。')}</p><p>{$t('也可在终端运行对应命令，或让 Agent 执行安装。')}</p><p>{$t('安装完成后，请在对应 Agent 中新开会话以加载 Skill。')}</p></div>
    {#each integrations as item(item.name)}<div class="cli-skill-path"><strong>{names[item.name]}</strong><code>{item.path}</code></div>{/each}
   </Collapsible.Content>
  </Collapsible.Root>
 <Collapsible.Root class="cli-guide" open={false}>
  <Collapsible.Trigger type="button" class="cli-disclosure cli-guide-trigger"><span aria-hidden="true">{@html icon('chevron')}</span>{$t('CLI 使用指南')}</Collapsible.Trigger>
  <Collapsible.Content class="cli-guide-content">
   <section>
    <h4>{$t('开始使用')}</h4>
    <p>{$t('在终端进入项目目录，将文件夹加入知识库，再检查索引状态。已添加的文件夹无需重复添加。')}</p>
    <pre><code>cd /path/to/project
sag add
sag check</code></pre>
   </section>
   <section>
    <h4>{$t('用完整问题搜索')}</h4>
    <p>{$t('默认进行语义搜索，并返回原文片段、路径和行号。用一句完整问题描述要查找的信息；不指定目录时搜索当前目录下已索引的内容。')}</p>
    <pre><code>sag "{$t('这个项目如何处理登录与权限？')}" -m 5</code></pre>
    <p>{$t('也可以限定到一个子目录：')}</p>
    <pre><code>sag "{$t('发布失败后如何回滚？')}" ./docs -m 5</code></pre>
    <dl>
     <div><dt><code>-m N</code></dt><dd>{$t('返回的证据块数量，默认 10，范围 1–100；建议从 3–5 条开始。')}</dd></div>
     <div><dt><code>--json</code></dt><dd>{$t('输出 JSON，方便脚本和 Agent 读取。')}</dd></div>
    </dl>
   </section>
   <section>
    <h4>{$t('移除与恢复目录')}</h4>
    <p>{$t('移除只影响知识库和索引，不会删除本地文件。子目录移除后，可用 add 恢复。')}</p>
    <pre><code>sag remove ./docs
sag add ./docs</code></pre>
    <p>{$t('不填写路径时操作当前目录。项目内的 .gitignore 和 .sagignore 会自动生效，.sagignore 优先。')}</p>
   </section>
   <section>
    <h4>{$t('配置与检查')}</h4>
    <p>{$t('CLI 与桌面端共用模型配置和密钥。需要重新配置时运行 sag config；首次使用尚未配置时会自动引导。')}</p>
    <pre><code>sag check
sag list
sag status
sag config
sag service status
sag service restart
sag --help</code></pre>
    <p>{$t('check 检查当前目录的索引覆盖、就绪状态和文件数，list 列出已添加目录，status 查看服务、CLI 接入和模型配置状态。service restart 重启服务，--help 查看完整命令说明。')}</p>
   </section>
  </Collapsible.Content>
 </Collapsible.Root>
 </div>
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
 .cli-download-description{margin:4px 0 0;font-size:var(--text-caption);line-height:1.5;color:var(--color-secondary)}
 .cli-settings :global(.cli-action){min-width:66px;min-height:32px;white-space:nowrap;flex-shrink:0}
 .cli-settings :global(.cli-action.cli-uninstall){background:var(--color-danger);border-color:var(--color-danger);color:var(--color-danger-ink)}
 .cli-settings :global(button:disabled){opacity:.5;cursor:default}
 .cli-settings :global(button:focus-visible){outline:2px solid var(--color-focus);outline-offset:3px}
 .cli-settings :global(.cli-guide){border-top:1px solid var(--color-rule)}
 .cli-settings :global(.cli-disclosure){display:flex;align-items:center;gap:6px;width:100%;padding:10px 16px;text-align:left;background:var(--color-transparent);border:0;color:var(--color-secondary);font:400 var(--text-caption)/1.5 var(--font-body)}
 .cli-settings :global(.cli-disclosure>span){display:flex}
 .cli-settings :global(.cli-disclosure .icon){width:12px;height:12px}
 .cli-settings :global(.cli-disclosure[data-state=open] .icon){transform:rotate(90deg)}
 .cli-settings :global(.cli-location-content){padding:16px 16px 24px;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.6}
 .cli-settings :global(.cli-location-content code){display:block;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono)}
 .cli-settings :global(.cli-location-content p){margin:6px 0 0}
 .cli-command-copy{display:flex;align-items:center;gap:6px;margin-top:3px}
 .cli-settings :global(.cli-copy){display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;flex-shrink:0;color:var(--color-secondary)}
 .cli-settings :global(.cli-copy .icon){width:14px;height:14px}
 .cli-install-command{display:block;user-select:text;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono);color:var(--color-secondary)}
 .cli-skill-help{margin-bottom:20px;font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
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
 .cli-settings :global(.cli-guide-trigger){padding:14px 16px;font-size:var(--text-ui);font-weight:500;color:var(--color-ink)}
 .cli-settings :global(.cli-guide-content){padding:16px 16px 24px;font-size:var(--text-caption);line-height:1.7;color:var(--color-secondary)}
 .cli-settings :global(.cli-guide-content section+section){margin-top:20px;padding-top:16px;border-top:1px solid var(--color-rule)}
 .cli-settings :global(.cli-guide-content h4){margin:0 0 6px;font-size:var(--text-ui);font-weight:500;color:var(--color-ink)}
 .cli-settings :global(.cli-guide-content p){margin:6px 0}
 .cli-settings :global(.cli-guide-content pre){margin:10px 0;padding:12px;border-radius:7px;background:var(--color-panel);white-space:pre-wrap;overflow-wrap:anywhere;color:var(--color-ink)}
 .cli-settings :global(.cli-guide-content code){font:12px/1.7 var(--font-mono);user-select:text}
 .cli-settings :global(.cli-guide-content dl){margin:12px 0 0}
 .cli-settings :global(.cli-guide-content dl>div){display:flex;gap:12px;margin-top:6px}
 .cli-settings :global(.cli-guide-content dt){min-width:60px;color:var(--color-ink)}
 .cli-settings :global(.cli-guide-content dd){margin:0}
 @media(hover:hover){.cli-settings :global(.cli-action.cli-uninstall:not(:disabled):hover){background:var(--color-danger-hover);border-color:var(--color-danger-hover)}.cli-settings :global(.cli-disclosure:hover){background:var(--color-hover);color:var(--color-ink)}}
 @media(max-width:600px){.cli-section-heading{align-items:flex-start}.cli-install-row{gap:8px}.cli-access-row{gap:12px}}
</style>
