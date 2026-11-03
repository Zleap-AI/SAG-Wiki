<script lang="ts">
 import {onDestroy,tick} from 'svelte';
 import {listen,type UnlistenFn} from '@tauri-apps/api/event';
 import {api} from '../lib/api';
 import {t,language} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {foldSource,type FoldSource,type FoldResult,type FoldProgress} from '../lib/document-fold';
 import {textfoldData} from '../lib/textfold-reader';
 export let editor:HTMLElement;
 export let path='';
 export let active=false;
 let source:FoldSource|null=null,result:FoldResult|null=null,open=false,busy=false,error='',job='';
 let frame:HTMLIFrameElement,ready=false,level=3,scrollTop=0;
 let percent=0,stage='preparing';
 let progressSubscription:{id:string;timer:ReturnType<typeof setInterval>;stop?:UnlistenFn}|null=null;
 const names=['全文','简述','关键句','要点','阅读主题'];
 function stopProgress(id:string){if(progressSubscription?.id===id){clearInterval(progressSubscription.timer);progressSubscription.stop?.();progressSubscription=null;}}
 async function start(){
  if(open){await close();return;}
  open=true;ready=false;error='';scrollTop=editor.closest('.document-sheet')!.scrollTop;
  if(busy)return;
  const current=foldSource(editor,path.split(/[\\/]/).at(-1));
  if(source?.html===current.html&&result){active=true;return;}
  source=current;result=null;error='';busy=true;percent=1;stage='preparing';
  if(!source.document.sections.some(s=>s.units.length||s.tables.length)){error=$t('文档没有可折叠的正文');busy=false;return;}
  const id=crypto.randomUUID();job=id;
  progressSubscription={id,timer:setInterval(()=>{
   if(percent<90)percent=Math.min(90,percent+(percent<30?1:percent<60?.5:.2));
  },2000)};
  try{
   const stop=await listen<FoldProgress>('document-fold-progress',({payload})=>{
    if(job!==id||payload.id!==id)return;
    percent=Math.max(percent,Math.min(payload.percent,99));stage=payload.stage;
   });
   if(job!==id){stop();return;}
   progressSubscription!.stop=stop;
   const folded=await api<FoldResult>('fold_document',{id,document:source.document});
   if(job!==id)return;
   if(JSON.stringify(foldSource(editor,path.split(/[\\/]/).at(-1)).document)!==JSON.stringify(source.document))throw new Error($t('原文已修改，请重新折叠。'));
   result=folded;percent=100;scrollTop=editor.closest('.document-sheet')!.scrollTop;level=3;active=open;
  }catch(e){if(job===id)error=$t(String(e));}finally{stopProgress(id);if(job===id){busy=false;job='';}}
 }
 function initialize(){
  const style=getComputedStyle(editor),palette:Record<string,string>={};
  for(const [target,from] of Object.entries({'--surface':'paper','--card':'paper','--ink':'ink','--ink-2':'secondary','--muted':'secondary','--line':'rule','--line-2':'panel'}))palette[target]=style.getPropertyValue(`--color-${from}`).trim();
  palette['--sans']=style.fontFamily;palette['--serif']=style.fontFamily;
  frame.contentWindow!.postMessage({type:'sag-fold-init',document:textfoldData(source!,result!,'sag:'+path),ui:$language==='zh-CN'?'zh':'en',level,palette,dark:!!editor.closest('[data-theme="dark"]')},location.origin);
 }
 function receive(event:MessageEvent){
  if(!active||event.source!==frame?.contentWindow||event.origin!==location.origin)return;
  if(event.data.type==='sag-fold-ready'){ready=true;error='';}
  else if(event.data.type==='sag-fold-level')level=event.data.level;
  else if(event.data.type==='sag-fold-link')window.dispatchEvent(new CustomEvent('open-external-link',{detail:event.data.href}));
  else if(event.data.type==='sag-fold-error'){error=event.data.message;active=false;ready=false;}
 }
 async function close(){
  const restore=active;active=false;open=false;ready=false;
  await tick();if(restore)editor.closest('.document-sheet')!.scrollTop=scrollTop;
 }
 async function cancel(){
  const id=job;job='';stopProgress(id);busy=false;active=false;ready=false;error=$t('折叠已停止');
  try{await api('cancel_query',{id});}catch(e){error=$t(String(e));}
 }
 onDestroy(()=>{if(job){const id=job;job='';stopProgress(id);void api('cancel_query',{id}).catch(e=>console.error(e));}});
</script>

<svelte:window onmessage={receive}/>
<div class="document-fold-controls">
 <button type="button" class="document-fold-toggle" aria-expanded={open} onclick={start}>{@html icon(open?'close':'minimize')}<span>{$t(open?'关闭':'文档折叠')}</span></button>
