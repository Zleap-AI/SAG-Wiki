import type {Candidate} from './types';
export interface ExploreDocument {id:number;path:string;name:string;hash:string;file_type:string}
export interface ExploreNode extends ExploreDocument {x:number;y:number}
export interface ExploreEdge {source:number;target:number;source_hash:string;target_hash:string;score:number;terms:string[];snapshot:string}
export interface ExploreOverview {scope:string;version:number;total:number;documents:ExploreDocument[];known_documents:ExploreDocument[];indexing:boolean;partial:boolean}
export interface ExploreNeighbors {version:number;snapshot:string;center:ExploreDocument;documents:ExploreDocument[];edges:ExploreEdge[];total:number;has_more:boolean;next_offset:number;candidate_limit_reached:boolean}
export interface ExploreEvidence {version:number;left:ExploreDocument;right:ExploreDocument;pairs:{left:Candidate;right:Candidate;terms:string[]}[]}
export interface ExploreGraph {nodes:ExploreNode[];edges:ExploreEdge[];center:number|null;selected:number|null;viewport:{x:number;y:number;scale:number};neighbors:ExploreNeighbors|null}
export interface LibrarySession {mode:'files'|'explore';fileScroll:number;initialized:boolean;version:number;graph:ExploreGraph;history:ExploreGraph[]}
export function emptyGraph():ExploreGraph {return{nodes:[],edges:[],center:null,selected:null,viewport:{x:0,y:0,scale:1},neighbors:null}}
export function createLibrarySession():LibrarySession {return{mode:'files',fileScroll:0,initialized:false,version:0,graph:emptyGraph(),history:[]}}
export function copyGraph(graph:ExploreGraph):ExploreGraph {return JSON.parse(JSON.stringify(graph))}
export function entryNodes(documents:ExploreDocument[]):ExploreNode[]{return documents.map((doc,i)=>({...doc,x:60+(i%4)*220,y:55+Math.floor(i/4)*95}))}
export const EXPLORE_NODE_WIDTH=220;
export const EXPLORE_NODE_HEIGHT=92;
export function addNeighbors(graph:ExploreGraph,result:ExploreNeighbors):ExploreGraph{
 const documents=graph.center===result.center.id?graph.nodes.filter(n=>n.id!==result.center.id):[];
 for(const doc of result.documents){if(!documents.some(n=>n.id===doc.id))documents.push({...doc,x:0,y:0})}
 const edges=graph.center===result.center.id?[...graph.edges]:[];
 for(const edge of result.edges){if(!edges.some(e=>e.source===edge.source&&e.target===edge.target))edges.push(edge)}
 const rows=Math.ceil(documents.length/2);
 const nodes=[{...result.center,x:320,y:Math.max(0,rows-1)*58},...documents.map((doc,index)=>({...doc,x:index%2===0?0:640,y:Math.floor(index/2)*116}))];
 return{...graph,nodes,edges,center:result.center.id,selected:result.center.id,neighbors:result};
}
