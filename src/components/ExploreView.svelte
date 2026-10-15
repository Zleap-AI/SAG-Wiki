<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import EvidenceDrawer from './ExploreEvidence.svelte';
 import {dirname} from '../lib/domain';import {api} from '../lib/api';import {t} from '../lib/i18n';import {icon} from '../lib/icons';
 import {addNeighbors,copyGraph,entryNodes,emptyGraph} from '../lib/explore';
 import type {LibrarySession,ExploreOverview,ExploreNeighbors,ExploreEdge,ExploreEvidence} from '../lib/explore';
 export let scope:string;export let session:LibrarySession;export let revision:number;
 export let onopen:(path:string,terms:string[])=>void;
 let overview:ExploreOverview|null=null,query='',found:ExploreOverview|null=null,loading=false,error='',notice='';
 let active=true,sequence=0,querySequence=0,requestId='',searchTimer:ReturnType<typeof setTimeout>;
 let retry:(()=>Promise<void>)|null=null;
 let evidenceOpen=false,evidence:ExploreEvidence|null=null,evidenceLoading=false,evidenceError='',evidenceSequence=0;
 let canvas:HTMLDivElement,width=1000,height=650;
 let drag:{id:number|null;startX:number;startY:number;x:number;y:number;moved:boolean}|null=null,suppressClick=false;
 $:graph=session.graph;
 $:selected=graph.nodes.find(node=>node.id===graph.selected);
 $:maxScore=Math.max(...graph.edges.map(edge=>edge.score),0.0001);
 $:nodeMap=new Map(graph.nodes.map(node=>[node.id,node]));
 $:if(revision>=0&&active)void refresh(revision);
 async function cancel(){const id=requestId;requestId='';if(id)await api('cancel_query',{id});}
 async function refresh(_revision:number){
  const request=++sequence;await cancel();if(!active||request!==sequence)return;
  loading=true;error='';retry=()=>refresh(_revision);closeEvidence();
  try{const result=await api<ExploreOverview>('explore_overview',{scope,query:'',visible:session.graph.nodes.map(n=>n.id)});
   if(!active||request!==sequence)return;overview=result;
   if(!session.initialized){session.graph={...emptyGraph(),nodes:entryNodes(result.documents)};session.initialized=true;session.version=result.version;fit();}
   else if(session.version!==result.version){
    const known=new Map(result.known_documents.map(doc=>[doc.id,doc]));
    session.graph={...session.graph,nodes:session.graph.nodes.length===0?entryNodes(result.documents):session.graph.nodes.filter(n=>known.has(n.id)).map(n=>({...n,...known.get(n.id)!})),edges:[],neighbors:null};
    if(!known.has(session.graph.center!))session.graph.center=null;
    if(!known.has(session.graph.selected!))session.graph.selected=null;
    session.history=[];session.version=result.version;found=null;querySequence++;if(query.trim())void search();notice='文件已更新，请重新展开关联。';closeEvidence();
   }
  }catch(e){if(active&&request===sequence){error=String(e);session.graph={...session.graph,edges:[],neighbors:null}}}finally{if(active&&request===sequence)loading=false}
 }
 async function search(){
  const request=++querySequence;const text=query.trim();found=null;if(!text)return;
  try{const result=await api<ExploreOverview>('explore_overview',{scope,query:text,visible:[]});if(active&&request===querySequence&&result.version>=session.version)found=result}catch(e){if(active&&request===querySequence){error=String(e);retry=search}}
 }
 function searchInput(){clearTimeout(searchTimer);querySequence++;found=null;searchTimer=setTimeout(search,180)}
 function choose(id:number){if(suppressClick){suppressClick=false;return}session.graph={...graph,selected:id}}
 function pickResult(id:number){const doc=found!.documents.find(d=>d.id===id)!;if(!graph.nodes.some(n=>n.id===id)){if(graph.nodes.length>=80){notice='已达到 80 个节点，请收起分支或重置视图。';return}session.graph={...graph,nodes:[...graph.nodes,{...doc,x:60,y:55+Math.ceil(graph.nodes.length/4)*95}]}}session.graph={...session.graph,selected:id};query='';found=null;centerView(id)}
 async function expand(id:number,more=false){
  const previous=copyGraph(session.graph);const request=++sequence;await cancel();if(!active||request!==sequence)return;
  loading=true;error='';notice='';retry=()=>expand(id,more);requestId=crypto.randomUUID();
  const previousResult=more?session.graph.neighbors:null;
  try{const result=await api<ExploreNeighbors>('explore_neighbors',{scope,centerId:id,snapshot:previousResult?.snapshot??null,offset:previousResult?.next_offset??0,requestId});
   if(!active||request!==sequence)return;
   const next=addNeighbors(session.graph,result);
   if(next.nodes.length>80||next.edges.length>160){notice='图已达到显示上限，请收起分支或重置视图。';return}
   if(!more){session.history=[...session.history.slice(-19),previous]}
   session.graph=next;session.version=result.version;
   if(result.total===0)notice='当前范围内没有找到有共同词语的文档。';
  }catch(e){if(active&&request===sequence)error=String(e)}finally{if(active&&request===sequence){loading=false;requestId=''}}
 }
 function centerView(id:number|null=session.graph.center){const current=session.graph;const node=current.nodes.find(n=>n.id===id);if(node)session.graph={...current,viewport:{...current.viewport,x:width/2-(node.x+90)*current.viewport.scale,y:height/2-(node.y+29)*current.viewport.scale}}}
 function fit(){const nodes=session.graph.nodes;if(!nodes.length)return;const minX=Math.min(...nodes.map(n=>n.x)),minY=Math.min(...nodes.map(n=>n.y)),maxX=Math.max(...nodes.map(n=>n.x))+180,maxY=Math.max(...nodes.map(n=>n.y))+58;const scale=Math.min(1,(width-70)/(maxX-minX),(height-70)/(maxY-minY));session.graph={...session.graph,viewport:{scale,x:(width-(maxX-minX)*scale)/2-minX*scale,y:(height-(maxY-minY)*scale)/2-minY*scale}}}
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
 async function reset(){sequence++;await cancel();session.initialized=false;session.graph=emptyGraph();session.history=[];notice='';closeEvidence();await refresh(revision)}
 async function back(){sequence++;await cancel();const previous=session.history.at(-1);if(previous){session.graph=previous;session.history=session.history.slice(0,-1)}loading=false;closeEvidence()}
 function collapse(){if(graph.selected===null)return;const id=graph.selected;session.history=[...session.history.slice(-19),copyGraph(graph)];session.graph={...graph,nodes:graph.nodes.filter(n=>n.id!==id),edges:graph.edges.filter(e=>e.source!==id&&e.target!==id),selected:null,center:graph.center===id?null:graph.center,neighbors:graph.center===id?null:graph.neighbors}}
 onMount(()=>{const observer=new ResizeObserver(entries=>{const box=entries[0].contentRect;width=box.width||1000;height=box.height||650});observer.observe(canvas);return()=>observer.disconnect()});
 onDestroy(()=>{active=false;sequence++;evidenceSequence++;clearTimeout(searchTimer);void cancel().catch(e=>console.error(e))});
