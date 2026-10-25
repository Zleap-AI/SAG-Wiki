<script lang="ts">
 import {Button} from 'bits-ui';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {isHtml,isMarkdown,readonlyHtml} from '../lib/domain';
 import {citationTerms,documentMatchIndex} from '../lib/search-highlights';
 import {legacyPreview} from '../lib/preview-mode';
 import type {Candidate,Document} from '../lib/types';
 import HtmlDocument from './HtmlDocument.svelte';
 import HighlightedDocument from './HighlightedDocument.svelte';
 export let source:Candidate;
 export let onOpen:(terms:string[])=>void;
 async function load(candidate:Candidate){
  const document=await api<Document>('read_document',{path:candidate.path});
  if(isMarkdown(candidate.path)&&!legacyPreview()){
   const [{markdownModel,candidateRanges,modelCitationTerms},{default:Reader}]=await Promise.all([import('../lib/document-model'),import('./MarkdownReader.svelte')]);
   const model=markdownModel(document),ranges=candidateRanges(model,candidate);
   return {document,terms:await modelCitationTerms(model,candidate,ranges),initialMatch:0,html:'',reader:{Reader,model,ranges}};
  }
  const terms=await citationTerms(candidate,document.body);
  const initialMatch=await documentMatchIndex(document.body,candidate.path,terms,candidate);
  const html=isMarkdown(candidate.path)?(await import('../lib/markdown')).renderMarkdown(document.body):isHtml(candidate.path)?'':readonlyHtml(candidate.path,document.body);
  return {document,terms,initialMatch,html,reader:null};
 }
 $:preview=load(source);
</script>

{#await preview}
 <div class="context-body"><p role="status">{$t('正在读取文件…')}</p></div>
{:then result}
 <div class="context-body citation-body">
  {#if result.document.hash!==source.hash}<p class="source-warning">{@html icon('info')}{$t('原文件已更新，以下显示当前内容。')}</p>{/if}
  <div class="citation-document">
   {#if result.reader}<svelte:component this={result.reader.Reader} model={result.reader.model} terms={result.terms} target={result.reader.ranges[0].from} within={result.reader.ranges} path={source.path} label="citation"/>
   {:else if isHtml(source.path)}<HtmlDocument body={result.document.body} terms={result.terms} initialMatch={result.initialMatch}/>
   {:else}<HighlightedDocument html={result.html} terms={result.terms} path={source.path} initialMatch={result.initialMatch}/>{/if}
  </div>
 </div>
 <div class="panel-footer"><span class="citation-hint">{$t('高亮为回答引用的段落')}</span><Button.Root class="primary citation-open" onclick={()=>onOpen(result.terms)}>{$t('打开当前文档')}{@html icon('forward')}</Button.Root></div>
{:catch error}
 <div class="context-body"><p class="form-error">{$t(String(error))}</p></div>
{/await}

<style>
 :global(.source-dialog .context-body.citation-body){display:flex;flex-direction:column;overflow:hidden;padding:0}
 .source-warning{display:flex;align-items:center;gap:8px;margin:12px 20px 0;padding:8px 12px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-panel);font-size:var(--text-caption);line-height:1.5;color:var(--color-secondary)}
 .source-warning :global(.icon){width:14px;height:14px;flex-shrink:0}
 .citation-document{display:flex;flex-direction:column;flex:1;min-height:0}
 /* Match navigation becomes a quiet toolbar under the header. */
 .citation-document :global(.match-navigation){justify-content:space-between;flex-shrink:0;margin:0;padding:8px 14px 8px 20px;border-bottom:1px solid var(--color-rule);background:var(--color-panel)}
 .citation-document :global(.search-preview-body){flex:1;min-height:0;overflow-x:hidden;overflow-y:auto;overscroll-behavior:contain;padding:22px 28px 40px;font-size:var(--text-reading);line-height:1.8;overflow-wrap:anywhere}
 .citation-document :global(.search-preview-body :is(pre,table)){max-width:100%;white-space:pre-wrap;overflow-wrap:anywhere}
 .citation-document :global(.search-preview-body img){max-width:100%;height:auto}
 /* Cited passages read as a soft highlight; the current one is a shade deeper. */
 .citation-document :global(.search-hit){background:color-mix(in srgb,var(--color-ink) 7%,transparent);color:inherit;box-shadow:none;border-radius:3px;padding:1px 0;-webkit-box-decoration-break:clone;box-decoration-break:clone}
 .citation-document :global(.search-hit.active){background:color-mix(in srgb,var(--color-ink) 14%,transparent);color:var(--color-ink);box-shadow:0 1px 0 color-mix(in srgb,var(--color-ink) 35%,transparent)}
 .citation-hint{flex:1;min-width:0;font-size:12px;color:var(--color-muted)}
 :global(.citation-open){display:inline-flex;align-items:center;gap:6px;min-height:32px;padding:6px 12px;border-radius:9px;font-size:var(--text-caption)}
 :global(.citation-open .icon){width:13px;height:13px}
</style>
