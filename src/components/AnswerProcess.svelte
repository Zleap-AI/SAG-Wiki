<script lang="ts">
 import {onDestroy} from 'svelte';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 export let hasContent=false;
 export let answering=false;
 export let running=false;
 export let startedAt:number|null|undefined=undefined;
 export let elapsedMs:number|null|undefined=undefined;
 let open=true;
 let measuredMs:number|null=null;
 let timer:ReturnType<typeof setInterval>|undefined;
 $: open=!answering;
 $: {
  clearInterval(timer);
  if(hasContent&&running&&answering){
   const start=startedAt??Date.now();
   const update=()=>{measuredMs=Date.now()-start};
   update();timer=setInterval(update,1000);
  }
 }
 onDestroy(()=>clearInterval(timer));
 $: duration=elapsedMs??measuredMs;
 $: seconds=Math.floor((duration??0)/1000);
 $: label=duration===null?$t('处理过程'):$t('用时 {value0}',{value0:seconds<60?$t('{value0}秒',{value0:seconds}):$t('{value0}分钟{value1}秒',{value0:Math.floor(seconds/60),value1:seconds%60})});
</script>

{#if hasContent}
 <details class="answer-process" class:collapsible={answering} bind:open>
  <summary class="answer-process-summary"><span>{label}</span><span class="process-chevron" aria-hidden="true">{@html icon('chevron')}</span></summary>
  <div class="answer-process-content"><slot/></div>
 </details>
{:else}<slot/>{/if}

<style>
 .answer-process{margin:0 0 16px}
 .answer-process-summary{display:inline-flex;align-items:center;gap:6px;min-height:28px;padding:3px 10px;border-radius:999px;background:var(--color-hover);list-style:none;color:var(--color-secondary);font-size:var(--text-caption);line-height:1.6;cursor:pointer;transition:background-color .15s,color .15s}
 .answer-process-summary:hover{background:var(--color-selected);color:var(--color-ink)}
 .answer-process-summary::-webkit-details-marker{display:none}
 .answer-process-summary:focus-visible{outline:2px solid var(--color-focus);outline-offset:4px}
 .process-chevron{display:flex;align-items:center}
 .process-chevron :global(.icon){width:12px;height:12px}
 .answer-process[open] .process-chevron{transform:rotate(90deg)}
 .answer-process-content{padding-top:12px}
 .answer-process:not(.collapsible),.answer-process:not(.collapsible)>.answer-process-content{display:contents}
 .answer-process:not(.collapsible)>.answer-process-summary{display:none}
</style>
