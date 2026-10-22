// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';localStorage.clear();vi.clearAllMocks()});
it('从原文件打开内置解析得到的 Markdown，不要求安装 Docling',async()=>{
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',language:'zh-CN',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],answer:{url:'https://example.com/v1',model:'test',models:['test']},jev:{url:'https://example.com/v1',model:'jev',models:[]},retrieval:{chunk_chars:512}};
  if(command==='get_docling_status')return{installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[]};
  if(command==='list_roots')return[{path:'/notes',name:'notes',status:'ready',count:1,error:null}];
  if(command==='list_files')return[{id:1,path:'/notes/.sag-converted/report.docx.md',name:'report.docx.md'}];
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries')return args.path==='/notes'?[{path:'/notes/report.docx',name:'report.docx',directory:false,symlink:false}]:[];
  if(command==='read_document')return{id:1,path:'/notes/.sag-converted/report.docx.md',name:'report.docx.md',body:'# Parsed document\n\nDocument evidence.',hash:'fixture'};
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-toggle')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.sidebar .tree-toggle')!.click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.sidebar .tree-name')].some(node=>node.textContent==='report.docx')).toBe(true));
 [...document.querySelectorAll<HTMLButtonElement>('.sidebar .tree-name')].find(node=>node.textContent==='report.docx')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('read_document',{path:'/notes/report.docx'}));
 await vi.waitFor(()=>expect(document.querySelector('.wysiwyg-editor')).not.toBeNull());
 expect(document.querySelector('.readonly-document')).toBeNull();
 expect(invoke.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
});
