// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Conversation,NoteDetail} from '../src/lib/types';

const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));

const evidence={id:1,document_id:1,path:'/kb/src/guard.rs',title:'guard.rs',heading:'',content:'fn check()',start_line:2,end_line:4,source_ranges:[[10,20]] as [number,number][],source_spans:[{start_byte:10,end_byte:20,start_line:2,start_column:1,end_line:4,end_column:1}],bm25:0,score:1,order:0,hash:'h1'};
const conversation:Conversation={id:'chat',title:'权限问题',updated:0,deleted:false,messages:[
 {question:'登录后权限在哪里校验？',answer:'在 guard 里 [1]。',scope:['/kb/src'],references:[],model:'test',evidence:[evidence],status:'complete',error:null,retrieval_question:'登录后权限在哪里校验？'},
 {question:'未完成的问题',answer:'',scope:[],references:[],model:'test',evidence:[],status:'failed',error:'x',retrieval_question:'未完成的问题'},
]};
const note:NoteDetail={summary:{path:'/kb/notes/登录后权限在哪里校验.md',title:'登录后权限在哪里校验',question:'登录后权限在哪里校验？',asked_at:1_761_370_800_000,model:'test',scope:['/kb/src'],state:'intact',source_count:1,attention:0,checked_at:1_761_370_800_000},
 sources:[{source:{id:1,path:'/kb/src/guard.rs',hash:'h1',start_byte:10,end_byte:20,start_line:2,end_line:4,text:'fn check()'},state:'intact',current:{start_byte:10,end_byte:20,start_line:2,start_column:1,end_line:4,end_column:1},current_text:'fn check()',similarity:1}]};
