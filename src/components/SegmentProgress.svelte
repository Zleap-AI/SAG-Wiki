<script lang="ts">
 import {t} from '../lib/i18n';
 import type {SegmentProgress} from '../lib/types';
 export let progress:SegmentProgress;
</script>
<span class="segment-progress">
 <progress value={progress.completed} max={progress.total} aria-label={$t('当前文档已完成分段')}></progress>
 <span>{$t('已完成 {value0}/{value1} 段',{value0:progress.completed,value1:progress.total})}</span>
 {#each progress.active as part (part.segment_id)}
  <span class="segment-row">
   <span>{$t('正在处理第 {value0}/{value1} 段',{value0:part.segment_id,value1:progress.total})} · {$t('第 {value0}–{value1} 页',{value0:part.start_page,value1:part.end_page})}</span>
   <span>{part.engine} · {$t(part.phase)}</span>
  </span>
 {/each}
 {#if progress.waiting_local}<span>{$t('等待本地解析：{value0} 段',{value0:progress.waiting_local})}</span>{/if}
 {#if progress.failed||progress.cancelled}<span>{$t('失败 {value0} 段 · 已取消 {value1} 段',{value0:progress.failed,value1:progress.cancelled})}</span>{/if}
</span>
<style>
 .segment-progress{display:grid;gap:6px;margin:8px 0;font-size:11px;color:var(--color-secondary)}
 progress{appearance:none;width:100%;height:4px;border:0;border-radius:2px;overflow:hidden;background:var(--color-rule)}
 progress::-webkit-progress-bar{background:var(--color-rule)}progress::-webkit-progress-value{background:var(--color-ink)}progress::-moz-progress-bar{background:var(--color-ink)}
 .segment-row{display:grid;gap:2px;color:var(--color-ink)}.segment-row>span:last-child{color:var(--color-secondary)}
</style>
