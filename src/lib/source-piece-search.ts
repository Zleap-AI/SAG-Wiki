import {pieceIndex,type MarkdownModel} from './document-model';
import {cooperativeTextMatches,textMatches,type TextMatch} from './text-matches';
import {sourcePieceLimit} from './large-document-policy';

const segmenter=new Intl.Segmenter(undefined,{granularity:'grapheme'});
type Window={base:number;from:number;end:number};
type PackedGroup={base:number;pairs:Uint32Array};
type PackedSlice={group:PackedGroup;first:number;last:number};
const yieldTask=()=>new Promise<void>(resolve=>setTimeout(resolve,0));

export class SourcePieceSearch{
 private windows=new Map<number,Window>();
 private packed=new Map<number,PackedSlice>();
 private nextStart=new Map<number,number>();
 private owner=new Map<number,number>();
 private nextBoundary=new Map<number,number>();
 private width:number;
 private cancelWorker?:()=>void;
 private disposed=false;
 constructor(private model:MarkdownModel,private terms:string[]){
  this.width=terms.reduce((n,term)=>Math.max(n,term.length,term.normalize('NFKC').length),0)*4+16;
 }
 dispose(){this.disposed=true;this.cancelWorker?.();}
 private lastInGroup(index:number){
  const pieces=this.model.pieces;let lo=index,hi=pieces.length;
  while(hi-lo>1){const mid=(lo+hi)>>1;if(pieces[mid].group===pieces[index].group)lo=mid;else hi=mid;}
  return lo;
 }
 // Inspect only a bounded neighbourhood starting at a previously established
 // grapheme boundary. If its right edge is inside a giant grapheme, the worker
 // owns the rest of this semantic group, including its artificial display cuts.
 private window(index:number,groupEnd:number):Window|null{
  const p=this.model.pieces[index],base=this.nextBoundary.get(index-1)??p.from;
  const from=Math.max(p.from,this.nextStart.get(index-1)??p.from);
  const right=Math.min(groupEnd,p.to+this.width),end=Math.min(groupEnd,right+sourcePieceLimit);
  if(end-base>4*sourcePieceLimit)return null;
  const text=this.model.content;
  const asciiBreak=(offset:number)=>offset===base||offset===groupEnd||(
   text.charCodeAt(offset-1)<128&&text.charCodeAt(offset)<128&&!(text[offset-1]==='\r'&&text[offset]==='\n')
  );
  if(asciiBreak(from)&&asciiBreak(p.to)&&asciiBreak(right)){
   this.nextBoundary.set(index,p.to);return {base:from,from,end:right};
  }
  const segments=segmenter.segment(text.slice(base,end));
  const bounds=(offset:number)=>{
   if(offset===end)return {start:end,end};
   const part=segments.containing(offset-base);
   return part?{start:base+part.index,end:base+part.index+part.segment.length}:{start:offset,end:offset};
  };
  const left=bounds(Math.min(from,end)),cut=bounds(p.to),tail=bounds(right);
  if(end<groupEnd&&[left,cut,tail].some(part=>part.end===end))return null;
  this.nextBoundary.set(index,cut.start);
  return {base:left.start,from,end:tail.start===right?right:tail.end};
 }
 matches(index:number):TextMatch[]{
  const slice=this.packed.get(index);
  if(slice){
   const {base,pairs}=slice.group,found:TextMatch[]=[];
   for(let i=slice.first;i<slice.last;i+=2)found.push({from:base+pairs[i],to:base+pairs[i+1]});
   return found;
  }
  const w=this.windows.get(index),p=this.model.pieces[index];
  if(!w||w.from>=p.to)return [];
  return textMatches(this.model.content.slice(w.base,w.end),this.terms,w.from-w.base)
   .map(match=>({from:w.base+match.from,to:w.base+match.to})).filter(match=>match.from>=w.from&&match.from<p.to);
 }
 private async scanExceptional(text:string,sourceStart:number,current:()=>boolean){
  if(typeof Worker!=='undefined'){
   try{
    const worker=new Worker(new URL('./source-search.worker.ts',import.meta.url),{type:'module'});
    return await new Promise<Uint32Array|null>((resolve,reject)=>{
     const stop=()=>{worker.terminate();this.cancelWorker=undefined;};
     this.cancelWorker=()=>{stop();resolve(null);};
     worker.onmessage=({data}:{data:Uint32Array})=>{stop();resolve(data);};
     worker.onerror=event=>{event.preventDefault();stop();reject(new Error('Source search worker failed'));};
     worker.onmessageerror=()=>{stop();reject(new Error('Source search worker response failed'));};
     try{worker.postMessage({text,terms:this.terms,sourceStart});}catch(e){stop();reject(e);}
    });
   }catch{/* Use the same once-per-group operation when a host cannot start a worker. */}
  }
  await yieldTask();
  return cooperativeTextMatches(text,this.terms,()=>!this.disposed&&current(),sourceStart);
 }
 private async prepareExceptional(first:number,last:number,current:()=>boolean){
  const pieces=this.model.pieces;
  const base=this.nextBoundary.get(first-1)??pieces[first].from;
  const from=Math.max(pieces[first].from,this.nextStart.get(first-1)??0);
  const pairs=await this.scanExceptional(this.model.content.slice(base,pieces[last].to),from-base,current);
  if(!pairs||this.disposed||!current())return false;
  const group={base,pairs};let end=this.nextStart.get(first-1)??0,owner=this.owner.get(first-1)??-1;
  let cursor=0,time=performance.now();
  for(let i=first;i<=last;i++){
   const start=cursor;
   while(cursor<pairs.length&&base+pairs[cursor]<pieces[i].to)cursor+=2;
   if(cursor>start){end=base+pairs[cursor-1];owner=base+pairs[cursor-2];}
   this.packed.set(i,{group,first:start,last:cursor});this.nextStart.set(i,end);this.owner.set(i,owner);
   if(performance.now()-time>=5){await yieldTask();if(this.disposed||!current())return false;time=performance.now();}
  }
  return true;
 }
 async prepare(index:number,current:()=>boolean){
  const p=this.model.pieces[index];let start=index;
  const first=pieceIndex(this.model,p.group??p.from),last=this.lastInGroup(index);
  while(start>first&&!this.nextStart.has(start-1))start--;
  let time=performance.now();
  for(let i=start;i<=index;i++){
   if(this.disposed||!current())return false;
   if(!this.nextStart.has(i)){
    const w=this.window(i,this.model.pieces[last].to);
    if(!w)return this.prepareExceptional(i,last,current);
    this.windows.set(i,w);
    const hit=this.matches(i).at(-1),end=hit?.to??0;
    this.owner.set(i,hit?.from??this.owner.get(i-1)??-1);
    this.nextStart.set(i,Math.max(end,this.nextStart.get(i-1)??0));
   }
   if(performance.now()-time>=5){await yieldTask();time=performance.now();}
  }
  return !this.disposed&&current();
 }
 paint(doc:Document,index:number,selected:number){
  const model=this.model,p=model.pieces[index],pre=doc.querySelector('pre')!;
  const matches=this.matches(index);
  const visible=matches.map((m,id)=>({...m,id,owner:m.from})).filter(m=>m.id<256||m.id===selected);
  const previous=this.nextStart.get(index-1)??0;
  if(previous>p.from)visible.unshift({from:p.from,to:previous,id:-1,owner:this.owner.get(index-1)??-1});
  for(const part of pre.querySelectorAll<HTMLElement>('.source-subpiece')){
   const from=Number(part.dataset.from),to=Number(part.dataset.to);let cursor=from;
   part.replaceChildren();
   for(const mark of visible){
    if(mark.from>=to||mark.to<=from)continue;
    const start=Math.max(mark.from,from),end=Math.min(mark.to,to);
    if(start>cursor)part.append(model.content.slice(cursor,start));
    const el=doc.createElement('mark');el.className='search-hit';el.dataset.searchMatch=String(mark.id);el.dataset.sourceHit=String(mark.owner);
    el.textContent=model.content.slice(start,end);part.append(el);cursor=end;
   }
   if(cursor<to)part.append(model.content.slice(cursor,to));
  }
  return matches.length;
 }
}
