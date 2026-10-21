<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import Input from './ui/Input.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 let enabled=false,loading=true,busy=false,error='',directory='',command='sag status';
 let roots:{path:string}[]=[],result:{code:number;stdout:string;stderr:string}|null=null;
 async function load(){
  try{const [access,folders]=await Promise.all([api<{enabled:boolean}>('get_access'),api<{path:string}[]>('list_roots')]);enabled=access.enabled;roots=folders;directory=roots[0]?.path??'';}
  catch(e){error=String(e)}finally{loading=false}
 }
 onMount(()=>{void load()});
 async function toggle(value:boolean){busy=true;error='';try{await api('set_access',{access:{enabled:value}});enabled=value}catch(e){error=String(e)}finally{busy=false}}
 async function run(value=command){busy=true;error='';result=null;command=value;try{result=await api('fnos_cli_run',{command,directory});enabled=(await api<{enabled:boolean}>('get_access')).enabled}catch(e){error=String(e)}finally{busy=false}}
</script>
<section class="settings-group" aria-busy={loading||busy}>
 <div class="setting-row"><div><strong>{$t('启用 CLI')}</strong><p>{$t('CLI 与网页共用当前飞牛用户的索引和模型配置，文件访问仍需飞牛授权。')}</p></div><Switch checked={enabled} disabled={loading||busy} onCheckedChange={toggle} label={$t('启用 CLI')}/></div>
 <details class="cli-install"><summary>{$t('终端安装')}</summary>
 <div class="settings-fields cli-body">
 <p class="help">{$t('SSH 登录后，首次运行以下命令安装到当前用户的终端路径：')}</p>
 <pre><code>/var/apps/sag-desktop-fnos/target/bin/sag install cli</code></pre>
 <p class="help">{$t('重新打开终端后使用 sag；关闭网页不影响 CLI，应用需保持运行。')}</p>
 </div>
 </details>
</section>
<section class="settings-group">
 <div class="setting-row"><div><strong>{$t('测试 CLI')}</strong><p>{$t('以当前飞牛用户运行实际 sag 程序。支持带引号的参数，不执行 Shell 命令。')}</p></div></div>
 <div class="settings-fields cli-body">
 <label class="field-label"><span>{$t('工作目录')}</span><Input bind:value={directory} list="cli-roots" placeholder="/vol1/1000/File" disabled={busy}/></label>
 <datalist id="cli-roots">{#each roots as root}<option value={root.path}></option>{/each}</datalist>
 <label class="field-label"><span>{$t('命令')}</span><Input bind:value={command} disabled={busy} spellcheck={false}/></label>
 <div class="actions"><Button.Root type="button" class="primary" disabled={busy||!directory.trim()} onclick={()=>run()}>{busy?$t('执行中…'):$t('执行命令')}</Button.Root>{#each ['sag status','sag list --json','sag check --json','sag --help'] as value}<Button.Root type="button" class="outline" disabled={busy||!directory.trim()} onclick={()=>run(value)}>{value}</Button.Root>{/each}</div>
 {#if result}<div class="result" role="status"><p>{$t('退出码')}：{result.code}</p><pre>{result.stdout}{result.stderr}</pre></div>{/if}
 {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 </div>
</section>
<details class="settings-group cli-guide">
 <summary>{$t('常用命令')}</summary>
 <div class="settings-fields cli-body"><p class="help">{$t('目录需先在飞牛中授权给 SAG Wiki。')}</p>
 <pre><code>sag add /vol1/1000/File --types md,txt,html,py,js,ts,json,yaml
sag check /vol1/1000/File --json
sag "{$t('这些资料如何说明项目的验收要求？')}" /vol1/1000/File -m 5
sag -F "keyword" /vol1/1000/File --json
sag config --if-needed
sag service status
sag install codex
sag install claude-code
sag install opencode</code></pre>
 <p class="help">{$t('service stop 和 restart 只影响当前用户的共享服务；下一次操作会自动启动服务。')}</p></div>
</details>
<style>
 .cli-body{padding:17px}
 .cli-body pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.65 var(--font-mono,monospace);background:var(--color-panel);border:1px solid var(--color-rule);border-radius:7px;padding:12px;margin:12px 0}
 .cli-body .help{font-size:var(--text-caption);color:var(--color-secondary);line-height:1.6;margin:0}
 .cli-body .field-label{display:block;margin:0 0 16px}
 .actions{display:flex;flex-wrap:wrap;gap:8px}
 .result{margin-top:20px;padding-top:16px;border-top:1px solid var(--color-rule)}
 .result p{font-size:var(--text-caption);color:var(--color-secondary);margin:0}
 .result pre{max-height:280px;overflow:auto;margin-bottom:0}
 .form-error{margin:12px 0 0}
 .cli-install summary,.cli-guide summary{padding:15px 17px;cursor:pointer;font-size:var(--text-ui);font-weight:500}
 .cli-install[open] summary,.cli-guide[open] summary{border-bottom:1px solid var(--color-rule)}
</style>
