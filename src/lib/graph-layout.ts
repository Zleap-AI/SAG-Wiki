import type {ExploreEdge} from './explore';

// Keep every component connected, then retain mutual strong links for local structure.
// The remaining evidence stays available when a document is hovered or selected.
export function overviewEdges(edges:ExploreEdge[],nodeCount:number,strength:number):ExploreEdge[]{
 const ranked=edges.filter(edge=>edge.score>=strength).sort((a,b)=>b.score-a.score||a.source-b.source||a.target-b.target);
 if(nodeCount<=40)return ranked;
 const parents=new Map<number,number>(),strongest=new Map<number,Set<number>>();
 function root(id:number):number{let parent=parents.get(id);if(parent===undefined){parents.set(id,id);return id}while(parent!==parents.get(parent)){parent=parents.get(parent)!}parents.set(id,parent);return parent}
 for(const edge of ranked)for(const [source,target] of [[edge.source,edge.target],[edge.target,edge.source]]){
  const neighbors=strongest.get(source)??new Set<number>();if(neighbors.size<2)neighbors.add(target);strongest.set(source,neighbors);
 }
 return ranked.filter(edge=>{
  const left=root(edge.source),right=root(edge.target);
  if(left!==right){parents.set(left,right);return true}
  return strongest.get(edge.source)!.has(edge.target)&&strongest.get(edge.target)!.has(edge.source);
 });
}


// Fit the main distribution without letting a few isolated nodes shrink the whole graph.
export function graphViewportBounds(points:{x:number;y:number}[]){
 const xs=points.map(point=>point.x).sort((a,b)=>a-b),ys=points.map(point=>point.y).sort((a,b)=>a-b);
 const trim=points.length>40?Math.floor(points.length*.025):0;
 const left=xs[trim],right=xs[xs.length-1-trim],top=ys[trim],bottom=ys[ys.length-1-trim];
 return{x:(left+right)/2,y:(top+bottom)/2,width:right-left,height:bottom-top};
}


export interface GraphPoint {x:number;y:number}
export function graphLayoutTargets(points:(GraphPoint&{id:string})[]){
 const bounds=graphViewportBounds(points);
 return points.map(point=>({id:point.id,x:(point.x-bounds.x)/(bounds.width||1)*2.6,y:(point.y-bounds.y)/(bounds.height||1)*1.55}));
}

export function approachGraphPoint(point:GraphPoint,target:GraphPoint,elapsed:number):GraphPoint{
 const amount=1-Math.exp(-Math.min(elapsed,64)/160);
 return{x:point.x+(target.x-point.x)*amount,y:point.y+(target.y-point.y)*amount};
}

export function graphFolderColor(path:string,dark=false):string{
 const normalized=path.replace(/\\/g,'/');
 const folder=normalized.slice(0,normalized.lastIndexOf('/'));
 let hash=0;for(const char of folder)hash=(hash*31+char.charCodeAt(0))|0;
 const palette=dark?['#A89ADD','#7AA5DA','#70B9B7','#A5BE83','#D3AC75','#D68C9F','#AF9CC2']:['#9180C5','#6E92C4','#61A5A2','#8AA768','#C39A61','#C47D95','#9B86AF'];
 return palette[(hash>>>0)%palette.length];
}
