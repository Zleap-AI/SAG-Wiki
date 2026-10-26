// The whole app against an in-memory backend, for screenshots and design review.
// ?q=KV%20Cache runs a search on load; ?theme=dark; ?lang=en; ?summary=0.
import {mount} from 'svelte';
import '../src/tokens.css';
import '../src/prototype.css';
import '../src/app.css';
import '../src/native.css';
import '../src/components/settings.css';
import '../src/components/ui/controls.css';
import '../src/styles/search.css';
import {fixture} from './fixtures';
import type {Candidate} from '../src/lib/types';

const params=new URLSearchParams(location.search);
const root='/Users/me/Downloads/一些 AI 文章资料';
const files=[
 ['AI-Agents-in-Depth-zh-CN.pdf',2_000_000,46],['深入理解 AI-Agents-in-Depth-zh-CN.pdf',600_000,44],['大模型推理优化笔记.md',80_000,12],['Transformer 架构详解.pdf',300_000,9],
 ['vLLM PagedAttention 原理.md',40_000,7],['提示词缓存实践.docx',60_000,5],['推理服务部署手册.md',120_000,4],['LLM 系统设计面试题.md',30_000,3],
 ['Attention 优化综述.pdf',200_000,3],['长上下文模型评测.md',25_000,2],['GPU 显存估算.md',15_000,2],['周报 2026-10.md',8_000,1],
] as const;
const docs=files.map(([name,size],index)=>{
 const converted=!name.endsWith('.md');
 const path=converted?`${root}/.sag-converted/${name}.md`:`${root}/${index%3===0?'笔记/':''}${name}`;
 return {path,name:converted?`${name}.md`:name,body:fixture(size,'KV Cache',300).body};
});
const results:Candidate[]=[];let id=1;
docs.forEach((doc,index)=>{
 const count=files[index][2];
 let from=0;
 for(let n=0;n<count;n++){
  const at=doc.body.indexOf('KV Cache',from);if(at<0)break;
  const start=doc.body.lastIndexOf('\n\n',at)+2,end=doc.body.indexOf('\n\n',at);from=end;
  const heading=/\n### ([^\n]+)\n[^#]*$/.exec(doc.body.slice(0,start))?.[1]??'';
  const encoder=new TextEncoder(),line=doc.body.slice(0,start).split('\n').length;
  results.push({id:id++,document_id:index+1,path:doc.path,title:doc.name,heading,content:doc.body.slice(start,end),start_line:line,end_line:line,source_ranges:[],source_spans:[{start_byte:encoder.encode(doc.body.slice(0,start)).length,end_byte:encoder.encode(doc.body.slice(0,end)).length,start_line:line,end_line:line,start_column:1,end_column:1}],bm25:1,score:1-results.length/400,order:results.length,hash:`h${index}`});
 }
});
const summary='KV Cache（键值缓存）是大语言模型推理中的核心优化技术，用于避免在生成每个新 token 时重复计算前文的注意力中间结果 [1]。模型把已计算的键（Key）和值（Value）向量缓存下来，下一轮只计算新增 token 的部分 [2]，从而显著加速单次请求内的生成过程 [3]。\n\n它对前缀高度敏感：被复用的上下文前缀必须完全一致，任何字符变化都会让后续缓存失效 [4]。因此设计提示词时，应把稳定内容放在前面、变化内容放在后面 [1]。\n\n此外，需要区分两个层级的缓存：\n\n- **单请求内的 KV Cache**：在一次生成过程中复用已计算的状态 [2]。\n- **跨请求的前缀缓存**：在多次请求之间复用相同前缀的计算结果 [5]。';
const settings={theme:params.get('theme')==='dark'?'dark':'light',language:params.get('lang')==='en'?'en':'zh-CN',start_page:'search',editor_size:15,search_summary_enabled:params.get('summary')!=='0',search_summary_sources:10,answer:{model:'demo',models:['demo'],url:'',has_key:true},jev:{model:'jev',models:['jev'],url:'',has_key:true},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:8,max_hops:2,top_k:50,chunk_chars:1200},watch:true,show_hidden:false,excluded:[],timeout_secs:60,answer_context_tokens:8000,answer_output_tokens:2000};
const delay=(ms:number)=>new Promise(resolve=>setTimeout(resolve,ms));
const handlers:Record<string,(args:Record<string,unknown>)=>unknown>={
 get_settings:()=>settings,
 list_roots:()=>[{path:root,name:'一些 AI 文章资料',status:'ready',count:52,error:null}],
 list_files:()=>docs.map((doc,index)=>({id:index+1,path:doc.path,name:doc.name})),
 get_index_status:()=>({paused:false,tasks:[]}),
 list_conversations:()=>[],list_drafts:()=>[],list_entries:()=>[],list_removed_directories:()=>[],
 run_query:async args=>{await delay(Number(params.get('delay')??600));const query=args.query as {kind:string};return {terms:['KV Cache','KV','Cache'],elapsed_ms:4100,results:query.kind==='deep_search'?results:results.slice(0,40),pagination:null,trace:[],conversation:null}},
 summarize_search:async args=>{await delay(900);return {text:summary,evidence:(args.results as Candidate[]).slice(0,10)}},
 read_document:args=>{const doc=docs.find(doc=>doc.path===args.path)!;return {id:1,path:doc.path,name:doc.name,body:doc.body,hash:`h${docs.indexOf(doc)}`}},
 load_image:()=>'data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="640" height="300"><rect width="640" height="300" fill="#e8e6df"/></svg>'),
};
(window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
 metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
 invoke:async(command:string,args:Record<string,unknown>={})=>handlers[command]?handlers[command](args):command.startsWith('plugin:')?0:null,
 transformCallback:()=>Math.floor(Math.random()*1e9),
 unregisterCallback:()=>{},
 convertFileSrc:(path:string)=>path,
};
localStorage.setItem('sag.search.mode','deep_search');localStorage.setItem('sag.onboarding.seen','1');
const {default:App}=await import('../src/App.svelte');
mount(App,{target:document.getElementById('app')!});
const query=params.get('q');
if(query){
 const input=await new Promise<HTMLInputElement>(resolve=>{const poll=()=>{const node=document.querySelector<HTMLInputElement>('.search-entry input');node?resolve(node):setTimeout(poll,50)};poll()});
 input.value=query;input.dispatchEvent(new Event('input',{bubbles:true}));
 await delay(50);input.closest('form')!.requestSubmit();
}
