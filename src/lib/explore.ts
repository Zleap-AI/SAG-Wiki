import type {Candidate} from './types';
export interface ExploreDocument {id:number;path:string;name:string;hash:string;file_type:string}
export interface ExploreNode extends ExploreDocument {x:number;y:number}
export interface ExploreEdge {source:number;target:number;source_hash:string;target_hash:string;score:number;terms:string[];snapshot:string}
export interface ExploreOverview {scope:string;version:number;total:number;documents:ExploreDocument[];known_documents:ExploreDocument[];indexing:boolean;partial:boolean}
export interface ExploreGraphData {scope:string;version:number;snapshot:string;documents:ExploreDocument[];edges:ExploreEdge[]}
export interface ExploreEvidence {version:number;left:ExploreDocument;right:ExploreDocument;pairs:{left:Candidate;right:Candidate;terms:string[]}[]}
export interface ExploreGraph {nodes:ExploreNode[];edges:ExploreEdge[];center:number|null;selected:number|null;viewport:{x:number;y:number;scale:number;angle:number};layoutReady:boolean;layoutVersion?:number}
export interface LibrarySession {mode:'files'|'explore';fileScroll:number;initialized:boolean;version:number;graph:ExploreGraph;history:ExploreGraph[];overview:ExploreGraphData|null;strength:number}
export function emptyGraph():ExploreGraph {return{nodes:[],edges:[],center:null,selected:null,viewport:{x:.5,y:.5,scale:1,angle:0},layoutReady:false}}
export function createLibrarySession():LibrarySession {return{mode:'files',fileScroll:0,initialized:false,version:0,graph:emptyGraph(),history:[],overview:null,strength:.08}}
export function copyGraph(graph:ExploreGraph):ExploreGraph {return structuredClone(graph)}
export function graphFromData(data:ExploreGraphData,center:number|null=null):ExploreGraph {
 const edges=center===null?data.edges:data.edges.filter(edge=>edge.source===center||edge.target===center);
 const ids=new Set(edges.flatMap(edge=>[edge.source,edge.target]));if(center!==null)ids.add(center);
 const documents=center===null?data.documents:data.documents.filter(doc=>ids.has(doc.id));
 const nodes=documents.map((doc,index)=>{const angle=index*Math.PI*(3-Math.sqrt(5)),radius=10*Math.sqrt(index+1);return{...doc,x:Math.cos(angle)*radius,y:Math.sin(angle)*radius}});
 return{...emptyGraph(),nodes,edges,center,selected:center};
}
