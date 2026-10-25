<script lang="ts">
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {displayPath,basename} from '../lib/domain';
 import {wordDiff} from '../lib/word-diff';
 import Timestamp from './Timestamp.svelte';
 import type {NoteDetail,NoteSourceStatus,NoteSourceState} from '../lib/types';

 export let note:NoteDetail;
 export let checking=false;
 export let onRecheck:()=>void|Promise<void>;
 export let onOpenSource:(status:NoteSourceStatus)=>void;
 export let onReask:(()=>void)|undefined=undefined;

 let compare:NoteSourceStatus|null=null;
 let expanded=false;

 const labels:Record<NoteSourceState,string>={intact:'一致',moved:'位置已更新',changed:'原文已改',missing:'来源缺失',unverified:'未核对'};
 export function stateLabel(state:NoteSourceState){return labels[state];}
 $:attention=note.sources.filter(s=>s.state==='changed'||s.state==='missing'||s.state==='unverified');
 $:moved=note.sources.filter(s=>s.state==='moved');
 $:summaryText=note.summary.parse_error?$t('笔记格式无法解析：{value0}',{value0:$t(note.summary.parse_error)})
  :!note.sources.length?$t('此笔记没有引用，不需要核对。')
  :attention.length?$t('{value0} 条引用需要关注',{value0:attention.length})
  :moved.length?$t('引用位置已自动更新，内容未变。')
  :$t('所有引用与原文一致。');
 function shownPath(status:NoteSourceStatus){return status.source.display_path||status.source.path;}
 function lines(status:NoteSourceStatus){const c=status.current;return c&&(c.start_line!==status.source.start_line||c.end_line!==status.source.end_line)?$t('第 {value0}–{value1} 行（保存时 {value2}–{value3}）',{value0:c.start_line,value1:c.end_line,value2:status.source.start_line,value3:status.source.end_line}):$t('第 {value0}–{value1} 行',{value0:status.source.start_line,value1:status.source.end_line});}
</script>

