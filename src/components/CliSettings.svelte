<script lang="ts">
 import {onMount} from 'svelte';
 import {Button,Checkbox,Collapsible} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {CliAccess,Root} from '../lib/types';

 export let access:CliAccess|undefined=undefined;
 export let pending=false;
 export let saving=false;
 type Installation={name:string;path:string;installed:boolean;version:string|null};
 let roots:Root[]=[],installations:Installation[]=[],loaded=false,error='',notice='',connection='';
 let operation:{target:string;kind:'install'|'uninstall'|'test'}|null=null;
 let savedAccess:CliAccess|undefined;
 const names:Record<string,string>={cli:'SAG CLI','claude-code':'Claude Code',codex:'Codex',opencode:'OpenCode'};
 $: disabled=pending||saving;
 $: selected=access?.roots.length??0;
 $: unsaved=!!access&&!!savedAccess&&(access.enabled!==savedAccess.enabled||access.roots.length!==savedAccess.roots.length||access.roots.some(path=>!savedAccess!.roots.includes(path)));
 $: cli=installations.find(item=>item.name==='cli');
 $: integrations=installations.filter(item=>item.name!=='cli'&&item.name in names);

 async function load(){
  const [config,added]=await Promise.all([api<{access:CliAccess;installations:Installation[]}>('get_cli_settings'),api<Root[]>('list_roots')]);
  savedAccess=config.access;
  const current=access??config.access;
  access={...current,roots:current.roots.filter(path=>added.some(root=>root.path===path))};
  installations=config.installations;roots=added;loaded=true;
 }
 async function refresh(){pending=true;error='';try{await load();}catch(e){error=String(e);}finally{pending=false;}}
 onMount(()=>{void refresh();});
 function toggle(path:string,checked:boolean){
  if(access)access={...access,roots:checked?[...access.roots,path]:access.roots.filter(root=>root!==path)};
 }
 async function manage(item:Installation){
  const uninstall=item.installed;
  pending=true;error='';notice='';connection='';operation={target:item.name,kind:uninstall?'uninstall':'install'};
  try{
   await api('install_cli_integration',{target:item.name,uninstall});
   await load();
   notice=$t(uninstall?'{value0} 已卸载。':'{value0} 已安装。',{value0:names[item.name]});
  }catch(e){error=String(e);}finally{pending=false;operation=null;}
 }
 async function testConnection(){
  pending=true;error='';notice='';connection='';operation={target:'cli',kind:'test'};
  try{connection=await api<string>('test_cli_connection',{path:access!.roots[0]});}
  catch(e){error=String(e);}finally{pending=false;operation=null;}
 }
</script>

