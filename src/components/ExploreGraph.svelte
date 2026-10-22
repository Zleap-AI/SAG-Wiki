<script lang="ts">
 import {onMount,onDestroy,tick} from 'svelte';
 import Sigma from 'sigma';
 import Graph from 'graphology';
 import ForceAtlas2 from 'graphology-layout-forceatlas2/worker';
 import type {ExploreGraph,ExploreEdge} from '../lib/explore';
 import {overviewEdges,graphViewportBounds,graphLayoutTargets,approachGraphPoint,graphFolderColor} from '../lib/graph-layout';
 export let graph:ExploreGraph;
 export let strength:number;
 export let onselect:(id:number|null)=>void;
 export let onfocus:(id:number)=>void;
 export let onevidence:(edge:ExploreEdge)=>void;
 export let onstate:(state:ExploreGraph)=>void;
 let container:HTMLDivElement,renderer:Sigma,network:Graph,layout:ForceAtlas2|undefined;
 let hovered:string|null=null,dragging:string|null=null,dragMoved=false,dragStart={x:0,y:0},dragBounds:{x:[number,number];y:[number,number]},key='',layoutTimer:ReturnType<typeof setTimeout>,layoutNetwork:Graph|undefined,settling=false,themeObserver:MutationObserver,resizeObserver:ResizeObserver;
 let dark=false,muted='#dadade',line='#dedee3',accent='#8573d6',paper='#ffffff';
 let edgeMap=new Map<string,ExploreEdge>(),primaryEdges=new Set<string>();
 let tooltipElement:HTMLDivElement,tooltipX=0,tooltipY=0;
 let layoutFrame=0,lastFrame=0,targetsDirty=false,simulationDone=false,targets=new Map<string,{x:number;y:number}>();
 const layoutBounds={x:[-1.5,1.5] as [number,number],y:[-1,1] as [number,number]};
 $:hoveredDocument=graph.nodes.find(node=>String(node.id)===hovered);
 function theme(){const css=getComputedStyle(container);dark=document.documentElement.dataset.theme==='dark';paper=css.getPropertyValue('--color-paper').trim()||paper;line=css.getPropertyValue('--color-rule').trim()||line;muted=line;renderer.refresh()}
 export function capture():ExploreGraph {
  if(!network)return graph;
  const camera=renderer.getCamera().getState();
  return{...graph,layoutReady:!settling,layoutVersion:7,nodes:graph.nodes.map(node=>network.hasNode(String(node.id))?{...node,x:network.getNodeAttribute(String(node.id),'x'),y:network.getNodeAttribute(String(node.id),'y')}:node),viewport:{x:camera.x,y:camera.y,angle:camera.angle,scale:camera.ratio}};
 }
 export function fit(animate=true){
  if(!network.order)return;
  if(settling)stop();renderer.setCustomBBox(layoutBounds);renderer.refresh();
  const bounds=graphViewportBounds(network.mapNodes((id,attributes)=>renderer.graphToViewport({x:attributes.x,y:attributes.y})));
  const {width,height}=renderer.getDimensions(),camera=renderer.getCamera();
  const center=renderer.viewportToFramedGraph(bounds);
  const ratio=Math.max(.08,Math.min(5,camera.ratio*Math.max(bounds.width/Math.max(1,width-96),bounds.height/Math.max(1,height-100))));
  const state={x:center.x,y:center.y,ratio};
  if(animate)void camera.animate(state,{duration:180});else camera.setState(state);
 }
 function finishLayout(){
  clearTimeout(layoutTimer);layout?.kill();layout=undefined;simulationDone=true;targetsDirty=true;
 }
 function animateLayout(now:number){
  const elapsed=lastFrame?now-lastFrame:16;lastFrame=now;
  if(targetsDirty){
   targets=new Map(graphLayoutTargets(layoutNetwork!.mapNodes((id,{x,y})=>({id,x,y}))).map(point=>[point.id,point]));
   targetsDirty=false;
  }
  const reduced=window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let remaining=0;
  if(!reduced||simulationDone)network.updateEachNodeAttributes((id,attributes)=>{
   const target=targets.get(id)!;
   const point=reduced?target:approachGraphPoint({x:attributes.x,y:attributes.y},target,elapsed);
   remaining=Math.max(remaining,Math.hypot(target.x-point.x,target.y-point.y));
   return{...attributes,...point};
  },{attributes:['x','y']});
  if(simulationDone&&remaining<.001){stop();save();return}
  layoutFrame=requestAnimationFrame(animateLayout);
 }
 export function zoom(factor:number){const camera=renderer.getCamera();void camera.animate({ratio:camera.ratio*factor},{duration:140})}
 export function locate(id:number){const point=renderer.getNodeDisplayData(String(id));if(point)void renderer.getCamera().animate({x:point.x,y:point.y,ratio:Math.min(.35,renderer.getCamera().ratio)},{duration:200})}
 function stop(){cancelAnimationFrame(layoutFrame);layoutFrame=0;lastFrame=0;clearTimeout(layoutTimer);layout?.kill();layout=undefined;layoutNetwork?.removeAllListeners();layoutNetwork=undefined;settling=false}
 function save(){onstate(capture());renderer.refresh()}
 function updateTooltip(){
  if(hovered===null||!tooltipElement)return;
  const point=renderer.graphToViewport({x:network.getNodeAttribute(hovered,'x'),y:network.getNodeAttribute(hovered,'y')});
  const {width,height}=renderer.getDimensions();
  tooltipX=Math.max(12,Math.min(point.x+12,width-tooltipElement.offsetWidth-12));
  tooltipY=Math.max(12,Math.min(point.y-tooltipElement.offsetHeight-12,height-tooltipElement.offsetHeight-12));
 }

 function sync(){
  if(!renderer)return;
  primaryEdges=new Set((graph.center===null?overviewEdges(graph.edges,graph.nodes.length,strength):graph.edges.filter(edge=>edge.score>=strength)).map(edge=>`${edge.source}:${edge.target}`));
  const next=graph.nodes.map(n=>`${n.id}:${n.hash}`).join('|')+';'+graph.edges.map(e=>`${e.source}:${e.target}:${e.score}`).join('|')+';'+graph.center;
  if(next!==key||(!graph.layoutReady&&!settling)){
   key=next;stop();network.clear();edgeMap=new Map();hovered=null;
   for(const [index,node] of graph.nodes.entries()){
    const angle=index*Math.PI*(3-Math.sqrt(5)),radius=.2*Math.sqrt((index+1)/graph.nodes.length),reset=!graph.layoutReady||graph.layoutVersion!==7;
    network.addNode(String(node.id),{x:reset?Math.cos(angle)*radius:node.x,y:reset?Math.sin(angle)*radius:node.y,label:node.name,path:node.path,size:3});
   }
   for(const edge of graph.edges){const id=`${edge.source}:${edge.target}`;edgeMap.set(id,edge);network.addUndirectedEdgeWithKey(id,String(edge.source),String(edge.target),{weight:edge.score,size:.6})}
   network.forEachNode(id=>network.setNodeAttribute(id,'size',Math.min(7,5+Math.log2(1+network.degree(id))*.4)));
   renderer.setCustomBBox(layoutBounds);
   renderer.getCamera().setState({x:graph.viewport.x,y:graph.viewport.y,ratio:graph.viewport.scale,angle:graph.viewport.angle});
   if((!graph.layoutReady||graph.layoutVersion!==7)&&network.order>1){
    layoutNetwork=network.copy();
    let index=0;layoutNetwork.updateEachNodeAttributes((id,attributes)=>{const angle=index*Math.PI*(3-Math.sqrt(5)),radius=10*Math.sqrt(++index);return{...attributes,x:Math.cos(angle)*radius,y:Math.sin(angle)*radius,size:10}});
    renderer.getCamera().setState({x:.5,y:.5,ratio:1,angle:0});
    simulationDone=false;targetsDirty=true;lastFrame=0;
    layoutNetwork.forEachEdge(id=>{if(!primaryEdges.has(id))layoutNetwork!.dropEdge(id)});
    const started=performance.now();let stable=0,iterations=0,lastPositions=new Map<string,{x:number;y:number}>();
    layoutNetwork.on('eachNodeAttributesUpdated',()=>{
     if(!layoutNetwork)return;
     let movement=0,span=1;
     layoutNetwork.forEachNode((id,{x,y})=>{
      const previous=lastPositions.get(id);
      if(previous)movement+=Math.hypot(x-previous.x,y-previous.y);
      span=Math.max(span,Math.hypot(x,y));lastPositions.set(id,{x,y});
     });
     targetsDirty=true;stable=movement/network.order/span<.001?stable+1:0;
     if(++iterations>=600&&stable>=8&&performance.now()-started>800)queueMicrotask(()=>{if(layout)finishLayout()})
    });
    layout=new ForceAtlas2(layoutNetwork,{settings:{barnesHutOptimize:true,linLogMode:false,outboundAttractionDistribution:false,gravity:1,scalingRatio:6,slowDown:2,edgeWeightInfluence:.5,adjustSizes:true}});
    settling=true;layout.start();layoutFrame=requestAnimationFrame(animateLayout);layoutTimer=setTimeout(finishLayout,5000);
   }
  }
  renderer.refresh();
 }
 $:if(renderer){graph;strength;sync()}
 onMount(()=>{
  network=new Graph({type:'undirected'});
  renderer=new Sigma(network,container,{
   itemSizesReference:'screen',zoomToSizeRatioFunction:()=>1,autoRescale:true,enableEdgeEvents:true,enableCameraRotation:false,stagePadding:32,renderLabels:false,renderEdgeLabels:false,minCameraRatio:.02,maxCameraRatio:5,allowInvalidContainer:true,
   defaultDrawNodeHover:()=>{},
   nodeReducer:(id,data)=>{
    const focus=hovered??(graph.selected===null?null:String(graph.selected));
    const adjacent=focus===null||id===focus||(network.hasNode(focus)&&network.hasEdge(id,focus)&&edgeMap.get(network.edge(id,focus)!)!.score>=strength);
    return{...data,size:data.size*(id===focus?1.25:1),color:adjacent?graphFolderColor(data.path,dark):muted,label:null,forceLabel:false,highlighted:false};
   },
   edgeReducer:(id,data)=>{
    const edge=edgeMap.get(id)!;const focus=hovered??(graph.selected===null?null:String(graph.selected));const adjacent=focus!==null&&(String(edge.source)===focus||String(edge.target)===focus);
    const color=adjacent?accent:focus!==null?paper:line;
    return{...data,color,size:adjacent?1:.6,hidden:edge.score<strength||(!primaryEdges.has(id)&&!adjacent)};
   }
  });
  theme();sync();resizeObserver=new ResizeObserver(()=>renderer.refresh({schedule:true}));resizeObserver.observe(container);themeObserver=new MutationObserver(theme);themeObserver.observe(document.documentElement,{attributes:true,attributeFilter:['data-theme','class','style']});
  renderer.on('afterRender',updateTooltip);
  renderer.on('enterNode',async({node})=>{hovered=node;renderer.refresh();await tick();updateTooltip()});
  renderer.on('leaveNode',()=>{hovered=null;renderer.refresh()});
  renderer.on('clickNode',({node})=>{hovered=null;if(!dragMoved)onselect(Number(node));dragMoved=false;renderer.refresh()});
  renderer.on('doubleClickNode',({node,event})=>{event.preventSigmaDefault();stop();save();onfocus(Number(node))});
  renderer.on('clickStage',()=>{if(!dragMoved)onselect(null);dragMoved=false});
  renderer.on('clickEdge',({edge})=>onevidence(edgeMap.get(edge)!));
  renderer.on('downNode',({node,event})=>{dragging=node;dragMoved=false;dragStart={x:event.x,y:event.y};stop();renderer.getCamera().disable();dragBounds=renderer.getCustomBBox()??layoutBounds});
  renderer.getMouseCaptor().on('mousemovebody',event=>{
   if(dragging===null)return;
   if(!dragMoved&&Math.hypot(event.x-dragStart.x,event.y-dragStart.y)>3){dragMoved=true;renderer.setCustomBBox(dragBounds)}
   if(dragMoved){const point=renderer.viewportToGraph(event);network.mergeNodeAttributes(dragging,point)}
   event.preventSigmaDefault();
  });
  renderer.getMouseCaptor().on('mouseup',()=>{if(dragging!==null){dragging=null;renderer.getCamera().enable();save()}});
 });
 onDestroy(()=>{if(renderer){const state=capture();stop();onstate(state);themeObserver.disconnect();resizeObserver.disconnect();renderer.kill()}});
</script>
<div class="explore-graph-renderer" bind:this={container}></div>


{#if hoveredDocument}<div class="graph-node-tooltip" role="tooltip" bind:this={tooltipElement} style:left="{tooltipX}px" style:top="{tooltipY}px">{hoveredDocument.name}</div>{/if}
<style>
 .graph-node-tooltip{position:absolute;z-index:4;pointer-events:none;width:max-content;max-width:min(360px,calc(100% - 24px));padding:8px 10px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-paper);color:var(--color-ink);font-size:13px;line-height:1.55;overflow-wrap:anywhere;box-shadow:0 4px 16px #00000012}
</style>
