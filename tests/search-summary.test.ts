// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import type {Settings,Candidate} from '../src/lib/types';
const {invoke,events}=vi.hoisted(()=>({invoke:vi.fn(),events:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{events.set(name,handler);return()=>events.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
const settings:Settings={answer:{model:'test',models:['test'],url:'http://localhost',has_key:true},jev:{model:'jev',models:[],url:'http://localhost',has_key:true},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'search',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
const hits:Candidate[]=Array.from({length:12},(_,i)=>({id:i+1,document_id:i+1,path:`/notes/${i}.txt`,title:`${i}.txt`,heading:'定义',content:`证据 ${i}`,start_line:1,end_line:2,source_ranges:[[0,4]],bm25:0,score:1,order:i,hash:`hash-${i}`}));
let sourceCount:number|undefined;
let enabled:boolean|undefined,requests:{id:string;question:string;results:Candidate[];resolve:(value:any)=>void;reject:(value:Error)=>void}[];
beforeEach(()=>{enabled=undefined;sourceCount=undefined;requests=[];events.clear();localStorage.setItem('sag.onboarding.seen','1');invoke.mockImplementation(async(command,args)=>{
 if(command==='get_settings')return{...settings,search_summary_enabled:enabled,search_summary_sources:sourceCount};
 if(command==='run_query')return{terms:[],results:[hits[0],{...hits[0],id:90},...hits.slice(1)],trace:[],conversation:null};
 if(command==='read_document')return{id:1,path:args.path,name:'source',body:'证据',hash:'hash-0'};
 if(command==='summarize_search')return new Promise((resolve,reject)=>requests.push({...args,resolve,reject}));
 return [];
})});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull())}
async function search(question:string){const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value=question;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick()}
it('默认用完整问题和前十个文件总结，结果先显示，流式文字与引用正常渲染',async()=>{
 await start();await search('这些文件如何定义协作？');
 await vi.waitFor(()=>expect(requests).toHaveLength(1));
 expect(requests[0].question).toBe('这些文件如何定义协作？');
 expect(requests[0].results.map(c=>c.path)).toEqual(hits.slice(0,10).map(c=>c.path));
 expect(document.querySelectorAll('.search-result')).toHaveLength(hits.length);
 expect(document.querySelector('.search-answer[aria-busy="true"]')).not.toBeNull();
 events.get('query-progress')!({payload:{id:requests[0].id,delta:'**共同协作** [1]',replace:false}});await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-answer strong')?.textContent).toBe('搜索总结'));
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .answer-text strong')?.textContent).toBe('共同协作'));
 requests[0].resolve({text:'共同协作 [1]',evidence:requests[0].results});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer[aria-busy="false"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.search-answer .citation')!.click();await tick();
 expect(document.querySelector('.source-dialog')).not.toBeNull();
});
it('关闭自动总结后不请求模型',async()=>{
 enabled=false;await start();await search('协作');
 await vi.waitFor(()=>expect(document.querySelector('.search-result')).not.toBeNull());
 expect(requests).toHaveLength(0);expect(document.querySelector('.search-answer')).toBeNull();
});
it('生成中可以折叠，完成后保持折叠，重新展开显示全文且不重复请求',async()=>{
 await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 document.querySelector<HTMLButtonElement>('[aria-label="折叠搜索总结"]')!.click();await tick();
 expect(document.querySelector('[aria-label="展开搜索总结"]')?.getAttribute('aria-expanded')).toBe('false');
 expect(document.querySelector('.search-answer')?.textContent).toContain('正在总结');
 events.get('query-progress')!({payload:{id:requests[0].id,delta:'总结全文',replace:false}});await tick();
 requests[0].resolve({text:'总结全文',evidence:requests[0].results});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer[aria-busy="false"]')).not.toBeNull());
 expect(document.querySelector('.search-answer .answer-text')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="展开搜索总结"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .answer-text')?.textContent).toBe('总结全文'));
 expect(requests).toHaveLength(1);
});
it('新搜索和返回会取消旧摘要，迟到的文字不能覆盖当前搜索',async()=>{
 await start();await search('问题一');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 await search('问题二');await vi.waitFor(()=>expect(requests).toHaveLength(2));
 expect(invoke.mock.calls.some(([command,args])=>command==='cancel_query'&&args.id===requests[0].id)).toBe(true);
 requests[0].resolve({text:'过期摘要',evidence:[]});
 events.get('query-progress')!({payload:{id:requests[0].id,delta:'过期流式文字'}});await tick();
 expect(document.querySelector('.search-answer')?.textContent).not.toContain('过期');
 document.querySelector<HTMLButtonElement>('[aria-label="返回搜索"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.search-answer')).toBeNull());
 requests[1].resolve({text:'迟到的摘要',evidence:[]});await tick();await tick();
 expect(document.querySelector('.search-answer')).toBeNull();
});
it('总结失败保留结果列表，显示错误',async()=>{
 await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 requests[0].reject(Error('模型服务不可用'));
 await vi.waitFor(()=>expect(document.querySelector('.search-answer [role="alert"]')?.textContent).toContain('模型服务不可用'));
 expect(document.querySelectorAll('.search-result')).toHaveLength(hits.length);
});
it('搜索设置开关默认开启，关闭状态随设置保存',async()=>{
 const save=vi.fn().mockResolvedValue(undefined);
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,initial:'retrieval',onClose:()=>{},onSave:save}});
 await vi.waitFor(()=>expect(document.querySelector('[role="switch"][aria-label="搜索结果自动总结"]')).not.toBeNull());
 const toggle=document.querySelector<HTMLButtonElement>('[role="switch"][aria-label="搜索结果自动总结"]')!;
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('搜索设置');
 const count=document.querySelector<HTMLInputElement>('[aria-label="自动总结参考数量"]')!;
 expect(count.value).toBe('10');expect(count.max).toBe('');
 count.value='100000';count.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(count.checkValidity()).toBe(true);
 expect(toggle.getAttribute('aria-checked')).toBe('true');toggle.click();await tick();
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(save).toHaveBeenCalled());expect(save.mock.calls[0][0].search_summary_enabled).toBe(false);expect(save.mock.calls[0][0].search_summary_sources).toBe(100000);
});

