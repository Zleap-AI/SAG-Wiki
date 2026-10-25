<script lang="ts">
 import {onDestroy,onMount} from 'svelte';
 import {hydrateImages} from '../lib/markdown';
 import {highlightDocument} from '../lib/search-highlights';
 import {pieceHtml,pieceIndex,piecePrefixHtml,type MarkdownModel} from '../lib/document-model';
 import MatchNavigation from './MatchNavigation.svelte';
 import {quietScroll} from '../lib/quiet-scroll';
 import {readReaderScroll,rememberReaderScroll} from '../lib/reader-scroll';
 // Renders a large Markdown document progressively: the piece holding `target`
 // and its neighbours first, so the passage shows at once, then the rest in
 // short slices outward from it. Matches are numbered per piece and summed.
 export let model:MarkdownModel;
 export let terms:string[]=[];
 // Content offset of the passage to show.
 export let target=0;
 // Limits highlighting to these content ranges' pieces (a citation); null highlights everywhere.
 export let within:{from:number;to:number}[]|null=null;
 export let path='';
 export let label:'match'|'citation'='match';
 export let showNavigation=true;
 export let memoryKey='';
 export let restoreScroll=false;

 let host:HTMLDivElement;
 let sections:HTMLElement[]=[];
 const counts:(number|undefined)[]=new Array(model.pieces.length);
 let current:{piece:number;local:number}|null=null;
 let total=0,complete=false,position:number|null=null,destroyed=false,userMoved=false;
 let charHeight=0.7;
 const highlighted=model.pieces.map(piece=>!within||within.some(range=>piece.from<Math.max(range.to,range.from+1)&&piece.to>range.from));
 const targetPiece=pieceIndex(model,target);
 let nextMatchPiece=targetPiece;
 const positionKey=memoryKey,saved=restoreScroll?readReaderScroll(positionKey):undefined;

 function remember(){
  if(!host||!sections.length)return;
  const piece=firstVisible();
  rememberReaderScroll(positionKey,{top:host.scrollTop,left:host.scrollLeft,piece,offset:host.scrollTop-sections[piece].offsetTop});
 }

 function estimate(index:number){const piece=model.pieces[index];return Math.max(24,Math.round((piece.to-piece.from)*charHeight))}

 function fill(index:number){
  const doc=new DOMParser().parseFromString(pieceHtml(model,index),'text/html');
  counts[index]=highlighted[index]&&terms.length?highlightDocument(doc,terms):0;
  const section=sections[index];
  section.style.minHeight='';section.dataset.state='rendered';
  section.replaceChildren(...doc.body.childNodes);
  observer?.observe(section);
 }

 // Renders a piece in place; keeps the reading position when it sits above the viewport.
 function render(index:number){
  const section=sections[index];
  if(destroyed||!section||section.dataset.state)return;
  const above=section.offsetTop+section.offsetHeight<=host.scrollTop,before=section.offsetHeight;
  fill(index);
  if(above)host.scrollTop+=section.offsetHeight-before;
 }

 // First piece whose bottom is below the top of the viewport.
 function firstVisible(){
  const top=host.scrollTop;let low=0,high=sections.length-1;
  while(low<high){const mid=(low+high)>>1;const section=sections[mid];if(section.offsetTop+section.offsetHeight<=top)low=mid+1;else high=mid}
  return low;
 }

 // Renders pieces for about 6 ms with one layout read before and one after,
 // instead of forcing a layout of the whole document per piece. Pieces above
 // the viewport still hold their placeholder height, so the shift is exact.
 function renderSlice(order:number[],next:number){
  const visible=firstVisible(),above:{index:number;height:number}[]=[],start=performance.now();
  while(next<order.length&&(next===0||performance.now()-start<6)){
   const index=order[next++];
   if(sections[index].dataset.state)continue;
   const height=parseFloat(sections[index].style.minHeight)||0;
   fill(index);
   if(index<visible)above.push({index,height});
  }
  const shift=above.reduce((sum,{index,height})=>sum+sections[index].offsetHeight-height,0);
  if(shift)host.scrollTop+=shift;
  return next;
 }

 function hitsIn(index:number){return sections[index]?.querySelectorAll<HTMLElement>('[data-search-match]')??[]}

 function activate(next:{piece:number;local:number},scroll=true){
  if(destroyed)return;
  host.querySelectorAll('.search-hit.active').forEach(mark=>mark.classList.remove('active'));
  current=next;
  const marks=[...hitsIn(next.piece)].filter(mark=>mark.dataset.searchMatch===String(next.local));
  marks.forEach(mark=>mark.classList.add('active'));
  if(scroll)marks[0]?.scrollIntoView({block:'center'});
  updatePosition();
 }

 function updatePosition(){
  if(!current){position=null;return}
  let sum=0;
  for(let index=0;index<current.piece;index++){if(counts[index]===undefined&&highlighted[index]){position=null;return}sum+=counts[index]??0}
  position=sum+current.local;
 }

 // Check the rendered prefix after the target. Resume from the first unknown
 // piece after each background slice instead of synchronously rendering ahead.
 function firstMatchFromTarget(){
  while(nextMatchPiece<model.pieces.length){
   const index=nextMatchPiece;
   if(!highlighted[index]){nextMatchPiece++;continue}
   if(counts[index]===undefined)return null;
   nextMatchPiece++;
   const before=index===targetPiece?highlightDocument(new DOMParser().parseFromString(piecePrefixHtml(model,index,target),'text/html'),terms):0;
   if((counts[index]??0)>before)return {piece:index,local:before};
  }
  return null;
 }

 function move(step:number){
  if(!current)return;
  let {piece,local}=current;local+=step;
  for(let guard=0;guard<model.pieces.length*2;guard++){
   if(local>=0&&local<(counts[piece]??0))return activate({piece,local});
   piece=step>0?piece+1:piece-1;
   if(piece>=model.pieces.length)piece=0;
   if(piece<0)piece=model.pieces.length-1;
   if(!highlighted[piece])continue;
   render(piece);
   local=step>0?0:(counts[piece]??0)-1;
  }
 }

 // Background work yields at the lowest priority so timers and input stay prompt.
 // (scheduler.yield would resume ahead of other tasks and starve them.)
 const channel=typeof MessageChannel==='undefined'?null:new MessageChannel();
 const yieldToMain=()=>new Promise<void>(resolve=>{
  const scheduler=(globalThis as {scheduler?:{postTask?:(task:()=>void,options:{priority:string})=>Promise<void>}}).scheduler;
  if(scheduler?.postTask)void scheduler.postTask(resolve,{priority:'background'});
  else if(channel){channel.port1.onmessage=()=>resolve();channel.port2.postMessage(0)}
  else setTimeout(resolve,0);
 });

 // Remaining pieces, alternating after and before the start, in slices of about 6 ms.
 async function renderRest(start:number){
  const order:number[]=[];
  for(let distance=1;order.length<model.pieces.length-1;distance++){
   if(start+distance<model.pieces.length)order.push(start+distance);
   if(start-distance>=0)order.push(start-distance);
  }
  // Let the target and its neighbours paint before doing any background work.
  await yieldToMain();
  if(destroyed)return;
  let next=0;
  while(next<order.length){
   if(destroyed)return;
   next=renderSlice(order,next);
   const first=!current&&terms.length?firstMatchFromTarget():null;
   if(first)activate(first,!userMoved);else updatePosition();
   await yieldToMain();
  }
  total=counts.reduce<number>((sum,count)=>sum+(count??0),0);
  complete=true;
  // A semantic result or an updated file may have matches only before its
  // target. Select the last one after background counting, as the whole-document
  // reader does, without forcing a synchronous scan of all earlier pieces.
  if(!current&&total){
   for(let piece=counts.length-1;piece>=0;piece--){
    if(counts[piece]){activate({piece,local:counts[piece]!-1},!userMoved);break}
   }
  }else updatePosition();
 }

 // Images load lazily near the viewport, a few pieces at a time.
 let observer:IntersectionObserver|null=null;
 let loading=0;const waiting:HTMLElement[]=[];
 function hydrate(section:HTMLElement){
  if(section.dataset.images||!section.querySelector('img[data-source]'))return;
  section.dataset.images='1';
  if(loading>=4){waiting.push(section);return}
  loading++;
  hydrateImages(section,path).finally(()=>{loading--;const next=waiting.shift();if(next){delete next.dataset.images;hydrate(next)}});
 }
 function realign(){if(!userMoved&&current)activate(current)}
 function interact(){userMoved=true}

 onMount(()=>{
  const fragment=document.createDocumentFragment();
  sections=model.pieces.map((_,index)=>{
   const section=document.createElement('section');
   section.className='reader-piece';section.dataset.piece=String(index);section.style.minHeight=`${estimate(index)}px`;
   fragment.append(section);return section;
  });
  host.append(fragment);
  // Images that finish loading push text down; keep the match in place until the reader scrolls.
  host.addEventListener('load',realign,true);
  for(const name of ['wheel','touchstart','keydown','pointerdown'])host.addEventListener(name,interact,{passive:true});
  if(path&&typeof IntersectionObserver!=='undefined')observer=new IntersectionObserver(entries=>{for(const entry of entries)if(entry.isIntersecting)hydrate(entry.target as HTMLElement)},{root:host,rootMargin:'800px 0px'});
  const restoredPiece=saved?.piece;
  const restoring=restoredPiece!==undefined&&restoredPiece>=0&&restoredPiece<sections.length;
  const start=restoring?restoredPiece:targetPiece;
  render(start);
  // Learn the real height per character from the first piece for better placeholders.
  const piece=model.pieces[start];
  if(piece.to>piece.from)charHeight=Math.min(4,Math.max(0.2,sections[start].offsetHeight/(piece.to-piece.from)));
  for(let index=0;index<sections.length;index++)if(!sections[index].dataset.state)sections[index].style.minHeight=`${estimate(index)}px`;
  if(start>0)render(start-1);
  if(start+1<sections.length)render(start+1);
  if(restoring){
   // Restore against a real block, then let the existing above-viewport height
   // compensation preserve it as placeholders become content.
   userMoved=true;
   host.scrollTop=sections[start].offsetTop+(saved!.offset??0);host.scrollLeft=saved!.left;
  }
  const first=terms.length?firstMatchFromTarget():null;
  if(first)activate(first,!restoring);else if(!restoring)sections[start].scrollIntoView({block:'start'});
  if(!observer&&path)for(const section of sections)if(section.dataset.state)hydrate(section);
  void renderRest(start).then(()=>{if(!observer&&path)sections.forEach(hydrate)});
 });
 onDestroy(()=>{remember();destroyed=true;observer?.disconnect();waiting.length=0;channel?.port1.close()});
</script>
{#if showNavigation}<MatchNavigation count={complete?total:0} index={position??0} pending={!complete&&!!current} known={position!==null} {label} onMove={move}/>{/if}
<div class="search-preview-body tiptap markdown-reader" bind:this={host} use:quietScroll on:scroll={remember}></div>

<style>
 /* Pieces above the viewport are compensated by hand (WebKit has no scroll anchoring); keep Chrome's native anchoring from adding the same shift twice. */
 .markdown-reader{position:relative;overflow-anchor:none}
 .markdown-reader :global(.reader-piece){display:flow-root}
</style>
