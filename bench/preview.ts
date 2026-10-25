// Open a large cited document the way the app does and report timings.
// Usage: npm run dev, then open /bench/preview.html?mb=5&mode=citation
// mode=citation mounts CitationDocument; mode=search mirrors focusResult.
import {mount,unmount,tick} from 'svelte';
import '../src/tokens.css';
import '../src/prototype.css';
import '../src/app.css';
import '../src/native.css';
import {citedSpan,fixture} from './fixtures';
import type {Candidate} from '../src/lib/types';

const params=new URLSearchParams(location.search);
const megabytes=Number(params.get('mb')??'1'),mode=params.get('mode')??'citation',position=Number(params.get('at')??'0.6');
const log=(line:string)=>{document.getElementById('log')!.textContent+=line+'\n'};

const {body}=fixture(megabytes*1_000_000);
const cited=citedSpan(body,position);
const path='/bench/AI-Agents-in-Depth-zh-CN.pdf.md';
const imageData='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="640" height="320"><rect width="640" height="320" fill="#ddd"/></svg>');
(window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
 invoke:async(command:string)=>{
  if(command==='read_document')return {path,name:'AI-Agents-in-Depth-zh-CN.pdf.md',body,hash:'same'};
  if(command==='load_image'){await new Promise(resolve=>setTimeout(resolve,15));return imageData}
  throw new Error(`unexpected command ${command}`);
 },
 transformCallback:()=>0,
};
const candidate:Candidate={id:1,document_id:1,path,title:'AI-Agents-in-Depth-zh-CN.pdf.md',heading:'',content:cited.content,start_line:cited.startLine,end_line:cited.startLine,source_ranges:[],source_spans:[cited.span],bm25:0,score:0,order:0,hash:'same'};

// The longest gap between animation frames approximates the worst main-thread stall.
function frameMonitor(){
 let last=performance.now(),worst=0,running=true;
 const loop=(now:number)=>{worst=Math.max(worst,now-last);last=now;if(running)requestAnimationFrame(loop)};
 requestAnimationFrame(loop);
 return ()=>{running=false;return worst};
}

async function visibleHit(stage:HTMLElement){
 for(;;){
  const hit=stage.querySelector<HTMLElement>('.search-hit.active');
  const scroller=stage.querySelector<HTMLElement>('.search-preview-body');
  if(hit&&scroller){const box=hit.getBoundingClientRect(),view=scroller.getBoundingClientRect();if(box.top>=view.top&&box.bottom<=view.bottom)return}
  await new Promise(resolve=>setTimeout(resolve,4));
 }
}

async function settled(stage:HTMLElement){
 // Done when the match counter stops changing for 300 ms.
 let previous='',stable=performance.now();
 for(;;){
  const text=stage.querySelector('.match-navigation')?.textContent??'';
  if(text!==previous){previous=text;stable=performance.now()}
  if(performance.now()-stable>300)return {time:stable,text};
  await new Promise(resolve=>setTimeout(resolve,16));
 }
}

async function open(){
 const stage=document.getElementById('stage')!;
 stage.innerHTML='';
 await new Promise(resolve=>requestAnimationFrame(()=>setTimeout(resolve)));
 const stop=frameMonitor();
 const start=performance.now();
 let app:ReturnType<typeof mount>;
 if(mode==='citation'){
  const {default:CitationDocument}=await import('../src/components/CitationDocument.svelte');
  app=mount(CitationDocument,{target:stage,props:{source:candidate,onOpen:()=>{}}});
 }else{
  const {mountSearchPreview}=await import('./search-preview');
  app=await mountSearchPreview(stage,candidate,['KV Cache']);
 }
 await visibleHit(stage);
 const visible=performance.now()-start;
 const done=await settled(stage);
 const worstFrame=stop();
 await tick();
 unmount(app);
 return {visible:Math.round(visible),complete:Math.round(done.time-start),worstFrame:Math.round(worstFrame),counter:done.text.trim()};
}

(async()=>{
 log(`${mode} ${megabytes}MB body=${body.length} chars, cited at ${Math.round(position*100)}%`);
 const first=await open();log('first  '+JSON.stringify(first));
 const second=await open();log('second '+JSON.stringify(second));
 (window as unknown as {__bench:unknown}).__bench={first,second};
})().catch(error=>{log('ERROR '+error);(window as unknown as {__bench:unknown}).__bench={error:String(error)}});
