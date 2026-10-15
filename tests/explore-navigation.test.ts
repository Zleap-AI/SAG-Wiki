// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
it('从子目录原文返回原来的探索范围，并保留问答范围',async()=>{
 const doc={id:1,name:'example.txt',path:'/notes/nested/example.txt',hash:'a',file_type:'txt'};
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',language:'zh-CN',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return[{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  if(command==='list_files')return[doc];
  if(command==='list_entries')return[{path:'/notes/nested',name:'nested',directory:true}];
  if(command==='explore_overview')return{scope:'/notes',version:1,total:1,documents:[doc],known_documents:[doc],indexing:false,partial:false};
  if(command==='read_document')return{path:doc.path,name:doc.name,body:'正文',hash:'a'};
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('button[title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('button[title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[data-explore-tab]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.explore-node')).not.toBeNull());
 document.querySelector('.explore-node')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='打开文档')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.readonly-document')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[title="返回文件夹"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.explore-node')).not.toBeNull());
 expect(invoke.mock.calls.filter(([name])=>name==='explore_overview').at(-1)?.[1]).toMatchObject({scope:'/notes'});
 expect(document.querySelector('[data-explore-tab]')?.getAttribute('data-state')).toBe('active');
 const ask=[...document.querySelectorAll<HTMLButtonElement>('.sidebar-nav button')].find(b=>b.textContent?.trim()==='问答')!;ask.click();
 await vi.waitFor(()=>expect(document.querySelector('.scope-button')?.textContent).toContain('全部文件'));
 expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(false);
});
