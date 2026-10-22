<script lang="ts">
 import {onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import type GraphCanvas from './ExploreGraph.svelte';
 import EvidenceDrawer from './ExploreEvidence.svelte';
 import {api} from '../lib/api';import {t} from '../lib/i18n';import {icon} from '../lib/icons';
 import {copyGraph,graphFromData} from '../lib/explore';
 import {overviewEdges} from '../lib/graph-layout';
 import type {LibrarySession,ExploreOverview,ExploreGraphData,ExploreGraph,ExploreEdge,ExploreEvidence} from '../lib/explore';
 export let scope:string;export let session:LibrarySession;export let revision:number;
 export let onopen:(path:string,terms:string[])=>void;
 let overview:ExploreOverview|null=null,query='',loading=false,error='',notice='';
 let active=true,sequence=0,requestId='',canvas:GraphCanvas,locateFrame=0;
 let preview='',previewLoading=false,previewError='',previewSequence=0;
 let evidenceOpen=false,evidence:ExploreEvidence|null=null,evidenceLoading=false,evidenceError='',evidenceSequence=0;
 $:graph=session.graph;
 $:visibleEdges=graph.center===null?overviewEdges(graph.edges,graph.nodes.length,session.strength):graph.edges.filter(edge=>edge.score>=session.strength);
 $:selected=graph.nodes.find(node=>node.id===graph.selected);
 $:matches=query.trim()?session.overview?.documents.filter(doc=>doc.name.toLowerCase().includes(query.trim().toLowerCase())).slice(0,12)??[]:[];
 $:relations=selected?graph.edges.filter(edge=>(edge.source===selected.id||edge.target===selected.id)&&edge.score>=session.strength).sort((a,b)=>b.score-a.score):[];
 $:documents=new Map(graph.nodes.map(node=>[node.id,node]));
 $:if(revision>=0&&active)void refresh();
 async function cancel(){const id=requestId;requestId='';if(id)await api('cancel_query',{id})}
 async function refresh(){
  const request=++sequence;await cancel();if(!active||request!==sequence)return;
  loading=true;error='';closeEvidence();requestId=crypto.randomUUID();
  try{
   const [status,result]=await Promise.all([
    api<ExploreOverview>('explore_overview',{scope,query:'',visible:[]}),
    api<ExploreGraphData>('explore_graph',{scope,requestId})
   ]);
   if(!active||request!==sequence)return;overview=status;
   if(!session.initialized||session.version!==result.version){
    const previous=canvas?.capture()??session.graph;
    const center=result.documents.some(doc=>doc.id===previous.center)?previous.center:null;
    const next=graphFromData(result,center);
    const positions=new Map(previous.nodes.map(node=>[node.id,node]));
    next.nodes=next.nodes.map(node=>{const old=positions.get(node.id);return old?{...node,x:old.x,y:old.y}:node});
    next.layoutReady=previous.layoutReady&&next.nodes.every(node=>positions.has(node.id));
    next.layoutVersion=previous.layoutVersion;next.viewport=previous.viewport;next.selected=result.documents.some(doc=>doc.id===previous.selected)?previous.selected:null;
    session.graph=next;session.history=[];session.overview=result;session.version=result.version;session.initialized=true;
    if(previous.nodes.length)notice='文件已更新，关联已刷新。';
    if(next.selected!==null)void select(next.selected);
   }
  }catch(e){if(active&&request===sequence)error=String(e)}finally{if(active&&request===sequence){loading=false;requestId=''}}
 }
 function save(state:ExploreGraph){session.graph={...session.graph,nodes:state.nodes,viewport:state.viewport,layoutReady:state.layoutReady,layoutVersion:state.layoutVersion}}
 async function select(id:number|null){
  session.graph={...session.graph,selected:id};const request=++previewSequence;preview='';previewError='';previewLoading=id!==null;
  if(id===null)return;
  const document=session.graph.nodes.find(node=>node.id===id)!;
  try{const result=await api<{body:string}>('read_document',{path:document.path});if(active&&request===previewSequence)preview=result.body.slice(0,1800)}
  catch(e){if(active&&request===previewSequence)previewError=String(e)}finally{if(active&&request===previewSequence)previewLoading=false}
 }
 function focus(id:number){
  if(graph.center===id)return;
  session.history=[...session.history.slice(-19),copyGraph(canvas.capture())];
  session.graph=graphFromData(session.overview!,id);query='';notice='';closeEvidence();void select(id);
 }
 function back(){const previous=session.history.at(-1);if(previous){session.graph=previous;session.history=session.history.slice(0,-1);void select(previous.selected)}notice='';closeEvidence()}
 function reset(){session.history=[];session.graph=graphFromData(session.overview!);query='';notice='';closeEvidence();void select(null)}
 function locate(id:number){query='';if(!graph.nodes.some(node=>node.id===id)){session.history=[...session.history.slice(-19),copyGraph(canvas.capture())];session.graph=graphFromData(session.overview!)}void select(id);locateFrame=requestAnimationFrame(()=>{if(active)canvas.locate(id)})}
 async function showEvidence(edge:ExploreEdge){const request=++evidenceSequence;evidenceOpen=true;evidence=null;evidenceError='';evidenceLoading=true;try{const result=await api<ExploreEvidence>('explore_evidence',{scope,snapshot:edge.snapshot,source:edge.source,target:edge.target});if(active&&request===evidenceSequence)evidence=result}catch(e){if(active&&request===evidenceSequence)evidenceError=String(e)}finally{if(active&&request===evidenceSequence)evidenceLoading=false}}
 function closeEvidence(){evidenceSequence++;evidenceOpen=false;evidence=null}
 onDestroy(()=>{active=false;cancelAnimationFrame(locateFrame);sequence++;previewSequence++;evidenceSequence++;void cancel().catch(e=>console.error(e))});
</script>
<section class="explore-view graph-view" aria-label={$t('文档探索')}>
 <div class="explore-toolbar">
  <div class="explore-search">{@html icon('search')}<Input value={query} oninput={(e)=>query=e.currentTarget.value} onkeydown={(e)=>{if(e.key==='Enter'&&matches.length)locate(matches[0].id);if(e.key==='Escape')query=''}} aria-label={$t('在关系图中查找文档')} placeholder={$t('在关系图中查找文档')} maxlength={200}/>
   {#if query.trim()}<div class="explore-picker">{#each matches as doc}<Button.Root title={doc.path} onclick={()=>locate(doc.id)}>{@html icon('file')}<span>{doc.name}</span></Button.Root>{:else}<p>{$t('没有匹配的文件')}</p>{/each}</div>{/if}
  </div>
  <div class="explore-controls">
   {#if graph.center!==null}<Button.Root class="outline" onclick={back} disabled={!session.history.length}>{@html icon('back')}{$t('返回上一步')}</Button.Root><Button.Root class="outline" onclick={reset}>{$t('关系总览')}</Button.Root>{/if}
  </div>
 </div>
 <div class="explore-status" aria-live="polite"><span>{#if loading}<i class="explore-spinner"></i>{$t('正在生成关系图…')}{:else}{$t(graph.center===null&&graph.nodes.length>40?'{value0} 个文档 · {value1} 条主要关联':'{value0} 个文档 · {value1} 条关联',{value0:graph.nodes.length,value1:visibleEdges.length})}{/if}{#if graph.center!==null}<span class="graph-mode">{$t('局部关系')}</span>{/if}</span><span>{#if overview?.indexing}{$t('正在索引，结果可能不完整')}{:else if overview?.partial}{$t('部分文件未能索引')}{:else}<span title={$t('总览突出主要关联，选中文档可查看全部关联')}>{$t('内容相似')}</span>{/if}</span></div>
 {#if error}<div class="explore-notice" role="alert">{$t(error)} <Button.Root onclick={refresh}>{$t('重试')}</Button.Root></div>{/if}
 {#if notice}<div class="graph-update" role="status">{$t(notice)}</div>{/if}
 <div class="graph-workspace">
  <div class="explore-canvas graph-canvas">
   {#if session.initialized}{#await import('./ExploreGraph.svelte') then module}<svelte:component this={module.default} bind:this={canvas} {graph} strength={session.strength} onselect={(id)=>void select(id)} onfocus={focus} onevidence={(edge)=>void showEvidence(edge)} onstate={save}/>{/await}{/if}
   {#if !loading&&!graph.nodes.length}<div class="explore-empty">{@html icon('search')}<h2>{$t('这里还没有可探索的文档')}</h2><p>{$t('等待索引完成，或在左侧选择其他文件夹。')}</p></div>{/if}
   <div class="graph-top-controls">
   <Button.Root class="icon-button" disabled={!canvas||!graph.nodes.length} title={$t('适应窗口')} aria-label={$t('适应窗口')} onclick={()=>canvas.fit()}>{@html icon('maximize')}</Button.Root>
    <div class="graph-settings"><label for="graph-strength">{$t('关联强度')}</label><input id="graph-strength" type="range" min="0" max="0.8" step="0.02" bind:value={session.strength} aria-label={$t('关联强度')}/><span>{$t('较强')}</span></div>
   </div>
   <div class="explore-zoom"><Button.Root class="icon-button" aria-label={$t('缩小')} disabled={!canvas||!graph.nodes.length} onclick={()=>canvas.zoom(1.3)}>{@html icon('minimize')}</Button.Root><Button.Root class="zoom-label" disabled={!canvas||!graph.nodes.length} onclick={()=>canvas.fit()}>{$t('适应')}</Button.Root><Button.Root class="icon-button" aria-label={$t('放大')} disabled={!canvas||!graph.nodes.length} onclick={()=>canvas.zoom(.77)}>{@html icon('plus')}</Button.Root></div>
   <div class="graph-help">{$t('拖动平移 · 滚轮缩放 · 双击文档查看局部')}</div>
  </div>
  {#if selected}<aside class="graph-preview" aria-label={$t('文档预览')}>
   <div class="graph-preview-head"><span>{@html icon('file')}<strong>{selected.name}</strong></span><Button.Root class="icon-button" aria-label={$t('关闭预览')} onclick={()=>void select(null)}>{@html icon('close')}</Button.Root></div>
   <p class="graph-preview-path" title={selected.path}>{selected.path}</p>
   <div class="graph-preview-actions"><Button.Root class="primary" onclick={()=>onopen(selected!.path,[])}>{$t('打开文档')}</Button.Root><Button.Root class="outline" onclick={()=>focus(selected!.id)} disabled={graph.center===selected.id}>{$t('查看局部')}</Button.Root></div>
   <div class="graph-preview-scroll">
    {#if previewLoading}<p class="graph-preview-note" role="status">{$t('正在读取原文…')}</p>{:else if previewError}<p role="alert">{$t(previewError)}</p>{:else}<p class="graph-excerpt">{preview}</p>{/if}
    <div class="graph-relations-heading"><strong>{$t('相关文档')}</strong><span>{relations.length}</span></div>
    {#each relations as edge}
     {@const other=documents.get(edge.source===selected.id?edge.target:edge.source)!}
     <div class="graph-relation"><Button.Root class="graph-relation-document" onclick={()=>locate(other.id)}><span>{other.name}</span><small>{edge.terms.slice(0,3).join(' · ')}</small></Button.Root><Button.Root class="graph-relation-evidence" onclick={()=>showEvidence(edge)}>{$t('依据')}</Button.Root></div>
    {:else}<p class="graph-preview-note">{$t('当前强度下没有关联文档')}</p>{/each}
   </div>
  </aside>{/if}
 </div>
</section>
{#if evidenceOpen}<EvidenceDrawer {evidence} loading={evidenceLoading} error={evidenceError} onClose={closeEvidence} onOpen={(path,terms)=>{closeEvidence();onopen(path,terms)}}/>{/if}
