// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>|undefined;
let roots:{path:string;name:string;status:string;count:number;error:null}[];
beforeEach(()=>{
 localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');
 roots=[{path:'/notes',name:'Notes',status:'ready',count:0,error:null},{path:'/projects',name:'Projects',status:'ready',count:0,error:null}];
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'files',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return roots;
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries')return args.path==='/notes'?[{path:'/notes/nested',name:'nested',directory:true,symlink:false}]:[];
  if(command==='browse_folders')return{path:'/home',parent:'/',home:'/home',folders:[]};
  if(command==='save_document')return{path:args.path,name:args.path.split('/').at(-1),body:args.body,hash:'saved'};
  return[];
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();localStorage.clear()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull())}
function newDocument(){document.querySelector<HTMLButtonElement>('.library-folder-action')!.click()}
async function saveAs(name:string){
 await vi.waitFor(()=>expect(document.querySelector('.tiptap')).not.toBeNull());
 const input=document.querySelector<HTMLInputElement>('[aria-label="文件名"]')!;
 input.value=name;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.editor-head button')].find(button=>button.textContent==='保存')!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(true));
}
it('总览新建文档默认选择第一个文件夹，保存后写入该文件夹',async()=>{
 await start();newDocument();await tick();
 expect(document.querySelector('.new-document-dialog')).not.toBeNull();
 expect(document.querySelector('[aria-label="保存位置"]')?.textContent).toBe('Notes/notes');
 document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.click();
 await saveAs('新文档.md');
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/新文档.md',body:'',expectedHash:null});
});
it('总览可以选择其他文件夹作为新文档保存位置',async()=>{
 await start();newDocument();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="保存位置"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role="option"][data-value="/projects"]')).not.toBeNull());
 document.querySelector('[role="option"][data-value="/projects"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
 document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.click();
 await saveAs('项目记录.md');
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/projects/项目记录.md',body:'',expectedHash:null});
});
it('取消文件夹选择不创建文档，重新打开仍默认第一个文件夹',async()=>{
 await start();newDocument();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="保存位置"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role="option"][data-value="/projects"]')).not.toBeNull());
 document.querySelector('[role="option"][data-value="/projects"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.new-document-dialog button')].find(button=>button.textContent==='取消')!.click();await tick();
 expect(document.querySelector('.new-document-dialog')).toBeNull();
 expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件');
 expect(invoke.mock.calls.some(([command])=>command==='save_document'||command==='put_draft')).toBe(false);
 newDocument();await tick();
 expect(document.querySelector('[aria-label="保存位置"]')?.textContent).toBe('Notes/notes');
});
it('在子文件夹内直接新建并保存，不再选择保存位置',async()=>{
 await start();
 document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/nested"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes/nested"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('nested'));
 newDocument();await saveAs('子目录文档.md');
 expect(document.querySelector('.new-document-dialog')).toBeNull();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/nested/子目录文档.md',body:'',expectedHash:null});
});
it('还没有添加文件夹时先打开添加文件夹窗口',async()=>{
 roots=[];await start();newDocument();
 await vi.waitFor(()=>expect(document.querySelector('[role="dialog"][aria-label="添加文件夹"]')).not.toBeNull());
 expect(document.querySelector('.new-document-dialog')).toBeNull();
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
});