it('按设置的参考数量提交，不改变搜索结果列表',async()=>{
 sourceCount=3;await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 expect(requests[0].results.map(c=>c.path)).toEqual(hits.slice(0,3).map(c=>c.path));
 expect(document.querySelectorAll('.search-result')).toHaveLength(hits.length);
});
it('复制图标复制完整总结，折叠时仍可复制且不改变展开状态',async()=>{
 const writeText=vi.spyOn(navigator.clipboard,'writeText').mockResolvedValue(undefined);
 try{
  await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
  expect(document.querySelector<HTMLButtonElement>('[aria-label="复制总结"]')!.disabled).toBe(true);
  const text='**第一段** [1]\n\n第二段完整总结。';
  requests[0].resolve({text,evidence:requests[0].results});
  await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[aria-label="复制总结"]')!.disabled).toBe(false));
  document.querySelector<HTMLButtonElement>('[aria-label="复制总结"]')!.click();await tick();
  expect(writeText).toHaveBeenLastCalledWith(text);
  expect(document.querySelector('[aria-label="折叠搜索总结"]')?.getAttribute('aria-expanded')).toBe('true');
  document.querySelector<HTMLButtonElement>('[aria-label="折叠搜索总结"]')!.click();await tick();
  document.querySelector<HTMLButtonElement>('[aria-label="复制总结"]')!.click();await tick();
  expect(writeText).toHaveBeenLastCalledWith(text);
  expect(document.querySelector('[aria-label="展开搜索总结"]')?.getAttribute('aria-expanded')).toBe('false');
  expect(document.querySelector('#notice')?.textContent).toContain('已复制');
 }finally{writeText.mockRestore()}
});
