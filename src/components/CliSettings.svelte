<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import Input from './ui/Input.svelte';
 import RemoteAccess from './RemoteAccess.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 let enabled=false,loading=true,busy=false,error='',directory='',command='sag status',mode='remote',copied='';
 const agents=[['codex','Codex','~/.agents/skills/sag'],['claude-code','Claude Code','~/.claude/skills/sag'],['opencode','OpenCode','~/.config/opencode/skills/sag']];
 let roots:{path:string}[]=[],result:{code:number;stdout:string;stderr:string}|null=null;
 async function load(){
  try{const [access,folders]=await Promise.all([api<{enabled:boolean}>('get_access'),api<{path:string}[]>('list_roots')]);enabled=access.enabled;roots=folders;directory=roots[0]?.path??'';}
  catch(e){error=String(e)}finally{loading=false}
 }
 onMount(()=>{void load()});
 async function toggle(value:boolean){busy=true;error='';try{await api('set_access',{access:{enabled:value}});enabled=value}catch(e){error=String(e)}finally{busy=false}}
 async function run(value=command){busy=true;error='';result=null;command=value;try{result=await api('fnos_cli_run',{command,directory});enabled=(await api<{enabled:boolean}>('get_access')).enabled}catch(e){error=String(e)}finally{busy=false}}
 async function copyCommand(value:string){error='';try{await navigator.clipboard.writeText(value);copied=value}catch(e){error=String(e)}}
</script>
<section class="settings-group" aria-busy={loading||busy}>
 <div class="setting-row"><div><strong>{$t('启用 CLI')}</strong><p>{$t('CLI 与网页共用当前飞牛用户的索引和模型配置，文件访问仍需飞牛授权。')}</p></div><Switch checked={enabled} disabled={loading||busy} onCheckedChange={toggle} label={$t('启用 CLI')}/></div>
</section>
<div class="connection-tabs" role="tablist" aria-label={$t('接入方式')}>
 <button type="button" role="tab" id="remote-tab" aria-controls="remote-panel" aria-selected={mode==='remote'} onclick={()=>mode='remote'}>{$t('电脑远程接入')}</button>
 <button type="button" role="tab" id="nas-tab" aria-controls="nas-panel" aria-selected={mode==='nas'} onclick={()=>mode='nas'}>{$t('NAS 本机 CLI')}</button>
</div>
<div id="remote-panel" role="tabpanel" aria-labelledby="remote-tab" hidden={mode!=='remote'}><RemoteAccess {enabled}/></div>
<div id="nas-panel" role="tabpanel" aria-labelledby="nas-tab" hidden={mode!=='nas'}>
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('1. 安装 NAS 命令行')}</strong><p>{$t('在 NAS 上运行 Agent 时使用。安装位置为当前飞牛用户的主目录。')}</p></div></div>
 <div class="settings-fields cli-body">
  <p class="help">{$t('先选择下方已授权的工作目录，再点击安装；也可以在 NAS 的 SSH 终端执行此命令。')}</p>
  <div class="command-row"><code>/var/apps/sag-desktop-fnos/target/bin/sag install cli</code><Button.Root type="button" class="outline" disabled={busy||!directory} onclick={()=>run('sag install cli')}>{$t('安装 CLI')}</Button.Root></div>
  <p class="help">{$t('重新打开终端后使用 sag；关闭网页不影响 CLI，应用需保持运行。')}</p>
 </div>