<section class="note-status" class:attention={note.summary.state==='changed'||note.summary.state==='missing'||note.summary.state==='unverified'} aria-label={$t('笔记状态')}>
 <div class="note-status-head">
  <div class="note-status-question"><strong>{note.summary.question}</strong><small>{#if note.summary.asked_at}<Timestamp value={note.summary.asked_at}/>{/if}{#if note.summary.model} · {note.summary.model}{/if}{#if note.summary.scope?.length} · {note.summary.scope.map(basename).join('、')}{/if}</small></div>
  <div class="note-status-actions">
   {#if onReask}<Button.Root type="button" class="outline small" onclick={onReask}>{@html icon('chat')}{$t('重新提问')}</Button.Root>{/if}
   <Button.Root type="button" class="outline small" disabled={checking} onclick={onRecheck}>{@html icon('refresh')}{$t(checking?'正在检查…':'重新检查')}</Button.Root>
  </div>
 </div>
 <div class="note-status-line" role="status">
  <span class="note-state-dot" data-state={note.summary.state}></span>
  <span>{summaryText}</span>
  {#if note.summary.checked_at}<small>· {$t('检查于')} <Timestamp value={note.summary.checked_at}/></small>{/if}
  {#if note.sources.length}<Button.Root type="button" class="note-status-toggle" aria-expanded={expanded} onclick={()=>expanded=!expanded}>{$t(expanded?'收起引用':'查看引用')}{@html icon(expanded?'down':'chevron')}</Button.Root>{/if}
 </div>
 {#if expanded}
  <ol class="note-sources">
   {#each note.sources as status(status.source.id)}
    <li class="note-source" data-state={status.state}>
     <span class="note-source-id">[{status.source.id}]</span>
     <div class="note-source-body">
      <div class="note-source-title"><span class="note-state-dot" data-state={status.state}></span><strong>{basename(shownPath(status))}</strong><span class="note-source-state">{$t(labels[status.state])}</span>{#if status.similarity!=null&&status.state==='changed'}<small>{$t('相似度 {value0}',{value0:Math.round(status.similarity*100)+'%'})}</small>{/if}</div>
      <small class="note-source-path" title={shownPath(status)}>{displayPath(shownPath(status))} · {lines(status)}</small>
      {#if status.reason}<small class="note-source-reason">{$t(status.reason)}</small>{/if}
     </div>
     <div class="note-source-actions">
      {#if status.state==='changed'}<Button.Root type="button" class="small outline" onclick={()=>compare=status}>{$t('查看对照')}</Button.Root>{/if}
      {#if status.state==='missing'||status.state==='unverified'}<Button.Root type="button" class="small outline" onclick={()=>compare=status}>{$t('保存时原文')}</Button.Root>{/if}
      {#if status.state!=='missing'}<Button.Root type="button" class="small outline" onclick={()=>onOpenSource(status)}>{$t('打开原文')}</Button.Root>{/if}
     </div>
    </li>
   {/each}
  </ol>
 {/if}
</section>

{#if compare}
 <div class="note-compare" role="dialog" aria-label={$t('引用对照')}>
  <div class="note-compare-head"><strong>[{compare.source.id}] {basename(shownPath(compare))}</strong><Button.Root type="button" class="icon-button" aria-label={$t('关闭对照')} onclick={()=>compare=null}>{@html icon('close')}</Button.Root></div>
  {#if compare.state==='changed'&&compare.current_text!=null}
   <p class="note-compare-hint">{$t('删除线为保存时的内容，高亮为当前内容。')}</p>
   <pre class="note-compare-diff">{#each wordDiff(compare.source.text,compare.current_text) as part}{#if part.kind==='same'}{part.text}{:else if part.kind==='removed'}<del>{part.text}</del>{:else}<ins>{part.text}</ins>{/if}{/each}</pre>
  {:else}
   <p class="note-compare-hint">{$t('保存时引用的原文：')}</p>
   <pre class="note-compare-diff">{compare.source.text}</pre>
  {/if}
 </div>
{/if}

<style>
 .note-status{margin:0 24px 8px;padding:10px 14px;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-panel);font-size:var(--text-ui);line-height:1.6}
 .note-status.attention{border-color:color-mix(in srgb,#d9822b 55%,var(--color-rule))}
 .note-status-head{display:flex;align-items:flex-start;justify-content:space-between;gap:12px}
 .note-status-question{display:flex;flex-direction:column;gap:2px;min-width:0}
 .note-status-question strong{font-weight:550;overflow-wrap:anywhere}
 .note-status-question small,.note-status-line small{color:var(--color-muted);font-size:var(--text-caption)}
 .note-status-actions{display:flex;gap:6px;flex-shrink:0}
 .note-status-actions :global(button){display:inline-flex;align-items:center;gap:5px;min-height:28px;padding:4px 10px;border-radius:7px;font-size:var(--text-caption)}
 .note-status-actions :global(.icon){width:13px;height:13px}
 .note-status-line{display:flex;align-items:center;gap:8px;margin-top:8px;color:var(--color-secondary)}
 .note-status-line :global(.note-status-toggle){margin-left:auto;display:inline-flex;align-items:center;gap:3px;padding:2px 6px;font-size:var(--text-caption);color:var(--color-secondary)}
 .note-status-line :global(.note-status-toggle .icon){width:12px;height:12px}
 .note-state-dot{width:8px;height:8px;border-radius:50%;flex-shrink:0;background:var(--color-muted)}
 .note-state-dot[data-state=intact],.note-state-dot[data-state=moved]{background:#3f9d63}
 .note-state-dot[data-state=changed]{background:#d9822b}
 .note-state-dot[data-state=missing]{background:#d14343}
 .note-state-dot[data-state=unverified]{background:transparent;border:1.5px dashed var(--color-muted)}
 .note-sources{list-style:none;margin:10px 0 0;padding:8px 0 0;border-top:1px solid var(--color-rule);display:flex;flex-direction:column;gap:8px}
 .note-source{display:flex;align-items:flex-start;gap:8px}
 .note-source-id{font-variant-numeric:tabular-nums;color:var(--color-secondary);flex-shrink:0;padding-top:1px}
 .note-source-body{flex:1;min-width:0;display:flex;flex-direction:column;gap:1px}
 .note-source-title{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
 .note-source-title strong{font-weight:550}
 .note-source-state{font-size:var(--text-caption);color:var(--color-secondary)}
 .note-source-title small,.note-source-path,.note-source-reason{font-size:var(--text-caption);color:var(--color-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .note-source-reason{color:#d14343;white-space:normal}
 .note-source-actions{display:flex;gap:4px;flex-shrink:0}
 .note-source-actions :global(button){min-height:26px;padding:3px 8px;border-radius:6px;font-size:var(--text-caption)}
 .note-compare{margin:0 24px 8px;padding:10px 14px;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-paper)}
 .note-compare-head{display:flex;align-items:center;justify-content:space-between;gap:8px}
 .note-compare-hint{margin:4px 0 8px;font-size:var(--text-caption);color:var(--color-muted)}
 .note-compare-diff{margin:0;max-height:40vh;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;font-family:var(--font-mono);font-size:var(--text-caption);line-height:1.7;padding:10px 12px;border-radius:8px;background:var(--color-panel)}
 .note-compare-diff del{background:color-mix(in srgb,#d14343 18%,transparent);text-decoration:line-through;text-decoration-color:color-mix(in srgb,#d14343 70%,transparent)}
 .note-compare-diff ins{background:color-mix(in srgb,#3f9d63 22%,transparent);text-decoration:none}
</style>
