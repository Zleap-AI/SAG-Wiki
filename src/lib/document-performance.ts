/** Opt-in diagnostics. Keep this schema closed: no paths, names, bodies or arbitrary metadata. */
export type DocumentStage='read-rpc'|'target'|'model'|'component'|'editor-init'|'reader-render'|'reader-dom'|'reader-index'|'reader-images';
export type DocumentRenderer='editor'|'reader'|'readonly';
export type DocumentSample={id:number;renderer:DocumentRenderer;status:'running'|'complete'|'cancelled';durationMs:number;firstPaintMs:number|null;stages:Partial<Record<DocumentStage,{durationMs:number;count:number}>>};
export type DocumentTrace={readonly enabled:boolean;readonly active:boolean;setRenderer:(renderer:DocumentRenderer)=>void;stage:(name:DocumentStage)=>()=>void;firstPaint:()=>void;end:()=>void;cancel:()=>void};
const stages=new Set<DocumentStage>(['read-rpc','target','model','component','editor-init','reader-render','reader-dom','reader-index','reader-images']);
const noop=()=>{};
const disabledTrace:DocumentTrace=Object.freeze({enabled:false,active:false,setRenderer:noop,stage:()=>noop,firstPaint:noop,end:noop,cancel:noop});
const finite=(value:number)=>Number.isFinite(value)?Math.max(0,value):0;

export function createDocumentPerformance({now=()=>performance.now()}:{now?:()=>number}={}){
 let enabled=false,maxSamples=30,nextId=0,active:DocumentTrace|undefined;
 const samples:DocumentSample[]=[];
 const trim=()=>{if(samples.length>maxSamples)samples.splice(0,samples.length-maxSamples);};
 function cancel(){active?.cancel();active=undefined;}
 return {
  configure(options:{enabled:boolean;maxSamples?:number}){
   if(!options.enabled)cancel();enabled=options.enabled;
   if(options.maxSamples!==undefined)maxSamples=Number.isFinite(options.maxSamples)?Math.max(1,Math.min(100,Math.floor(options.maxSamples))):30;
   trim();
  },
  begin(renderer:DocumentRenderer='editor'):DocumentTrace{
   if(!enabled)return disabledTrace;
   cancel();
   const started=finite(now());
   const elapsed=()=>finite(now()-started);
   const sample:DocumentSample={id:++nextId,renderer:['editor','reader','readonly'].includes(renderer)?renderer:'editor',status:'running',durationMs:0,firstPaintMs:null,stages:{}};
   samples.push(sample);trim();
   const finish=(status:'complete'|'cancelled')=>{if(sample.status!=='running')return;sample.durationMs=elapsed();sample.status=status;if(active===trace)active=undefined;};
   const trace:DocumentTrace={enabled:true,get active(){return sample.status==='running';},
    setRenderer(value){if(sample.status==='running'&&['editor','reader','readonly'].includes(value))sample.renderer=value;},
    stage(name){
     if(sample.status!=='running'||!stages.has(name))return noop;
     const start=elapsed();let stopped=false;
     return ()=>{if(stopped||sample.status!=='running')return;stopped=true;const value=sample.stages[name]??{durationMs:0,count:0};value.durationMs=finite(value.durationMs+finite(elapsed()-start));value.count=Math.min(Number.MAX_SAFE_INTEGER,value.count+1);sample.stages[name]=value;};
    },
    firstPaint(){if(sample.status==='running'&&sample.firstPaintMs===null)sample.firstPaintMs=elapsed();},
    end(){finish('complete');},cancel(){finish('cancelled');}
   };
   active=trace;return trace;
  },
  cancel,
  snapshot():DocumentSample[]{return samples.map(sample=>({...sample,stages:Object.fromEntries(Object.entries(sample.stages).map(([key,value])=>[key,{...value}]))}));},
  reset(){cancel();samples.length=0;}
 };
}

// Shared by App, editor and reader; enabling and resetting are explicit diagnostic actions.
const documentPerformance=createDocumentPerformance();
export const configureDocumentPerformance=documentPerformance.configure;
export const beginDocumentTrace=documentPerformance.begin;
export const cancelDocumentTrace=documentPerformance.cancel;
export const documentPerformanceSnapshot=documentPerformance.snapshot;
export const resetDocumentPerformance=documentPerformance.reset;

/** Settle component microtasks, then allow one frame to paint before recording the next. */
export async function afterDocumentPaint(trace:DocumentTrace,current:()=>boolean=()=>true,frame:(callback:FrameRequestCallback)=>number=requestAnimationFrame):Promise<void>{
 if(!trace.enabled)return;
 await Promise.resolve();
 await new Promise<void>(resolve=>{
  const valid=()=>{if(trace.active&&current())return true;trace.cancel();resolve();return false;};
  if(!valid())return;
  frame(()=>{if(!valid())return;frame(()=>{if(valid()){trace.firstPaint();trace.end();resolve();}});});
 });
}
