// Diagnostic entry point only. Drives the current product's real sidebar route.
import {mount} from 'svelte';
import {configureDocumentPerformance,documentPerformanceSnapshot,resetDocumentPerformance} from '../src/lib/document-performance';
import {createFeedbackMonitor,createFrameGapMonitor} from './document-frame-gaps';
import type {Document,Settings} from '../src/lib/types';
import '../src/tokens.css';
import '../src/prototype.css';
import '../src/app.css';
import '../src/native.css';
import '../src/styles/search.css';
import '../src/components/settings.css';
import '../src/components/ui/controls.css';

const root='/benchmark',path=root+'/fixture.md';
const settings:Settings={theme:'light',language:'zh-CN',start_page:'files',editor_size:15,
 search_summary_enabled:false,answer:{model:'benchmark',models:[],url:'',has_key:false},jev:{model:'',models:[],url:'',has_key:false},
 retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:8,max_hops:2,top_k:50,chunk_chars:1200},watch:false,show_hidden:false,excluded:[],timeout_secs:60,answer_context_tokens:8000,answer_output_tokens:2000};
let fixture:{bytes:number;sha256:string}|null=null;
let imageIds=new Map<string,number>(),pendingImages=0;
let images={declared:0,requested:0,completed:0,failed:0,bytes:0,settledMs:0};
let invocation={run:0,read:0,sidebarClicks:0,trustedSidebarClicks:0};
document.addEventListener('click',event=>{if((event.target as Element).closest<HTMLButtonElement>('.tree .tree-name')?.title===path){invocation.sidebarClicks++;if(event.isTrusted)invocation.trustedSidebarClicks++;}},true);
const handlers:Record<string,(args:Record<string,unknown>)=>unknown>={
 get_settings:()=>settings,list_roots:()=>[{path:root,name:'Benchmark',status:'ready',count:1,error:null}],
 list_files:()=>[{id:1,path,name:'fixture.md'}],get_index_status:()=>({paused:false,tasks:[]}),
 list_conversations:()=>[],list_drafts:()=>[],list_removed_directories:()=>[],list_notes:()=>[],get_index_history:()=>[],
 list_entries:()=>[{path,name:'fixture.md',directory:false,symlink:false}],
 get_quick_search:()=>({preferences:{enabled:false,shortcut:'CommandOrControl+Shift+Space'},error:null}),
 read_document:async args=>{
  invocation.read++;
  if(args.path!==path)throw new Error('Unexpected benchmark document');
  const response=await fetch('/__document-fixture',{cache:'no-store'});
  if(!response.ok)throw new Error('Local fixture unavailable; check benchmark server configuration');
  const value=await response.json() as {bytes:number;sha256:string;document:Document;images:{path:string;id:number}[]};
  imageIds=new Map(value.images.map(image=>[image.path,image.id]));images.declared=imageIds.size;
  fixture={bytes:value.bytes,sha256:value.sha256};return value.document;
 },
 load_image:async args=>{
  const id=imageIds.get(String(args.path));images.requested++;
  if(id===undefined){images.failed++;throw new Error('Image unavailable');}
  pendingImages++;
  try{
   const response=await fetch(`/__document-image/${id}`,{cache:'no-store'});
   if(!response.ok)throw new Error('Image unavailable');
   const value=await response.json() as {data:string;bytes:number};images.completed++;images.bytes+=value.bytes;return value.data;
  }catch(error){images.failed++;throw error;}
  finally{pendingImages--;images.settledMs=performance.now()-started;}
 }
};
(window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
 metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
 invoke:async(command:string,args:Record<string,unknown>={})=>handlers[command]?handlers[command](args):command.startsWith('plugin:')?0:null,
 transformCallback:()=>Math.floor(Math.random()*1e9),unregisterCallback:()=>{},convertFileSrc:(value:string)=>value
};
localStorage.setItem('sag.onboarding.seen','1');
localStorage.setItem('sag.sidebar.expanded',JSON.stringify([root]));
const {default:App}=await import('../src/App.svelte');
mount(App,{target:document.getElementById('app')!});

