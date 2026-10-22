// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return[{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  if(command==='list_files')return[{id:1,path:'/notes/doc.md',name:'doc.md'}];
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries')return[{path:'/notes/doc.md',name:'doc.md',directory:false,symlink:false}];
  if(command==='read_document')return{path:args.path,name:'doc.md',body:'# 标题\n\n正文。\n',hash:'original'};
  return[];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();localStorage.clear()});
async function openEditor(create=false){
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.ask-content')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.workspace-switch [title="文件"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/doc.md"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>(create?'.library-folder-action':'.simple-file-list [title="/notes/doc.md"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.tiptap')).not.toBeNull());await tick();
 await new Promise(resolve=>setTimeout(resolve,150));
}
async function leaveEditor(){
 document.querySelector<HTMLButtonElement>('.editor-head [title="返回文件夹"]')!.click();
 await tick();
}
for(const create of [false,true])it(`${create?'新建':'已有'}文档打开后未修改，可以直接离开`,async()=>{
 await openEditor(create);
 expect(document.querySelector('.document-status')?.textContent).not.toBe('未保存');
 await leaveEditor();
 expect(document.querySelector('[role="dialog"]')).toBeNull();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());
 expect(invoke.mock.calls.some(([command])=>command==='put_draft'||command==='save_document')).toBe(false);
});
for(const create of [false,true])it(`${create?'新建':'已有'}文档正文修改后才显示未保存并阻止直接离开`,async()=>{
 await openEditor(create);
 const paste=new Event('paste',{bubbles:true,cancelable:true});
 Object.defineProperty(paste,'clipboardData',{value:{getData:(format:string)=>format==='text/plain'?'新增文字':''}});
 document.querySelector<HTMLElement>('.tiptap')!.dispatchEvent(paste);await tick();
 expect(document.querySelector('.document-status')?.textContent).toBe('未保存');
 await leaveEditor();
 expect(document.querySelector('[role="dialog"]')?.textContent).toContain('还有未保存内容');
});
it('新文档修改文件名后提示保存',async()=>{
 await openEditor(true);
 const name=document.querySelector<HTMLInputElement>('[aria-label="文件名"]')!;
 name.value='我的文档.md';name.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelector('.document-status')?.textContent).toBe('未保存');
 await leaveEditor();
 expect(document.querySelector('[role="dialog"]')?.textContent).toContain('我的文档.md');
});
it('末尾为代码块的文档初始化不会提示保存',async()=>{
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>command==='read_document'
  ?{path:args.path,name:'doc.md',body:'# 验收代码\n\n```python\ndef launch_quota():\n    return 37\n```\n',hash:'original'}
  :base(command,args));
 await openEditor();
 expect(document.querySelector('.document-status')?.textContent).toBe('已保存');
 await leaveEditor();
 expect(document.querySelector('[role="dialog"]')).toBeNull();
});