<!-- Hallmark · component: settings · existing monochrome tokens · P5 H4 E4 S5 R5 V4 -->
<div class="cli-settings" aria-busy={pending}>
 {#if !loaded}
  <div class="cli-loading" role="status">{$t('正在读取 CLI 设置…')}</div>
 {:else if access}
  <section class="cli-section" aria-labelledby="cli-access-heading">
   <header class="cli-section-heading"><h3 id="cli-access-heading">{$t('访问权限')}</h3><span>{$t('保存设置后生效')}</span></header>
   <div class="cli-panel">
    <div class="cli-access-row">
     <div><strong>{$t('启用 CLI')}</strong><p>{$t('允许编程工具搜索已授权的本地文件夹。')}</p></div>
     <Switch checked={access.enabled} onCheckedChange={enabled=>access={...access!,enabled}} disabled={disabled} label={$t('启用 CLI')}/>
    </div>
    <div class="cli-folder-heading"><span>{$t('允许搜索的文件夹')}</span><span>{$t('已选 {value0} / {value1}',{value0:selected,value1:roots.length})}</span></div>
    {#each roots as root,index(root.path)}
     <div class="cli-folder" data-selected={access.roots.includes(root.path)}>
      <Checkbox.Root id={`cli-root-${index}`} class="ui-checkbox" disabled={disabled} checked={access.roots.includes(root.path)} onCheckedChange={checked=>toggle(root.path,checked)} aria-label={$t('允许搜索 {value0}',{value0:root.name})}>
       {#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}
      </Checkbox.Root>
      <label for={`cli-root-${index}`}><span class="cli-folder-icon" aria-hidden="true">{@html icon('folder')}</span><span class="cli-folder-text"><strong>{root.name}</strong><span class="cli-path" title={root.path}>{root.path}</span></span></label>
     </div>
    {:else}<div class="cli-empty">{@html icon('folder')}<p>{$t('尚未添加文件夹。请先在文件区域添加项目。')}</p></div>{/each}
   </div>
  </section>

  {#if cli}
   <section class="cli-section" aria-labelledby="cli-command-heading">
    <header class="cli-section-heading"><h3 id="cli-command-heading">{$t('命令行工具')}</h3></header>
    <div class="cli-panel">
     <div class="cli-install-row cli-binary-row">
      <span class="cli-product-icon" aria-hidden="true">{@html icon('codeBlock')}</span>
      <div class="cli-install-info"><div class="cli-install-title"><strong>SAG CLI</strong><span class="cli-badge" data-installed={cli.installed}>{cli.installed?$t('已安装'):$t('未安装')}</span></div><p>{cli.version?`v${cli.version}`:$t('安装后可在终端运行 sag 命令。')}</p></div>
      <Button.Root type="button" class={cli.installed?'outline cli-action':'primary cli-action'} disabled={disabled} aria-label={$t(cli.installed?'卸载 {value0}':'安装 {value0}',{value0:'SAG CLI'})} onclick={()=>manage(cli!)}>{operation?.target==='cli'&&operation.kind!=='test'?$t(operation.kind==='uninstall'?'卸载中…':'安装中…'):$t(cli.installed?'卸载':'安装')}</Button.Root>
     </div>
     <div class="cli-connection-row"><span>{$t('检查应用连接与目录授权。')}</span><Button.Root type="button" class="outline cli-action" disabled={disabled||!cli.installed||!selected||unsaved} onclick={testConnection}>{operation?.kind==='test'?$t('连接中…'):$t('测试连接')}</Button.Root></div>
     {#if !cli.installed||!selected||unsaved}<p class="cli-prerequisite">{!cli.installed?$t('安装 CLI 后可测试连接。'):unsaved?$t('请先保存访问权限，再测试连接。'):$t('选择文件夹并保存后可测试连接。')}</p>{/if}
     <Collapsible.Root class="cli-location">
      <Collapsible.Trigger class="cli-disclosure"><span aria-hidden="true">{@html icon('chevron')}</span>{$t('安装位置')}</Collapsible.Trigger>
      <Collapsible.Content class="cli-location-content"><code>{cli.path}</code><p>{$t('终端路径在安装时自动配置。安装后请新开终端运行 sag。')}</p></Collapsible.Content>
     </Collapsible.Root>
    </div>
   </section>
  {/if}

  <section class="cli-section" aria-labelledby="cli-tools-heading">
   <header class="cli-section-heading"><h3 id="cli-tools-heading">{$t('编程工具')}</h3></header>
   <p class="cli-section-description">{$t('安装 SAG Skill，让编程工具知道如何通过 CLI 搜索已授权的本地文件。')}</p>
   <div class="cli-panel cli-integrations">
    {#each integrations as item(item.name)}
     <div class="cli-install-row">
      <div class="cli-install-info"><strong>{names[item.name]}</strong><p>SAG Skill<span aria-hidden="true"> · </span>{item.installed?$t('已安装'):$t('未安装')}</p><code class="cli-install-command" aria-label={$t('{value0} 的安装命令',{value0:names[item.name]})}>sag install {item.name}</code></div>
      <Button.Root type="button" class="outline cli-action" disabled={disabled} aria-label={$t(item.installed?'卸载 {value0}':'安装 {value0}',{value0:names[item.name]})} onclick={()=>manage(item)}>{operation?.target===item.name&&operation.kind!=='test'?$t(operation.kind==='uninstall'?'卸载中…':'安装中…'):$t(item.installed?'卸载':'安装')}</Button.Root>
     </div>
    {/each}
    <div class="cli-skill-help"><p>{$t('也可在终端运行对应命令，或让编程工具执行安装；使用命令前需先安装上方的 SAG CLI。')}</p><p>{$t('安装完成后，请在对应工具中新开会话以加载 Skill。')}</p></div>
    <Collapsible.Root class="cli-location">
     <Collapsible.Trigger class="cli-disclosure"><span aria-hidden="true">{@html icon('chevron')}</span>{$t('Skill 安装位置')}</Collapsible.Trigger>
     <Collapsible.Content class="cli-location-content">{#each integrations as item(item.name)}<div class="cli-skill-path"><strong>{names[item.name]}</strong><code>{item.path}</code></div>{/each}</Collapsible.Content>
    </Collapsible.Root>
   </div>
  </section>
  <aside class="cli-note"><span aria-hidden="true">{@html icon('info')}</span><div><p>{$t('关闭窗口后仍可使用 CLI；退出应用后停止服务。')}</p><p>{$t('检索问题和候选片段会发送到已配置的 JEV 服务。')}</p></div></aside>
 {/if}
 {#if error}<div class="cli-feedback cli-error" role="alert"><span aria-hidden="true">{@html icon('info')}</span><div><strong>{$t('操作未完成')}</strong><p>{error}</p></div>{#if !loaded}<Button.Root type="button" class="outline" disabled={pending} onclick={refresh}>{$t('重试')}</Button.Root>{/if}</div>{/if}
 {#if notice}<div class="cli-feedback" role="status"><span aria-hidden="true">{@html icon('check')}</span><p>{notice}</p></div>{/if}
 {#if connection}<div class="cli-connection-result" role="status"><strong>{$t('连接状态')}</strong><pre>{connection}</pre></div>{/if}
</div>

<style>
 .cli-settings{display:flex;flex-direction:column;gap:24px;color:var(--color-ink)}
 .cli-section{min-width:0}
 .cli-section-heading{display:flex;align-items:baseline;justify-content:space-between;gap:12px;margin-bottom:10px}
 .cli-section-heading h3{margin:0;font:600 14px/1.5 var(--font-body)}
 .cli-section-heading>span,.cli-section-description{font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .cli-section-description{margin:-4px 0 10px}
 .cli-panel{border:1px solid var(--color-rule);border-radius:10px;background:var(--color-paper);overflow:hidden}
 .cli-access-row{display:flex;align-items:center;justify-content:space-between;gap:24px;padding:16px}
 .cli-access-row strong,.cli-install-info>strong,.cli-install-title strong,.cli-folder strong{font-size:var(--text-ui);font-weight:500;line-height:1.5}
 .cli-access-row p,.cli-install-info p{margin:4px 0 0;font-size:var(--text-caption);line-height:1.5;color:var(--color-secondary)}
 .cli-folder-heading{display:flex;justify-content:space-between;gap:12px;padding:11px 16px;border-top:1px solid var(--color-rule);background:var(--color-panel);font-size:var(--text-caption);color:var(--color-secondary)}
 .cli-folder{display:flex;align-items:center;gap:12px;padding:0 16px;min-height:64px;border-top:1px solid var(--color-rule)}
 .cli-folder label{display:flex;align-items:center;gap:10px;min-width:0;flex:1;padding:12px 0;cursor:pointer}
 .cli-folder-icon{display:flex;color:var(--color-secondary)}
 .cli-folder-icon :global(.icon){width:17px;height:17px}
 .cli-folder-text{display:flex;flex-direction:column;gap:3px;min-width:0}
 .cli-path{font:11px/1.5 var(--font-mono);color:var(--color-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .cli-install-row{display:flex;align-items:center;gap:12px;padding:14px 16px;min-height:68px}
 .cli-install-row+.cli-install-row{border-top:1px solid var(--color-rule)}
 .cli-install-info{flex:1;min-width:0}
 .cli-install-title{display:flex;align-items:center;flex-wrap:wrap;gap:8px}
 .cli-badge{display:inline-flex;align-items:center;border-radius:4px;background:var(--color-panel);color:var(--color-secondary);padding:2px 6px;font:400 11px/1.4 var(--font-body)}
 .cli-badge[data-installed=true]{color:var(--color-ink);background:var(--color-selected)}
 .cli-product-icon{display:grid;place-items:center;width:34px;height:34px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel);flex-shrink:0}
 .cli-product-icon :global(.icon){width:18px;height:18px}
 .cli-settings :global(.cli-action){min-width:66px;min-height:32px;white-space:nowrap;flex-shrink:0}
 .cli-settings :global(button:disabled){opacity:.5;cursor:default}
 .cli-settings :global(button:focus-visible){outline:2px solid var(--color-focus);outline-offset:3px}
 .cli-connection-row{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 16px;border-top:1px solid var(--color-rule);font-size:var(--text-caption);color:var(--color-secondary)}
 .cli-prerequisite{margin:0;padding:0 16px 12px;font-size:var(--text-caption);line-height:1.5;color:var(--color-secondary)}
 .cli-settings :global(.cli-location){border-top:1px solid var(--color-rule)}
 .cli-settings :global(.cli-disclosure){display:flex;align-items:center;gap:6px;width:100%;padding:10px 16px;text-align:left;background:var(--color-transparent);border:0;color:var(--color-secondary);font:400 var(--text-caption)/1.5 var(--font-body)}
 .cli-settings :global(.cli-disclosure>span){display:flex}
 .cli-settings :global(.cli-disclosure .icon){width:12px;height:12px}
 .cli-settings :global(.cli-disclosure[data-state=open] .icon){transform:rotate(90deg)}
 .cli-settings :global(.cli-location-content){padding:0 16px 14px;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.6}
 .cli-settings :global(.cli-location-content code){display:block;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono)}
 .cli-settings :global(.cli-location-content p){margin:6px 0 0}
 .cli-install-command{display:block;margin-top:7px;user-select:text;overflow-wrap:anywhere;font:11px/1.6 var(--font-mono);color:var(--color-secondary)}
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
 .cli-connection-result{padding:14px 16px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-paper)}
 .cli-connection-result>strong{font-size:var(--text-caption);font-weight:500}
 .cli-connection-result pre{margin:8px 0 0;white-space:pre-wrap;overflow-wrap:anywhere;font:11px/1.7 var(--font-mono);color:var(--color-secondary)}
 .cli-loading{padding:24px 0;font-size:var(--text-ui);color:var(--color-secondary)}
 .cli-empty{display:flex;align-items:center;gap:10px;padding:18px 16px;color:var(--color-secondary)}
 .cli-empty :global(.icon){width:18px;height:18px;flex-shrink:0}
 .cli-empty p{margin:0;font-size:var(--text-caption);line-height:1.6}
 @media(hover:hover){.cli-folder:hover{background:var(--color-panel)}.cli-settings :global(.cli-disclosure:hover){background:var(--color-hover);color:var(--color-ink)}}
 @media(max-width:600px){.cli-section-heading{align-items:flex-start}.cli-install-row{gap:8px}.cli-access-row{gap:12px}.cli-connection-row{gap:8px}.cli-binary-row{flex-wrap:wrap}.cli-binary-row .cli-install-info{min-width:120px}.cli-product-icon{display:none}}
</style>
