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
const input=()=>document.querySelector<HTMLTextAreaElement>('.composer textarea')!;
const entry=()=>document.querySelector<HTMLButtonElement>('.help-entry button')!;
const back=()=>document.querySelector<HTMLButtonElement>('.workspace-back.chat-new')!;
const path='sag-help/zh-CN/folders.md';
const passage='在侧栏添加本地文件夹。';
const body='# 文件夹管理\n\n## 浏览文件夹\n\n查看已添加的文件。\n\n## 添加文件夹\n\n'+passage+'\n';
const startByte=new TextEncoder().encode(body.slice(0,body.indexOf(passage))).length;
const source={id:1,document_id:1,path,title:'文件夹管理',heading:'文件夹管理 / 添加文件夹',content:passage,hash:'bundled',
 start_line:9,end_line:9,source_ranges:[[9,9]],source_spans:[{start_byte:startByte,end_byte:startByte+new TextEncoder().encode(passage).length}],bm25:0,score:1,order:0};
function press(key:string){
 const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});
 input().dispatchEvent(event);
 return event;
}

beforeEach(async()=>{
 localStorage.clear();
 localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {theme:'light',language:'zh-CN',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='read_help_document')return {id:1,path,name:'文件夹管理',body,hash:'bundled'};
  if(command==='run_query'){
   const {id,question,scope,references,model}=args.query;
   const message={help:true,question,answer:`${passage} [1]`,evidence:[source],scope,references,model,status:'complete',error:null,retrieval_question:question};
   return {terms:[],results:[source],trace:[],conversation:{id,title:question,messages:[message],updated:0,deleted:false}};
  }
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(entry()?.textContent).toContain('SAG Wiki 助手'));
});
afterEach(async()=>{
 await unmount(app);
 document.body.innerHTML='';
 invoke.mockReset();
 localStorage.clear();
});

it('cycles the suggestion on entry, fills it with Tab and sends a typed edit',async()=>{
 entry().click();
 await vi.waitFor(()=>expect(input().value).toBe(''));
 await vi.waitFor(()=>expect(document.querySelector('h1')?.textContent).toBe('SAG Wiki 助手'));
 const initial=input().placeholder;
 expect(initial).toBe('怎么添加文件夹？');
 expect(press('Tab').defaultPrevented).toBe(true);
 await tick();
 expect(input().value).toBe(initial);
 expect(press('Tab').defaultPrevented).toBe(false);
 expect(entry()?.textContent).toContain('切回我的文件');
 expect(back()).toBeNull();
 entry().click();
 await vi.waitFor(()=>expect(entry().textContent).toContain('SAG Wiki 助手'));
 entry().click();
 await vi.waitFor(()=>expect(input().placeholder).toBe('怎么配置模型？'));
 input().value='怎么配置本地模型？';
 input().dispatchEvent(new Event('input',{bubbles:true}));
 await tick();
 press('Enter');
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('run_query',{query:expect.objectContaining({
  kind:'ask_help',question:'怎么配置本地模型？',scope:[],references:[]
 })}));
});

it('sends the empty input suggestion and opens the local citation at the referenced passage',async()=>{
 entry().click();
 await vi.waitFor(()=>expect(input().placeholder).toBe('怎么添加文件夹？'));
 expect(input().value).toBe('');
 press('Enter');
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('run_query',{query:expect.objectContaining({
  kind:'ask_help',question:'怎么添加文件夹？',scope:[],references:[]
 })}));
 const citation=()=>document.querySelector<HTMLButtonElement>('button[data-citation="0"]');
 await vi.waitFor(()=>expect(citation()).not.toBeNull());
 expect(entry()).toBeNull();
 citation()!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('read_help_document',{path}));
 await vi.waitFor(()=>expect(document.querySelector('.citation-document .search-hit.active')?.textContent).toContain(passage));
 expect(document.querySelector('.source-path')?.textContent).toContain('添加文件夹');
 expect(invoke.mock.calls.some(([command])=>['open_external_link','read_document'].includes(command))).toBe(false);
 document.querySelector<HTMLButtonElement>('.citation-close')!.click();
 back().click();
 await vi.waitFor(()=>expect(entry()?.textContent).toContain('切回我的文件'));
 entry().click();
 await vi.waitFor(()=>expect(entry()?.textContent).toContain('SAG Wiki 助手'));
});
