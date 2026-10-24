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

const message={question:'原有问题',answer:'当前会话的回答',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'原有问题'};
let app:ReturnType<typeof mount>,history:Conversation[],resolveQuery:(output:QueryOutput)=>void;
beforeEach(async()=>{
 events.clear();localStorage.setItem('sag.onboarding.seen','1');
 history=[
  {id:'first',title:'文档导入',updated:2,deleted:false,messages:[{...message}]},
  {id:'second',title:'架构讨论',updated:1,deleted:false,messages:[{...message,question:'缓存策略',answer:'Use a persistent document cache.'}]},
 ];
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return structuredClone(history.filter(conversation=>!conversation.deleted));
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='edit_conversation'){
   const conversation=history.find(conversation=>conversation.id===args.id)!;
   if(args.title!==null)conversation.title=args.title;
   if(args.deleted!==null)conversation.deleted=args.deleted;
   return;
  }
  if(command==='run_query')return new Promise<QueryOutput>(resolve=>{resolveQuery=resolve});
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelectorAll('.conversation-sidebar-row')).toHaveLength(2));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();events.clear()});

function button(selector:string){return document.querySelector<HTMLButtonElement>(selector)!}
function sidebarItem(title:string){return button(`.conversation-sidebar-item[title="${title}"]`)}
async function openCurrent(){sidebarItem('文档导入').click();await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答'))}
async function openSidebar(){button('#sidebar-tab-conversations').click();await vi.waitFor(()=>expect(document.querySelector('#sidebar-pane-conversations:not([hidden])')).not.toBeNull())}
async function searchSidebar(value:string){const input=document.querySelector<HTMLInputElement>('.conversation-sidebar [aria-label="搜索会话"]')!;input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));await tick()}
async function menu(title:string,label:string){button(`.conversation-sidebar-more[aria-label="${title} 的选项"]`).click();await vi.waitFor(()=>expect(document.querySelector('[role="menuitem"]')).not.toBeNull());[...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item=>item.textContent?.trim()===label)!.click();await tick()}
async function ask(){const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;input.value='继续分析';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(true));const id=invoke.mock.calls.find(([command])=>command==='run_query')![1].query.id;events.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'正在输出的正文',count:0}});await vi.waitFor(()=>expect(document.querySelectorAll('.answer-text')[1]?.textContent).toBe('正在输出的正文'));return id}
function cancellations(){return invoke.mock.calls.filter(([command])=>command==='cancel_query')}

it('在侧栏会话与文件夹之间切换，保留当前正文、输入草稿和聊天容器',async()=>{
 await openCurrent();
 const log=document.querySelector('.chat-log');
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='还未发送的问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(sidebarItem('文档导入').getAttribute('aria-current')).toBe('page');
 button('#sidebar-tab-folders').click();await tick();
 expect(document.querySelector('#sidebar-pane-conversations')?.hasAttribute('hidden')).toBe(true);
 await openSidebar();
 expect(button('#sidebar-tab-conversations').getAttribute('aria-selected')).toBe('true');
 expect(document.querySelector('.chat-log')).toBe(log);expect(input.value).toBe('还未发送的问题');
 expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答');expect(cancellations()).toHaveLength(0);
});

it('会话搜索框中 Escape 先清空关键词，输入框的 Escape 不影响会话',async()=>{
 await openCurrent();
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.focus();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));await tick();
 expect(document.activeElement).toBe(input);
 await searchSidebar('PERSISTENT');
 const search=document.querySelector<HTMLInputElement>('.conversation-sidebar-search input')!;
 search.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(search.value).toBe(''));
 expect(document.querySelectorAll('.conversation-sidebar-item')).toHaveLength(2);
 expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答');
});

it('对话内搜索问答内容并切换到另一会话',async()=>{
 await openCurrent();await openSidebar();await searchSidebar('PERSISTENT');
 expect(document.querySelectorAll('.conversation-sidebar-item')).toHaveLength(1);
 sidebarItem('架构讨论').click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.'));
 expect(cancellations()).toHaveLength(0);
 if(!document.querySelector('.conversation-sidebar'))await openSidebar();
 expect(sidebarItem('架构讨论').getAttribute('aria-current')).toBe('page');
});

it('侧栏菜单重命名和删除刷新真实 API 状态，删除可恢复',async()=>{
 await openCurrent();await openSidebar();await menu('架构讨论','重命名');
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="问答名称"]')).not.toBeNull());
 const input=document.querySelector<HTMLInputElement>('[aria-label="问答名称"]')!;
 input.value='缓存架构';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(sidebarItem('缓存架构')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'second',title:'缓存架构',deleted:null});
 expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答');
 await menu('缓存架构','删除');
 await vi.waitFor(()=>expect(sidebarItem('缓存架构')).toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'second',title:null,deleted:true});
 expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答');
 [...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(item=>item.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(sidebarItem('缓存架构')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'second',title:null,deleted:false});
});

