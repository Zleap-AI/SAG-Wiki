<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import EvidenceDrawer from './ExploreEvidence.svelte';
 import {dirname} from '../lib/domain';import {api} from '../lib/api';import {t} from '../lib/i18n';import {icon} from '../lib/icons';
 import {addNeighbors,copyGraph,entryNodes,emptyGraph,EXPLORE_NODE_WIDTH as nodeWidth,EXPLORE_NODE_HEIGHT as nodeHeight} from '../lib/explore';
 import type {LibrarySession,ExploreOverview,ExploreNeighbors,ExploreEdge,ExploreEvidence,ExploreDocument} from '../lib/explore';
 export let scope:string;export let session:LibrarySession;export let revision:number;
 export let onopen:(path:string,terms:string[])=>void;
 let overview:ExploreOverview|null=null,query='',found:ExploreOverview|null=null,loading=false,error='',notice='';
 let active=true,sequence=0,querySequence=0,requestId='',searchTimer:ReturnType<typeof setTimeout>;
 let retry:(()=>Promise<void>)|null=null;
 let evidenceOpen=false,evidence:ExploreEvidence|null=null,evidenceLoading=false,evidenceError='',evidenceSequence=0;
 let canvas:HTMLDivElement,width=1000,height=650;
 let drag:{id:number|null;startX:number;startY:number;x:number;y:number;moved:boolean}|null=null,suppressClick=false;
 $:graph=session.graph;
 $:selected=graph.nodes.find(node=>node.id===graph.center);
 $:nodeMap=new Map(graph.nodes.map(node=>[node.id,node]));
 $:edgeMap=new Map(graph.edges.map(edge=>[edge.source===graph.center?edge.target:edge.source,edge]));
 $:rankMap=new Map(graph.nodes.filter(node=>node.id!==graph.center).map((node,index)=>[node.id,index+1]));
 $:if(revision>=0&&active)void refresh(revision);
 async function cancel(){const id=requestId;requestId='';if(id)await api('cancel_query',{id});}
 async function refresh(_revision:number){
  const request=++sequence;await cancel();if(!active||request!==sequence)return;
  loading=true;error='';retry=()=>refresh(_revision);closeEvidence();
  try{
   const result=await api<ExploreOverview>('explore_overview',{scope,query:'',visible:session.graph.nodes.map(n=>n.id)});
   if(!active||request!==sequence)return;overview=result;
   if(!session.initialized){session.graph={...emptyGraph(),nodes:entryNodes(result.documents)};session.initialized=true;session.version=result.version;}
   else if(session.version!==result.version){
    const known=new Map(result.known_documents.map(doc=>[doc.id,doc]));
    const center=known.get(session.graph.center!);
    session.graph=center?{...emptyGraph(),nodes:entryNodes([center]),center:center.id,selected:center.id}:{...emptyGraph(),nodes:entryNodes(result.documents)};
    session.history=[];session.version=result.version;found=null;querySequence++;if(query.trim())void search();notice='文件已更新，关联已刷新。';
   }
   if(session.graph.center!==null&&!session.graph.neighbors)await expand(session.graph.center);
  }catch(e){if(active&&request===sequence){error=String(e);session.graph={...session.graph,edges:[],neighbors:null}}}
  finally{if(active&&request===sequence)loading=false}
 }
 async function search(){
  const request=++querySequence;const text=query.trim();found=null;if(!text)return;
  try{const result=await api<ExploreOverview>('explore_overview',{scope,query:text,visible:[]});if(active&&request===querySequence&&result.version>=session.version)found=result}catch(e){if(active&&request===querySequence){error=String(e);retry=search}}
 }
 function searchInput(){clearTimeout(searchTimer);querySequence++;found=null;searchTimer=setTimeout(search,180)}
 function choose(id:number,document?:ExploreDocument){if(suppressClick){suppressClick=false;return}if(graph.center===id&&(loading||graph.neighbors))return;void expand(id,false,document)}
 function pickResult(id:number){const doc=found!.documents.find(d=>d.id===id)!;query='';found=null;choose(id,doc)}
 async function expand(id:number,more=false,picked?:ExploreDocument){
  const previous=copyGraph(session.graph);const document=picked??previous.nodes.find(node=>node.id===id)!;
  const request=++sequence;await cancel();if(!active||request!==sequence)return;
  const previousResult=more?previous.neighbors:null;
  if(!more){
   if(previous.center!==id)session.history=[...session.history.slice(-19),previous];
   session.graph={...emptyGraph(),nodes:[{...document,x:0,y:0}],center:id,selected:id};fit();
  }
  loading=true;error='';notice='';retry=()=>expand(id,more);requestId=crypto.randomUUID();closeEvidence();
  try{
   const result=await api<ExploreNeighbors>('explore_neighbors',{scope,centerId:id,snapshot:previousResult?.snapshot??null,offset:previousResult?.next_offset??0,requestId});
   if(!active||request!==sequence)return;
   const next=addNeighbors(session.graph,result);
   if(next.nodes.length>80||next.edges.length>160){notice='已达到显示上限，请选择其他文档继续探索。';return}
   session.graph=next;session.version=result.version;fit();
   if(result.total===0)notice='当前范围内没有找到有共同词语的文档。';
  }catch(e){if(active&&request===sequence)error=String(e)}finally{if(active&&request===sequence){loading=false;requestId=''}}
 }
 function centerView(){const current=session.graph;const node=current.nodes.find(n=>n.id===current.center);if(node)session.graph={...current,viewport:{...current.viewport,x:width/2-(node.x+nodeWidth/2)*current.viewport.scale,y:height/2-(node.y+nodeHeight/2)*current.viewport.scale}}}
 function fit(){const nodes=session.graph.nodes;if(!nodes.length)return;const minX=Math.min(...nodes.map(n=>n.x)),minY=Math.min(...nodes.map(n=>n.y)),maxX=Math.max(...nodes.map(n=>n.x))+nodeWidth,maxY=Math.max(...nodes.map(n=>n.y))+nodeHeight;const scale=Math.max(.5,Math.min(1,(width-48)/(maxX-minX),(height-48)/(maxY-minY)));session.graph={...session.graph,viewport:{scale,x:(width-(maxX-minX)*scale)/2-minX*scale,y:(height-(maxY-minY)*scale)/2-minY*scale}}}
 function zoom(factor:number,x=width/2,y=height/2){const v=graph.viewport,scale=Math.max(.25,Math.min(2.5,v.scale*factor));session.graph={...graph,viewport:{scale,x:x-(x-v.x)*scale/v.scale,y:y-(y-v.y)*scale/v.scale}}}
 function startDrag(e:PointerEvent,id:number|null){if(e.button!==0)return;e.stopPropagation();const node=graph.nodes.find(n=>n.id===id);drag={id,startX:e.clientX,startY:e.clientY,x:node?.x??graph.viewport.x,y:node?.y??graph.viewport.y,moved:false}}
 function moveDrag(e:PointerEvent){if(!drag)return;const dx=e.clientX-drag.startX,dy=e.clientY-drag.startY;if(Math.hypot(dx,dy)<5&&!drag.moved)return;drag.moved=true;
  if(drag.id===null)session.graph={...graph,viewport:{...graph.viewport,x:drag.x+dx,y:drag.y+dy}};
  else session.graph={...graph,nodes:graph.nodes.map(n=>n.id===drag!.id?{...n,x:drag!.x+dx/graph.viewport.scale,y:drag!.y+dy/graph.viewport.scale}:n)};
 }
 function endDrag(){suppressClick=Boolean(drag?.moved&&drag.id!==null);drag=null}
 function wheel(e:WheelEvent){e.preventDefault();if(e.ctrlKey||e.metaKey){const rect=canvas.getBoundingClientRect();zoom(Math.exp(-e.deltaY*.005),e.clientX-rect.left,e.clientY-rect.top)}else session.graph={...graph,viewport:{...graph.viewport,x:graph.viewport.x-e.deltaX,y:graph.viewport.y-e.deltaY}}}
 async function showEvidence(edge:ExploreEdge){const request=++evidenceSequence;evidenceOpen=true;evidence=null;evidenceError='';evidenceLoading=true;try{const result=await api<ExploreEvidence>('explore_evidence',{scope,snapshot:edge.snapshot,source:edge.source,target:edge.target});if(active&&request===evidenceSequence)evidence=result}catch(e){if(active&&request===evidenceSequence)evidenceError=String(e)}finally{if(active&&request===evidenceSequence)evidenceLoading=false}}
 function closeEvidence(){evidenceSequence++;evidenceOpen=false;evidence=null}
 async function reset(){sequence++;await cancel();session.initialized=false;session.graph=emptyGraph();session.history=[];query='';found=null;notice='';closeEvidence();await refresh(revision)}
 async function back(){sequence++;await cancel();const previous=session.history.at(-1);if(previous){session.graph=previous;session.history=session.history.slice(0,-1)}loading=false;error='';notice='';closeEvidence();if(session.graph.center!==null&&!session.graph.neighbors)await expand(session.graph.center)}
 onMount(()=>{const observer=new ResizeObserver(entries=>{const box=entries[0].contentRect;width=box.width||1000;height=box.height||650});observer.observe(canvas);return()=>observer.disconnect()});
 onDestroy(()=>{active=false;sequence++;evidenceSequence++;clearTimeout(searchTimer);void cancel().catch(e=>console.error(e))});
