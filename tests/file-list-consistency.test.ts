// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';localStorage.clear();vi.restoreAllMocks();vi.clearAllMocks()});
it.each([
 {system:'macOS',root:'/notes',sep:'/',agent:'Macintosh'},
 {system:'Windows',root:'C:\\notes',sep:'\\',agent:'Windows NT 10.0'},
])('$system 左侧仅列出索引文件，右侧显示未索引文件和所有文件夹',async({root,sep,agent})=>{
 vi.spyOn(navigator,'userAgent','get').mockReturnValue(agent);
 const path=(name:string)=>root+sep+name;
 const entry=(name:string,directory=false)=>({path:path(name),name,directory,symlink:false});
 let removed=false;
 const entries=[entry('图片',true),entry('视频',true),entry('空目录',true),entry('.cache',true),entry('正文.md'),entry('原文.pdf')];
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',language:'zh-CN',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],answer:{url:'https://example.com/v1',model:'test',models:['test']},jev:{url:'https://example.com/v1',model:'jev',models:[]},retrieval:{chunk_chars:512}};
  if(command==='get_docling_status')return{installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
  if(command==='list_roots')return[{path:root,name:'notes',status:'ready',count:1,error:null}];
  if(command==='list_files')return removed?[]:[{id:1,path:path('正文.md'),name:'正文.md'}];
  if(command==='list_removed_directories')return removed?[path('正文.md')]:[];
  if(command==='remove_root'){expect(args.path).toBe(path('正文.md'));removed=true;return;}
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries'){
   if(args.path===root)return entries;
   if(args.path===path('图片'))return[{path:path('图片'+sep+'cover.png'),name:'cover.png',directory:false,symlink:false}];
   return[];
  }
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-name')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.sidebar .tree-toggle')!.click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent==='正文.md')).toBe(true));
 expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent==='原文.pdf')).toBe(false);
 expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent==='.cache')).toBe(false);
 for(const name of ['图片','视频','空目录'])expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent===name)).toBe(true);
 document.querySelector<HTMLButtonElement>('.sidebar .tree-name')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.simple-file-entry')).toHaveLength(6));
 const pdf=[...document.querySelectorAll<HTMLButtonElement>('.simple-file-row')].find(x=>x.textContent==='原文.pdf')!;
 pdf.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')).not.toBeNull());
 expect(document.querySelector('#context-menu')?.textContent).not.toContain('从知识库移除');
 document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')).toBeNull());
 const indexed=[...document.querySelectorAll<HTMLButtonElement>('.simple-file-row')].find(x=>x.textContent==='正文.md')!;
 indexed.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')?.textContent).toContain('从知识库移除'));
 [...document.querySelectorAll<HTMLElement>('#context-menu [role=menuitem]')].find(x=>x.textContent==='从知识库移除')!.click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.simple-file-entry.unindexed')].some(x=>x.textContent==='正文.md')).toBe(true));
 expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent==='正文.md')).toBe(false);
 expect(document.querySelectorAll('.simple-file-entry')).toHaveLength(6);
 indexed.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')).not.toBeNull());
 expect(document.querySelector('#context-menu')?.textContent).not.toContain('从知识库移除');
 document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('#context-menu')).toBeNull());
 expect(pdf.getAttribute('aria-disabled')).toBeNull();
 expect([...document.querySelectorAll('.simple-file-row')].some(x=>x.textContent==='.cache')).toBe(true);
 expect(pdf.closest('.simple-file-entry')?.classList.contains('unindexed')).toBe(true);
 pdf.click();await tick();
 expect(document.querySelector('[role=dialog]')?.textContent).toContain('需要转换文档后才能检索');
 expect(document.querySelector('.settings-window')).toBeNull();
 expect(invoke.mock.calls.some(([name])=>name==='read_document')).toBe(false);
 [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(x=>x.textContent==='取消')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).toBeNull());
 const imageDirectory=[...document.querySelectorAll<HTMLButtonElement>('.simple-file-row')].find(x=>x.textContent==='图片')!;
 imageDirectory.click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.simple-file-row')].some(x=>x.textContent==='cover.png')).toBe(true));
 expect([...document.querySelectorAll('.sidebar .tree-name')].some(x=>x.textContent==='cover.png')).toBe(false);
 expect([...document.querySelectorAll('.simple-file-entry')].find(x=>x.textContent==='cover.png')?.classList.contains('unindexed')).toBe(true);
 [...document.querySelectorAll<HTMLButtonElement>('.simple-file-row')].find(x=>x.textContent==='cover.png')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(x=>x.textContent==='去设置')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('文件设置'));
 expect(document.querySelector('.other-formats')).not.toBeNull();
 expect(invoke.mock.calls.some(([name])=>name==='install_docling'||name==='save_settings')).toBe(false);
});
