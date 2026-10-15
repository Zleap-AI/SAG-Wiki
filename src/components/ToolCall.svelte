<script lang="ts">
 import {t} from '../lib/i18n';
 import{icon}from '../lib/icons';import type{ToolCallRecord}from '../lib/types';import{toolResultText}from '../lib/tool-result';
 export let call:ToolCallRecord;export let state:'running'|'complete'|'stopped'|'failed'='complete';export let stage='';export let scope=$t("全部文件");
 let expanded=false,rawOpen=false;
 $:phase=call.denied?'denied':state==='stopped'?'stopped':state==='failed'||call.is_error?'failed':state;
 $:sagData=call.server_name==='SAG'&&call.result&&typeof call.result==='object'?(call.result as {structuredContent?:{sources?:unknown[];omitted_sources?:number}}).structuredContent:undefined;
 $:emptySearch=call.tool_name==='search'&&Array.isArray(sagData?.sources)&&sagData.sources.length===0&&sagData.omitted_sources===0;
 $:label=phase==='denied'?$t("已拒绝"):phase==='running'?$t("正在调用"):phase==='stopped'?$t("已停止"):phase==='failed'?$t("调用失败"):emptySearch?$t("未找到相关内容"):$t("已完成");
 $:resultText=call.result==null?'':toolResultText(call.result,$t,call.server_name==='SAG'?call.tool_name??undefined:undefined);
</script>
<details class="tool-call" class:mcp-call={!!call.server_name} class:running={phase==='running'} bind:open={expanded}>
 <summary><span class="tool-icon">{@html icon(call.server_name?'plug':'search')}</span><strong>{call.server_name?`${call.server_name} · ${call.tool_name}`:$t("SAG2 文件检索")}</strong><span class="tool-state">{label}</span><span class="tool-chevron">{@html icon('chevron')}</span></summary>
 <div class="tool-call-body">
  {#if call.server_name}
   <section class="tool-detail"><h4>{$t("调用参数")}</h4><pre class="tool-data">{call.query}</pre></section>
   {#if call.error}<section class="tool-detail tool-error"><h4>{phase==='denied'?$t("未执行原因"):phase==='stopped'?$t("停止原因"):$t("失败原因")}</h4><pre class="tool-data">{$t(call.error)}</pre></section>
   {:else if phase==='failed'}<p class="tool-error">{$t("此历史调用未保存具体失败原因。")}</p>{/if}
   {#if call.result!=null}<section class="tool-detail"><h4>{$t("返回结果")}</h4><pre class="tool-data tool-output">{resultText}</pre><details class="tool-raw" bind:open={rawOpen}><summary>{$t("原始 JSON")}</summary>{#if rawOpen}<pre class="tool-data">{JSON.stringify(call.result,null,2)}</pre>{/if}</details></section>
   {:else if phase==='complete'}<p class="tool-history-note">{$t("此历史调用未保存返回结果。")}</p>{/if}
  {:else}<p class="tool-query">{call.query}</p><p class="tool-scope">{$t("范围：{value0}",{value0:scope})}</p>{/if}
  {#if phase==='running'}<p class="tool-step" role="status">{$t(stage)}</p>{:else if !call.server_name&&phase==='complete'}<p class="tool-result">{$t("返回 {value0} 个原文片段",{value0:call.source_count})}</p>{/if}
 </div>
</details>