</section>
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('2. 安装 SAG Skill')}</strong><p>{$t('让 Agent 知道如何搜索已授权的 NAS 文件。选择所用 Agent 安装，然后新开会话。')}</p></div></div>
 <div class="settings-fields cli-body">
  {#each agents as [target,name,path]}
   <div class="skill-row">
    <div class="skill-info"><strong>{name}</strong><code>{path}</code><div class="command-row"><code>sag install {target}</code><Button.Root type="button" class="outline" onclick={()=>copyCommand(`sag install ${target}`)} aria-label={$t('复制 {value0} 安装命令',{value0:name})}>{copied===`sag install ${target}`?$t('已复制'):$t('复制')}</Button.Root></div></div>
    <div class="actions"><Button.Root type="button" class="outline" disabled={busy||!directory} onclick={()=>run(`sag install ${target}`)} aria-label={$t('安装 {value0}',{value0:name})}>{$t('安装')}</Button.Root><Button.Root type="button" class="outline" disabled={busy||!directory} onclick={()=>run(`sag uninstall ${target}`)} aria-label={$t('卸载 {value0}',{value0:name})}>{$t('卸载')}</Button.Root></div>
   </div>
  {/each}
  <div class="skill-row"><div class="skill-info"><strong>{$t('其他 Agent')}</strong><p class="help">{$t('下载完整 Skill 文件夹，包含参数、安装和忽略规则说明。解压到 NAS 上对应 Agent 的 Skills 目录。')}</p></div><a class="outline download" href={`${import.meta.env.BASE_URL}skills/sag-cli-skill.zip`} download>{$t('下载 CLI Skill')}</a></div>
  {#if result&&/^sag (un)?install /.test(command)}<div class="result" role="status"><strong>{command} · {$t('退出码')}：{result.code}</strong><pre>{result.stdout}{result.stderr}</pre></div>{/if}
 </div>
</section>
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('测试 CLI')}</strong><p>{$t('以当前飞牛用户运行实际 sag 程序。支持带引号的参数，不执行 Shell 命令。')}</p></div></div>
 <div class="settings-fields cli-body">
 <label class="field-label"><span>{$t('工作目录')}</span><Input bind:value={directory} list="cli-roots" placeholder="/vol1/1000/File" disabled={busy}/></label>
 <datalist id="cli-roots">{#each roots as root}<option value={root.path}></option>{/each}</datalist>
 <label class="field-label"><span>{$t('命令')}</span><Input bind:value={command} disabled={busy} spellcheck={false}/></label>
 <div class="actions"><Button.Root type="button" class="primary" disabled={busy||!directory.trim()} onclick={()=>run()}>{busy?$t('执行中…'):$t('执行命令')}</Button.Root>{#each ['sag status','sag list --json','sag check --json','sag --help'] as value}<Button.Root type="button" class="outline" disabled={busy||!directory.trim()} onclick={()=>run(value)}>{value}</Button.Root>{/each}</div>
 {#if result&&!/^sag (un)?install /.test(command)}<div class="result" role="status"><p>{$t('退出码')}：{result.code}</p><pre>{result.stdout}{result.stderr}</pre></div>{/if}
 </div>
</section>
<details class="settings-group cli-guide" open>
 <summary>{$t('CLI 使用指南')}</summary>
 <div class="settings-fields cli-body"><p class="help">{$t('目录需先在飞牛中授权给 SAG Wiki。')}</p>
 <h4>{$t('开始使用')}</h4>
 <p class="help">{$t('在 NAS 终端进入已授权目录；已添加的目录无需重复添加。先检查索引，再用完整问题搜索。')}</p>
 <pre><code>sag add /vol1/1000/File --types md,txt,html,py,js,ts,json,yaml
sag check /vol1/1000/File --json</code></pre>
 <h4>{$t('用完整问题搜索')}</h4>
 <p class="help">{$t('语义搜索返回原文片段、路径和行号；-m 指定结果数，--json 便于 Agent 读取。已知关键词可用 -F 精确匹配。')}</p>
 <pre><code>sag "{$t('这些资料如何说明项目的验收要求？')}" /vol1/1000/File -m 5 --json
sag -F "keyword" /vol1/1000/File --json</code></pre>
 <h4>{$t('移除与恢复目录')}</h4>
 <p class="help">{$t('移除只影响索引，原始文件保留。恢复同一目录时使用 add；.gitignore 和 .sagignore 继续生效。')}</p>
 <pre><code>sag remove /vol1/1000/File/docs
sag add /vol1/1000/File/docs</code></pre>
 <h4>{$t('配置与检查')}</h4>
 <p class="help">{$t('CLI 复用网页的模型配置。check 检查目录就绪状态，list 列出索引目录，status 查看连接和模型配置。')}</p>
 <pre><code>sag status
sag list --json
sag config --if-needed
sag service status
sag service restart
sag --help</code></pre>
 <p class="help">{$t('service stop 和 restart 只影响当前用户的共享服务；下一次操作会自动启动服务。')}</p></div>
</details>
</div>
{#if error}<p class="form-error" role="alert">{error}</p>{/if}
<style>
 .connection-tabs{display:flex;gap:4px;padding:4px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-panel)}
 .connection-tabs button{flex:1;padding:9px 12px;border:0;border-radius:6px;background:transparent;color:var(--color-secondary);font:500 var(--text-ui)/1.5 var(--font-body);cursor:pointer}
 .connection-tabs button[aria-selected=true]{background:var(--color-paper);color:var(--color-ink);box-shadow:0 1px 3px #00000012}
 .connection-tabs button:focus-visible{outline:2px solid var(--color-focus)}
 #remote-panel,#nas-panel{margin-top:18px}
 #nas-panel:not([hidden]){display:grid;gap:18px}
 .cli-body{padding:17px}
 .cli-body pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.65 var(--font-mono,monospace);background:var(--color-panel);border:1px solid var(--color-rule);border-radius:7px;padding:12px;margin:12px 0}
 .cli-body .help{font-size:var(--text-caption);color:var(--color-secondary);line-height:1.6;margin:0}
 .cli-body .field-label{display:block;margin:0 0 16px}
 .cli-body h4{margin:20px 0 7px;font-size:var(--text-ui);font-weight:500}
 .actions{display:flex;flex-wrap:wrap;gap:8px}
 .command-row{display:flex;align-items:center;gap:10px;margin:10px 0}
 .command-row code{flex:1;min-width:0;overflow-wrap:anywhere;font:12px/1.6 var(--font-mono)}
 .skill-row{display:flex;align-items:center;gap:16px;padding:15px 0}
 .skill-row+.skill-row{border-top:1px solid var(--color-rule)}
 .skill-row:first-child{padding-top:0}.skill-row:last-child{padding-bottom:0}
 .skill-info{flex:1;min-width:0}.skill-info>code{display:block;font:11px/1.7 var(--font-mono);color:var(--color-secondary);overflow-wrap:anywhere}
 .download{white-space:nowrap;text-decoration:none;font-size:var(--text-ui);padding:8px 12px}
 .result{margin-top:20px;padding-top:16px;border-top:1px solid var(--color-rule)}
 .result p{font-size:var(--text-caption);color:var(--color-secondary);margin:0}
 .result pre{max-height:280px;overflow:auto;margin-bottom:0}
 .form-error{margin:12px 0 0}
 .cli-guide summary{padding:15px 17px;cursor:pointer;font-size:var(--text-ui);font-weight:500}
 .cli-guide[open] summary{border-bottom:1px solid var(--color-rule)}
 @media(max-width:600px){.skill-row{align-items:flex-start;flex-wrap:wrap}.command-row{flex-wrap:wrap}}
</style>
