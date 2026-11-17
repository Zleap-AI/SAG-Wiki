export type TextMatch={from:number;to:number};
const segmenter=new Intl.Segmenter(undefined,{granularity:'grapheme'});

// Preserve the source extent of a normalized grapheme without allocating a mapping
// object for every UTF-16 code unit of that grapheme.
function* scanMatches(text:string,terms:string[],sourceStart:number,segmentFirst=false):Generator<TextMatch|null>{
 const words=[...new Set(terms.map(term=>term.normalize('NFKC').toLowerCase()).filter(Boolean))].sort((a,b)=>b.length-a.length);
 if(!words.length)return;
 const pattern=new RegExp(words.map(word=>word.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')).join('|'),'gu');
 let normalized=segmentFirst?'':text.normalize('NFKC').toLowerCase();yield null;
 const spans:{from:number;to:number;sourceFrom:number;sourceTo:number}[]=[];
 if(segmentFirst||normalized!==text.toLowerCase()||normalized.length!==text.length){
  normalized='';let canonicalText='',checkpoint=0;
  for(const {segment,index} of segmenter.segment(text)){
   const canonical=segment.normalize('NFKC'),folded=canonical.toLowerCase(),from=normalized.length;
   canonicalText+=canonical;
   normalized+=folded;
   if(segment.length!==1||folded.length!==1)spans.push({from,to:normalized.length,sourceFrom:index,sourceTo:index+segment.length});
   if(index+segment.length>=checkpoint+4096){checkpoint=index+segment.length;yield null;}
  }
  // Lowercase the complete canonical text to retain context-sensitive casing
  // (notably Greek final sigma) even when another grapheme changed under NFKC.
  // Contextual default casing changes characters, not the mapped UTF-16 widths.
  normalized=canonicalText.toLowerCase();
  if(segmentFirst&&normalized.length===text.length&&normalized===text.toLowerCase())spans.length=0;
 }
 let spanIndex=0,delta=0,lastEnd=0;
 const offset=(position:number,end:boolean)=>{
  while(spanIndex<spans.length&&spans[spanIndex].to<=position){
   const span=spans[spanIndex++];delta=span.sourceTo-span.to;
  }
  const span=spans[spanIndex];
  if(span&&span.from<=position)return end?span.sourceTo:span.sourceFrom;
  return position+delta+(end?1:0);
 };
 // A suffix of one longest term preserves matches crossing each search window.
 const width=Math.max(4096,words[0].length);
 const boundary=(at:number)=>at<normalized.length&&/[\uD800-\uDBFF]/.test(normalized[at-1])&&/[\uDC00-\uDFFF]/.test(normalized[at])?at+1:at;
 // Start matching after the previous owner, while retaining the earlier
 // graphemes as normalization context. Filtering matches afterwards would
 // shift the phase of repeated nonoverlapping terms.
 while(spanIndex<spans.length&&spans[spanIndex].sourceTo<=sourceStart){
  const span=spans[spanIndex++];delta=span.sourceTo-span.to;
 }
 const containing=spans[spanIndex];
 const normalizedStart=containing&&containing.sourceFrom<sourceStart?containing.to:sourceStart-delta;
 for(let start=boundary(normalizedStart);start<normalized.length;){
  const end=boundary(Math.min(normalized.length,start+width));
  const tail=boundary(Math.min(normalized.length,end+words[0].length));
  let next=end;
  for(const match of normalized.slice(start,tail).matchAll(pattern)){
   const at=start+match.index;if(at>=end)break;
   next=Math.max(next,at+match[0].length);
   const from=offset(at,false),to=offset(at+match[0].length-1,true);
   if(from<lastEnd)continue;
   yield {from,to};lastEnd=to;
  }
  start=next;yield null;
 }
}
export function textMatches(text:string,terms:string[],sourceStart=0){
 const found:TextMatch[]=[];
 for(const hit of scanMatches(text,terms,sourceStart))if(hit)found.push(hit);
 return found;
}
function* pack(text:string,terms:string[],sourceStart:number):Generator<void,Uint32Array>{
 let pairs=new Uint32Array(0),size=0;
 for(const hit of scanMatches(text,terms,sourceStart,true)){
  if(!hit){yield;continue;}
  if(size===pairs.length){const next=new Uint32Array(Math.max(64,size*2));next.set(pairs);pairs=next;}
  pairs[size++]=hit.from;pairs[size++]=hit.to;
 }
 return pairs.subarray(0,size);
}
export function packedTextMatches(text:string,terms:string[],sourceStart=0){
 const scan=pack(text,terms,sourceStart);let step=scan.next();while(!step.done)step=scan.next();
 return step.value;
}
// A host that denies workers can still yield between batches of normalization
// and matching. A single indivisible grapheme is normalized once as a unit.
export async function cooperativeTextMatches(text:string,terms:string[],current:()=>boolean,sourceStart=0){
 const scan=pack(text,terms,sourceStart);let start=performance.now();
 while(current()){
  const step=scan.next();if(step.done)return step.value;
  if(performance.now()-start>=5){await new Promise(resolve=>setTimeout(resolve,0));start=performance.now();}
 }
 return null;
}
