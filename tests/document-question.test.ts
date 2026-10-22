// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import {setLanguage} from '../src/lib/i18n';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
const previous={id:'previous',title:'已有会话',updated:0,deleted:false,messages:[{
 question:'原来的问题',answer:'原来的回答',model:'test',evidence:[],scope:[],references:[],status:'complete',error:null,retrieval_question:'原来的问题',
}]};
const paths=['/notes/项目说明.md','/notes/其他文档.md','/notes/文本.txt'];
beforeEach(()=>{
 localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return[{path:'/notes',name:'Notes',status:'ready',count:paths.length,error:null}];
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_files')return paths.map((path,id)=>({id,path,name:path.split('/').at(-1)}));
  if(command==='list_entries')return paths.map(path=>({path,name:path.split('/').at(-1),directory:false,symlink:false}));
  if(command==='list_conversations')return[previous];
  if(command==='read_document')return{path:args.path,name:args.path.split('/').at(-1),body:args.path.endsWith('.html')?'<h1>网页正文</h1>':'# 项目说明\n\n当前文档的正文。',hash:'original'};
  if(command==='save_document')return{path:args.path,name:args.path.split('/').at(-1),body:args.body,hash:'saved'};
  if(command==='run_query')return new Promise(()=>{});
  return[];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';localStorage.clear();vi.clearAllMocks();});
async function start(){
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-toggle')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.sidebar .tree-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-name[title="/notes/项目说明.md"]')).not.toBeNull());
}
async function openDocument(name='项目说明.md'){
 document.querySelector<HTMLButtonElement>(`.sidebar .tree-name[title="/notes/${name}"]`)!.click();
 await vi.waitFor(()=>expect(document.querySelector('.document-question-toggle')).not.toBeNull());
}
async function expand(){document.querySelector<HTMLButtonElement>('.document-question-toggle')!.click();await tick();}
async function typeQuestion(value='这份文档的重点是什么？'){
 const input=document.querySelector<HTMLInputElement>('.document-question input')!;
 input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();return input;
}
function submit(){document.querySelector<HTMLButtonElement>('.document-question [type="submit"]')!.click();}
async function request(){await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(true));return invoke.mock.calls.find(([command])=>command==='run_query')![1].query;}

