<script lang="ts">
 import {onDestroy} from 'svelte';
 import {isLargeMarkdown} from '../lib/large-document-policy';
 import {beginDocumentTrace,type DocumentTrace} from '../lib/document-performance';
 import {Button} from 'bits-ui';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {isHelpDocument,isHtml,isMarkdown,readonlyHtml} from '../lib/domain';
 import {citationTerms,documentMatchIndex} from '../lib/search-highlights';
 import {legacyPreview} from '../lib/preview-mode';
 import type {Candidate,Document} from '../lib/types';
 import HtmlDocument from './HtmlDocument.svelte';
 import HighlightedDocument from './HighlightedDocument.svelte';
 export let source:Candidate;
 export let compact=false;
 export let onOpen:(terms:string[])=>void;
 let generation=0,tracing:DocumentTrace|undefined,largeTerms:string[]=[];
 onDestroy(()=>{generation++;tracing?.cancel();});
 function citedRow(pane:ParentNode){
  const rows=pane.querySelector('table')?.rows;
  return rows?.item(rows.length-1);
 }
 function markCitedRow(pane:HTMLElement){
  const row=citedRow(pane);
  row?.classList.add('cited-table-row');
  row?.setAttribute('aria-current','true');
 }
 async function load(candidate:Candidate){
  const request=++generation;largeTerms=[];const trace=beginDocumentTrace('reader');tracing=trace;
  const read=trace.stage('read-rpc');let document:Document;
  const help=isHelpDocument(candidate.path);
  try{document=await api<Document>(help?'read_help_document':'read_document',{path:candidate.path});}catch(e){trace.cancel();throw e;}finally{read();}
  if(request!==generation){trace.cancel();return null;}
  if(candidate.table_row&&document.hash===candidate.hash){
   trace.cancel();
   const {renderMarkdown}=await import('../lib/markdown');
   const html=renderMarkdown(candidate.content);
   const row=citedRow(new DOMParser().parseFromString(html,'text/html'));
   const terms=Array.from(row?.cells??[],cell=>cell.textContent?.trim()??'').filter(Boolean);
   return {document,terms,initialMatch:0,html,reader:null,LargeReader:null,trace,tableRow:true};
  }
  if(isLargeMarkdown(document)){
   const done=trace.stage('component');
   try{const {default:LargeReader}=await import('./LargeDocumentReader.svelte');
    return {document,terms:[],initialMatch:0,html:'',reader:null,LargeReader,trace};
   }finally{done();}
  }
  trace.cancel();
  if(help||isMarkdown(candidate.path)&&!legacyPreview()){
   const [{markdownModel,candidateRanges,modelCitationTerms},{default:Reader}]=await Promise.all([import('../lib/document-model'),import('./MarkdownReader.svelte')]);
   const model=markdownModel(document),ranges=candidateRanges(model,candidate);
   return {document,terms:await modelCitationTerms(model,candidate,ranges),initialMatch:0,html:'',LargeReader:null,trace,reader:{Reader,model,ranges}};
  }
  const terms=await citationTerms(candidate,document.body);
  const initialMatch=await documentMatchIndex(document.body,candidate.path,terms,candidate);
  const html=isMarkdown(candidate.path)?(await import('../lib/markdown')).renderMarkdown(document.body):isHtml(candidate.path)?'':readonlyHtml(candidate.path,document.body);
  return {document,terms,initialMatch,html,reader:null,LargeReader:null,trace};
 }
 $:preview=load(source);
</script>

{#await preview}
 <div class="context-body"><p role="status">{$t('正在读取文件…')}</p></div>
{:then result}
 {#if result}
 <div class="context-body citation-body">
  {#if result.document.hash!==source.hash}<p class="source-warning">{@html icon('info')}{$t('原文件已更新，以下显示当前内容。')}</p>{/if}
  <div class="citation-document">
   {#if result.tableRow}
    <div class="search-preview-body citation-table" use:markCitedRow>
     {#if source.heading}<p class="citation-table-heading">{source.heading}</p>{/if}
     {@html result.html}
    </div>
   {:else if result.LargeReader}<svelte:component this={result.LargeReader} doc={result.document} candidate={source} deriveTerms restrictCitation onTerms={value=>largeTerms=value} label={compact?'match':'citation'} showNavigation={!compact} trace={result.trace}/>
   {:else if result.reader}<svelte:component this={result.reader.Reader} model={result.reader.model} terms={result.terms} target={result.reader.ranges[0].from} within={result.reader.ranges} path={source.path} label={compact?'match':'citation'} showNavigation={!compact}/>
   {:else if isHtml(source.path)}<HtmlDocument body={result.document.body} terms={result.terms} initialMatch={result.initialMatch} showNavigation={!compact}/>
   {:else}<HighlightedDocument html={result.html} terms={result.terms} path={source.path} initialMatch={result.initialMatch} showNavigation={!compact}/>{/if}
  </div>
 </div>
  {#if !compact&&!isHelpDocument(source.path)}<div class="panel-footer"><span class="citation-hint">{$t('高亮为回答引用的段落')}</span><Button.Root class="primary citation-open" onclick={()=>onOpen(result.LargeReader?largeTerms:result.terms)}>{$t('打开当前文档')}{@html icon('forward')}</Button.Root></div>{/if}
 {/if}
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
 .citation-document .citation-table{overflow-x:auto}
 .citation-table-heading{margin:0 0 12px;color:var(--color-secondary)}
 .citation-document .citation-table :global(table){width:max-content;min-width:100%;max-width:none;white-space:normal}
 .citation-table :global(th),.citation-table :global(td){min-width:64px;max-width:250px;padding:8px 12px;overflow-wrap:normal;border:1px solid var(--color-rule)}
 .citation-table :global(tr.cited-table-row > :is(td,th)){background:color-mix(in srgb,var(--color-ink) 14%,transparent)}
 .citation-hint{flex:1;min-width:0;font-size:12px;color:var(--color-muted)}
 :global(.citation-open){display:inline-flex;align-items:center;gap:6px;min-height:32px;padding:6px 12px;border-radius:9px;font-size:var(--text-caption)}
 :global(.citation-open .icon){width:13px;height:13px}
</style>