</script>
<svelte:window onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag}/>
<section class="explore-view" aria-label={$t('文档探索')}>
 <div class="explore-toolbar"><div class="explore-search">{@html icon('search')}<Input value={query} oninput={(e)=>{query=e.currentTarget.value;searchInput()}} aria-label={$t('按文件名查找入口')} placeholder={$t('按文件名查找入口')} maxlength={200}/>
  {#if found}<div class="explore-picker">{#each found.documents as doc}<Button.Root title={doc.path} onclick={()=>pickResult(doc.id)}>{@html icon('file')}<span>{doc.name}</span></Button.Root>{:else}<p>{$t('没有匹配的文件')}</p>{/each}</div>{/if}
 </div><div class="explore-controls"><Button.Root class="icon-button" disabled={!session.history.length} aria-label={$t('返回上一探索')} title={$t('返回上一探索')} onclick={back}>{@html icon('back')}</Button.Root><Button.Root class="icon-button" disabled={graph.center===null} aria-label={$t('回到中心')} title={$t('回到中心')} onclick={()=>centerView()}>{@html icon('maximize')}</Button.Root><Button.Root class="outline" onclick={reset}>{$t('重置视图')}</Button.Root></div></div>
 <div class="explore-status" aria-live="polite"><span>{#if loading}<i class="explore-spinner"></i>{$t('正在查找关联…')}{:else if overview}{$t('当前范围 {value0} 个文档 · 已显示 {value1} 个',{value0:overview.total,value1:graph.nodes.length})}{/if}</span><span>{#if overview?.indexing}{$t('正在索引，结果可能不完整')}{:else if overview?.partial}{$t('部分文件未能索引')}{/if}</span></div>
 {#if error}<div class="explore-notice" role="alert">{$t(error)} <Button.Root onclick={()=>retry?.()}>{$t('重试')}</Button.Root></div>{:else if notice}<div class="explore-notice" role="status">{$t(notice)}</div>{/if}
 <div class="explore-canvas" bind:this={canvas}>
  <svg viewBox={`0 0 ${width} ${height}`} aria-label={$t('文档关联图')} role="group" onpointerdown={(e)=>startDrag(e,null)} onwheel={wheel}>
   <g transform={`translate(${graph.viewport.x} ${graph.viewport.y}) scale(${graph.viewport.scale})`}>
    {#each graph.edges as edge (`${edge.source}:${edge.target}`)}
     {@const a=nodeMap.get(edge.source)!}{@const b=nodeMap.get(edge.target)!}
     <g class="explore-edge" class:connected={graph.selected===edge.source||graph.selected===edge.target}>
      <path d={`M${a.x+90},${a.y+29} L${b.x+90},${b.y+29}`} class="edge-line" style:stroke-width={edge.score/maxScore>.66?2.4:edge.score/maxScore>.33?1.7:1.1}/>
      <path d={`M${a.x+90},${a.y+29} L${b.x+90},${b.y+29}`} class="edge-hit" tabindex="0" role="button" aria-label={`${$t('查看关联依据')}：${a.name} — ${b.name}`} onpointerdown={(e)=>e.stopPropagation()} onclick={()=>showEvidence(edge)} onkeydown={(e)=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();void showEvidence(edge)}}}><title>{edge.terms.slice(0,3).join(' · ')}</title></path>
      <g class="edge-label" transform={`translate(${(a.x+b.x)/2+90} ${(a.y+b.y)/2+20})`}><rect x="-90" y="-14" width="180" height="28" rx="9"/><text text-anchor="middle" dy="5">{edge.terms.slice(0,2).join(' · ').slice(0,23)}</text></g>
     </g>
    {/each}
    {#each graph.nodes as node(node.id)}<foreignObject class="explore-node-shell" x={node.x} y={node.y} width="180" height="58"><button class="explore-node" class:selected={node.id===graph.selected} class:center={node.id===graph.center} data-document-id={node.id} title={node.path} onpointerdown={(e)=>startDrag(e,node.id)} onclick={()=>choose(node.id)}>{@html icon('file')}<span>{node.name}<small>{graph.nodes.some(other=>other.id!==node.id&&other.name===node.name)?dirname(node.path).slice(scope.length+1)||'./':node.file_type.toUpperCase()}</small></span>{#if node.id===graph.center}<i></i>{/if}</button></foreignObject>{/each}
   </g>
  </svg>
  {#if !loading&&!graph.nodes.length}<div class="explore-empty"><span>{@html icon('file')}</span><h2>{$t('这里还没有可探索的文档')}</h2><p>{$t('等待索引完成，或在左侧选择其他文件夹。')}</p></div>{/if}
  <div class="explore-zoom"><Button.Root class="icon-button" aria-label={$t('缩小')} onclick={()=>zoom(.8)}>{@html icon('minimize')}</Button.Root><Button.Root class="zoom-label" onclick={fit} title={$t('适应窗口')}>{Math.round(graph.viewport.scale*100)}%</Button.Root><Button.Root class="icon-button" aria-label={$t('放大')} onclick={()=>zoom(1.25)}>{@html icon('plus')}</Button.Root></div>
 </div>
 <div class="explore-footer">{#if selected}<div class="explore-selection"><span title={selected.path}><strong>{selected.name}</strong><small>{selected.path}</small></span><Button.Root class="outline" onclick={()=>expand(selected!.id)}>{$t('设为中心')}</Button.Root><Button.Root class="outline" onclick={()=>onopen(selected!.path,[])}>{$t('打开文档')}</Button.Root><Button.Root class="icon-button" aria-label={$t('收起此节点')} title={$t('收起此节点')} disabled={loading} onclick={collapse}>{@html icon('close')}</Button.Root></div>{:else}<p>{$t('先选择文档，再展开关联或打开原文。')}</p>{/if}
  {#if graph.neighbors}<div class="explore-more"><span>{$t('已展开 {value0} / {value1} 个关联',{value0:graph.neighbors.next_offset,value1:graph.neighbors.total})}{#if graph.neighbors.candidate_limit_reached} · {$t('当前最多比较 100 个候选文档')}{/if}</span>{#if graph.neighbors.has_more}<Button.Root class="outline" disabled={loading} onclick={()=>expand(graph.center!,true)}>{$t('继续展开 12 个')}</Button.Root>{/if}</div>{:else}<small class="explore-caption">{$t('初始仅展示部分文档入口，按文件名排序。关联来自当前文件夹及其子目录。')}</small>{/if}
 </div>
</section>
{#if evidenceOpen}<EvidenceDrawer {evidence} loading={evidenceLoading} error={evidenceError} onClose={closeEvidence} onOpen={(path,terms)=>{closeEvidence();onopen(path,terms)}}/>{/if}
