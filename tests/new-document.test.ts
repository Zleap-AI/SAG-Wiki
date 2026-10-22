// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Draft} from '../src/lib/types';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>|undefined;
let roots:{path:string;name:string;status:string;count:number;error:null}[];
let savedDrafts:Draft[];
beforeEach(()=>{
 localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');savedDrafts=[];
 roots=[{path:'/notes',name:'Notes',status:'ready',count:0,error:null},{path:'/projects',name:'Projects',status:'ready',count:0,error:null}];
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'files',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return roots;
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='list_entries')return args.path==='/notes'?[{path:'/notes/nested',name:'nested',directory:true,symlink:false},{path:'/notes/readme.md',name:'readme.md',directory:false,symlink:false}]:args.path==='/notes/nested'?[{path:'/notes/nested/deep',name:'deep',directory:true,symlink:false}]:[];
  if(command==='browse_folders')return{path:'/home',parent:'/',home:'/home',folders:[]};
  if(command==='add_root'){roots=[...roots,{path:args.path,name:'Home',status:'ready',count:0,error:null}];return args.path;}
  if(command==='list_drafts')return savedDrafts;
  if(command==='put_draft'){savedDrafts=[structuredClone(args.draft)];return;}
  if(command==='save_document')return{path:args.path,name:args.path.split('/').at(-1),body:args.body,hash:'saved'};
  return[];
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();localStorage.clear()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull())}
async function newDocument(){document.querySelector<HTMLButtonElement>('.library-folder-action')!.click();await vi.waitFor(()=>expect(document.querySelector('.tiptap')).not.toBeNull())}
async function nameDocument(name:string){const input=document.querySelector<HTMLInputElement>('[aria-label="文件名"]')!;input.value=name;input.dispatchEvent(new Event('input',{bubbles:true}));await tick()}
async function nameBeforeSave(name:string){const input=document.querySelector<HTMLInputElement>('.new-document-dialog [aria-label="文件名"]')!;input.value=name;input.dispatchEvent(new Event('input',{bubbles:true}));await tick()}
async function writeBody(text:string){const paste=new Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(paste,'clipboardData',{value:{getData:(format:string)=>format==='text/plain'?text:''}});document.querySelector<HTMLElement>('.tiptap')!.dispatchEvent(paste);await tick()}
function save(){document.querySelector<HTMLButtonElement>('.editor-head .primary')!.click()}
async function chooseProjects(){document.querySelector<HTMLButtonElement>('.save-folder-tree [title="Projects"]')!.click();await tick()}
async function confirmSave(){document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.click();await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(true));await vi.waitFor(()=>expect(document.querySelector('.document-status')?.textContent).not.toBe('未保存'))}
async function cancelSave(){[...document.querySelectorAll<HTMLButtonElement>('.new-document-dialog button')].find(button=>button.textContent==='取消')!.click();await tick()}