</div>
{#if open&&!ready}
 <div class="fold-panel" aria-label={$t('阅读层级')}>
  {#if busy||active}
   <div class="fold-progress" role="status" aria-live="polite">
    <div><span>{$t(active?'正在准备阅读器…':stage==='preparing'?'正在检查缓存…':stage==='summarizing'?'正在汇总章节…':'正在生成阅读层级…')}</span><strong>{Math.floor(percent)}%</strong></div>
    <div class="fold-progress-track"><progress max="100" value={percent} aria-label={$t('折叠进度')}></progress>{#if busy}<button type="button" class="fold-stop" aria-label={$t('停止折叠')} title={$t('停止折叠')} onclick={cancel}>{@html icon('close')}</button>{/if}</div>
   </div>
  {/if}
  {#if error}<p role="alert">{error} <button type="button" onclick={()=>{open=false;void start();}}>{$t('重试')}</button></p>{/if}
  <div class="fold-levels" role="group" aria-label={$t('阅读层级')}>
   {#each names as name,index}<button type="button" disabled aria-pressed={(active?4-level:0)===index}>{$t(name)}</button>{/each}
  </div>
 </div>
{/if}
{#if active}<div class="fold-reader" class:ready><iframe bind:this={frame} title={$t('阅读层级')} src="/textfold/reader.html" onload={initialize}></iframe></div>{/if}
{#if open&&ready&&result?.warnings.length}<p class="fold-warning" role="status">{$t('部分段落使用原句，点击段落可展开。')}</p>{/if}

<style>
 .document-fold-controls{position:absolute;right:24px;bottom:80px;z-index:4;font-size:var(--text-ui)}
 .document-fold-toggle{display:flex;align-items:center;gap:8px;min-height:42px;padding:10px 16px;border:1px solid var(--color-rule);border-radius:22px;background:var(--color-paper);color:var(--color-ink);box-shadow:0 4px 18px var(--color-shadow)}
 button{appearance:none;-webkit-appearance:none;cursor:pointer;font:inherit;color:var(--color-ink);text-shadow:none}
 button:active{transform:scale(.97)}
 button:disabled{cursor:wait;opacity:1}
 .fold-panel{position:absolute;left:0;right:0;bottom:18px;width:max-content;margin-inline:auto;z-index:3;max-width:calc(100% - 220px);padding:5px;border:1px solid var(--color-rule);border-radius:15px;background:var(--color-panel);box-shadow:0 4px 18px var(--color-shadow);font-size:var(--text-ui)}
 .fold-panel :is(p,.fold-progress){position:absolute;bottom:100%;left:0;right:0;min-width:240px;max-width:420px;padding:10px 14px;background:var(--color-paper);border:1px solid var(--color-rule);border-radius:10px;font-size:13px;line-height:1.6;color:var(--color-ink);margin:0 auto 10px;text-align:center}
 .fold-progress>div:not(.fold-progress-track){display:flex;justify-content:space-between;gap:16px}
 .fold-progress strong{font-variant-numeric:tabular-nums;font-weight:600}
 .fold-progress-track{display:flex;align-items:center;gap:8px;margin-top:8px}
 .fold-progress .fold-stop{display:flex;align-items:center;justify-content:center;flex-shrink:0;width:24px;height:24px;padding:3px;border:0;border-radius:5px;background:transparent;color:var(--color-secondary)}
 .fold-progress .fold-stop:hover{background:var(--color-hover);color:var(--color-ink)}
 .fold-progress .fold-stop :global(svg){width:16px;height:16px}
 .fold-progress progress{display:block;flex:1;min-width:0;width:100%;height:5px;margin-top:0;border:0;border-radius:5px;overflow:hidden;appearance:none;background:var(--color-rule);accent-color:var(--color-ink)}
 .fold-progress progress::-webkit-progress-bar{background:var(--color-rule)}
 .fold-progress progress::-webkit-progress-value{background:var(--color-ink)}
 .fold-progress progress::-moz-progress-bar{background:var(--color-ink)}
 .fold-panel p button{border:0;background:transparent;text-decoration:underline}
 .fold-levels{display:flex;gap:3px}
 .fold-levels button{white-space:nowrap;border:0;border-radius:10px;padding:8px 15px;background:transparent;color:var(--color-secondary);transition:background-color 160ms,color 160ms,transform 160ms}
 .fold-levels button[aria-pressed=true]{background:var(--color-paper);color:var(--color-ink);box-shadow:0 1px 4px var(--color-shadow);font-weight:600}
 .fold-reader{position:absolute;inset:0;z-index:2;background:var(--color-paper)}
 .fold-reader iframe{border:0;width:100%;height:100%;display:block}
 .fold-reader:not(.ready) iframe{visibility:hidden}
 .fold-warning{position:absolute;bottom:104px;left:24px;right:120px;z-index:3;text-align:center;font-size:12px;color:var(--color-secondary);pointer-events:none}
 @media(max-width:1080px){.document-fold-controls{right:20px}.fold-levels button{padding:8px 10px}}
 @media(max-width:640px){.fold-panel{left:16px;right:auto;max-width:calc(100% - 140px)}.fold-levels{flex-wrap:wrap}}
 @media(prefers-reduced-motion:reduce){.fold-levels button{transition:none}}
</style>
