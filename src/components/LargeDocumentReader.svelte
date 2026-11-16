<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {copyText} from '../lib/web-connection';
 import {hydrateImages} from '../lib/markdown';
 import {prepareCandidateRanges,prepareMarkdownModel,modelCitationTerms,pieceDocument,pieceIndex,piecePrefixHtml,type MarkdownModel} from '../lib/document-model';
 import {countDocumentMatches,highlightDocument,selectSearchMatches} from '../lib/search-highlights';
 import {afterDocumentPaint,type DocumentTrace} from '../lib/document-performance';
 import {SourcePieceSearch} from '../lib/source-piece-search';
 import {captureViewportAnchor,restoreViewportAnchor,type ViewportAnchor} from '../lib/reader-viewport-anchor';
 import {ReaderLayout,pieceHeadings,headingSelector,type OutlineHeading} from '../lib/large-reader-layout';
 import {readReaderScroll,rememberReaderScroll} from '../lib/reader-scroll';
 import type {Candidate,Document as SourceDocument} from '../lib/types';
 import type {Chunk} from '../lib/markdown-chunks';
 import MatchNavigation from './MatchNavigation.svelte';

 export let doc:SourceDocument;
 export let path=doc.path;
 export let terms:string[]=[];
 export let candidate:Candidate|null=null;
 export let deriveTerms=false;
 export let restrictCitation=false;
 export let onTerms:(value:string[])=>void=()=>{};
 export let target=0;
 export let within:Chunk[]|null=null;
 export let label:'match'|'citation'='match';
 export let showNavigation=true;
 export let showFind=false;
 export let explicitFind=false;
 export let memoryKey='';
 export let restoreScroll=false;
 export let trace:DocumentTrace|undefined=undefined;

 let model:MarkdownModel,layout:ReaderLayout;
 let host:HTMLElement,body:HTMLElement,topSpacer:HTMLElement,bottomSpacer:HTMLElement,findInput:HTMLInputElement;
 let disposed=false,ready=false,userMoved=false,epoch=0,windowEpoch=0,frame=0;
 let first=0,last=0,ranges:Chunk[]|null=null,initialOffset=0;
 let query:string[]=[],findDraft='',findOpen=false,feedback='',error='';
 let current:{piece:number;local:number}|null=null,counts:(number|undefined)[]=[],total=0,position=0,complete=false;
 let sourceSearch:SourcePieceSearch|undefined,activeSource:string|undefined;
 let pendingMoves=0,positionKnown=false,activeHeading=-1,outlineOpen=false,outlineReady=false,outlineRunning=false,page=0;
 let headings:OutlineHeading[]=[],pageSize=48;
 let sections=new Map<number,HTMLElement>(),heights=new Map<number,number>(),observer:ResizeObserver|undefined;
 const yieldTask=()=>new Promise<void>(resolve=>setTimeout(resolve,0));
 const storageKey=JSON.stringify([memoryKey,doc.source_path??doc.path,doc.hash]);
 const images=new Map<HTMLElement,number>();
 let imageRequests=0,imageRevision=0;
 function drainImages(){
  while(!disposed&&imageRequests<3&&images.size){
   const [el,revision]=images.entries().next().value!;
   images.delete(el);
   const valid=()=>!disposed&&el.isConnected&&el.dataset.imageRevision===String(revision);
   if(!valid())continue;
   imageRequests++;
   const done=trace?.stage('reader-images');
   void hydrateImages(el,path,valid).finally(()=>{done?.();imageRequests--;drainImages();});
  }
 }
 const parse=(html:string)=>new DOMParser().parseFromString(html,'text/html');
 function permitted(index:number){const p=model.pieces[index];return !ranges||ranges.some(r=>p.from<Math.max(r.from+1,r.to)&&p.to>r.from);}
 let visibleAnchor:ViewportAnchor|null=null,anchorScrollTop=0;
 function captureAnchor(){visibleAnchor=captureViewportAnchor(host);anchorScrollTop=host.scrollTop;}
 function measure(){
  if(!ready||disposed)return;
  const top=host.scrollTop,anchor=layout.at(top),delta=top-layout.top(anchor);
  for(const [index,el] of sections)if(el.dataset.state==='rendered')heights.set(index,el.offsetHeight);
  layout.update(heights);heights.clear();spacers();
  if(Math.abs(top-anchorScrollTop)>1||!restoreViewportAnchor(host,visibleAnchor)){
   const next=layout.top(anchor)+delta;if(Math.abs(next-top)>0.5)host.scrollTop=next;
  }
  captureAnchor();
 }
 function spacers(){
  topSpacer.style.height=layout.top(first)+'px';
  bottomSpacer.style.height=Math.max(0,layout.total-layout.top(last+1))+'px';
 }
 function setWindow(start:number,end:number){
  first=start;last=end;
  for(const [index,section] of sections)if(index<first||index>last){
   images.delete(section);observer?.unobserve(section);section.remove();sections.delete(index);
  }
  for(let i=end;i>=start;i--)if(!sections.has(i)){
   const section=document.createElement('section');
   section.className='reader-piece';section.dataset.piece=String(i);
   section.style.minHeight=(layout.top(i+1)-layout.top(i))+'px';
   body.insertBefore(section,sections.get(i+1)??null);sections.set(i,section);
  }
  spacers();
 }
 function activeMarks(section:HTMLElement,local:number){
  return Array.from(section.querySelectorAll<HTMLElement>('[data-search-match="'+local+'"]'));
 }
 function scrollToNode(node:HTMLElement,center=false){
  const delta=node.getBoundingClientRect().top-host.getBoundingClientRect().top;
  if(host.clientHeight)host.scrollTop+=delta-(center?host.clientHeight/2:12);
  captureAnchor();
 }
 function fill(index:number,force=false){
  const section=sections.get(index);if(disposed||!section||(!force&&section.dataset.state==='rendered'&&section.dataset.query===String(epoch)))return;
  const stop=trace?.stage('reader-render');
  const parsed=pieceDocument(model,index);
  if(query.length&&permitted(index)){
   const selected=current?.piece===index?current.local:-1;
   if(!model.pieces[index].source)counts[index]=highlightDocument(parsed,query,256,selected);
   else if(counts[index]!==undefined&&sourceSearch)sourceSearch.paint(parsed,index,selected);
  }
  stop?.();
  const dom=trace?.stage('reader-dom');
  section.replaceChildren(...parsed.body.childNodes);
  if(model.pieces[index].source){
   const note=document.createElement('small');note.className='source-caption';note.textContent=$t('超大段落 · Markdown 源码');section.prepend(note);
  }
  section.style.minHeight='';section.dataset.state='rendered';section.dataset.query=String(epoch);observer?.observe(section);dom?.();
  section.dataset.imageRevision=String(++imageRevision);
  if(section.querySelector('img[data-source]')){images.set(section,imageRevision);drainImages();}
  if(activeSource!==undefined&&model.pieces[index].source)selectSearchMatches(section,[...section.querySelectorAll<HTMLElement>('[data-source-hit="'+activeSource+'"]')]);
  else if(current?.piece===index)selectSearchMatches(section,activeMarks(section,current.local));
 }
 async function updateWindow(){
  if(!ready||disposed)return;
  const generation=++windowEpoch,anchor=layout.at(host.scrollTop);
  let {first:start,last:end}=layout.window(host.scrollTop,host.clientHeight||500);
  // Reserve room for bounded source subpieces if dense rich output expands on render.
  const cost=(i:number)=>Math.max(1,Math.ceil((model.pieces[i].to-model.pieces[i].from)/4000));
  let slots=0;for(let i=start;i<=end;i++)slots+=cost(i);
  while(slots>12&&end>start){if(anchor-start>end-anchor)slots-=cost(start++);else slots-=cost(end--);}
  setWindow(start,end);
  const order=Array.from({length:end-start+1},(_,i)=>start+i).sort((a,b)=>Math.abs(a-anchor)-Math.abs(b-anchor));
  for(const i of order){
   if(disposed||generation!==windowEpoch)return;
   const el=sections.get(i);if(el?.dataset.state==='rendered'&&el.dataset.query===String(epoch))continue;
   fill(i);measure();await yieldTask();
  }
  updateActiveHeading();
 }
 function schedule(){
  if(!ready||disposed||frame)return;
  frame=requestAnimationFrame(()=>{frame=0;if(!disposed){measure();void updateWindow();}});
 }
 function goToPiece(index:number,delta=0){
  visibleAnchor=null;windowEpoch++;setWindow(index,index);fill(index);measure();
  host.scrollTop=layout.top(index)+delta;captureAnchor();
  schedule();return sections.get(index)!;
 }
 function remember(){
  if(!ready||!memoryKey)return;
  const i=layout.at(host.scrollTop);
  rememberReaderScroll(storageKey,{top:host.scrollTop,left:host.scrollLeft,piece:model.pieces[i].from,offset:host.scrollTop-layout.top(i)});
 }
 function scroll(){if(ready){captureAnchor();remember();schedule();updateActiveHeading();}}
 function updateActiveHeading(){
  if(!outlineReady||!headings.length)return;
  const i=layout.at(host.scrollTop),section=sections.get(i),edge=host.getBoundingClientRect().top+24;
  const local=section?Array.from(section.querySelectorAll(headingSelector)).filter(el=>el.getBoundingClientRect().top<=edge).length-1:-1;
  let lo=-1,hi=headings.length;
  while(hi-lo>1){const mid=(lo+hi)>>1,h=headings[mid];if(h.piece<i||(h.piece===i&&h.ordinal<=local))lo=mid;else hi=mid;}
  if(host.clientHeight)activeHeading=host.scrollHeight>host.clientHeight&&host.scrollTop+host.clientHeight>=host.scrollHeight-2?headings.length-1:Math.max(0,lo);
 }
 async function buildOutline(){
  if(outlineRunning||outlineReady||!ready)return;
  outlineRunning=true;const list:OutlineHeading[]=[];let start=performance.now();
  for(let i=0;i<model.pieces.length;i++){
   if(disposed) return;
   if(!outlineOpen){outlineRunning=false;return;}
   list.push(...pieceHeadings(model,i));
   if(performance.now()-start>=5){await yieldTask();start=performance.now();}
  }
  headings=list;outlineReady=true;outlineRunning=false;updateActiveHeading();page=Math.max(0,Math.floor(activeHeading/pageSize));
 }
 function jumpHeading(i:number){
  userMoved=true;const h=headings[i],section=goToPiece(pieceIndex(model,h.offset));
  const node=section.querySelectorAll<HTMLElement>(headingSelector)[h.ordinal];
  if(node)scrollToNode(node);
  activeHeading=i;host.focus({preventScroll:true});remember();
 }
 function updatePosition(){
  if(!current){position=0;positionKnown=false;return;}
  let previous=0;
  for(let i=0;i<current.piece;i++){
   if(counts[i]===undefined&&permitted(i)){positionKnown=false;return;}
   previous+=counts[i]??0;
  }
  position=previous+current.local;positionKnown=true;
 }
 function activate(piece:number,local:number,jump:boolean){
  current={piece,local};updatePosition();
  const section=jump?(sections.get(piece)?.dataset.state==='rendered'?sections.get(piece):goToPiece(piece)):sections.get(piece);
  if(section)fill(piece);
  let marks=section?activeMarks(section,local):[];
  if(section&&!marks.length){fill(piece,true);marks=activeMarks(section,local);}
  activeSource=marks[0]?.dataset.sourceHit;
  if(activeSource!==undefined)marks=[...host.querySelectorAll<HTMLElement>('[data-source-hit="'+activeSource+'"]')];
  selectSearchMatches(host,marks);
  if(jump&&marks[0])scrollToNode(marks[0],true);
 }
 function move(step:number){
  if(!complete){pendingMoves+=step;return;}
  if(!total)return;
  const ordinal=((position+step)%total+total)%total;
  let before=0;
  for(let i=0;i<counts.length;i++){const n=counts[i]??0;if(before+n>ordinal){activate(i,ordinal-before,true);return;}before+=n;}
 }
 async function indexMatches(generation:number){
  const startPiece=pieceIndex(model,initialOffset),length=model.pieces.length;
  let fallback:{piece:number;local:number}|null=null,start=performance.now();
  for(let n=0;n<length;n++){
   if(disposed||generation!==epoch)return;
   const index=(startPiece+n)%length;
   if(permitted(index)){
    const stop=trace?.stage('reader-index');
    if(counts[index]===undefined){
     const parsed=pieceDocument(model,index); // A dense rich piece may switch to source presentation here.
     if(model.pieces[index].source&&sourceSearch){
      if(!await sourceSearch.prepare(index,()=>!disposed&&generation===epoch)){stop?.();return;}
      counts[index]=sourceSearch.matches(index).length;
      if(sections.get(index)?.dataset.state==='rendered')fill(index,true);
     }else counts[index]=countDocumentMatches(parsed,query);
    }
    const hits=counts[index]!;
    if(!current&&index>=startPiece&&hits){
     let before=0;
     if(index===startPiece&&initialOffset>model.pieces[index].from){
      before=model.pieces[index].source&&sourceSearch?sourceSearch.matches(index).filter(match=>match.to<=initialOffset).length:
       countDocumentMatches(parse(piecePrefixHtml(model,index,initialOffset)),query);
     }
     if(before<hits)activate(index,before,!userMoved);
    }
    if(hits&&(!fallback||index>fallback.piece))fallback={piece:index,local:hits-1};
    stop?.();
   }else counts[index]=0;
   if(performance.now()-start>=5){await yieldTask();start=performance.now();}
  }
  if(!current&&fallback)activate(fallback.piece,fallback.local,!userMoved);
  total=counts.reduce<number>((sum,n)=>sum+(n??0),0);complete=true;updatePosition();
  if(pendingMoves){const moves=pendingMoves;pendingMoves=0;move(moves);}
 }
 function search(value:string[],offset:number,scope:Chunk[]|null){
  const generation=++epoch;query=value;sourceSearch?.dispose();sourceSearch=value.length?new SourcePieceSearch(model,value):undefined;initialOffset=offset;ranges=scope;counts=new Array(model.pieces.length);
  total=0;current=null;activeSource=undefined;pendingMoves=0;position=0;positionKnown=false;complete=!query.length;
  // Keep the old text visible; repaint mounted matches one piece at a time in the viewport loop.
  schedule();
  if(query.length)void yieldTask().then(()=>{if(!disposed&&epoch===generation)void indexMatches(generation);});
 }
 let initialTerms:string[]=[],originalTarget=0,originalRanges:Chunk[]|null=null;
 async function initialize(){
  try{
   const done=trace?.stage('model');
   const prepared=await prepareMarkdownModel(doc,()=>!disposed);done?.();
   if(!prepared||disposed)return;model=prepared;layout=new ReaderLayout(model);
   const targetDone=trace?.stage('target');
   const spans=candidate?await prepareCandidateRanges(model,candidate,()=>!disposed):null;
   if(disposed){targetDone?.();return;}
   originalTarget=spans?.[0]?.from??target;originalRanges=restrictCitation?spans:within;
   targetDone?.();
   const saved=restoreScroll&&memoryKey?readReaderScroll(storageKey):undefined;
   const index=pieceIndex(model,saved?.piece??originalTarget);
   ready=true;userMoved=!!saved;goToPiece(index,saved?.offset??0);if(saved)host.scrollLeft=saved.left;
   if(trace?.enabled)void afterDocumentPaint(trace,()=>!disposed&&!!body.querySelector('[data-state=rendered]'));
   // Let the first mounted piece become visible before counting matches or preparing neighbours.
   const previous=epoch;await yieldTask();if(disposed)return;
   initialTerms=deriveTerms&&candidate&&spans?await modelCitationTerms(model,candidate,spans,()=>!disposed&&epoch===previous):terms;
   if(disposed||previous!==epoch)return;
   onTerms(initialTerms);
   if(initialTerms.length)search(initialTerms,originalTarget,originalRanges);
   if(outlineOpen)void buildOutline();
  }catch(e){if(!disposed){error=String(e);trace?.cancel();}}
 }
 async function toggleFind(){
  if(!ready)return;
  findOpen=!findOpen;
  if(findOpen){await tick();findInput?.focus();}
  else{findDraft='';applied='';search(initialTerms,originalTarget,originalRanges);}
 }
 let applied='';
 function submitFind(){
  if(!ready)return;
  const term=findDraft.trim();
  if(term&&term===applied){move(1);return;}
  applied=term;userMoved=false;search(term?[term]:initialTerms,term?0:originalTarget,term?null:originalRanges);
 }
 function keys(e:KeyboardEvent){
  if(!showFind||!ready)return;
  if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='f'){e.preventDefault();if(!findOpen)void toggleFind();else findInput.focus();}
  if(findOpen&&(e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='g'){e.preventDefault();move(e.shiftKey?-1:1);}
 }
 async function copy(){
  try{await copyText(doc.body);feedback=$t('已复制');}catch(e){feedback=$t('复制失败：{value0}',{value0:String(e)});}
 }
 onMount(()=>{
  if(typeof ResizeObserver!=='undefined'){observer=new ResizeObserver(()=>schedule());observer.observe(host);}
  void initialize();
  return ()=>{remember();disposed=true;epoch++;windowEpoch++;sourceSearch?.dispose();images.clear();observer?.disconnect();cancelAnimationFrame(frame);if(trace?.active)trace.cancel();};
 });
