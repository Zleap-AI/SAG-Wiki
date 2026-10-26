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
async function search(question:string,openSummary=true){const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value=question;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();if(enabled!==false){await vi.waitFor(()=>expect(document.querySelector('.search-summary-toggle')?.getAttribute('aria-pressed')).toBe('true'));if(!openSummary){document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick()}}}
it('默认展开搜索总结，使用完整问题和前十个文件，流式文字与引用正常渲染',async()=>{
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
 expect(document.querySelector('.search-preview .search-answer')).not.toBeNull();
 document.querySelector<HTMLButtonElement>('.search-answer .citation')!.click();await tick();
 // A citation opens its passage in the reader beside the results, not in a dialog.
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe(hits[0].title));
 expect(document.querySelector('.source-dialog')).toBeNull();
 expect(document.querySelector('.search-result.selected .search-result-title strong')?.textContent).toBe(hits[0].title);
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 expect(document.querySelector('.search-preview .search-answer')).not.toBeNull();
});
it('关闭自动总结后不请求模型',async()=>{
 enabled=false;await start();await search('协作');
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('0.txt'));
 expect(requests).toHaveLength(0);expect(document.querySelector('.search-answer')).toBeNull();
 expect(document.querySelector('[aria-label="返回搜索总结"]')).toBeNull();
});
it('结果列表按 Esc 返回总结，输入框中的 Esc 保留原文',async()=>{
 await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 requests[0].resolve({text:'共同协作 [1]',evidence:requests[0].results});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .citation')).not.toBeNull());
 const list=document.querySelector<HTMLElement>('.search-results')!;
 list.focus();list.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('1.txt'));
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
 input.focus();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await tick();
 expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('1.txt');
 list.focus();list.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await tick();
 expect(document.querySelector('.search-preview .search-answer')).not.toBeNull();
});
it('引用徽标只计实际引用，并在原文打开期间继续跟随流式总结',async()=>{
 await start();await search('协作');await vi.waitFor(()=>expect(requests).toHaveLength(1));
 const badges=()=>[...document.querySelectorAll('.search-result-group')].filter(node=>node.querySelector('.search-result-cited')).map(node=>[node.querySelector('strong')!.textContent,node.querySelector('.search-result-cited')!.textContent]);
 const text='代码 `arr[1]` 与[链接 [3]](https://example.com)。实际依据 [2]。';
 events.get('query-progress')!({payload:{id:requests[0].id,delta:text,replace:false}});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .citation')?.textContent).toBe('1'));
 expect(badges()).toEqual([['1.txt','1']]);
 document.querySelector<HTMLButtonElement>('.search-answer .citation')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('1.txt'));
 events.get('query-progress')!({payload:{id:requests[0].id,delta:' 还有依据 [4]。',replace:false}});
 await vi.waitFor(()=>expect(badges()).toEqual([['1.txt','1'],['3.txt','2']]));
 expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('1.txt');
 expect(document.querySelector('.search-answer')).toBeNull();
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.search-answer .citation')].map(node=>node.textContent)).toEqual(['1','2']));
 requests[0].resolve({text:text+' 还有依据 [4]。',evidence:requests[0].results});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer[aria-busy="false"]')).not.toBeNull());
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
 document.querySelector<HTMLButtonElement>('[aria-label="折叠搜索总结"]')!.click();await tick();
 await search('问题二');await vi.waitFor(()=>expect(requests).toHaveLength(2));
 expect(document.querySelector('[aria-label="折叠搜索总结"]')?.getAttribute('aria-expanded')).toBe('true');
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

it('默认打开总结，不读取首个文件，点击总结按钮可查看首个文件并切回',async()=>{
 await start();await search('协作');
 await vi.waitFor(()=>expect(requests).toHaveLength(1));
 expect(document.querySelector('.search-answer')).not.toBeNull();
 expect(invoke.mock.calls.some(([command])=>command==='read_document')).toBe(false);
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('0.txt'));
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 expect(document.querySelector('.search-answer')).not.toBeNull();
});

it('未配置回答模型时默认查看原文，不请求总结',async()=>{
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,args)=>command==='get_settings'?Promise.resolve({...settings,answer:{...settings.answer,has_key:false}}):base(command,args));
 await start();
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
 input.value='协作';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('0.txt'));
 expect(requests).toHaveLength(0);expect(document.querySelector('.search-summary-toggle')).toBeNull();
});

it('总结返回空内容时自动打开首个文件',async()=>{
 await start();await search('协作');
 requests[0].resolve({text:'',evidence:requests[0].results});
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe('0.txt'));
 expect(document.querySelector('.search-summary-toggle')).toBeNull();
});

it.each(['md','html','txt'])('%s 原文左上角返回已生成的搜索总结，不重新搜索或生成总结',async extension=>{
 const base=invoke.getMockImplementation()!;
 const source={...hits[0],path:`/notes/source.${extension}`,title:`source.${extension}`};
 invoke.mockImplementation((command,args)=>command==='run_query'?Promise.resolve({terms:[],results:[source],trace:[],conversation:null}):base(command,args));
 await start();await search('协作');
 requests[0].resolve({text:'已有的搜索总结',evidence:[source]});
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .answer-text')?.textContent).toBe('已有的搜索总结'));
 document.querySelector<HTMLElement>('.search-result')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-title strong')?.textContent).toBe(source.title));
 document.querySelector<HTMLButtonElement>('[aria-label="返回搜索总结"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .answer-text')?.textContent).toBe('已有的搜索总结'));
 expect(document.querySelector('.search-preview-title')).toBeNull();
 expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(1);
 expect(requests).toHaveLength(1);
});

it('选中第二片段后往返总结和列表，仍保留第二片段',async()=>{
 await start();await search('协作',false);
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.search-passage-trigger')!.click();await tick();
 document.querySelectorAll<HTMLButtonElement>('.search-passage-option')[1].click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('2 / 2'));
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('2 / 2'));
 document.querySelector<HTMLButtonElement>('.search-preview-back')!.click();await tick();
 expect(document.querySelector('.search-answer')).not.toBeNull();
 document.querySelector<HTMLElement>('.search-result')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('2 / 2'));
});

it('翻页后打开总结引用，仍可查看引用文件的所有相关片段',async()=>{
 localStorage.setItem('sag.search.mode','deep_search');
 await start();
 const base=invoke.getMockImplementation()!;
 const extra=Array.from({length:10},(_,i)=>({...hits[0],id:100+i,document_id:100+i,path:`/notes/extra-${i}.txt`,title:`extra-${i}.txt`}));
 invoke.mockImplementation((command,args)=>command==='run_query'?Promise.resolve({terms:[],results:[hits[0],{...hits[0],id:90},...hits.slice(1),...extra],trace:[],conversation:null}):base(command,args));
 await search('协作',false);await vi.waitFor(()=>expect(requests).toHaveLength(1));
 requests[0].resolve({text:'摘要 [1]',evidence:requests[0].results});
 document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-result')).toHaveLength(2));
 document.querySelector<HTMLButtonElement>('.search-summary-toggle')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-answer .citation')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.search-answer .citation')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('1 / 2'));
 document.querySelector<HTMLButtonElement>('.search-passage-trigger')!.click();await tick();
 expect(document.querySelectorAll('.search-passage-option')).toHaveLength(2);
});