let app:ReturnType<typeof mount>,saved:Conversation,saveArgs:Record<string,unknown>|null,noteBody:string;
beforeEach(async()=>{
 vi.clearAllMocks();saved=structuredClone(conversation);saveArgs=null;localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');
 noteBody='---\nsag_note: 1\nquestion: "登录后权限在哪里校验？"\nsources: []\n---\n\n在 guard 里 [1]。\n';
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']},notes:{folder:null,ask_location:false}};
  if(command==='list_conversations')return[saved];
  if(command==='list_roots')return[{path:'/kb',name:'kb',status:'ready',count:2,error:null}];
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries')return args.path==='/kb'?[{path:'/kb/notes',name:'notes',directory:true,symlink:false},{path:'/kb/src',name:'src',directory:true,symlink:false}]:[];
  if(command==='list_notes')return[note.summary];
  if(command==='save_note'){saveArgs=args as Record<string,unknown>;saved={...saved,messages:saved.messages.map((m,i)=>i===0?{...m,note_path:note.summary.path}:m)};return note;}
  if(command==='read_document')return{path:args.path,name:'登录后权限在哪里校验.md',body:noteBody,hash:'n1'};
  if(command==='get_note')return note;
  if(command==='verify_notes')return[note.summary];
  return[];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
 document.querySelector<HTMLButtonElement>('[title="权限问题"]')!.click();await tick();
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const buttons=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>(`[aria-label="${label}"]`)];

it('完成的回答可以保存为笔记，失败的回答不能',async()=>{
 await vi.waitFor(()=>expect(buttons('保存为笔记').length).toBe(2));
 const [complete,failed]=buttons('保存为笔记');
 expect(complete.disabled).toBe(false);
 expect(failed.disabled).toBe(true);
 complete.click();await tick();
 const dialog=document.querySelector('.save-note-dialog')!;
 expect(dialog).not.toBeNull();
 expect(dialog.querySelector<HTMLInputElement>('[aria-label="文件名"]')!.value).toBe('登录后权限在哪里校验.md');
 expect(dialog.textContent).toContain('1 条引用，来自 1 个文件');
 expect(dialog.textContent).toContain('默认笔记文件夹');
 dialog.querySelector<HTMLButtonElement>('.primary')!.click();
 await vi.waitFor(()=>expect(saveArgs).not.toBeNull());
 expect(saveArgs).toMatchObject({conversationId:'chat',messageIndex:0,target:null,fileName:'登录后权限在哪里校验.md',remember:false});
 await vi.waitFor(()=>expect(document.querySelector('.save-note-dialog')).toBeNull());
 await vi.waitFor(()=>expect(buttons('打开笔记').length).toBe(1));
 expect(document.querySelector('#notice')?.textContent).toContain('已保存笔记《登录后权限在哪里校验》');
});

it('选择文件夹后保存到该目录并可记住位置',async()=>{
 await vi.waitFor(()=>expect(buttons('保存为笔记').length).toBe(2));
 buttons('保存为笔记')[0].click();await tick();
 const dialog=document.querySelector('.save-note-dialog')!;
 dialog.querySelector<HTMLButtonElement>('.save-folder-tree [aria-label="展开kb"]')!.click();
 await vi.waitFor(()=>expect(dialog.querySelector('.save-folder-tree [title="notes"]')).not.toBeNull());
 dialog.querySelector<HTMLButtonElement>('.save-folder-tree [title="notes"]')!.click();await tick();
 expect(dialog.textContent).toContain('/kb/notes');
 dialog.querySelector<HTMLButtonElement>('[aria-label="记住此位置，下次直接保存"]')!.click();await tick();
 dialog.querySelector<HTMLButtonElement>('.primary')!.click();
 await vi.waitFor(()=>expect(saveArgs).not.toBeNull());
 expect(saveArgs).toMatchObject({target:'/kb/notes',fileName:'登录后权限在哪里校验.md',remember:true});
});

it('默认目录下修改的文件名会传给服务',async()=>{
 await vi.waitFor(()=>expect(buttons('保存为笔记').length).toBe(2));
 buttons('保存为笔记')[0].click();await tick();
 const input=document.querySelector<HTMLInputElement>('.save-note-dialog [aria-label="文件名"]')!;
 input.value='权限校验要点';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector<HTMLButtonElement>('.save-note-dialog .primary')!.click();
 await vi.waitFor(()=>expect(saveArgs).not.toBeNull());
 expect(saveArgs).toMatchObject({target:null,fileName:'权限校验要点.md'});
});

it('记住位置且关闭询问时直接保存，按住 Option 仍打开面板',async()=>{
 await unmount(app);document.body.innerHTML='';
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>command==='get_settings'?{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']},notes:{folder:'/kb/notes',ask_location:false}}:base(command,args));
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
 document.querySelector<HTMLButtonElement>('[title="权限问题"]')!.click();await tick();
 await vi.waitFor(()=>expect(buttons('保存为笔记').length).toBe(2));
 buttons('保存为笔记')[0].click();
 await vi.waitFor(()=>expect(saveArgs).not.toBeNull());
 expect(document.querySelector('.save-note-dialog')).toBeNull();
 expect(saveArgs).toMatchObject({target:'/kb/notes',fileName:null,remember:false});
 await vi.waitFor(()=>expect(buttons('打开笔记').length).toBe(1));
 saveArgs=null;saved=structuredClone(conversation);listeners.get('history-changed')?.({payload:null});
 await vi.waitFor(()=>expect(buttons('保存为笔记').length).toBe(2));
 buttons('保存为笔记')[0].dispatchEvent(new MouseEvent('click',{bubbles:true,altKey:true}));await tick();
 expect(document.querySelector('.save-note-dialog')).not.toBeNull();
 expect(saveArgs).toBeNull();
});

it('打开笔记时显示状态条，并能重新检查和打开原文',async()=>{
 saved={...saved,messages:saved.messages.map((m,i)=>i===0?{...m,note_path:note.summary.path}:m)};
 listeners.get('history-changed')?.({payload:null});
 await vi.waitFor(()=>expect(buttons('打开笔记').length).toBe(1));
 buttons('打开笔记')[0].click();
 await vi.waitFor(()=>expect(document.querySelector('.note-status')).not.toBeNull());
 const status=document.querySelector('.note-status')!;
 expect(status.textContent).toContain('登录后权限在哪里校验？');
 expect(status.textContent).toContain('所有引用与原文一致。');
 [...status.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes('重新检查'))!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([command,args])=>command==='verify_notes'&&(args as {paths:string[]}).paths[0]===note.summary.path)).toBe(true));
 [...status.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes('查看引用'))!.click();await tick();
 expect(status.querySelector('.note-source')?.textContent).toContain('guard.rs');
 [...status.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes('打开原文'))!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.source-dialog')).not.toBeNull());
 expect(document.querySelector('.source-dialog .citation-title')?.textContent).toBe('guard.rs');
});

it('普通 Markdown 不显示笔记状态',async()=>{
 noteBody='# 普通文档\n\n正文';
 saved={...saved,messages:saved.messages.map((m,i)=>i===0?{...m,note_path:'/kb/notes/plain.md'}:m)};
 listeners.get('history-changed')?.({payload:null});
 await vi.waitFor(()=>expect(buttons('打开笔记').length).toBe(1));
 buttons('打开笔记')[0].click();
 await vi.waitFor(()=>expect(document.querySelector('.tiptap')).not.toBeNull());
 await tick();
 expect(document.querySelector('.note-status')).toBeNull();
 expect(invoke.mock.calls.some(([command])=>command==='get_note')).toBe(false);
});
