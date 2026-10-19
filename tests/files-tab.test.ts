// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import {indexedDirectoryEntries} from '../src/lib/domain';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const files=[{id:1,path:'/notes/intro.txt',name:'intro.txt'},{id:2,path:'/notes/nested/deep/result.txt',name:'result.txt'}];
let app:ReturnType<typeof mount>|undefined;
beforeEach(()=>{
 localStorage.removeItem('sag.sidebar.expanded');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return[{path:'/notes',name:'Notes',status:'ready',count:2,error:null}];
  if(command==='list_files')return files;
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='browse_folders')return{path:'/home',parent:'/',home:'/home',folders:[]};
  if(command==='list_entries'){
   if(args.path==='/notes')return[{path:'/notes/intro.txt',name:'intro.txt',directory:false,symlink:false},{path:'/notes/private.bin',name:'private.bin',directory:false,symlink:false},{path:'/notes/nested',name:'nested',directory:true,symlink:false}];
   if(args.path==='/notes/nested')return[{path:'/notes/nested/deep',name:'deep',directory:true,symlink:false}];
   if(args.path==='/notes/nested/deep')return[{path:'/notes/nested/deep/result.txt',name:'result.txt',directory:false,symlink:false}];
   return[];
  }
  if(command==='read_document')return{id:2,path:args.path,name:args.path.split('/').at(-1),body:'已索引文档正文',hash:'original'};
  return[];
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();localStorage.removeItem('sag.sidebar.expanded')});
const fileTab=()=>document.querySelector<HTMLButtonElement>('.workspace-switch [title="文件"]')!;
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.ask-content')).not.toBeNull())}
async function open(path:string){document.querySelector<HTMLButtonElement>(`.simple-file-list [title="${path}"]`)!.click();await vi.waitFor(()=>expect(path.endsWith('.txt')?document.querySelector('.readonly-document'):document.querySelector(`.folder-browser h1`)?.textContent===path.split('/').at(-1)).toBeTruthy())}
it('文件标签从总览逐层进入，保留未索引文件，原文返回所属目录',async()=>{
 await start();fileTab().click();
 await vi.waitFor(()=>expect(fileTab().getAttribute('aria-current')).toBe('page'));
 expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件');
 expect(document.querySelector('.folder-count')?.textContent).toBe('1 个文件夹 · 2 个已索引文件');
 expect(document.querySelector('.library-folder-title .library-folder-action')?.textContent).toBe('添加文件夹');
 expect(document.querySelector('.workspace-header .library-folder-action')).toBeNull();
 expect(document.querySelector('.file-stats-heading strong')?.textContent).toBe('2');
 document.querySelector<HTMLButtonElement>('.library-index-details')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')).not.toBeNull());
 expect(document.querySelector('.index-history')).not.toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="关闭索引详情"]')!.click();await tick();
 await open('/notes');
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('notes'));
 expect(document.querySelector('.simple-file-list [title="/notes/intro.txt"]')).not.toBeNull();
 expect(document.querySelector('.library-topbar')).toBeNull();
 expect(document.querySelector('.library-tabs')).toBeNull();
 expect(document.querySelector('.workspace-header .workspace-back')).toBeNull();
 expect(document.querySelector('.library-folder-title .library-folder-action')?.textContent).toBe('新建文档');
 expect(document.querySelector('.library-folder-title .library-explore')?.textContent).toBe('探索模式');
 expect(document.querySelector('.library-stats')).toBeNull();
 expect([...document.querySelectorAll('.simple-file-entry.unindexed')].some(entry=>entry.textContent==='private.bin')).toBe(true);
 await open('/notes/nested');await open('/notes/nested/deep');await open('/notes/nested/deep/result.txt');
 await vi.waitFor(()=>expect(document.querySelector('.readonly-document')?.textContent).toContain('已索引文档正文'));
 expect(fileTab().getAttribute('aria-current')).toBe('page');
 document.querySelector<HTMLButtonElement>('.editor-head [title="返回文件夹"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('deep'));
 for(const title of ['nested','notes','已索引文件']){
  document.querySelector<HTMLButtonElement>('.library-parent')!.click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe(title));
 }
 expect(invoke.mock.calls.some(([command,args])=>command==='list_entries'&&args.path==='/')).toBe(false);
});
it('从问答页点击侧边栏文件，自动选中文件标签，返回时显示文件夹',async()=>{
 await start();
 document.querySelector<HTMLButtonElement>('[aria-label="展开Notes"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.tree [title="/notes/intro.txt"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.tree [title="/notes/intro.txt"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.readonly-document')).not.toBeNull());
 expect(fileTab().getAttribute('aria-current')).toBe('page');
 expect(document.querySelector('.editor-head [title="返回会话"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('.editor-head [title="返回文件夹"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
});
it('文件标签切换保留问答和搜索输入',async()=>{
 await start();
 const ask=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 ask.value='未发送的问题';ask.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.workspace-switch [title="问答"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="向你的文件提问"]')).not.toBeNull());
 expect(document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!.value).toBe('未发送的问题');
 document.querySelector<HTMLButtonElement>('.workspace-switch [title^="搜索"]')!.click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 const search=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
 search.value='未搜索的内容';search.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.workspace-switch [title^="搜索"]')!.click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 expect(document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!.value).toBe('未搜索的内容');
});
it('点击文件夹管理从文档返回已索引文件最外层，添加按钮仍独立工作',async()=>{
 await start();fileTab().click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 await open('/notes');await open('/notes/intro.txt');
 document.querySelector<HTMLButtonElement>('.folder-management-link')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件'));
 expect(fileTab().getAttribute('aria-current')).toBe('page');
 await open('/notes');
 document.querySelector<HTMLButtonElement>('.tree-heading [aria-label="添加文件夹"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="dialog"]')).not.toBeNull());
 expect(document.querySelector('.folder-browser h1')?.textContent).toBe('notes');
});
it('Windows 路径只生成当前目录的直接子项，不混入相似目录',()=>{
 const entries=indexedDirectoryEntries([{id:1,path:'C:\\notes\\deep\\a.txt',name:'a.txt'},{id:2,path:'C:\\notes\\deep\\b.txt',name:'b.txt'},{id:3,path:'C:\\notes-other\\outside.txt',name:'outside.txt'}],'C:\\notes');
 expect(entries).toEqual([{path:'C:\\notes\\deep',name:'deep',directory:true,symlink:false}]);
});
it('文件页面隐藏顶部返回按钮，保留逐级返回和文件夹管理入口',async()=>{
 await start();fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 expect(document.querySelector('.library-parent')).toBeNull();
 await open('/notes');await open('/notes/nested');await open('/notes/nested/deep');
 expect(document.querySelector('.simple-file-list')?.firstElementChild?.textContent).toBe('返回上层');
 expect(document.querySelector('.workspace-header .workspace-back')).toBeNull();
 document.querySelector<HTMLButtonElement>('.folder-management-link')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件'));
 await open('/notes');await open('/notes/intro.txt');
 expect(document.querySelector('.workspace-header .workspace-back')).toBeNull();
 document.querySelector<HTMLButtonElement>('.folder-management-link')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件'));
});
async function fileAction(label:string){
 document.querySelector<HTMLButtonElement>('.simple-file-entry [title="/notes/intro.txt"]')!.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')).not.toBeNull());
 [...document.querySelectorAll<HTMLElement>('#context-menu [role="menuitem"]')].find(item=>item.textContent===label)!.click();await tick();
}
it('文件列表仅用右键打开菜单，连续右键可以切换文件与文件夹',async()=>{
 await start();fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());await open('/notes');
 expect(document.querySelector('.simple-file-list .file-options')).toBeNull();
 for(const path of ['/notes/intro.txt','/notes/nested','/notes/intro.txt']){
  const row=document.querySelector<HTMLButtonElement>(`.simple-file-list [title="${path}"]`)!;
  row.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,cancelable:true,button:2,pointerType:'mouse'}));
  row.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));
  await tick();
  await vi.waitFor(()=>expect(document.querySelectorAll('#context-menu')).toHaveLength(1));
  expect(getComputedStyle(document.body).pointerEvents).not.toBe('none');
 }
 [...document.querySelectorAll<HTMLElement>('#context-menu [role="menuitem"]')].find(item=>item.textContent==='问答')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.reference-chip')?.textContent).toContain('intro.txt'));
});
it('单文件菜单限定问答引用与搜索范围',async()=>{
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{...await base(command,args),search_summary_enabled:false};
  if(command==='run_query')return{terms:[],results:[],trace:[],conversation:null};
  return base(command,args);
 });
 await start();fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());await open('/notes');
 await fileAction('问答');await vi.waitFor(()=>expect(document.querySelector('.reference-chip')?.textContent).toContain('intro.txt'));
 const ask=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;ask.value='这篇文档讲了什么？';ask.dispatchEvent(new Event('input',{bubbles:true}));await tick();ask.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(true));
 expect(invoke.mock.calls.find(([command])=>command==='run_query')![1].query).toMatchObject({scope:['/notes/intro.txt'],references:['/notes/intro.txt']});
 fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());await fileAction('搜索');
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 const search=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;search.value='正文';search.dispatchEvent(new Event('input',{bubbles:true}));await tick();search.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(2));
 expect(invoke.mock.calls.filter(([command])=>command==='run_query')[1][1].query.scope).toEqual(['/notes/intro.txt']);
});
it('单文件从知识库移除后列表更新，撤销只恢复该文件',async()=>{
 const base=invoke.getMockImplementation()!;let removed=false;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='remove_root'){removed=true;return;}
  if(command==='add_root'){removed=false;return '/notes';}
  if(command==='list_removed_directories')return removed?['/notes/intro.txt']:[];
  if(command==='list_files')return removed?files.slice(1):files;
  const result=await base(command,args);
  return command==='list_entries'&&removed?result.filter((entry:{path:string})=>entry.path!=='/notes/intro.txt'):result;
 });
 await start();fileTab().click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());await open('/notes');
 await fileAction('从知识库移除');
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/intro.txt"]')).toBeNull());
 expect(invoke.mock.calls.find(([command])=>command==='remove_root')![1]).toEqual({path:'/notes/intro.txt'});
 await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('原文件保留'));
 [...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(button=>button.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/intro.txt"]')).not.toBeNull());
 expect(invoke.mock.calls.find(([command])=>command==='add_root')![1]).toEqual({path:'/notes/intro.txt'});
});
