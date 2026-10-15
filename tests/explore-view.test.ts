// @vitest-environment happy-dom
import {afterEach, expect, it, vi} from 'vitest';
import {mount, unmount, tick} from 'svelte';
import LibraryWorkspace from '../src/components/LibraryWorkspace.svelte';
import {createLibrarySession} from '../src/lib/explore';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN')});
const a={id:1,name:'监听.md',path:'/notes/监听.md',hash:'a',file_type:'md'};
const b={id:2,name:'索引.md',path:'/notes/索引.md',hash:'b',file_type:'md'};
const c={id:3,name:'检索.md',path:'/notes/检索.md',hash:'c',file_type:'md'};
const edge={source:1,target:2,source_hash:'a',target_hash:'b',score:0.8,terms:['增量索引'],snapshot:'snap'};
const resultA={version:1,snapshot:'snap',center:a,documents:[b],edges:[edge],total:1,has_more:false,next_offset:1,candidate_limit_reached:false};
const resultB={...resultA,center:b,documents:[c],edges:[{...edge,source:2,target:3,source_hash:'b',target_hash:'c'}]};
const neighbors=()=>invoke.mock.calls.filter(([name])=>name==='explore_neighbors');
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text)!;
const choose=(id:number)=>document.querySelector(`[data-document-id="${id}"]`)!.dispatchEvent(new MouseEvent('click',{bubbles:true}));
async function enter(){document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();await vi.waitFor(()=>expect(document.querySelectorAll('.explore-entry')).toHaveLength(2));}
function setup(session=createLibrarySession()){
 invoke.mockImplementation(async(command,args)=>{
  if(command==='explore_overview')return{scope:'/notes',version:1,total:3,documents:args.query?[b]:[a,b],known_documents:[a,b,c],indexing:false,partial:false};
  if(command==='explore_neighbors')return args.centerId===2?resultB:resultA;
  return null;
 });
 const open=vi.fn();
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session,entries:[{name:a.name,path:a.path,directory:false,symlink:false}],isRoot:true,reading:false,revision:0,onbrowse:vi.fn(),onopen:open,oncreate:vi.fn(),onmenu:vi.fn()}});
 return{session,open};
}
it('明确展示起点列表，点击文档自动关联，切换视图保留结果',async()=>{
 const {session,open}=setup();expect(invoke).not.toHaveBeenCalled();await enter();
 expect(document.body.textContent).toContain('选择一篇文档开始探索');choose(1);
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-edge')).toHaveLength(1));
 expect(neighbors()).toHaveLength(1);expect(open).not.toHaveBeenCalled();expect(document.body.textContent).not.toContain('设为中心');
 const before=JSON.stringify(session.graph);
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();await tick();
 expect(JSON.stringify(session.graph)).toBe(before);expect(neighbors()).toHaveLength(1);
});
it('更换文档只显示当前一层关联，返回不重新查询',async()=>{
 const {session}=setup();await enter();choose(1);
 await vi.waitFor(()=>expect(session.graph.center).toBe(1));await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 choose(2);await vi.waitFor(()=>expect(session.graph.nodes.map(n=>n.id)).toEqual([2,3]));
 expect(session.graph.edges.map(e=>[e.source,e.target])).toEqual([[2,3]]);
 document.querySelector<HTMLButtonElement>('[aria-label="返回上一步"]')!.click();await tick();
 await vi.waitFor(()=>expect(session.graph.center).toBe(1));expect(session.graph.nodes.map(n=>n.id)).toEqual([1,2]);expect(neighbors()).toHaveLength(2);
});
it('快速切换时晚到的旧结果不能覆盖新文档',async()=>{
 const {session}=setup();await enter();let finish!:(value:unknown)=>void;
 invoke.mockImplementation(async(command,args)=>command==='explore_neighbors'?(args.centerId===1?new Promise(resolve=>{finish=resolve}):resultB):command==='explore_overview'?{scope:'/notes',version:1,total:3,documents:[b],known_documents:[],indexing:false,partial:false}:null);
 choose(1);await vi.waitFor(()=>expect(finish).toBeTypeOf('function'));
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索文档，开始探索"]')!;input.value='索引';input.dispatchEvent(new Event('input',{bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-picker button')).not.toBeNull());
 (document.querySelector('.explore-picker button') as HTMLButtonElement).click();
 await vi.waitFor(()=>expect(session.graph.nodes.map(n=>n.id)).toEqual([2,3]));
 finish(resultA);await tick();await tick();expect(session.graph.center).toBe(2);
 expect(invoke.mock.calls.some(([name])=>name==='cancel_query')).toBe(true);
});
it('从文件名搜索选择文档后直接查询关联',async()=>{
 const {session}=setup();await enter();
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索文档，开始探索"]')!;
 input.value='索引';input.dispatchEvent(new Event('input',{bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-picker button')).not.toBeNull());
 (document.querySelector('.explore-picker button') as HTMLButtonElement).click();
 await vi.waitFor(()=>expect(session.graph.center).toBe(2));await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 expect(neighbors().at(-1)?.[1].centerId).toBe(2);expect(document.querySelector('.explore-selection strong')?.textContent).toBe(b.name);
});
it('每个关联有可见主题和依据按钮，依据可通过 Escape 关闭',async()=>{
 setup();await enter();choose(1);
 await vi.waitFor(()=>expect(document.querySelector('.explore-node-evidence')).not.toBeNull());
 expect(document.querySelector('[data-document-id="2"]')?.closest('.explore-node-wrap')?.textContent).toContain('增量索引');
 document.querySelector<HTMLButtonElement>('.explore-node-evidence')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).not.toBeNull());
 expect(invoke.mock.calls.find(([name])=>name==='explore_evidence')?.[1]).toMatchObject({scope:'/notes',source:1,target:2,snapshot:'snap'});
 expect(neighbors()).toHaveLength(1);
 document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).toBeNull());
});
it('继续加载复用排序快照，点击当前文档不重复查询',async()=>{
 const {session}=setup();await enter();
 invoke.mockImplementation(async(command,args)=>{
  if(command==='explore_neighbors')return args.offset===0?{...resultA,total:2,has_more:true}:{...resultA,documents:[c],edges:[{...edge,target:3,target_hash:'c'}],total:2,next_offset:2};
  return null;
 });
 choose(1);await vi.waitFor(()=>expect(session.graph.neighbors?.has_more).toBe(true));
 choose(1);await tick();expect(neighbors()).toHaveLength(1);
 button('继续展开 12 个').click();await vi.waitFor(()=>expect(session.graph.nodes.map(n=>n.id)).toEqual([1,2,3]));
 expect(neighbors()[1][1]).toMatchObject({snapshot:'snap',offset:1,centerId:1});
});
it('关联失败可重试同一文档，返回可恢复起点列表',async()=>{
 const {session}=setup();await enter();let attempts=0;
 invoke.mockImplementation(async(command)=>{
  if(command==='explore_neighbors'){attempts++;if(attempts===1)throw new Error('temporary failure');return resultA}return null;
 });
 choose(1);await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());
 button('重试').click();await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));expect(attempts).toBe(2);
 document.querySelector<HTMLButtonElement>('[aria-label="返回上一步"]')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelectorAll('.explore-entry')).toHaveLength(2));expect(session.graph.center).toBeNull();
});
it('文档移除时清除失效关联并回到可选文档列表',async()=>{
 const {session}=setup();await enter();choose(1);await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 invoke.mockImplementation(async command=>command==='explore_overview'?{scope:'/notes',version:2,total:1,documents:[b],known_documents:[b],indexing:false,partial:false}:null);
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(session.graph.center).toBeNull());expect(session.graph.edges).toHaveLength(0);
 expect(document.querySelector('[data-document-id="1"]')).toBeNull();expect(document.querySelectorAll('.explore-entry')).toHaveLength(1);
});
it('离开探索后旧请求不能覆盖会话，重新进入自动恢复未完成的查询',async()=>{
 const {session}=setup();await enter();let finish!:(value:unknown)=>void;
 invoke.mockImplementation(async(command)=>command==='explore_neighbors'?new Promise(resolve=>{finish=resolve}):command==='explore_overview'?{scope:'/notes',version:1,total:3,documents:[a,b],known_documents:[a,b,c],indexing:false,partial:false}:null);
 choose(1);await vi.waitFor(()=>expect(finish).toBeTypeOf('function'));
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();finish(resultA);await tick();await tick();expect(session.graph.edges).toHaveLength(0);
 invoke.mockImplementation(async(command)=>command==='explore_neighbors'?resultA:command==='explore_overview'?{scope:'/notes',version:1,total:3,documents:[a,b],known_documents:[a,b,c],indexing:false,partial:false}:null);
 document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
});
it('探索指引、自动关联和操作支持英文',async()=>{
 setLanguage('en');setup();await enter();expect(document.body.textContent).toContain('Choose a document to explore');choose(1);
 await vi.waitFor(()=>expect(document.querySelector('.explore-node-evidence')).not.toBeNull());
 expect(document.body.textContent).toContain('Open document');expect(document.body.textContent).toContain('Evidence');expect(document.body.textContent).not.toContain('Set as center');
});
it('搜索其他文档后返回不会把新文档混入原关联图',async()=>{
 const {session}=setup();await enter();choose(1);await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 invoke.mockImplementation(async(command)=>command==='explore_overview'?{scope:'/notes',version:1,total:3,documents:[c],known_documents:[],indexing:false,partial:false}:command==='explore_neighbors'?{...resultB,center:c,documents:[b]}:null);
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索文档，开始探索"]')!;input.value='检索';input.dispatchEvent(new Event('input',{bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.explore-picker button')).not.toBeNull());
 (document.querySelector('.explore-picker button') as HTMLButtonElement).click();await vi.waitFor(()=>expect(session.graph.center).toBe(3));
 document.querySelector<HTMLButtonElement>('[aria-label="返回上一步"]')!.click();
 await vi.waitFor(()=>expect(session.graph.center).toBe(1));expect(session.graph.nodes.map(n=>n.id)).toEqual([1,2]);
});
it('文件内容更新后自动刷新当前关联，不保留旧证据',async()=>{
 const {session}=setup();await enter();choose(1);await vi.waitFor(()=>expect(session.graph.edges).toHaveLength(1));
 const updated={...a,hash:'updated'};
 invoke.mockImplementation(async command=>command==='explore_overview'?{scope:'/notes',version:2,total:3,documents:[updated,b],known_documents:[updated,b],indexing:false,partial:false}:command==='explore_neighbors'?{...resultA,version:2,center:updated,edges:[{...edge,source_hash:'updated',snapshot:'new'}]}:null);
 document.querySelector<HTMLButtonElement>('[data-files-tab]')!.click();await tick();document.querySelector<HTMLButtonElement>('[data-explore-tab]')!.click();
 await vi.waitFor(()=>expect(session.graph.edges[0]?.source_hash).toBe('updated'));
 expect(session.graph.center).toBe(1);expect(session.history).toHaveLength(0);expect(neighbors()).toHaveLength(2);
});
