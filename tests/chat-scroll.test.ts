// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>,resolveQuery:(value:unknown)=>void,viewportHeight=200;
const resizeCallbacks=new Set<()=>void>();
const message={question:'历史问题',answer:'历史回答。'.repeat(80),evidence:[],model:'test',scope:[],references:[],status:'done',error:null,retrieval_question:'历史问题'};
const conversations=[{id:'first',title:'较早对话',messages:[message],updated:0,deleted:false},{id:'second',title:'较新对话',messages:[{...message,question:'另个问题'}],updated:0,deleted:false}];
let savedConversations=conversations;
const log=()=>document.querySelector<HTMLElement>('.chat-log')!;
const bottom=()=>log().scrollHeight-log().clientHeight;
const openChat=async(title='较早对话')=>{
 if(!document.querySelector(`[title="${title}"]`)){
  document.querySelector<HTMLButtonElement>('[aria-label="新建问答"]')!.click();
  await vi.waitFor(()=>expect(document.querySelector(`[title="${title}"]`)).not.toBeNull());
 }
 document.querySelector<HTMLButtonElement>(`[title="${title}"]`)!.click();await vi.waitFor(()=>expect(log()).not.toBeNull())
};
beforeEach(async()=>{
 viewportHeight=200;resizeCallbacks.clear();savedConversations=conversations;
 vi.spyOn(HTMLElement.prototype,'scrollHeight','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('chat-log')?400+(this.querySelector('.chat-messages')?.textContent?.length||0):0});
 vi.spyOn(HTMLElement.prototype,'clientHeight','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('chat-log')?viewportHeight:0});
 vi.stubGlobal('ResizeObserver',class{callback:()=>void;constructor(callback:()=>void){this.callback=callback;resizeCallbacks.add(callback)}observe(){}disconnect(){resizeCallbacks.delete(this.callback)}});
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return savedConversations;
  if(command==='run_query')return new Promise(resolve=>resolveQuery=resolve);
  return [];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.restoreAllMocks();vi.unstubAllGlobals();listeners.clear()});
it('打开历史对话和切换到相同高度的另一段对话，均定位到末尾',async()=>{
 await openChat();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 log().scrollTop=0;log().dispatchEvent(new Event('scroll'));await openChat('较新对话');await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});
it('发送新问题、流式文本增长和最终回答替换均跟随到底部',async()=>{
 await openChat();
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='新的问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 for(const delta of ['第一段。'.repeat(60),'第二段。'.repeat(60)]){
  listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta,count:0}});
  await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 }
 resolveQuery({terms:[],results:[],trace:[],conversation:{...conversations[0],messages:[message,{...message,question:'新的问题',answer:'最终回答。'.repeat(150)}]}});
 await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});
it('阅读历史后点击发送，立即回到底部并恢复流式跟随',async()=>{
 await openChat();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 log().scrollTop=80;log().dispatchEvent(new Event('scroll'));
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='从历史位置继续提问';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="发送问题"]')!.click();
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'新回答。'.repeat(80),count:0}});
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});
it('渲染后的内容或窗口高度变化，仍保持在底部',async()=>{
 await openChat();await tick();log().scrollTop=0;viewportHeight=120;
 resizeCallbacks.forEach(callback=>callback());
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});

it('手动上滚时暂停跟随，回到底部后继续跟随流式输出',async()=>{
 await openChat();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='继续回答';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 const delta=()=>listeners.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'新增内容。'.repeat(40),count:0}});
 delta();await tick();
 // The scroll can occur while an output-following animation frame is already queued.
 log().scrollTop=100;log().dispatchEvent(new Event('scroll'));
 delta();await tick();resizeCallbacks.forEach(callback=>callback());
 await new Promise(requestAnimationFrame);
 await new Promise(requestAnimationFrame);
 expect(log().scrollTop).toBe(100);
 log().scrollTop=bottom();log().dispatchEvent(new Event('scroll'));
 delta();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});

it.each(['思考中','正在生成回答'])('%s 时轻微上滚也暂停跟随，主动回到底部后恢复',async stage=>{
 await openChat();
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='继续回答';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
 const id=invoke.mock.calls.find(([name])=>name==='run_query')![1].query.id;
 const delta=()=>listeners.get('query-progress')!({payload:{id,stage,count:0,
  delta:stage==='思考中'?null:'新增回答。'.repeat(40),
  reasoning_delta:stage==='思考中'?'新增思考。'.repeat(40):undefined}});
 delta();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 const readingTop=bottom()-1;
 log().dispatchEvent(new WheelEvent('wheel',{deltaY:-1}));
 log().scrollTop=readingTop;log().dispatchEvent(new Event('scroll'));
 for(let i=0;i<3;i++){
  delta();await tick();resizeCallbacks.forEach(callback=>callback());
  await new Promise(requestAnimationFrame);
  await new Promise(requestAnimationFrame);
  expect(log().scrollTop).toBe(readingTop);
 }
 log().scrollTop=bottom();log().dispatchEvent(new Event('scroll'));
 delta();await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
});

it('新对话生成完成并取得历史记录 ID 时，不打断手动阅读位置',async()=>{
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='第一条问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(log()).not.toBeNull());
 await vi.waitFor(()=>expect(log().scrollTop).toBe(bottom()));
 log().scrollTop=50;log().dispatchEvent(new Event('scroll'));
 const complete={...conversations[0],id:'new-chat',messages:[{...message,question:'第一条问题'}]};
 savedConversations=[complete,...conversations];
 resolveQuery({terms:[],results:[],trace:[],conversation:complete});
 await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
 await new Promise(requestAnimationFrame);
 await new Promise(requestAnimationFrame);
 expect(log().scrollTop).toBe(50);
});
