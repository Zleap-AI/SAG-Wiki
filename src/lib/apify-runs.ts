import {get, writable} from 'svelte/store';
import {api} from './api';

type Phase = 'starting'|'running'|'cancelling'|'loading-preview'|'preview'|'preview-error'|'importing'|'import-error'|'done'|'failed'|'aborted';
type Preview = {fields:[string,string][];samples:string[];total:number|null};
type Counts = {imported:number;updated:number;skipped:number;conflicts:number;empty:number};
type Imported = Counts & {directory:string;next:number|null};
type Mapping = {title:string|null;url:string|null;id:string|null;content:string[]};
type Run = {
 phase:Phase; remoteId:string; preset:boolean; checking:boolean; error?:string; finalStatus?:string;
 preview?:Preview; mapping?:Mapping; progress?:Imported; onImported?:()=>Promise<void>;
};

// The connection dialog can be closed while a run is still in progress.
const runs = writable(new Map<string,Run>());
export const apifyRuns = {subscribe:runs.subscribe};

export function apifyRunPending(run:Run|undefined) {
 return !!run && ['starting','running','cancelling','loading-preview','importing'].includes(run.phase);
}

export function forgetApifyRun(id:string) {
 runs.update(current=>{
  const next=new Map(current);
  next.delete(id);
  return next;
 });
}

function update(id:string, change:Partial<Run>) {
 runs.update(current=>new Map(current).set(id,{...current.get(id)!,...change}));
}

export function apifyRequest<T>(id:string, action:string, args:Record<string,unknown>, token='') {
 return api<T>('online_import',{action:{action:'apify',token,connection:id||null,request:{action,...args}}});
}

function errorMessage(reason:unknown) {
 return String(reason).replace(/^Error: /,'');
}

export async function startApifyRun(id:string, actor:string, input:unknown, preset:boolean, onImported?:()=>Promise<void>) {
 if(apifyRunPending(get(runs).get(id)))return;
 runs.update(current=>new Map(current).set(id,{phase:'starting',remoteId:'',preset,checking:false,onImported}));
 try{
  const {id:remoteId}=await apifyRequest<{id:string}>(id,'start',{actor,input});
  update(id,{phase:'running',remoteId});
 }catch(reason){
  update(id,{phase:'failed',error:errorMessage(reason)});
  return;
 }
 void checkApifyRun(id);
}

export async function checkApifyRun(id:string) {
 const run=get(runs).get(id);
 if(!run||run.checking||(run.phase!=='running'&&run.phase!=='cancelling'))return;
 update(id,{checking:true,error:undefined});
 let status:string;
 try{
  ({status}=await apifyRequest<{status:string}>(id,'status',{run:run.remoteId}));
 }catch(reason){
  update(id,{checking:false,error:errorMessage(reason)});
  return;
 }
 switch(status){
  case 'ABORTING':
  case 'READY':
  case 'RUNNING':
  case 'TIMING-OUT':
   if(status==='ABORTING')update(id,{phase:'cancelling'});
   setTimeout(()=>{
    update(id,{checking:false});
    void checkApifyRun(id);
   },2000);
   break;
  case 'SUCCEEDED':
   update(id,{phase:run.preset?'import-error':'preview-error',checking:false,error:undefined});
   if(run.preset)void importApifyRun(id,{title:null,url:null,id:null,content:[]});
   else void previewApifyRun(id);
   break;
  case 'ABORTED':
   update(id,{phase:'aborted',checking:false,error:undefined});
   break;
  default:
   update(id,{phase:'failed',checking:false,error:undefined,finalStatus:status});
 }
}

export async function cancelApifyRun(id:string) {
 const run=get(runs).get(id);
 if(run?.phase!=='running')return;
 update(id,{phase:'cancelling'});
 try{
  await apifyRequest(id,'cancel',{run:run.remoteId});
  void checkApifyRun(id);
 }catch(reason){
  const current=get(runs).get(id);
  if(current?.remoteId===run.remoteId&&current.phase==='cancelling')update(id,{phase:'running',error:errorMessage(reason)});
 }
}

export async function previewApifyRun(id:string) {
 const run=get(runs).get(id);
 if(run?.phase!=='preview-error')return;
 update(id,{phase:'loading-preview',error:undefined});
 try{
  const preview=await apifyRequest<Preview>(id,'preview',{run:run.remoteId});
  update(id,{phase:'preview',preview});
 }catch(reason){update(id,{phase:'preview-error',error:errorMessage(reason)})}
}

export async function importApifyRun(id:string, selection?:Mapping) {
 const run=get(runs).get(id);
 if(!run||(run.phase!=='preview'&&run.phase!=='import-error'))return;
 const mapping=run.mapping??selection;
 if(!mapping)return;
 update(id,{phase:'importing',mapping,error:undefined});
 let progress=run.progress;
 let changed=false;
 let error:string|undefined;
 try{
  do{
   const batch=await apifyRequest<Imported>(id,'import',{
    run:run.remoteId,directory:'',offset:progress?.next??0,preset:run.preset,mapping,
   });
   progress={...batch,
    imported:(progress?.imported??0)+batch.imported,updated:(progress?.updated??0)+batch.updated,
    skipped:(progress?.skipped??0)+batch.skipped,conflicts:(progress?.conflicts??0)+batch.conflicts,
    empty:(progress?.empty??0)+batch.empty,
   };
   changed||=!!(batch.imported||batch.updated);
  }while(progress.next!==null);
 }catch(reason){error=errorMessage(reason)}
 if(changed){
  try{await run.onImported?.()}
  catch(reason){error??=errorMessage(reason)}
 }
 update(id,{phase:progress?.next===null?'done':'import-error',progress,error});
}
