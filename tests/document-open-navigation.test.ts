// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
let app:ReturnType<typeof mount>;
const files=['a.pdf','b.pdf','c.pdf'].map((name,id)=>({id,path:'/notes/'+name,name}));
let pending=new Map<string,{resolve:(value:any)=>void;reject:(reason:any)=>void}>();
beforeEach(()=>{localStorage.clear();localStorage.setItem('sag.onboarding.seen','1');pending=new Map();invoke.mockImplementation(async(command,args)=>{
 if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
 if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:3,error:null}];
 if(command==='list_files')return files;
 if(command==='get_index_status')return {paused:false,tasks:[]};
 if(command==='list_entries')return files.map(file=>({...file,directory:false,symlink:false}));
 if(command==='read_document')return new Promise((resolve,reject)=>pending.set(args.path,{resolve,reject}));
 return [];
});});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();localStorage.clear()});
const click=(name:string)=>document.querySelector<HTMLButtonElement>(`.sidebar .tree-name[title="/notes/${name}"]`)!.click();
async function openLibrary(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.tree-toggle[aria-label="展开Notes"]')).not.toBeNull());document.querySelector<HTMLButtonElement>('.tree-toggle[aria-label="展开Notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-name[title="/notes/a.pdf"]')).not.toBeNull());}
async function openFirst(){await openLibrary();click('a.pdf');await vi.waitFor(()=>expect(pending.has('/notes/a.pdf')).toBe(true));resolve('a.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document')).not.toBeNull());}
function resolve(name:string){pending.get('/notes/'+name)!.resolve({path:'/notes/'+name,name,body:'# '+name+' content',hash:name})}
it('keeps the current reader while another file loads, without painting the folder list',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));await tick();expect(document.querySelector('.folder-browser')).toBeNull();expect(document.querySelector('.readonly-document')?.textContent).toContain('a.pdf');resolve('b.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('b.pdf'));});
it('a slower previous file cannot replace the latest selection',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));click('c.pdf');await vi.waitFor(()=>expect(pending.has('/notes/c.pdf')).toBe(true));resolve('c.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('c.pdf'));resolve('b.pdf');await tick();await tick();expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('c.pdf');});
it('a failed read keeps the original reader and reports the failure',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));pending.get('/notes/b.pdf')!.reject('read failed');await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('read failed'));expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('a.pdf');});
it('returning to the same folder cancels a pending file navigation',async()=>{await openFirst();document.querySelector<HTMLButtonElement>('.sidebar .tree-name[title="/notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));document.querySelector<HTMLButtonElement>('.sidebar .tree-name[title="/notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());resolve('b.pdf');await tick();await tick();expect(document.querySelector('.folder-browser')).not.toBeNull();expect(document.querySelector('.readonly-document')).toBeNull();});
it('shows cancellable loading, opens large converted Markdown in reading mode and edits the original full body on request',async()=>{
 await openFirst();click('b.pdf');
 await vi.waitFor(()=>expect(document.querySelector('.document-loading [role=status]')?.textContent).toContain('正在读取'));
 const body='# Start\n\n'+'long paragraph. '.repeat(6500)+'\n\nEND-OF-DOCUMENT';
 pending.get('/notes/b.pdf')!.resolve({id:2,path:'/converted/b.pdf.md',source_path:'/notes/b.pdf',name:'b.pdf.md',body,hash:'book-1'});
 // Vite transforms these lazy chunks on demand in the test process.
 await import('../src/components/LargeDocumentReader.svelte');
 await vi.waitFor(()=>expect(document.querySelector('.large-document-reader .reader-piece')).not.toBeNull());
 expect(document.querySelector('[contenteditable=true]')).toBeNull();
 const original=[...document.querySelectorAll('button')].find(node=>node.textContent==='打开原文件');
 original!.click();expect(invoke).toHaveBeenCalledWith('open_source_file',{path:'/notes/b.pdf'});
 document.querySelector<HTMLButtonElement>('.edit-large-document')!.click();
 await import('../src/components/RichEditor.svelte');
 await vi.waitFor(()=>expect(document.querySelector('[contenteditable=true]')?.textContent).toContain('END-OF-DOCUMENT'));
 expect(invoke.mock.calls.filter(([name,args])=>name==='read_document'&&args.path==='/notes/b.pdf')).toHaveLength(1);
});
it('cancels a pending document read without losing the current document',async()=>{
 await openFirst();click('b.pdf');
 await vi.waitFor(()=>expect(document.querySelector('.document-loading button')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.document-loading button')!.click();
 resolve('b.pdf');await tick();await tick();
 expect(document.querySelector('.document-loading')).toBeNull();
 expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('a.pdf');
});
it.each(['edit and save','ask'])('uses the relocated conversion for %s while reading the same book',async action=>{
 const original='/notes/b.pdf',oldFolder='/notes/.sag-converted',newFolder='/app-data/converted';
 const newPath=newFolder+'/b.pdf.md',hash='revision-'+action;
 const body=('A paragraph about reading. '.repeat(180)+'\n\n').repeat(25)+'# Chart\n\n![Chart](images/chart.png)\n\nBook end.';
 let mode='library',finishOldImage!:(value:string)=>void;
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  const path=(mode==='library'?oldFolder:newFolder)+'/b.pdf.md';
  const file={id:1,path,source_path:original,name:'b.pdf.md'};
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,excluded:[],watch:true,show_hidden:false,conversion_storage:{mode},retrieval:{chunk_chars:512},answer:{url:'https://example.com',model:'test',models:['test']},jev:{url:'https://example.com',model:'jev',models:[]}};
  if(command==='save_settings'){mode=args.settings.conversion_storage.mode;return;}
  if(command==='list_files')return [files[0],file,files[2]];
  if(command==='read_document'&&args.path===original)return {...file,body,hash};
  if(command==='load_image')return args.path===oldFolder+'/images/chart.png'?new Promise<string>(resolve=>finishOldImage=resolve):'data:image/png;base64,YmJi';
  if(command==='save_document')return {...file,body:args.body,hash:'saved'};
  if(command==='run_query')return new Promise(()=>{});
  return base(command,args);
 });
 await openLibrary();click('b.pdf');await import('../src/components/LargeDocumentReader.svelte');
 await vi.waitFor(()=>expect(document.querySelector('.reader-piece[data-state=rendered]')).not.toBeNull());
 const scroll=document.querySelector<HTMLElement>('.markdown-reader')!;
 scroll.scrollTop=78000;scroll.dispatchEvent(new Event('scroll'));
 await vi.waitFor(()=>expect(finishOldImage).toBeTypeOf('function'));
 const oldImage=document.querySelector<HTMLImageElement>('.reader-piece img')!;
 const before=scroll.scrollTop;
 expect(before).toBeGreaterThan(0);
 document.querySelector<HTMLButtonElement>('.sidebar-settings')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(b=>b.textContent==='文件设置')!.click();await tick();
 document.querySelector('.conversion-storage-select')!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option]')).not.toBeNull());
 [...document.querySelectorAll('[role=option]')].find(el=>el.textContent==='应用数据目录')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
 document.querySelector('form.settings-form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
 expect(mode).toBe('app_data');
 if(action==='edit and save'){
  await vi.waitFor(()=>expect(document.querySelector('.reader-piece img')?.getAttribute('src')).toBe('data:image/png;base64,YmJi'));
  expect(invoke).toHaveBeenCalledWith('load_image',{path:newFolder+'/images/chart.png'});
  expect(document.querySelector<HTMLElement>('.markdown-reader')!.scrollTop).toBe(before);
  finishOldImage('data:image/png;base64,YWFh');await new Promise(resolve=>setTimeout(resolve,0));
  expect(oldImage.getAttribute('src')).not.toBe('data:image/png;base64,YWFh');
  expect(document.querySelector('.reader-piece img')?.getAttribute('src')).toBe('data:image/png;base64,YmJi');
  document.querySelector<HTMLButtonElement>('.edit-large-document')!.click();await import('../src/components/RichEditor.svelte');
  await vi.waitFor(()=>expect(document.querySelector('[contenteditable=true]')?.textContent).toContain('Book end.'));
  document.querySelector<HTMLButtonElement>('.editor-save-actions .primary')!.click();
  await vi.waitFor(()=>expect(invoke.mock.calls.find(([cmd])=>cmd==='save_document')?.[1].path).toBe(newPath));
  expect(invoke.mock.calls.find(([cmd])=>cmd==='save_document')![1]).toEqual({path:newPath,body,expectedHash:hash});
 }else{
  document.querySelector<HTMLButtonElement>('.document-question-toggle')!.click();await tick();
  const input=document.querySelector<HTMLInputElement>('.document-question input')!;
  input.value='Explain the chart';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  document.querySelector<HTMLButtonElement>('.document-question [type=submit]')!.click();
  await vi.waitFor(()=>expect(invoke.mock.calls.find(([cmd])=>cmd==='run_query')?.[1].query).toMatchObject({scope:[newPath],references:[newPath]}));
  finishOldImage('data:image/png;base64,YWFh');
 }
 expect(invoke.mock.calls.filter(([cmd])=>cmd==='read_document')).toHaveLength(1);
});
