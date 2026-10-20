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
afterEach(async()=>{await unmount(app);document.body.innerHTML='';localStorage.clear();vi.clearAllMocks()});
it('默认隐藏，设置开关仅控制右侧，保存关闭后立即恢复隐藏',async()=>{
 let showHidden=false;
 const entries=[{path:'/notes/.cache',name:'.cache',directory:true,symlink:false},{path:'/notes/正文.md',name:'正文.md',directory:false,symlink:false}];
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',language:'zh-CN',start_page:'ask',editor_size:15,show_hidden:showHidden,watch:true,excluded:[],answer:{url:'https://example.com/v1',model:'test',models:['test']},jev:{url:'https://example.com/v1',model:'jev',models:[]},retrieval:{chunk_chars:512}};
  if(command==='save_settings'){showHidden=args.settings.show_hidden;return;}
  if(command==='list_roots')return[{path:'/notes',name:'notes',status:'ready',count:1,error:null}];
  if(command==='list_files')return[{id:1,path:'/notes/正文.md',name:'正文.md'}];
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='get_docling_status')return{installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
  if(command==='list_entries')return args.path==='/notes'?entries.filter(e=>showHidden||!e.name.startsWith('.')):[];
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-toggle')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.sidebar .tree-toggle')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.sidebar .tree-name')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-entry')?.textContent).toBe('正文.md'));
 for(const enabled of [true,false]){
  [...document.querySelectorAll<HTMLButtonElement>('.sidebar button')].find(b=>b.textContent==='设置')!.click();await tick();
  [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(b=>b.textContent==='文件设置')!.click();await tick();
  expect(document.querySelectorAll('[aria-label="显示隐藏文件夹"]')).toHaveLength(1);
  const section=document.querySelector('.hidden-folder-settings')!;
  expect(section.closest('.settings-group')?.textContent).toContain('自动监听文件变化');
  expect(section.closest('.settings-group')?.textContent).toContain('索引并行数');
  expect(section.textContent).toContain('仅在右侧文件列表显示');
  document.querySelector<HTMLButtonElement>('[aria-label="显示隐藏文件夹"]')!.click();await tick();
  document.querySelector('form.settings-form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
  expect(showHidden).toBe(enabled);
  expect([...document.querySelectorAll('.simple-file-entry')].some(e=>e.textContent==='.cache')).toBe(enabled);
  expect([...document.querySelectorAll('.sidebar .tree-name')].some(e=>e.textContent==='.cache')).toBe(false);
 }
 expect(invoke.mock.calls.some(([command])=>command==='reindex')).toBe(false);
});