</script>

<svelte:window on:keydown={keys}/>
<div class="large-document-reader">
 <div class="reader-tools">
  <button type="button" class="outline-toggle" aria-expanded={outlineOpen} aria-label={$t('文档目录')} title={$t('文档目录')} on:click={()=>{outlineOpen=!outlineOpen;if(outlineOpen)void buildOutline();}}>{@html icon('bulletList')}<span>{$t('文档目录')}</span></button>
  {#if showFind}<button type="button" disabled={!ready} aria-expanded={findOpen} on:click={toggleFind}>{@html icon('search')}{$t('正文查找')}</button>{/if}
  <button type="button" class="copy-full-document" on:click={copy}>{$t('复制完整源码')}</button>
  {#if feedback}<span role="status">{feedback}</span>{/if}
 </div>
 {#if findOpen}<form class="large-find" on:submit|preventDefault={submitFind}><input bind:this={findInput} bind:value={findDraft} aria-label={$t('正文查找')} placeholder={$t('输入词语，按 Enter 查找')}/><button type="submit">{$t('查找')}</button></form>{/if}
 {#if showNavigation}<MatchNavigation count={total} index={position} pending={!complete&&!!query.length} showEmpty={complete&&!!query.length&&(!!applied||explicitFind)} known={positionKnown} {label} onMove={move}/>{/if}
 {#if error}<p role="alert">{error}</p>{:else if !ready}<p role="status" data-document-loading>{$t('正在读取文件…')}</p>{/if}
 <div class="reader-layout">
  {#if outlineOpen}<nav class="reader-outline" aria-label={$t('文档目录')}>
   {#if !outlineReady}<span role="status">{$t('正在读取文件…')}</span>{:else if !headings.length}<span>{$t('没有标题')}</span>{/if}
   {#if headings.length>pageSize}<div class="outline-pages"><button type="button" aria-label={$t('上一页')} disabled={page===0} on:click={()=>page--}>‹</button><span>{page+1} / {Math.ceil(headings.length/pageSize)}</span><button type="button" aria-label={$t('下一页')} disabled={(page+1)*pageSize>=headings.length} on:click={()=>page++}>›</button></div>{/if}
   {#each headings.slice(page*pageSize,(page+1)*pageSize) as heading,i}
    <button type="button" class="outline-heading" aria-current={activeHeading===page*pageSize+i?'location':undefined} title={heading.title} style:padding-inline-start={8+(heading.level-1)*8+'px'} on:click={()=>jumpHeading(page*pageSize+i)}>{heading.title}</button>
   {/each}
  </nav>{/if}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="markdown-reader search-preview-body tiptap" role="region" aria-label={$t('文档正文')} tabindex="0" bind:this={host} on:scroll={scroll} on:wheel={()=>userMoved=true} on:pointerdown={()=>userMoved=true} on:touchstart={()=>userMoved=true} on:keydown={()=>userMoved=true}>
   <div aria-hidden="true" bind:this={topSpacer}></div><div bind:this={body}></div><div aria-hidden="true" bind:this={bottomSpacer}></div>
  </div>
 </div>
</div>
<style>
 .large-document-reader,.reader-outline{display:flex;flex-direction:column;min-height:0}
 .large-document-reader,.reader-layout,.markdown-reader{flex:1;min-width:0;min-height:0}
 .reader-layout,.reader-tools,.large-find,.outline-pages{display:flex}
 .reader-tools,.large-find,.outline-pages{align-items:center;gap:8px;padding:4px 12px}
 .reader-tools,.large-find{flex-wrap:wrap;border-bottom:1px solid var(--color-rule);font-size:var(--text-caption)}
 .reader-tools button{display:inline-flex;align-items:center;gap:5px}
 .reader-tools :global(svg){width:15px;height:15px}
 .reader-outline{flex:0 0 min(220px,35%);padding:8px;overflow:auto;border-inline-end:1px solid var(--color-rule);font-size:var(--text-caption)}
 .outline-heading{text-align:start;white-space:nowrap;text-overflow:ellipsis;overflow:hidden;flex-shrink:0}
 .outline-heading[aria-current]{background:var(--color-selected);color:var(--color-accent)}
 .markdown-reader{overflow:auto;overscroll-behavior:contain;overflow-anchor:none;padding:16px clamp(12px,3vw,48px) 36px;overflow-wrap:anywhere}
 .markdown-reader:focus-visible{outline:2px solid var(--color-focus);outline-offset:-2px}
 .markdown-reader :global(.reader-piece){display:flow-root}
 .markdown-reader :global(img){max-width:100%;height:auto}
 .markdown-reader :global(.source-presentation){white-space:pre-wrap;overflow-wrap:anywhere}
 .markdown-reader :global(.source-caption){color:var(--color-secondary)}
</style>
