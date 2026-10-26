// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const conversations=Array.from({length:50},(_,i)=>({id:`chat-${i}`,title:`会话 ${i}`,updated:i,deleted:false,messages:[{question:`问题 ${i}`,answer:i===41?'Deep connection tracing':'回答 '+i,model:'test',evidence:[],scope:[],references:[],status:'complete',error:null,retrieval_question:`问题 ${i}`}]}));
const keys=['sag.history.scroll','sag.sidebar.library.scroll','sag.sidebar.expanded'];
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 keys.forEach(key=>localStorage.removeItem(key));localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return conversations;
  if(command==='list_roots')return[{path:'/notes',name:'Knowledge',status:'ready',count:80,error:null}];
  if(command==='list_entries')return Array.from({length:80},(_,i)=>({path:`/notes/file-${i}.md`,name:`file-${i}.md`,directory:false,symlink:false}));
  return[];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';keys.forEach(key=>localStorage.removeItem(key));vi.restoreAllMocks();vi.clearAllMocks()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelectorAll('.history-row')).toHaveLength(50));await tick()}
it('搜索历史标题和内容，键盘打开结果，并保留原列表位置',async()=>{
 await start();
 const list=document.querySelector<HTMLElement>('.ask-content')!;list.scrollTop=520;list.dispatchEvent(new Event('scroll'));
 document.querySelector<HTMLButtonElement>('button[aria-label="搜索会话"]')!.click();await tick();
 const input=document.querySelector<HTMLInputElement>('input[aria-label="搜索会话"]')!;
 input.value='TRACING';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 const result=document.querySelector<HTMLButtonElement>('.history-search-result')!;
 expect(result.tabIndex).toBe(-1);expect(result.textContent).toContain('会话 41');expect(document.querySelectorAll('.history-search-result')).toHaveLength(1);
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.history-search-dialog')).toBeNull());
 expect(document.querySelector('.answer-text')?.textContent).toBe('Deep connection tracing');
 document.querySelector<HTMLButtonElement>('[aria-label="返回会话列表"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('button[aria-label="搜索会话"]')).not.toBeNull());
 expect(document.querySelector<HTMLElement>('.ask-content')!.scrollTop).toBe(520);
 document.querySelector<HTMLButtonElement>('button[aria-label="搜索会话"]')!.click();await tick();
 const titleInput=document.querySelector<HTMLInputElement>('input[aria-label="搜索会话"]')!;
 titleInput.value='会话 23';titleInput.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelectorAll('.history-search-result')).toHaveLength(1);
 expect(document.querySelector('.history-search-result')!.textContent).toContain('会话 23');
 titleInput.value='完全不存在';titleInput.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelector('.history-search-empty')?.textContent).toContain('没有找到相关会话');
});
it('会话列表放在右侧问答首页，左侧只保留知识库',async()=>{
 localStorage.setItem('sag.sidebar.tab','conversations');
 await start();
 expect(document.querySelector('.sidebar-tabs')).toBeNull();
 expect(document.querySelector('.conversation-sidebar')).toBeNull();
 expect(document.querySelector('.sidebar>.tree')).not.toBeNull();
 expect(document.querySelector('.sidebar .chat-new')).toBeNull();
 expect(document.querySelector('.desktop-window>.desktop-titlebar[data-tauri-drag-region]')).not.toBeNull();
 expect(document.querySelector('.window-title')).toBeNull();
 expect(document.querySelector('.sidebar .sidebar-titlebar[data-tauri-drag-region]')).not.toBeNull();
 expect(document.querySelector('.sidebar-titlebar [aria-label="收起侧边栏"]')).not.toBeNull();
 expect(document.querySelector('.brand-row button')).toBeNull();
 expect(document.querySelector('.workspace-header[data-tauri-drag-region]')).not.toBeNull();
 expect(document.querySelectorAll('.desktop-titlebar .caption-controls button')).toHaveLength(3);
 expect(document.querySelector('.sidebar [aria-label="最小化"]')).toBeNull();
 expect(document.querySelector('.sidebar .recent-conversations')).toBeNull();
 expect(document.querySelectorAll('.workspace .recent-conversations .history-row')).toHaveLength(50);
 expect(document.querySelector('.workspace .topbar')).toBeNull();
 expect(document.querySelector('[aria-label="调整会话与知识库区域大小"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('.workspace .conversation-item')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('回答 0'));
 expect(document.querySelector('.workspace .recent-conversations')).toBeNull();
 expect(document.querySelector('.workspace .topbar')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="返回会话列表"]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.history-row')).toHaveLength(50));
 expect(document.querySelector('.answer-text')).toBeNull();
 document.querySelector<HTMLButtonElement>('.conversation-item')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('回答 0'));
});
it('会话和知识库独立保存滚动位置，收起侧栏与重开应用后恢复',async()=>{
 await start();
 document.querySelector<HTMLButtonElement>('[aria-label="展开Knowledge"]')!.click();await tick();await tick();
 const list=document.querySelector<HTMLElement>('.ask-content')!;
 const tree=document.querySelector<HTMLElement>('.tree')!;
 list.scrollTop=430;list.dispatchEvent(new Event('scroll'));
 tree.scrollTop=170;tree.dispatchEvent(new Event('scroll'));
 document.querySelector<HTMLButtonElement>('[aria-label="收起侧边栏"]')!.click();await tick();
 expect(document.querySelector('.sidebar-titlebar [aria-label="展开侧边栏"]')).not.toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="展开侧边栏"]')!.click();await tick();
 expect(list.scrollTop).toBe(430);expect(tree.scrollTop).toBe(170);
 await unmount(app);await start();
 expect(document.querySelector<HTMLElement>('.ask-content')!.scrollTop).toBe(430);
 expect(document.querySelector<HTMLElement>('.tree')!.scrollTop).toBe(170);
 expect(document.querySelector('[aria-label="折叠Knowledge"]')).not.toBeNull();
});