it('重命名失败保留输入并显示错误，随后可用原输入重试',async()=>{
 await openCurrent();await openSidebar();await menu('架构讨论','重命名');
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="问答名称"]')).not.toBeNull());
 const original=invoke.getMockImplementation()!;let fail=true;
 invoke.mockImplementation((command,...args)=>{
  if(command==='edit_conversation'&&fail){fail=false;return Promise.reject(new Error('暂时无法修改会话名称'))}
  return original(command,...args);
 });
 const input=document.querySelector<HTMLInputElement>('[aria-label="问答名称"]')!;
 input.value='  失败后重试的名称  ';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('暂时无法修改会话名称'));
 expect(document.querySelector<HTMLInputElement>('[aria-label="问答名称"]')?.value).toBe('  失败后重试的名称  ');
 expect(sidebarItem('架构讨论')).not.toBeNull();expect(sidebarItem('失败后重试的名称')).toBeNull();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(sidebarItem('失败后重试的名称')).not.toBeNull());
 expect(document.querySelector('[aria-label="重命名问答"]')).toBeNull();
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'second',title:'失败后重试的名称',deleted:null});
});

it('生成中开关侧栏和选择自身保留输出，其他会话与新建被禁用且不发取消',async()=>{
 await openCurrent();await ask();await openSidebar();
 expect(sidebarItem('架构讨论').disabled).toBe(true);
 expect(sidebarItem('文档导入').disabled).toBe(false);
 expect(button('.chat-new').disabled).toBe(true);
 
 expect(button('.conversation-sidebar-more[aria-label="文档导入 的选项"]').disabled).toBe(true);
 sidebarItem('架构讨论').click();button('.chat-new').click();
 sidebarItem('文档导入').click();await tick();
 expect(document.querySelectorAll('.answer-text')[1]?.textContent).toBe('正在输出的正文');
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'n',ctrlKey:true,bubbles:true,cancelable:true}));await tick();
 expect(document.querySelectorAll('.answer-text')[1]?.textContent).toBe('正在输出的正文');
 expect(button('[aria-label="停止生成"]').disabled).toBe(false);expect(cancellations()).toHaveLength(0);
});

it('显式停止后允许切换，旧流式事件与迟到响应不会覆盖新选中的会话',async()=>{
 await openCurrent();const id=await ask();await openSidebar();
 button('.conversation-sidebar-stop').click();
 await vi.waitFor(()=>expect(sidebarItem('架构讨论').disabled).toBe(false));
 expect(cancellations().map(([,args])=>args.id)).toEqual([id]);
 sidebarItem('架构讨论').click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.'));
 events.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'迟到的旧片段',count:0}});
 resolveQuery({terms:[],results:[],trace:[],conversation:{...history[0],messages:[...history[0].messages,{...message,question:'继续分析',answer:'迟到的完整回答'}]}});
 await tick();await tick();
 expect(document.querySelectorAll('.answer-text')).toHaveLength(1);
 expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.');
 expect(cancellations()).toHaveLength(1);
});

it('停止后从侧栏删除当前会话，已输出的临时回答和当前选择同时清理',async()=>{
 await openCurrent();await ask();await openSidebar();
 button('.conversation-sidebar-stop').click();
 await vi.waitFor(()=>expect(button('.conversation-sidebar-more[aria-label="文档导入 的选项"]').disabled).toBe(false));
 await menu('文档导入','删除');
 await vi.waitFor(()=>expect(sidebarItem('文档导入')).toBeNull());
 expect(document.querySelector('.chat-log')).toBeNull();
 expect(document.querySelector('.answer-text')).toBeNull();
 expect(document.querySelector('.conversation-sidebar-item[aria-current="page"]')).toBeNull();
 expect(document.querySelector('.ask-home')).not.toBeNull();
});

it('较旧的历史刷新响应不会覆盖已到达的新会话名称',async()=>{
 await openCurrent();await openSidebar();
 const original=invoke.getMockImplementation()!;
 const responses:((conversations:Conversation[])=>void)[]=[];
 invoke.mockImplementation((command,...args)=>command==='list_conversations'?new Promise(resolve=>responses.push(resolve)):original(command,...args));
 events.get('history-changed')!({payload:null});events.get('history-changed')!({payload:null});
 const newer=structuredClone(history);newer[1].title='最新名称';
 responses[1](newer);await vi.waitFor(()=>expect(sidebarItem('最新名称')).not.toBeNull());
 responses[0](structuredClone(history));await tick();await tick();
 expect(sidebarItem('最新名称')).not.toBeNull();expect(sidebarItem('架构讨论')).toBeNull();
});