</script>
<svelte:window onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag}/>
<section class="explore-view" aria-label={$t('文档探索')}>
 <div class="explore-toolbar"><div class="explore-search">{@html icon('search')}<Input value={query} oninput={(e)=>{query=e.currentTarget.value;searchInput()}} aria-label={$t('搜索文档，开始探索')} placeholder={$t('搜索文档，开始探索')} maxlength={200}/>
  {#if found}<div class="explore-picker">{#each found.documents as doc}<Button.Root title={doc.path} onclick={()=>pickResult(doc.id)}>{@html icon('file')}<span>{doc.name}</span></Button.Root>{:else}<p>{$t('没有匹配的文件')}</p>{/each}</div>{/if}
 </div><div class="explore-controls">{#if graph.center!==null||session.history.length}<Button.Root class="outline" disabled={!session.history.length} aria-label={$t('返回上一步')} onclick={back}>{@html icon('back')}{$t('返回上一步')}</Button.Root><Button.Root class="icon-button" disabled={graph.center===null} aria-label={$t('回到当前文档')} title={$t('回到当前文档')} onclick={centerView}>{@html icon('maximize')}</Button.Root><Button.Root class="outline" onclick={reset}>{$t('更换文档')}</Button.Root>{/if}</div></div>
 <div class="explore-status" aria-live="polite"><span>{#if loading}<i class="explore-spinner"></i>{$t('正在查找关联…')}{:else if overview}{$t('当前范围 {value0} 个文档',{value0:overview.total})}{/if}</span><span>{#if overview?.indexing}{$t('正在索引，结果可能不完整')}{:else if overview?.partial}{$t('部分文件未能索引')}{/if}</span></div>
 {#if error}<div class="explore-notice" role="alert">{$t(error)} <Button.Root onclick={()=>retry?.()}>{$t('重试')}</Button.Root></div>{:else if notice}<div class="explore-notice" role="status">{$t(notice)}</div>{/if}
 <div class="explore-canvas" class:choosing={graph.center===null} bind:this={canvas} onwheel={(e)=>{if(graph.center!==null)wheel(e)}}>
  {#if graph.center===null}
   <div class="explore-start"><div class="explore-start-heading"><h2>{$t('选择一篇文档开始探索')}</h2><p>{$t('点击文档，自动查看相关内容和原文依据。')}</p></div>
    <div class="explore-entry-list">{#each graph.nodes as node}<Button.Root class="explore-entry" data-document-id={node.id} title={node.path} onclick={()=>choose(node.id)}>{@html icon('file')}<span><strong>{node.name}</strong><small>{dirname(node.path).slice(scope.length+1)||node.file_type.toUpperCase()}</small></span>{@html icon('chevron')}</Button.Root>{/each}</div>
    {#if !loading&&!graph.nodes.length}<p class="explore-start-empty">{$t('这里还没有可探索的文档')} · {$t('等待索引完成，或在左侧选择其他文件夹。')}</p>{/if}
   </div>
  {:else}
   <svg viewBox={`0 0 ${width} ${height}`} aria-label={$t('文档关联图')} role="group" onpointerdown={(e)=>startDrag(e,null)}>
    <g transform={`translate(${graph.viewport.x} ${graph.viewport.y}) scale(${graph.viewport.scale})`}>
     {#each graph.edges as edge (`${edge.source}:${edge.target}`)}
      {@const a=nodeMap.get(edge.source)!}{@const b=nodeMap.get(edge.target)!}
      {@const left=a.x<b.x?a:b}{@const right=a.x<b.x?b:a}{@const sx=left.x+nodeWidth}{@const ex=right.x}{@const sy=left.y+nodeHeight/2}{@const ey=right.y+nodeHeight/2}{@const middle=(sx+ex)/2}
      {@const line=`M${sx},${sy} C${middle},${sy} ${middle},${ey} ${ex},${ey}`}
      <g class="explore-edge">
       <path d={line} class="edge-line"/>
       <path d={line} class="edge-hit" tabindex="0" role="button" aria-label={`${$t('查看关联依据')}：${a.name} — ${b.name}`} onpointerdown={(e:PointerEvent)=>e.stopPropagation()} onclick={()=>showEvidence(edge)} onkeydown={(e)=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();void showEvidence(edge)}}}><title>{edge.terms.slice(0,3).join(' · ')}</title></path>
       <g class="edge-label" transform={`translate(${middle} ${(sy+ey)/2})`}><rect x="-72" y="-12" width="144" height="24" rx="8"/><text text-anchor="middle" dy="4">{edge.terms.slice(0,2).join(' · ').slice(0,20)}</text></g>
      </g>
     {/each}
    </g>
   </svg>
   <div class="explore-nodes" style:transform={`translate(${graph.viewport.x}px,${graph.viewport.y}px) scale(${graph.viewport.scale})`}>
     {#each graph.nodes as node(node.id)}
      {@const edge=edgeMap.get(node.id)}
      <div class="explore-node-shell" style:transform={`translate(${node.x}px,${node.y}px)`} style:width={`${nodeWidth}px`} style:height={`${nodeHeight}px`}><div class="explore-node-wrap" class:center={node.id===graph.center}>
       <button class="explore-node" data-document-id={node.id} title={node.path} onpointerdown={(e)=>startDrag(e,node.id)} onclick={()=>choose(node.id)}>{#if node.id===graph.center}{@html icon('file')}{:else}<span class="explore-node-rank">{rankMap.get(node.id)}</span>{/if}<span class="explore-node-copy"><strong>{node.name}</strong></span></button>
       <div class="explore-node-meta"><small>{edge?edge.terms.slice(0,2).join(' · '):$t('当前文档')}</small>{#if edge}<Button.Root class="explore-node-evidence" aria-label={`${$t('查看关联依据')}：${node.name}`} onpointerdown={(e:PointerEvent)=>e.stopPropagation()} onclick={()=>showEvidence(edge)}>{$t('依据')}</Button.Root>{/if}</div>
      </div></div>
     {/each}
   </div>
   <div class="explore-zoom"><Button.Root class="icon-button" aria-label={$t('缩小')} onclick={()=>zoom(.8)}>{@html icon('minimize')}</Button.Root><Button.Root class="zoom-label" onclick={fit} title={$t('适应窗口')}>{Math.round(graph.viewport.scale*100)}%</Button.Root><Button.Root class="icon-button" aria-label={$t('放大')} onclick={()=>zoom(1.25)}>{@html icon('plus')}</Button.Root></div>
  {/if}
 </div>
 {#if selected}<div class="explore-footer"><div class="explore-selection"><span title={selected.path}><strong>{selected.name}</strong><small>{selected.path}</small></span><Button.Root class="outline" onclick={()=>onopen(selected!.path,[])}>{$t('打开文档')}</Button.Root></div>
  <div class="explore-more"><span>{#if graph.neighbors}{$t('已展开 {value0} / {value1} 个关联',{value0:graph.neighbors.next_offset,value1:graph.neighbors.total})} · {/if}{$t('点击文档继续探索，点击依据核对原文。')}</span>{#if graph.neighbors?.has_more}<Button.Root class="outline" disabled={loading} onclick={()=>expand(graph.center!,true)}>{$t('继续展开 12 个')}</Button.Root>{/if}</div>
 </div>{/if}
</section>
{#if evidenceOpen}<EvidenceDrawer {evidence} loading={evidenceLoading} error={evidenceError} onClose={closeEvidence} onOpen={(path,terms)=>{closeEvidence();onopen(path,terms)}}/>{/if}
