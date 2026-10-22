<script lang="ts">
 import {Button,Collapsible} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {answerHtml} from '../lib/answer-html';
 import type {Candidate} from '../lib/types';
 export let text:string;
 export let running:boolean;
 export let error:string;
 export let evidence:Candidate[];
 export let onSource:(candidate:Candidate)=>void;
 export let onCopy:(text:string)=>void;
 let open=true;
</script>

<section class="search-answer" aria-label={$t('搜索总结')} aria-busy={running}>
 <Collapsible.Root bind:open>
 <div class="search-answer-header">
 <Collapsible.Trigger class="search-answer-heading" aria-label={open?$t('折叠搜索总结'):$t('展开搜索总结')}>
  <strong>{$t('搜索总结')}</strong><span class="search-answer-state">{running?$t('正在总结…'):$t('基于 {value0} 条参考结果',{value0:evidence.length})}</span><span class="search-answer-chevron" aria-hidden="true">{@html icon('chevron')}</span>
 </Collapsible.Trigger>
 <Button.Root class="icon-button search-answer-copy" aria-label={$t('复制总结')} title={$t('复制总结')} disabled={running||!text} onclick={()=>onCopy(text)}>{@html icon('copy')}</Button.Root>
 </div>
 <Collapsible.Content class="search-answer-content">
 {#if open}
 {#if text}
  {#await import('../lib/markdown')}
   <p class="search-answer-pending">{text}</p>
  {:then markdown}
   <div class="answer-text" role="presentation" on:click={event=>{const target=(event.target as HTMLElement).closest<HTMLElement>('[data-citation]');if(target)onSource(evidence[Number(target.dataset.citation)])}} use:answerHtml={markdown.renderAnswer(text,evidence).html}></div>
  {/await}
 {:else if running}<p class="muted" role="status">{$t('正在根据问题整理搜索结果…')}</p>{/if}
 {#if error}<p class="form-error" role="alert">{error}</p>{/if}
 {/if}
 </Collapsible.Content>
 </Collapsible.Root>
</section>

<style>
 .search-answer{flex-shrink:0;border:1px solid var(--color-rule);border-radius:12px;margin-top:16px;background:var(--color-panel);overflow:hidden}
 .search-answer-header{position:relative}
 .search-answer-header :global(.search-answer-copy){position:absolute;right:48px;top:50%;transform:translateY(-50%);width:28px;height:28px;padding:6px;color:var(--color-secondary)}
 .search-answer :global(.search-answer-heading){display:flex;align-items:center;gap:12px;padding:16px 20px;width:100%;border-radius:0;background:transparent;text-align:left;font-size:var(--text-ui)}
 .search-answer :global(.search-answer-heading:hover){background:var(--color-hover)}
 .search-answer :global(.search-answer-heading strong){font-weight:550}
 .search-answer-state{margin-left:auto;margin-right:40px;font-size:var(--text-caption);color:var(--color-secondary)}
 .search-answer-chevron{display:flex;color:var(--color-secondary)}
 .search-answer-chevron :global(svg){width:16px;height:16px}
 .search-answer :global(.search-answer-heading[data-state=open] .search-answer-chevron){transform:rotate(90deg)}
 .search-answer :global(.search-answer-content){padding:0 20px 16px;max-height:min(26vh,224px);overflow:auto}
 .search-answer :global(.answer-text){font-size:var(--text-reading);line-height:1.75}
 .search-answer :global(.answer-text>:first-child){margin-top:0}
 .search-answer :global(.answer-text>:last-child){margin-bottom:0}
 .search-answer-pending{white-space:pre-wrap}
 .search-answer .form-error{margin:8px 0 0;font-size:var(--text-caption)}
</style>
