// @vitest-environment happy-dom
import {afterEach, expect, it, vi} from 'vitest';
import {mount, unmount, tick} from 'svelte';
import LibraryWorkspace from '../src/components/LibraryWorkspace.svelte';
import {createLibrarySession} from '../src/lib/explore';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const a={id:1,name:'监听.md',path:'/notes/监听.md',hash:'a',file_type:'md'};
const b={id:2,name:'索引.md',path:'/notes/索引.md',hash:'b',file_type:'md'};
const edge={source:1,target:2,source_hash:'a',target_hash:'b',score:0.8,terms:['增量索引'],snapshot:'snap'};
function setup(){
 invoke.mockImplementation(async(command,args)=>{
  if(command==='explore_overview')return{scope:'/notes',version:1,total:2,documents:[a,b],known_documents:args.visible.includes(1)?[a,b]:[],indexing:false,partial:false};
  if(command==='explore_neighbors')return{version:1,snapshot:'snap',center:a,documents:[b],edges:[edge],total:1,has_more:false,next_offset:1,candidate_limit_reached:false};
  return null;
 });
 const session=createLibrarySession();const open=vi.fn();
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session,entries:[{name:a.name,path:a.path,directory:false,symlink:false}],isRoot:true,reading:false,revision:0,onbrowse:vi.fn(),onopen:open,oncreate:vi.fn(),onmenu:vi.fn()}});
 return{session,open};
}
it('默认显示文件；按需打开探索，选择和展开分开，切换保留布局',async()=>{
 const {session,open}=setup();
 expect(invoke).not.toHaveBeenCalled();
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 expect(invoke.mock.calls.filter(([name])=>name==='explore_neighbors')).toHaveLength(0);expect(open).not.toHaveBeenCalled();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-edge')).toHaveLength(1));
 const before=JSON.stringify(session.graph.nodes);
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();await tick();
 expect(JSON.stringify(session.graph.nodes)).toBe(before);
 expect(invoke.mock.calls.filter(([name])=>name==='explore_neighbors')).toHaveLength(1);
});
it('新索引版本移除失效连线，不再展示旧依据',async()=>{
 const {session}=setup();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 invoke.mockImplementation(async command=>command==='explore_overview'?{scope:'/notes',version:2,total:1,documents:[a],known_documents:[a],indexing:false,partial:false}:null);
 // Re-entering a view checks the current index without rebuilding the graph.
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(0));
 expect(session.graph.nodes.map(n=>n.id)).toEqual([1]);expect(document.body.textContent).toContain('文件已更新');
});
it('旧请求完成后不能覆盖重新进入的探索界面',async()=>{
 const {session}=setup();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 let finish!:(value:unknown)=>void;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='explore_neighbors')return new Promise(resolve=>{finish=resolve});
  if(command==='explore_overview')return{scope:'/notes',version:1,total:2,documents:[a,b],known_documents:[a,b],indexing:false,partial:false};
  return null;
 });
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(finish).toBeTypeOf('function'));
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();
 finish({version:1,snapshot:'old',center:a,documents:[b],edges:[edge],total:1,has_more:false,next_offset:1,candidate_limit_reached:false});
 await tick();await tick();expect(session.graph.edges).toHaveLength(0);
 expect(invoke.mock.calls.some(([name])=>name==='cancel_query')).toBe(true);
});
it('关联依据使用现有的弹出层，可通过 Escape 关闭',async()=>{
 setup();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.edge-hit')).not.toBeNull());
 document.querySelector('.edge-hit')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).not.toBeNull());
 document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).toBeNull());
});
it('重置清空中心、连线和展开状态，收起在请求中禁用',async()=>{
 const {session}=setup();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 session.graph.nodes.push({id:99,path:'/notes/extra.md',name:'extra.md',hash:'z',file_type:'md',x:800,y:800});
 session.graph.edges.push({...edge,source:1,target:99});
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='重置视图')!.click();
 await vi.waitFor(()=>expect(session.graph.center).toBeNull());
 expect(session.graph.edges).toHaveLength(0);expect(session.graph.neighbors).toBeNull();expect(session.graph.nodes).toHaveLength(2);
 let resolve!:(value:unknown)=>void;invoke.mockImplementation(async(command)=>command==='explore_neighbors'?new Promise(r=>resolve=r):null);
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(resolve).toBeTypeOf('function'));
 expect(document.querySelector<HTMLButtonElement>('[aria-label="收起此节点"]')!.disabled).toBe(true);
});
it('关联失败后重试同一个展开请求',async()=>{
 setup();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-node')).toHaveLength(2));
 let attempts=0;
 invoke.mockImplementation(async(command)=>{
  if(command==='explore_neighbors'){attempts++;if(attempts===1)throw new Error('temporary failure');return{version:1,snapshot:'snap',center:a,documents:[b],edges:[edge],total:1,has_more:false,next_offset:1,candidate_limit_reached:false};}
  return null;
 });
 document.querySelector('[data-document-id="1"]')!.dispatchEvent(new MouseEvent('click',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='设为中心')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='重试')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-edge')).toHaveLength(1));expect(attempts).toBe(2);
});
