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
 await vi.waitFor(()=>expect(document.querySelectorAll('.history-row')).toHaveLength(2));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();events.clear()});

function button(selector:string){return document.querySelector<HTMLButtonElement>(selector)!}
function sidebarItem(title:string){return button(`.conversation-item[title="${title}"]`)}
async function openCurrent(){sidebarItem('文档导入').click();await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('当前会话的回答'))}
async function home(){button('[aria-label="返回会话列表"]').click();await vi.waitFor(()=>expect(document.querySelector('.recent-conversations')).not.toBeNull())}
async function menu(title:string,label:string){button(`.history-more[aria-label="${title} 的选项"]`).click();await vi.waitFor(()=>expect(document.querySelector('[role="menuitem"]')).not.toBeNull());[...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item=>item.textContent?.trim()===label)!.click();await tick()}
async function ask(){const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;input.value='继续分析';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(true));const id=invoke.mock.calls.find(([command])=>command==='run_query')![1].query.id;events.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'正在输出的正文',count:0}});await vi.waitFor(()=>expect(document.querySelectorAll('.answer-text')[1]?.textContent).toBe('正在输出的正文'));return id}
function cancellations(){return invoke.mock.calls.filter(([command])=>command==='cancel_query')}


it('侧栏始终显示文件夹，折叠不影响当前问答与草稿',async()=>{
 await openCurrent();const log=document.querySelector('.chat-log');
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='未发送的问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 button('[aria-label="收起侧边栏"]').click();await tick();button('[aria-label="展开侧边栏"]').click();await tick();
 expect(document.querySelector('.sidebar-tabs')).toBeNull();expect(document.querySelector('.conversation-sidebar')).toBeNull();
 expect(document.querySelector('.chat-log')).toBe(log);expect(input.value).toBe('未发送的问题');expect(cancellations()).toHaveLength(0);
});
it('返回首页后搜索回答内容并用键盘打开原会话',async()=>{
 await openCurrent();await home();button('[aria-label="搜索会话"]').click();await tick();
 const input=document.querySelector<HTMLInputElement>('input[aria-label="搜索会话"]')!;
 input.value='PERSISTENT';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelectorAll('.history-search-result')).toHaveLength(1);
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.'));
 expect(document.querySelector('.history-search-dialog')).toBeNull();expect(cancellations()).toHaveLength(0);
});
it('首页菜单重命名和删除会话，删除后可以恢复',async()=>{
 await menu('架构讨论','重命名');
 const input=document.querySelector<HTMLInputElement>('[aria-label="问答名称"]')!;
 input.value='缓存架构';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(sidebarItem('缓存架构')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'second',title:'缓存架构',deleted:null});
 await menu('缓存架构','删除');await vi.waitFor(()=>expect(sidebarItem('缓存架构')).toBeNull());
 [...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(item=>item.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(sidebarItem('缓存架构')).not.toBeNull());
});
it('重命名失败保留输入并允许重试',async()=>{
 await menu('架构讨论','重命名');const original=invoke.getMockImplementation()!;let fail=true;
 invoke.mockImplementation((command,...args)=>{if(command==='edit_conversation'&&fail){fail=false;return Promise.reject(new Error('暂时无法修改会话名称'))}return original(command,...args)});
 const input=document.querySelector<HTMLInputElement>('[aria-label="问答名称"]')!;
 input.value='新名称';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('暂时无法修改会话名称'));
 expect(input.value).toBe('新名称');input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(sidebarItem('新名称')).not.toBeNull());
});
it('生成中禁用返回与新建快捷键，折叠侧栏不取消生成',async()=>{
 await openCurrent();await ask();expect(button('.chat-new').disabled).toBe(true);
 button('.chat-new').click();window.dispatchEvent(new KeyboardEvent('keydown',{key:'n',ctrlKey:true,bubbles:true,cancelable:true}));
 button('[aria-label="收起侧边栏"]').click();await tick();
 expect(document.querySelectorAll('.answer-text')[1]?.textContent).toBe('正在输出的正文');
 expect(document.querySelector('.recent-conversations')).toBeNull();expect(cancellations()).toHaveLength(0);
});
it('停止后返回首页并打开另一会话，旧响应不会覆盖新会话',async()=>{
 await openCurrent();const id=await ask();button('[aria-label="停止生成"]').click();
 await vi.waitFor(()=>expect(button('.chat-new').disabled).toBe(false));await home();sidebarItem('架构讨论').click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.'));
 events.get('query-progress')!({payload:{id,stage:'正在生成回答',delta:'迟到内容',count:0}});
 resolveQuery({terms:[],results:[],trace:[],conversation:{...history[0],messages:[...history[0].messages,{...message,answer:'旧回答'}]}});
 await tick();await tick();expect(document.querySelector('.answer-text')?.textContent).toBe('Use a persistent document cache.');expect(cancellations()).toHaveLength(1);
});
it('旧历史刷新不会覆盖已到达的新名称',async()=>{
 const original=invoke.getMockImplementation()!;const responses:((conversations:Conversation[])=>void)[]=[];
 invoke.mockImplementation((command,...args)=>command==='list_conversations'?new Promise(resolve=>responses.push(resolve)):original(command,...args));
 events.get('history-changed')!({payload:null});events.get('history-changed')!({payload:null});
 const newer=structuredClone(history);newer[1].title='最新名称';responses[1](newer);
 await vi.waitFor(()=>expect(sidebarItem('最新名称')).not.toBeNull());responses[0](structuredClone(history));await tick();await tick();
 expect(sidebarItem('最新名称')).not.toBeNull();expect(sidebarItem('架构讨论')).toBeNull();
});