it('没有历史会话时只显示提问区，不保留列表或顶部导航的空位',async()=>{
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>command==='list_conversations'?[]:original(command,...args));
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="发送问题"]')).not.toBeNull());
 expect(document.querySelector('.ask-home')).not.toBeNull();
 expect(document.querySelector('.ask-home.has-history')).toBeNull();
 expect(document.querySelector('.recent-conversations')).toBeNull();
 expect(document.querySelector('.workspace .topbar')).toBeNull();
});

it('问答与搜索切换位于右侧顶部，折叠左栏后仍可切换并保留会话',async()=>{
 await start();
 expect(document.querySelector('.sidebar nav')).toBeNull();
 const nav=document.querySelector<HTMLElement>('.workspace-header nav')!;
 expect(nav).not.toBeNull();
 const ask=nav.querySelector<HTMLButtonElement>('[title="问答"]')!;
 const search=nav.querySelector<HTMLButtonElement>('[title="搜索（⌘ / Ctrl + K）"]')!;
 expect(ask.getAttribute('aria-current')).toBe('page');
 document.querySelector<HTMLButtonElement>('.conversation-item')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('回答 0'));
 expect(document.querySelector('.workspace-header [aria-label="返回会话列表"]')).not.toBeNull();
 search.click();
 await vi.waitFor(()=>expect(search.getAttribute('aria-current')).toBe('page'));
 expect(document.querySelector('.search-page')).not.toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="收起侧边栏"]')!.click();await tick();
 ask.click();
 await vi.waitFor(()=>expect(ask.getAttribute('aria-current')).toBe('page'));
 expect(document.querySelector('.answer-text')?.textContent).toBe('回答 0');
});

it('没有文件夹时说明用途，快捷按钮打开文件夹选择窗口',async()=>{
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>{
  if(command==='list_roots')return [];
  if(command==='browse_folders')return {path:'/home',parent:'/',home:'/home',folders:[]};
  return original(command,...args);
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.folder-empty-state')).not.toBeNull());
 expect(document.querySelector('.folder-empty-state')?.textContent).toContain('添加文件夹后，即可对其中的文件提问和搜索。');
 document.querySelector<HTMLButtonElement>('.folder-empty-state button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-picker')).not.toBeNull());
 expect(invoke.mock.calls.some(([command])=>command==='browse_folders')).toBe(true);
});

it('右上角接入入口直达 CLI 设置，保留未发送的问题',async()=>{
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>{
  if(command==='get_cli_settings')return{access:{enabled:true},installations:[]};
  const result=await original(command,...args);
  if(command==='get_settings')return{...result,
   answer:{...result.answer,url:'https://example.com/v1',has_key:false},
   jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},
   retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},
   excluded:[],watch:true,show_hidden:false,timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048,
  };
  return result;
 });
 await start();
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='未发送的问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelector('.agent-connect-hint')).toBeNull();
 expect(document.querySelector('.sidebar .agent-connect-entry')).toBeNull();
 expect(document.querySelectorAll('.agent-connect-entry')).toHaveLength(1);
  document.querySelector<HTMLButtonElement>('.workspace-header .agent-connect-entry')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('CLI设置'));
  expect(document.querySelector('.cli-settings')).not.toBeNull();
  document.querySelector<HTMLButtonElement>('[aria-label="关闭设置"]')!.click();await tick();
  expect(input.value).toBe('未发送的问题');
 expect(invoke.mock.calls.some(([command])=>command==='install_cli_integration'||command==='run_query')).toBe(false);
});
