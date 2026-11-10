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
async function openFirst(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.tree-toggle[aria-label="展开Notes"]')).not.toBeNull());document.querySelector<HTMLButtonElement>('.tree-toggle[aria-label="展开Notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.sidebar .tree-name[title="/notes/a.pdf"]')).not.toBeNull());click('a.pdf');await vi.waitFor(()=>expect(pending.has('/notes/a.pdf')).toBe(true));resolve('a.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document')).not.toBeNull());}
function resolve(name:string){pending.get('/notes/'+name)!.resolve({path:'/notes/'+name,name,body:'# '+name+' content',hash:name})}
it('keeps the current reader while another file loads, without painting the folder list',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));await tick();expect(document.querySelector('.folder-browser')).toBeNull();expect(document.querySelector('.readonly-document')?.textContent).toContain('a.pdf');resolve('b.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('b.pdf'));});
it('a slower previous file cannot replace the latest selection',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));click('c.pdf');await vi.waitFor(()=>expect(pending.has('/notes/c.pdf')).toBe(true));resolve('c.pdf');await vi.waitFor(()=>expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('c.pdf'));resolve('b.pdf');await tick();await tick();expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('c.pdf');});
it('a failed read keeps the original reader and reports the failure',async()=>{await openFirst();click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));pending.get('/notes/b.pdf')!.reject('read failed');await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('read failed'));expect(document.querySelector('.readonly-document .editor-head')?.textContent).toContain('a.pdf');});
it('returning to the same folder cancels a pending file navigation',async()=>{await openFirst();document.querySelector<HTMLButtonElement>('.sidebar .tree-name[title="/notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());click('b.pdf');await vi.waitFor(()=>expect(pending.has('/notes/b.pdf')).toBe(true));document.querySelector<HTMLButtonElement>('.sidebar .tree-name[title="/notes"]')!.click();await vi.waitFor(()=>expect(document.querySelector('.folder-browser')).not.toBeNull());resolve('b.pdf');await tick();await tick();expect(document.querySelector('.folder-browser')).not.toBeNull();expect(document.querySelector('.readonly-document')).toBeNull();});
