// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Conversation,QueryOutput} from '../src/lib/types';
const {invoke,events}=vi.hoisted(()=>({invoke:vi.fn(),events:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{events.set(name,callback);return()=>events.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>,history:Conversation[];
const requests=new Map<string,{id:string;resolve:(output:QueryOutput)=>void;reject:(error:Error)=>void}>();
const hit={id:1,document_id:1,path:'/notes/result.md',title:'result.md',heading:'',content:'搜索命中',start_line:1,end_line:1,source_ranges:[],bm25:1,score:1,order:1,hash:'hash'};
beforeEach(async()=>{
 history=[];requests.clear();events.clear();localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return history;
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_roots')return[{path:'/notes',name:'notes',status:'ready',count:1,error:null}];
  if(command==='run_query')return new Promise<QueryOutput>((resolve,reject)=>requests.set(args.query.kind,{id:args.query.id,resolve,reject}));
  if(command==='read_document')return{id:1,path:hit.path,name:hit.title,body:hit.content,hash:hit.hash};
  return[];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(events.has('query-progress')).toBe(true));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function tab(name:string){[...document.querySelectorAll<HTMLButtonElement>('.workspace-switch button')].find(button=>button.textContent===name)!.click();await tick()}
async function ask(){const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;input.value='问答问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(requests.has('ask')).toBe(true))}
async function search(){const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='搜索问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(requests.has('deep_search')).toBe(true))}
async function progress(kind:string,stage:string,delta:string|null=null){events.get('query-progress')!({payload:{id:requests.get(kind)!.id,stage,delta,count:0}});await tick()}
function finishAsk(){const conversation:Conversation={id:'answer',title:'问答问题',updated:1,deleted:false,messages:[{question:'问答问题',answer:'问答完成',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'问答问题'}]};history=[conversation];requests.get('ask')!.resolve({terms:[],results:[],trace:[],conversation})}
it('问答和搜索切换后继续独立运行，后台完成不会停止另一个页面',async()=>{
 await ask();await tab('文件');
 expect(document.querySelector('.folder-browser')).not.toBeNull();
 await tab('搜索');
 expect(document.querySelector('[aria-label="停止搜索"]')).toBeNull();
 await search();expect(invoke.mock.calls.find(([command,args])=>command==='run_query'&&args.query.kind==='deep_search')![1].query.conversation_id).toBeNull();await progress('deep_search','正在扩展检索');await progress('ask','正在生成回答','后台回答');
 expect(document.querySelector('.search-loading')!.textContent).toContain('正在扩展搜索范围');
 await tab('问答');expect(document.querySelector('.answer-text')?.textContent).toContain('后台回答');
 expect(document.querySelector('[aria-label="停止生成"]')).not.toBeNull();
 requests.get('deep_search')!.resolve({terms:[],results:[hit],trace:[],conversation:null});await tick();await tick();
 expect(document.querySelector('[aria-label="停止生成"]')).not.toBeNull();
 await tab('搜索');await vi.waitFor(()=>expect(document.querySelector('.search-result')?.textContent).toContain('result.md'));
 expect(document.querySelector('[aria-label="停止搜索"]')).toBeNull();
 finishAsk();await tick();await tick();await tab('问答');await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('问答完成'));
 expect(invoke.mock.calls.filter(([command])=>command==='cancel_query')).toHaveLength(0);
});
it('搜索后台运行时停止问答，只取消问答请求',async()=>{
 await tab('搜索');await search();await tab('问答');await ask();
 document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();await tick();await tick();
 expect(invoke.mock.calls.filter(([command])=>command==='cancel_query').map(([,args])=>args.id)).toEqual([requests.get('ask')!.id]);
 await tab('搜索');expect(document.querySelector('[aria-label="停止搜索"]')).not.toBeNull();
 await progress('deep_search','Jev 正在判断相关原文');expect(document.querySelector('.search-loading')!.textContent).toContain('正在按相关性排序');
});
it('后台搜索失败只影响搜索页，问答仍继续生成',async()=>{
 await tab('搜索');await search();await tab('问答');await ask();
 requests.get('deep_search')!.reject(new Error('搜索服务失败'));await tick();await tick();
 await progress('ask','正在生成回答','问答仍在继续');
 expect(document.querySelector('.answer-text')?.textContent).toContain('问答仍在继续');
 expect(document.body.textContent).not.toContain('搜索服务失败');
 expect(document.querySelector('[aria-label="停止生成"]')).not.toBeNull();
 await tab('搜索');await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('搜索服务失败'));
});
