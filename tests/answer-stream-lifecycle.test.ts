// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Conversation,Message} from '../src/lib/types';

const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));

let app:ReturnType<typeof mount>;
let resolveQuery:(value:unknown)=>void;
let currentHistory:Conversation[]=[];
const message:Message={question:'解释流式过程',answer:'',evidence:[],model:'test',scope:[],references:[],status:'running',error:null,retrieval_question:'解释流式过程'};
const answer=()=>document.querySelector('.answer > .answer-text')?.textContent;

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
 input.value=message.question;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';listeners.clear();vi.clearAllMocks()});

it('does not let a delayed running-history snapshot erase a completed streamed answer',async()=>{
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 const stale:Conversation={id,title:message.question,messages:[message],updated:1,deleted:false};
 let resolveEarlierHistory:(value:Conversation[])=>void;
 invoke.mockImplementationOnce(()=>new Promise(resolve=>resolveEarlierHistory=resolve));
 // The backend emits history-changed after storing the running placeholder.
 listeners.get('history-changed')!({payload:null});
 listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'保留完整回答。',count:0}});
 await vi.waitFor(()=>expect(answer()).toBe('保留完整回答。'));
 const completed={...stale,messages:[{...message,answer:'保留完整回答。',status:'complete'}],updated:2};
 currentHistory=[completed];
 resolveQuery({terms:[],results:[],trace:[],conversation:completed});
 await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
 expect(answer()).toBe('保留完整回答。');
 // Its older IPC response arrives after the completion-triggered refresh.
 resolveEarlierHistory!([stale]);await tick();await tick();
 expect(answer()).toBe('保留完整回答。');
});

async function cancelling(){
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 let resolveCancellation:()=>void;
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,...args)=>command==='cancel_query'?new Promise<void>(resolve=>resolveCancellation=resolve):original(command,...args));
 document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='cancel_query')).toBe(true));
 return {id,finish:()=>resolveCancellation!()};
}

it('ignores query progress while a cancellation request is still pending',async()=>{
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'停止前的内容。',count:0}});
 await vi.waitFor(()=>expect(answer()).toBe('停止前的内容。'));
 const cancellation=await cancelling();
 try{
  listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'取消期间迟到的内容。',count:0}});
  await tick();await tick();
  expect(answer()).toBe('停止前的内容。');
 }finally{cancellation.finish();await tick()}
});

it('does not reopen a tool approval dialog while a cancellation request is still pending',async()=>{
 const cancellation=await cancelling();
 try{
  listeners.get('tool-approval')!({payload:{id:'late-approval',query_id:cancellation.id,server_name:'迟到工具',tool_name:'read',arguments:{path:'/docs/a.md'}}});
  await tick();await tick();
  expect(document.querySelector('[aria-label="确认 MCP 工具调用"]')).toBeNull();
 }finally{cancellation.finish();await tick()}
});