it('总览直接打开空白文档，未修改返回时不选择文件夹或写文件',async()=>{
 await start();await newDocument();
 expect(document.querySelector('[role="dialog"]')).toBeNull();
 expect(document.querySelector('[aria-label="文件夹路径"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('.editor-head [title="返回文件夹"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('已索引文件'));
 expect(invoke.mock.calls.some(([command])=>command==='save_document'||command==='put_draft')).toBe(false);
 expect(invoke.mock.calls.some(([command,args])=>command==='list_entries'&&args.path==='')).toBe(false);
});
it('第一次保存才选择文件夹，默认第一个；再次保存不再选择',async()=>{
 await start();await newDocument();await nameDocument('新文档.md');save();await tick();
 expect(document.querySelector('.new-document-dialog h2')?.textContent).toBe('保存文档');
 expect(document.querySelector('.new-document-dialog [aria-label="文件名"]')).toBeNull();
 expect(document.querySelector('.save-folder-tree [aria-pressed="true"]')?.textContent).toBe('Notes');
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
 await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/新文档.md',body:'',expectedHash:null});
 save();await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='save_document')).toHaveLength(2));
 expect(document.querySelector('.new-document-dialog')).toBeNull();
 expect(invoke.mock.calls.filter(([command])=>command==='save_document')[1][1]).toMatchObject({path:'/notes/新文档.md',expectedHash:'saved'});
});
it('先写内容，再选择其他文件夹保存，内容保留',async()=>{
 await start();await newDocument();await nameDocument('项目记录.md');await writeBody('项目记录正文');save();await tick();await chooseProjects();await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/projects/项目记录.md',body:'项目记录正文',expectedHash:null});
 expect(document.querySelector('[aria-label="文件夹路径"]')?.textContent).toContain('projects');
});
it('首次保存可以直接编辑名称，并自动补充 Markdown 后缀',async()=>{
 await start();await newDocument();await writeBody('使用保存时输入的名称');save();await tick();
 expect(document.querySelector<HTMLInputElement>('.new-document-dialog [aria-label="文件名"]')?.value).toBe('未命名.md');
 await nameBeforeSave('项目说明');await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/项目说明.md',body:'使用保存时输入的名称',expectedHash:null});
 expect(document.querySelector<HTMLInputElement>('.editor-head [aria-label="文件名"]')?.value).toBe('项目说明.md');
});
it('空名称不能保存，无效名称在保存弹窗中提示并允许修正',async()=>{
 await start();await newDocument();save();await tick();await nameBeforeSave('  ');
 expect(document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.disabled).toBe(true);
 await nameBeforeSave('无效/名称');document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.click();await tick();
 expect(document.querySelector('.new-document-dialog [role="alert"]')?.textContent).toContain('请输入有效的 Markdown 文件名');
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
 await nameBeforeSave('有效名称');await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/有效名称.md',body:'',expectedHash:null});
});
it('保存位置树可展开多层并选择子文件夹，隐藏文件条目且不展开侧栏',async()=>{
 await start();await newDocument();await nameDocument('层级文档.md');await writeBody('保留到子文件夹的正文');save();await tick();
 expect(document.querySelector('.new-document-dialog [role="combobox"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('.save-folder-tree [aria-label="展开Notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.save-folder-tree [title="nested"]')).not.toBeNull());
 expect(document.querySelector('.save-folder-tree [title="readme.md"]')).toBeNull();
 expect(document.querySelector('.sidebar [aria-label="展开Notes"]')?.getAttribute('aria-expanded')).toBe('false');
 document.querySelector<HTMLButtonElement>('.save-folder-tree [aria-label="展开nested"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.save-folder-tree [title="deep"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.save-folder-tree [title="deep"]')!.click();await tick();
 expect(document.querySelectorAll('.save-folder-tree [aria-pressed="true"]')).toHaveLength(1);
 expect(document.querySelector('.save-folder-tree [aria-pressed="true"]')?.textContent).toBe('deep');
 await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/nested/deep/层级文档.md',body:'保留到子文件夹的正文',expectedHash:null});
});
it('展开目录失败时显示错误，文档内容与已选择的保存位置保留',async()=>{
 const implementation=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{if(command==='list_entries'&&args.path==='/projects')throw new Error('无法读取项目目录');return implementation(command,args)});
 await start();await newDocument();await writeBody('读取目录失败也要保留正文');save();await tick();
 document.querySelector<HTMLButtonElement>('.save-folder-tree [aria-label="展开Projects"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.new-document-dialog [role="alert"]')?.textContent).toBe('Error: 无法读取项目目录'));
 expect(document.querySelector('.save-folder-tree [aria-pressed="true"]')?.getAttribute('title')).toBe('Notes');
 await cancelSave();
 expect(document.querySelector('.tiptap')?.textContent).toBe('读取目录失败也要保留正文');
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
});
it('取消保存位置选择后保留文件名和正文，可以继续保存',async()=>{
 await start();await newDocument();await writeBody('不能丢失的正文');save();await tick();await nameBeforeSave('取消后保留.md');await chooseProjects();await cancelSave();
 expect(document.querySelector('[role="dialog"]')).toBeNull();
 expect(document.querySelector<HTMLInputElement>('[aria-label="文件名"]')!.value).toBe('取消后保留.md');
 expect(document.querySelector('.tiptap')?.textContent).toBe('不能丢失的正文');
 expect(document.querySelector('.document-status')?.textContent).toBe('未保存');
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
 save();await tick();expect(document.querySelector('.save-folder-tree [aria-pressed="true"]')?.textContent).toBe('Notes');expect(document.querySelector('.new-document-dialog [aria-label="文件名"]')).toBeNull();await confirmSave();
});
it('在子文件夹内已命名的新文档直接保存，不再要求输入名称',async()=>{
 await start();document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/nested"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes/nested"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('nested'));
 await newDocument();await nameDocument('子目录文档.md');save();
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(true));
 expect(document.querySelector('.new-document-dialog')).toBeNull();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/nested/子目录文档.md',body:'',expectedHash:null});
});
it('在子文件夹内未命名的新文档，首次保存时输入名称并保留当前保存位置',async()=>{
 await start();document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-list [title="/notes/nested"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.simple-file-list [title="/notes/nested"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-browser h1')?.textContent).toBe('nested'));
 await newDocument();save();
 await vi.waitFor(()=>expect(document.querySelector('.new-document-dialog')).not.toBeNull());
 expect(document.querySelector('.save-document-location')?.textContent).toContain('nested');
 expect(document.querySelector('.save-folder-tree')).toBeNull();
 await nameBeforeSave('子目录保存名称');await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/nested/子目录保存名称.md',body:'',expectedHash:null});
});
it('没有文件夹也能先写文档，保存时可以添加文件夹后继续保存',async()=>{
 roots=[];await start();await newDocument();await writeBody('没有文件夹时的正文');
 expect(document.querySelector('[role="dialog"]')).toBeNull();save();await tick();
 expect(document.querySelector('.new-document-dialog')?.textContent).toContain('请先添加一个文件夹');
 expect(document.querySelector<HTMLButtonElement>('.new-document-dialog .primary')!.disabled).toBe(true);
 document.querySelector<HTMLButtonElement>('.new-document-dialog .outline')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="选择当前文件夹"]')).not.toBeNull());
 expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
 (document.querySelector('[aria-label="选择当前文件夹"]')! as HTMLElement).click();await tick();
 document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.save-folder-tree [aria-pressed="true"]')?.textContent).toBe('Home'));
 await confirmSave();expect(invoke).toHaveBeenCalledWith('save_document',{path:'/home/未命名.md',body:'没有文件夹时的正文',expectedHash:null});
});
it('离开未保存文档时先确认保存，再选位置，完成后才切换页面',async()=>{
 await start();await newDocument();await nameDocument('离开时保存.md');
 document.querySelector<HTMLButtonElement>('.workspace-switch [title="问答"]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[role="dialog"] .primary')!.click();await tick();
 expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
 expect(document.querySelector('.new-document-dialog')).not.toBeNull();
 await confirmSave();await vi.waitFor(()=>expect(document.querySelector('.ask-content')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/notes/离开时保存.md',body:'',expectedHash:null});
});
it('离开时取消位置选择，回到未保存确认且正文仍在',async()=>{
 await start();await newDocument();await writeBody('保留编辑');
 document.querySelector<HTMLButtonElement>('.workspace-switch [title="问答"]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[role="dialog"] .primary')!.click();await tick();await cancelSave();
 expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
 expect(document.querySelector('[role="dialog"]')?.textContent).toContain('还有未保存内容');
 [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(button=>button.textContent==='取消')!.click();await tick();
 expect(document.querySelector('.tiptap')?.textContent).toBe('保留编辑');
 expect(invoke.mock.calls.some(([command])=>command==='save_document')).toBe(false);
});
it('恢复未选择文件夹的草稿后，仍在第一次保存时选择位置',async()=>{
 await start();await newDocument();await nameDocument('恢复草稿.md');await writeBody('恢复正文');
 await vi.waitFor(()=>expect(savedDrafts).toHaveLength(1));
 expect(savedDrafts[0].path).toMatch(/^\.draft-/);
 await unmount(app!);app=undefined;document.body.innerHTML='';await start();
 [...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(button=>button.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.tiptap')?.textContent).toBe('恢复正文'));
 expect(document.querySelector('[aria-label="文件夹路径"]')).toBeNull();
 save();await tick();await chooseProjects();await confirmSave();
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/projects/恢复草稿.md',body:'恢复正文',expectedHash:null});
});
