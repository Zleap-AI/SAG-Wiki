<script lang="ts">
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {durationLabel} from '../lib/domain';
 import {toolArgs as args,toolKind as kind,toolLabel} from '../lib/tool-label';
 import type {ToolCallRecord} from '../lib/types';
 type State='running'|'complete'|'stopped'|'failed';
 export let calls:ToolCallRecord[];
 export let states:State[];
 let open=false;
 $:running=states.includes('running');
 $:current=calls[states.indexOf('running')];
 $:failures=states.filter(state=>state==='failed').length;
 $:searches=calls.filter(call=>kind(call)==='search').length;
 $:reads=new Set(calls.filter(call=>kind(call)==='read').map(call=>String(args(call).path??call.id))).size;
 $:others=calls.length-searches-reads-calls.filter(call=>kind(call)==='list').length;
 $:total=calls.reduce((sum,call)=>sum+(call.elapsed_ms??0),0);
 $:summary=[searches?$t('检索 {value0} 次',{value0:searches}):'',reads?$t('阅读 {value0} 份资料',{value0:reads}):'',others>0?$t('调用工具 {value0} 次',{value0:others}):''].filter(Boolean).join(' · ')||$t('查阅资料');
</script>

<details class="tool-group" class:running bind:open>
 <summary>
  <span class="tool-group-mark" aria-hidden="true">{#if running}<span class="tool-group-spinner"></span>{:else}{@html icon(failures?'alert':'check')}{/if}</span>
  {#if running}<span class="tool-group-title shimmer" role="status">{current?toolLabel(current,$t):$t('正在查阅资料')}</span>
  {:else}<span class="tool-group-title">{summary}</span>{#if failures}<span class="tool-group-note">{$t('{value0} 次失败',{value0:failures})}</span>{/if}{#if total}<span class="tool-group-time">{total<1000?total+' ms':durationLabel(total)}</span>{/if}{/if}
  <span class="tool-group-chevron" aria-hidden="true">{@html icon('chevron')}</span>
 </summary>
 <div class="tool-group-steps"><slot/></div>
</details>

<style>
 .tool-group{margin:0 0 16px}
 .tool-group>summary{display:inline-flex;align-items:center;gap:8px;max-width:100%;min-height:30px;padding:4px 10px 4px 6px;border-radius:999px;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.5;list-style:none;cursor:pointer;transition:background-color .15s,color .15s}
 .tool-group>summary::-webkit-details-marker{display:none}
 .tool-group>summary:hover{background:var(--color-hover);color:var(--color-ink)}
 .tool-group>summary:focus-visible{outline:2px solid var(--color-focus);outline-offset:1px}
 .tool-group-mark{display:grid;place-items:center;width:20px;height:20px;flex-shrink:0;border-radius:50%;background:var(--color-hover);color:var(--color-secondary)}
 .tool-group-mark :global(.icon){width:12px;height:12px}
 .tool-group-spinner{width:10px;height:10px;border:1.5px solid var(--color-rule);border-top-color:var(--color-ink);border-radius:50%;animation:tool-spin .8s linear infinite}
 @keyframes tool-spin{to{transform:rotate(360deg)}}
 .tool-group-title{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .tool-group-note{color:var(--color-ink);font-weight:500;white-space:nowrap}
 .tool-group-time{color:var(--color-muted);font-variant-numeric:tabular-nums;white-space:nowrap}
 .tool-group-time::before,.tool-group-note::before{content:'·';margin-right:8px;color:var(--color-muted)}
 .tool-group-chevron{display:flex;color:var(--color-muted);transition:transform .15s}
 .tool-group-chevron :global(.icon){width:12px;height:12px}
 .tool-group[open] .tool-group-chevron{transform:rotate(90deg)}
 /* The running step reads as live text: a soft highlight sweeps across it. */
 .shimmer{background:linear-gradient(90deg,var(--color-secondary) 0%,var(--color-secondary) 40%,var(--color-ink) 50%,var(--color-secondary) 60%,var(--color-secondary) 100%);background-size:250% 100%;-webkit-background-clip:text;background-clip:text;color:transparent;animation:tool-shimmer 1.8s linear infinite}
 @keyframes tool-shimmer{from{background-position:100% 0}to{background-position:-50% 0}}
 @media(prefers-reduced-motion:reduce){.shimmer,.tool-group-spinner{animation:none}.shimmer{color:var(--color-secondary)}}
 .tool-group-steps{position:relative;margin:6px 0 0 15px;padding:2px 0 2px 14px;border-left:1px solid var(--color-rule)}
</style>
