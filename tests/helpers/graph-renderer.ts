import {vi} from 'vitest';
import type Graph from 'graphology';
const state=vi.hoisted(()=>({layouts:[] as Array<{graph:Graph}>,instances:[] as Array<{emit:(event:string,payload:unknown)=>void;camera:{x:number;y:number;ratio:number;angle:number};bounds:unknown;graph:Graph;refresh:ReturnType<typeof vi.fn>;options:any}> }));
vi.mock('sigma',()=>({DEFAULT_STYLES:{nodes:{x:{attribute:'x'},y:{attribute:'y'}},edges:{}},default:class {
 bounds:unknown=null;handlers=new Map<string,Function>();camera={x:.5,y:.5,ratio:1,angle:0};
 constructor(public graph:any,container:HTMLElement,public options:any){state.instances.push(this)}
 on(name:string,handler:Function){this.handlers.set(name,handler)}
 emit(name:string,payload:unknown){this.handlers.get(name)?.(payload)}
 refresh=vi.fn();resize(){}kill(){}setCustomBBox(value:unknown){this.bounds=value}getBBox(){return{x:[0,1],y:[0,1]}}getCustomBBox(){return this.bounds}
 getCamera(){return{getState:()=>({...this.camera}),setState:(state:any)=>Object.assign(this.camera,state),animate:(state:any)=>Object.assign(this.camera,state),animatedReset:()=>Object.assign(this.camera,{x:.5,y:.5,ratio:1,angle:0}),disable(){},enable(){}}}
 getNodeDisplayData(id:string){return this.graph.getNodeAttributes(id)}
 getMouseCaptor(){return{on:(name:string,handler:Function)=>this.handlers.set('mouse:'+name,handler)}}
 viewportToGraph(point:unknown){return point}
 viewportToFramedGraph(point:unknown){return point}
 getDimensions(){return{width:1000,height:650}}
 graphToViewport(point:{x:number;y:number}){return point}
}}));
vi.mock('graphology-layout-forceatlas2/worker',()=>({default:class{constructor(public graph:any){state.layouts.push(this)}start(){}kill(){}}}));
export const rendererState=state;
export function graphEvent(event:string,payload:unknown){rendererState.instances.at(-1)!.emit(event,payload)}
