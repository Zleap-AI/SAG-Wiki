<script lang="ts">
 import {displayPath} from '../lib/paths';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {isHtml,isMarkdown,readonlyHtml} from '../lib/domain';
 import {citationTerms,documentMatchIndex} from '../lib/search-highlights';
 import type {Candidate,Document} from '../lib/types';
 import HtmlDocument from './HtmlDocument.svelte';
 import HighlightedDocument from './HighlightedDocument.svelte';
 export let source:Candidate;
 export let onOpen:(terms:string[])=>void;
 async function load(candidate:Candidate){
  const document=await api<Document>('read_document',{path:candidate.path});
  const terms=await citationTerms(candidate,document.body);
  const initialMatch=await documentMatchIndex(document.body,candidate.path,terms,candidate);
  const html=isMarkdown(candidate.path)?(await import('../lib/markdown')).renderMarkdown(document.body):isHtml(candidate.path)?'':readonlyHtml(candidate.path,document.body);
  return {document,terms,initialMatch,html};
 }
 $:preview=load(source);
</script>

{#await preview}
 <div class="context-body"><p role="status">{$t('正在读取文件…')}</p></div>
{:then result}
 <div class="context-body citation-body">
  {#if result.document.hash!==source.hash}<p class="source-warning">{$t('原文件已更新，以下显示当前内容。')}</p>{/if}
  <div class="citation-document">
   {#if isHtml(source.path)}<HtmlDocument body={result.document.body} terms={result.terms} initialMatch={result.initialMatch}/>
   {:else}<HighlightedDocument html={result.html} terms={result.terms} path={source.path} initialMatch={result.initialMatch}/>{/if}
  </div>
 </div>
 <div class="panel-footer"><Button.Root class="primary" onclick={()=>onOpen(result.terms)}>{$t('打开当前文档')}</Button.Root></div>
{:catch error}
 <div class="context-body"><p class="form-error">{$t(String(error))}</p></div>
{/await}

<style>
 :global(.source-dialog .context-body.citation-body){display:flex;flex-direction:column;overflow:hidden;padding:0}
 .source-warning{margin:0;padding:0 24px;color:var(--color-secondary)}
 .citation-document{display:flex;flex-direction:column;flex:1;min-height:0}
 .citation-document :global(.search-preview-body){flex:1;min-height:0;overflow:auto;padding:16px 24px}
 .citation-document :global(.match-navigation){padding:4px 24px 10px;flex-shrink:0}
</style>