it('展开文档问答并聚焦输入框，空问题不能发送，收起后保留输入',async()=>{
 await start();await openDocument();await expand();
 const input=document.querySelector<HTMLInputElement>('.document-question input')!;
 expect(input.placeholder).toBe('对当前文档想问些什么？');expect(document.activeElement).toBe(input);
 expect(document.querySelector('.document-question textarea')).toBeNull();
 expect(document.querySelector('.document-question-form')?.lastElementChild?.getAttribute('type')).toBe('submit');
 expect(document.querySelector('.document-question-form')?.textContent).not.toContain('项目说明.md');
 expect(document.querySelector<HTMLButtonElement>('.document-question [type="submit"]')!.disabled).toBe(true);
 await typeQuestion('  ');expect(document.querySelector<HTMLButtonElement>('.document-question [type="submit"]')!.disabled).toBe(true);
 await typeQuestion();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await tick();
 expect(document.querySelector('.document-question-form')).toBeNull();
 await expand();expect(document.querySelector<HTMLInputElement>('.document-question input')!.value).toBe('这份文档的重点是什么？');
 expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(false);
});
it('从已有会话打开文档后提问，新建会话并立即引用当前文件发送',async()=>{
 await start();document.querySelector<HTMLButtonElement>('[title="已有会话"]')!.click();await tick();
 await openDocument();await expand();await typeQuestion('  这份文档的重点是什么？  ');submit();
 expect(await request()).toMatchObject({kind:'ask',question:'这份文档的重点是什么？',conversation_id:null,scope:[paths[0]],references:[paths[0]],replace_from:null});
 expect(document.querySelector('.document-question')).toBeNull();
 expect(document.querySelector('.ask-content')?.textContent).toContain('这份文档的重点是什么？');
 expect(document.querySelector('.reference-chip')?.textContent).toContain('项目说明.md');
 expect(document.querySelector('.ask-content')?.textContent).not.toContain('原来的回答');
 expect(invoke.mock.calls.some(([command])=>command==='replace_messages'||command==='edit_conversation')).toBe(false);
});
it('只读文档同样可以直接引用并提问',async()=>{
 await start();await openDocument('文本.txt');await expand();await typeQuestion();submit();
 expect(await request()).toMatchObject({references:[paths[2]],scope:[paths[2]],conversation_id:null});
});
it('切换文档关闭输入框，不携带上一份文档的问题',async()=>{
 await start();await openDocument();await expand();await typeQuestion();await openDocument('其他文档.md');await expand();
 expect(document.querySelector<HTMLInputElement>('.document-question input')!.value).toBe('');
 expect(document.querySelector('.document-question-form')?.getAttribute('aria-label')).toContain('其他文档.md');
 await typeQuestion();submit();expect((await request()).references).toEqual([paths[1]]);
});
it('中文输入确认和 Shift+Enter 不发送，普通 Enter 发送',async()=>{
 await start();await openDocument();await expand();const input=await typeQuestion();
 input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true}));
 input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await tick();
 expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(false);
 input.dispatchEvent(new KeyboardEvent('keyup',{key:'Enter',bubbles:true}));
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true}));await tick();
 expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(false);
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));expect((await request()).references).toEqual([paths[0]]);
});
it('存在未保存修改时取消离开，保留文档和问题；保存后才发送',async()=>{
 await start();await openDocument();
 const paste=new Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(paste,'clipboardData',{value:{getData:(format:string)=>format==='text/plain'?'新补充内容':''}});
 await vi.waitFor(()=>expect(document.querySelector('.tiptap')).not.toBeNull());document.querySelector('.tiptap')!.dispatchEvent(paste);await tick();
 await expand();await typeQuestion();submit();await tick();
 const cancel=[...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(button=>button.textContent==='取消')!;cancel.click();await tick();
 expect(document.querySelector('.tiptap')?.textContent).toContain('新补充内容');
 expect(document.querySelector<HTMLInputElement>('.document-question input')!.value).toBe('这份文档的重点是什么？');
 expect(invoke.mock.calls.some(([command])=>command==='run_query')).toBe(false);
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('.document-question [type="submit"]')!.disabled).toBe(false));
 submit();await vi.waitFor(()=>expect(document.querySelector('[role="dialog"]')).not.toBeNull());
 [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(button=>button.textContent==='保存')!.click();
 expect((await request()).references).toEqual([paths[0]]);
 expect(invoke.mock.calls.findIndex(([command])=>command==='save_document')).toBeLessThan(invoke.mock.calls.findIndex(([command])=>command==='run_query'));
});
it('新文档先选择名称和保存位置，再引用真实文件路径',async()=>{
 await start();document.querySelector<HTMLButtonElement>('.workspace-switch [title="文件"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.library-folder-action')).not.toBeNull());document.querySelector<HTMLButtonElement>('.library-folder-action')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.document-question-toggle')).not.toBeNull());await expand();await typeQuestion();submit();
 await vi.waitFor(()=>expect(document.querySelector('.new-document-dialog')).not.toBeNull());
 const name=document.querySelector<HTMLInputElement>('.new-document-dialog [aria-label="文件名"]')!;name.value='新项目说明.md';name.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.click();
 expect((await request()).references).toEqual(['/notes/新项目说明.md']);
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(true);
});
it('英文界面使用对应的问答提示',async()=>{
 await start();await openDocument();setLanguage('en');await tick();await expand();
 expect(document.querySelector<HTMLInputElement>('.document-question input')!.placeholder).toBe('What would you like to ask about this document?');
});
