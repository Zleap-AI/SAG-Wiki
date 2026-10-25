// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const files=[{id:1,path:'/kb/notes/a.md',name:'a.md'},{id:2,path:'/kb/notes/b.md',name:'b.md'},{id:3,path:'/kb/notes/plain.md',name:'plain.md'}];
const notes=[
 {path:'/kb/notes/a.md',title:'a',question:'q',state:'changed',source_count:2,attention:1},
 {path:'/kb/notes/b.md',title:'b',question:'q',state:'intact',source_count:1,attention:0},
];
let app:ReturnType<typeof mount>|undefined;
beforeEach(()=>{
 localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'files',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return[{path:'/kb',name:'kb',status:'ready',count:3,error:null}];
  if(command==='list_files')return files;
  if(command==='list_notes')return notes;
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries'){
   if(args.path==='/kb')return[{path:'/kb/notes',name:'notes',directory:true,symlink:false}];
   if(args.path==='/kb/notes')return files.map(f=>({path:f.path,name:f.name,directory:false,symlink:false}));
   return[];
  }
  return[];
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();listeners.clear()});

it('文件列表为笔记显示状态点，文件夹聚合需要关注的数量，并可筛选待核对笔记',async()=>{
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 expect(document.querySelector('.simple-file-row .note-badge-count')?.textContent).toBe('1');
 document.querySelector<HTMLButtonElement>('.simple-file-row')!.click();
 const notesRow=()=>[...document.querySelectorAll<HTMLButtonElement>('.simple-file-row')].find(row=>row.textContent?.includes('notes'));
 await vi.waitFor(()=>expect(notesRow()).toBeDefined());
 expect(notesRow()!.querySelector('.note-badge-count')?.textContent).toBe('1');
 notesRow()!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.simple-file-row .note-badge').length).toBe(2));
 const states=[...document.querySelectorAll('.simple-file-entry')].map(row=>[row.textContent?.trim(),row.querySelector('.note-badge')?.getAttribute('data-state')??null]);
 expect(states).toEqual([['a.md','changed'],['b.md','intact'],['plain.md',null]]);
 const filter=document.querySelector<HTMLButtonElement>('.library-file-filter')!;
 filter.click();filter.click();filter.click();
 await vi.waitFor(()=>expect(filter.textContent).toContain('待核对笔记'));
 expect([...document.querySelectorAll('.simple-file-entry')].map(row=>row.textContent?.trim())).toEqual(['a.md']);
});
