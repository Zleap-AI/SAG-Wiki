// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Conversation,Message,ToolCallRecord} from '../src/lib/types';

const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
vi.mock('../src/lib/markdown',async(importOriginal)=>{
 const module=await importOriginal<typeof import('../src/lib/markdown')>();
 const renderAnswer:typeof module.renderAnswer=(text,...rest)=>{if(text.includes('RENDER-FAILURE'))throw new RangeError('Invalid content for node table');return module.renderAnswer(text,...rest)};
 return {...module,renderAnswer};
});

let app:ReturnType<typeof mount>;
let resolveQuery:(value:unknown)=>void;
let currentHistory:Conversation[]=[];
const search:ToolCallRecord={id:'search-1',answer_offset:0,query:JSON.stringify({query:'清岚制造 合同审查'}),source_count:1,elapsed_ms:4900,tool_name:'search',server_name:'SAG',result:{isError:false,structuredContent:{sources:[{source_id:1,title:'a.md',path:'/a.md',content:'原文'}],omitted_sources:0}}};
const list:ToolCallRecord={id:'list-1',answer_offset:0,query:JSON.stringify({pattern:'**/*清岚*'}),source_count:0,elapsed_ms:12,tool_name:'list',server_name:'SAG',result:{isError:false,structuredContent:{items:[],sources:[],omitted_sources:0}}};
const skipped:ToolCallRecord={id:'list-2',answer_offset:0,query:JSON.stringify({pattern:'**/*清岚*'}),source_count:0,tool_name:'list',server_name:'SAG',skipped:true,error:'该调用与本轮先前的调用完全相同，结果不会改变，已跳过执行。请改用不同参数，或根据已有结果作答。'};
const base:Message={question:'问题',answer:'',evidence:[],model:'test',scope:[],references:[],status:'running',error:null,retrieval_question:'问题'};
const context={limit:65536,reserved_output:2048,estimated_input:1000,omitted_sources:0,omitted_history:0,input_tokens:1200,output_tokens:300,elapsed_ms:37600,generation_ms:20000};

beforeEach(async()=>{
 currentHistory=[];invoke.mockReset();listeners.clear();
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return currentHistory;
  if(command==='run_query')return new Promise(resolve=>resolveQuery=resolve);
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value=base.question;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';listeners.clear();vi.clearAllMocks()});

const queryId=()=>invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id as string;
async function finish(message:Message){
 const id=queryId();
 currentHistory=[{id,title:base.question,messages:[message],updated:1,deleted:false}];
 resolveQuery({terms:[],results:[],trace:[],conversation:currentHistory[0]});
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='list_conversations').length).toBeGreaterThan(1));
 await tick();await tick();
}

it('shows a placeholder with a regenerate action when a completed turn has no answer text',async()=>{
 const progress=listeners.get('query-progress')!;
 progress({payload:{id:queryId(),stage:'调用完成',delta:null,count:1,tool:{call:search,finished:true}}});await tick();
 await finish({...base,status:'complete',tool_calls:[search],context});
 const placeholder=document.querySelector('.answer-empty')!;
 expect(placeholder.textContent).toContain('本轮没有正文');
 expect(document.querySelector('.tool-group-title')?.textContent).toBe('检索 1 次');
 const process=document.querySelector<HTMLDetailsElement>('.answer-process')!;
 expect(process.open).toBe(true);
 expect(process.classList.contains('collapsible')).toBe(false);
 const regenerate=placeholder.querySelector('button')!;
 expect(regenerate.textContent).toBe('重新生成回答');
 const before=invoke.mock.calls.filter(([name])=>name==='run_query').length;
 regenerate.click();await tick();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='run_query').length).toBe(before+1));
 const [,request]=invoke.mock.calls.filter(([name])=>name==='run_query').at(-1)!;
 expect(request.query.replace_from).toBe(0);
});

it('explains a skipped repeated call and keeps it out of the counts',async()=>{
 await finish({...base,answer:'知识库里没有相关台账 [1]。',status:'complete',tool_calls:[search,list,skipped],evidence:[{id:1,document_id:1,path:'/a.md',title:'a.md',heading:'',content:'原文',start_line:1,end_line:1,source_ranges:[[1,1]],bm25:0,score:1,order:0,hash:'h'}],context});
 expect(document.querySelector('.tool-group-title')?.textContent).toBe('检索 1 次');
 expect(document.querySelector('.tool-group-note')?.textContent).toBe('1 次已跳过');
 expect(document.querySelector('.tool-group-time')?.textContent).toBe('4.9s');
 const states=[...document.querySelectorAll('.tool-call')].map(call=>[call.getAttribute('data-phase'),call.querySelector('.tool-state')?.textContent,call.querySelector('.tool-duration')?.textContent??'']);
 expect(states).toEqual([['complete','已完成','4.9s'],['complete','已完成','12 ms'],['skipped','已跳过','']]);
 const detail=[...document.querySelectorAll('.tool-call')][2];
 expect([...detail.querySelectorAll('h4')].map(h=>h.textContent)).toEqual(['调用参数','未执行原因']);
 expect([...detail.querySelectorAll('.tool-data')].at(-1)?.textContent).toContain('已跳过执行');
 expect(detail.querySelector('.tool-error')).toBeNull();
 expect(document.querySelector('.answer-empty')).toBeNull();
});

it('reports a failed summary above the answer and in the context line',async()=>{
 await finish({...base,answer:'仅根据本轮资料作答。',status:'complete',context:{...context,omitted_history:9,summary_error:'上下文摘要请求超过总时间限制'}});
 const notice=document.querySelector('.answer-notice')!;
 expect(notice.textContent).toContain('本次未能生成上下文摘要');
 expect(notice.textContent).toContain('更早的 9 轮内容本次未参考');
 expect(notice.querySelector('details')!.open).toBe(false);
 expect(notice.querySelector('pre')?.textContent).toBe('上下文摘要请求超过总时间限制');
 expect(document.querySelector('.answer-context')?.textContent).toContain('已略去 9 轮较早对话（本次未生成摘要）');
 expect(document.querySelector('.answer > .answer-text')?.textContent).toBe('仅根据本轮资料作答。');
});

it('keeps every answer readable when the renderer throws on one of them',async()=>{
 const errors=vi.spyOn(console,'error').mockImplementation(()=>{});
 const progress=listeners.get('query-progress')!;
 progress({payload:{id:queryId(),stage:'正在生成回答',delta:'正常开头 ',count:0,tool:null}});await tick();
 progress({payload:{id:queryId(),stage:'正在生成回答',delta:'RENDER-FAILURE <b>表格</b>',count:0,tool:null}});await tick();await tick();
 expect(document.querySelector('.answer > .answer-text')?.textContent).toBe('正常开头 RENDER-FAILURE <b>表格</b>');
 await finish({...base,answer:'正常开头 RENDER-FAILURE <b>表格</b>',status:'complete',context});
 expect(document.querySelector('.answer > .answer-text')?.textContent).toBe('正常开头 RENDER-FAILURE <b>表格</b>');
 expect(document.querySelector('.answer > .answer-text b')).toBeNull();
 expect(document.querySelector('.answer-empty')).toBeNull();
 expect(errors).toHaveBeenCalled();
 errors.mockRestore();
});