type Status='ready'|'running'|'complete'|'cancelled'|'timeout'|'error';
let status:Status='ready',frameId=0,measuring=false;
let frameGaps=createFrameGapMonitor(performance.now());
let feedback=createFeedbackMonitor(performance.now());
let longTasks={supported:typeof PerformanceObserver!=='undefined'&&PerformanceObserver.supportedEntryTypes?.includes('longtask'),count:0,totalMs:0,worstMs:0};
let dom={before:0,after:0,content:0},started=0,elapsedMs=0;
let observer:PerformanceObserver|undefined;
let memoryBefore:number|null=null,memoryAfter:number|null=null;
let cacheUsage:(()=>{modelBytes:number;htmlBytes:number;models:number;htmlPieces:number})|undefined;
const memory=()=>(performance as Performance&{memory?:{usedJSHeapSize:number}}).memory?.usedJSHeapSize??null;
function report(){return {schemaVersion:1,status,fixture,traces:documentPerformanceSnapshot(),feedback:feedback.feedback,elapsedMs,worstFrameMs:frameGaps.worstFrameMs,longTasks:{...longTasks},dom:{...dom},images:{...images,pending:pendingImages},invocation:{...invocation},cache:cacheUsage?.()??null,heap:{before:memoryBefore,after:memoryAfter},environment:{userAgent:navigator.userAgent,viewport:[innerWidth,innerHeight],devicePixelRatio}};}
function sidebarFile(){return Array.from(document.querySelectorAll<HTMLButtonElement>('.tree .tree-name')).find(button=>button.title===path);}
async function waitFor(test:()=>boolean,timeout=180000){const start=performance.now();while(!test()){if(performance.now()-start>timeout)throw new Error('Benchmark timed out');await new Promise(resolve=>setTimeout(resolve,20));}}
function frame(time:number){
 frameGaps.frame(time);
 const visible=(selector:string)=>Array.from(document.querySelectorAll(selector)).some(element=>{const box=element.getBoundingClientRect();return box.width>0&&box.height>0&&box.bottom>0&&box.right>0&&box.top<innerHeight&&box.left<innerWidth;});
 feedback.frame(time,visible('[data-document-loading]')?'loading':visible('.large-document-reader [data-state="rendered"],#rich-editor .ProseMirror')?'content':null);
 if(measuring)frameId=requestAnimationFrame(frame);
}
async function run(){
 if(status==='running')throw new Error('Benchmark already running');
 invocation={run:invocation.run+1,read:0,sidebarClicks:0,trustedSidebarClicks:0};
 status='running';
 await waitFor(()=>!!sidebarFile());
 // Return via the real folder route so a repeated run also mounts a fresh document component.
 const folder=Array.from(document.querySelectorAll<HTMLButtonElement>('.tree .tree-name')).find(button=>button.title===root);
 folder?.click();await new Promise(resolve=>setTimeout(resolve,0));await waitFor(()=>!document.querySelector('.wysiwyg-editor,.readonly-document,.library-document'));
 resetDocumentPerformance();configureDocumentPerformance({enabled:true,maxSamples:30});
 fixture=null;status='running';
 images={declared:0,requested:0,completed:0,failed:0,bytes:0,settledMs:0};
 longTasks={...longTasks,count:0,totalMs:0,worstMs:0};
 if(longTasks.supported){observer=new PerformanceObserver(list=>{for(const entry of list.getEntries()){longTasks.count++;longTasks.totalMs+=entry.duration;longTasks.worstMs=Math.max(longTasks.worstMs,entry.duration);}});observer.observe({entryTypes:['longtask']});}
 dom={before:document.querySelectorAll('*').length,after:0,content:0};
 memoryBefore=memory();memoryAfter=null;started=performance.now();frameGaps=createFrameGapMonitor(started);feedback=createFeedbackMonitor(started);
 measuring=true;frameId=requestAnimationFrame(frame);
 sidebarFile()!.click();
 try{
  await waitFor(()=>{const trace=documentPerformanceSnapshot().at(-1);return !!trace&&trace.status!=='running';});
  status=documentPerformanceSnapshot().at(-1)!.status as 'complete'|'cancelled';
  // The trace records first content paint; separately wait for image RPCs already in flight.
  await waitFor(()=>pendingImages===0,30000);
 }catch{status='timeout';}
 finally{
  elapsedMs=performance.now()-started;measuring=false;frameGaps.frame(performance.now());cancelAnimationFrame(frameId);
  // Deliver pending observer callbacks before disconnecting, including the initial blocking task.
  await new Promise(resolve=>setTimeout(resolve,0));observer?.disconnect();observer=undefined;
  dom.after=document.querySelectorAll('*').length;
  dom.content=document.querySelector('#rich-editor,.markdown-reader,.library-document')?.querySelectorAll('*').length??0;
  memoryAfter=memory();
  // Load diagnostics after paint/frame measurement so the probe cannot prewarm the reader.
  cacheUsage=(await import('../src/lib/document-model')).documentCacheUsage;
 }
 return report();
}
async function referenceMatches(terms:string[]){
 if(status==='running')throw new Error('Run reference checks outside timing measurements');
 const response=await fetch('/__document-fixture',{cache:'no-store'});
 if(!response.ok)throw new Error('Local fixture unavailable');
 const {document:doc}=await response.json() as {document:Document};
 const [{renderMarkdown},{highlightedHtml}]=await Promise.all([import('../src/lib/markdown'),import('../src/lib/search-highlights')]);
 return highlightedHtml(renderMarkdown(doc.body),terms).count;
}
const benchmark={ready:false,run,report,referenceMatches,snapshot:documentPerformanceSnapshot,reset:()=>{if(status==='running')throw new Error('Benchmark running');resetDocumentPerformance();status='ready';}};
(window as unknown as {__SAG_DOCUMENT_BENCHMARK__:typeof benchmark}).__SAG_DOCUMENT_BENCHMARK__=benchmark;
await waitFor(()=>!!sidebarFile());benchmark.ready=true;
// ?autorun=1 is useful for browser runners; result stays in the documented global.
if(new URLSearchParams(location.search).has('autorun'))void run();
