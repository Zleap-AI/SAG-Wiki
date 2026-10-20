// @vitest-environment happy-dom
import {graphEvent,rendererState} from './helpers/graph-renderer';
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import LibraryWorkspace from './helpers/LibraryWorkspaceHarness.svelte';
import {createLibrarySession} from '../src/lib/explore';
import {graphLayoutTargets} from '../src/lib/graph-layout';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.useRealTimers();vi.clearAllMocks();vi.restoreAllMocks();rendererState.layouts.length=0;rendererState.instances.length=0;setLanguage('zh-CN')});
const a={id:1,name:'监听.md',path:'/notes/监听.md',hash:'a',file_type:'md'};
const b={id:2,name:'索引.md',path:'/notes/索引.md',hash:'b',file_type:'md'};
const c={id:3,name:'宇宙.md',path:'/notes/宇宙.md',hash:'c',file_type:'md'};
const edge={source:1,target:2,source_hash:'a',target_hash:'b',score:.8,terms:['增量索引'],snapshot:'snap'};
const data={scope:'/notes',version:1,snapshot:'snap',documents:[a,b,c],edges:[edge]};
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text)!;
async function enter(){document.querySelector<HTMLButtonElement>('.library-explore')!.click();await vi.waitFor(()=>expect(document.querySelector('.explore-graph-renderer')).not.toBeNull());await tick()}
function select(id:number){graphEvent('clickNode',{node:String(id)})}
function focus(id:number){graphEvent('doubleClickNode',{node:String(id),event:{preventSigmaDefault(){}}})}
function setup(session=createLibrarySession()){
 invoke.mockImplementation(async(command)=>{
  if(command==='explore_overview')return{...data,total:3,known_documents:[],indexing:false,partial:false};
  if(command==='explore_graph')return data;
  if(command==='read_document')return{body:'增量索引的正文。'};
  return null;
 });
 const open=vi.fn();app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session,entries:[],reading:false,revision:0,onbrowse:vi.fn(),onopen:open,onmenu:vi.fn()}});
 return{session,open};
}
it('Windows 关系图预览显示普通路径，打开和读取仍使用原始路径',async()=>{
 const {open}=setup(),path=String.raw`\\?\D:\资料\监听.md`;
 const windowsData={...data,documents:[{...a,path},b,c]};
 invoke.mockImplementation(async command=>{
  if(command==='explore_overview')return{...windowsData,total:3,known_documents:[],indexing:false,partial:false};
  if(command==='explore_graph')return windowsData;
  if(command==='read_document')return{body:'正文'};
 });
 await enter();select(1);
 await vi.waitFor(()=>expect(document.querySelector('.graph-preview-path')?.textContent).toBe(String.raw`D:\资料\监听.md`));
 expect(document.querySelector('.graph-preview-path')?.getAttribute('title')).toBe(String.raw`D:\资料\监听.md`);
 expect(invoke).toHaveBeenCalledWith('read_document',{path});
 button('打开文档').click();expect(open).toHaveBeenCalledWith(path,[]);
});
it('进入即显示完整关系总览，包含没有关联的文档',async()=>{
 const {session}=setup();await enter();expect(session.graph.nodes.map(n=>n.id)).toEqual([1,2,3]);expect(session.graph.edges).toEqual([edge]);
 expect(document.body.textContent).toContain('3 个文档 · 1 条关联');expect(document.body.textContent).not.toContain('选择一篇文档');expect(invoke.mock.calls.some(([name])=>name==='explore_neighbors')).toBe(false);
});
it('单击只打开预览，不重新布局或查询关联',async()=>{
 const {session,open}=setup();await enter();const positions=session.graph.nodes;select(1);
 await vi.waitFor(()=>expect(document.querySelector('.graph-excerpt')?.textContent).toContain('正文'));
 expect(session.graph.nodes).toBe(positions);expect(session.graph.center).toBeNull();expect(open).not.toHaveBeenCalled();
 button('打开文档').click();expect(open).toHaveBeenCalledWith(a.path,[]);
});
it('双击进入局部，返回恢复原位置与缩放而不重新调用后端',async()=>{
 const {session}=setup();await enter();Object.assign(rendererState.instances.at(-1)!.camera,{x:.7,y:.3,ratio:.6});focus(1);await tick();
 expect(session.graph.center).toBe(1);expect(session.graph.nodes.map(n=>n.id)).toEqual([1,2]);
 button('返回上一步').click();await tick();expect(session.graph.center).toBeNull();expect(session.graph.nodes).toHaveLength(3);expect(session.graph.viewport).toMatchObject({x:.7,y:.3,scale:.6});
 expect(invoke.mock.calls.filter(([name])=>name==='explore_graph')).toHaveLength(1);
});
it('搜索定位孤立文档，支持 Enter，不触发检索模型',async()=>{
 const {session}=setup();await enter();const input=document.querySelector<HTMLInputElement>('[aria-label="在关系图中查找文档"]')!;input.value='宇宙';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await tick();expect(session.graph.selected).toBe(3);expect(session.graph.nodes).toHaveLength(3);
 expect(invoke.mock.calls.some(([name])=>name==='run_query'||name==='explore_neighbors')).toBe(false);
});
it('关系依据使用总览快照及两端文件，且可关闭',async()=>{
 setup();await enter();select(1);await tick();button('依据').click();await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).not.toBeNull());
 expect(invoke.mock.calls.find(([name])=>name==='explore_evidence')?.[1]).toEqual({scope:'/notes',source:1,target:2,snapshot:'snap'});
 document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await vi.waitFor(()=>expect(document.querySelector('.explore-evidence')).toBeNull());
});
it('旧文档预览晚返回不会覆盖新选择',async()=>{
 const {session}=setup();await enter();let finish!:(value:unknown)=>void;
 invoke.mockImplementation(async(command,args)=>command==='read_document'?(args.path===a.path?new Promise(resolve=>finish=resolve):{body:'新选择正文'}):data);
 select(1);await vi.waitFor(()=>expect(finish).toBeTypeOf('function'));select(2);await vi.waitFor(()=>expect(document.querySelector('.graph-excerpt')?.textContent).toBe('新选择正文'));
 finish({body:'旧正文'});await tick();await tick();expect(session.graph.selected).toBe(2);expect(document.querySelector('.graph-excerpt')?.textContent).toBe('新选择正文');
});
it('返回文件页再进入保留图位置，文件删除后移除失效关系',async()=>{
 const {session}=setup();await enter();focus(1);await tick();
 document.querySelector<HTMLButtonElement>('[data-files-mode]')!.click();await tick();
 invoke.mockImplementation(async command=>command==='explore_graph'?{...data,version:2,documents:[b,c],edges:[]}:command==='explore_overview'?{...data,version:2,total:2,indexing:false,partial:false}:null);
 await enter();await vi.waitFor(()=>expect(session.version).toBe(2));expect(session.graph.center).toBeNull();expect(session.graph.nodes.map(n=>n.id)).toEqual([2,3]);expect(session.graph.edges).toHaveLength(0);expect(session.history).toHaveLength(0);
});
it('总览加载失败显示明确错误，允许重试',async()=>{
 setup();let attempts=0;invoke.mockImplementation(async(command)=>{if(command==='explore_graph'){attempts++;if(attempts===1)throw new Error('graph failed');return data}return{...data,total:3}});
 document.querySelector<HTMLButtonElement>('.library-explore')!.click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('graph failed'));button('重试').click();await vi.waitFor(()=>expect(document.querySelector('.explore-graph-renderer')).not.toBeNull());expect(attempts).toBe(2);
});
it('中英文图谱说明和预览操作一致',async()=>{
 setLanguage('en');setup();await enter();expect(document.body.textContent).toContain('3 documents · 1 connections');select(1);await tick();expect(document.body.textContent).toContain('Local graph');expect(document.body.textContent).toContain('Related documents');
});

it('普通点击和拖动保持固定画布边界，避免视野跳变',async()=>{
 setup();await enter();const renderer=rendererState.instances.at(-1)!,bounds=renderer.bounds;
 graphEvent('downNode',{node:'1',event:{x:100,y:100}});
 graphEvent('mouse:mouseup',{});expect(renderer.bounds).toEqual(bounds);
 graphEvent('downNode',{node:'1',event:{x:100,y:100}});
 graphEvent('mouse:mousemovebody',{x:120,y:125,preventSigmaDefault(){}});
 expect(renderer.bounds).not.toEqual({x:[0,1],y:[0,1]});expect(renderer.bounds).not.toBeNull();
 graphEvent('mouse:mouseup',{});
});

it('展开侧栏改变画布尺寸后会重新绘制，而不只清空画布',async()=>{
 let notify!:ResizeObserverCallback;
 const observer={observe:vi.fn(),unobserve:vi.fn(),disconnect:vi.fn()};
 const mock=vi.spyOn(globalThis,'ResizeObserver').mockImplementation(function(callback:ResizeObserverCallback){notify=callback;return observer});
 setup();await enter();const renderer=rendererState.instances.at(-1)!;renderer.refresh.mockClear();
 select(1);await tick();notify([],observer);
 expect(renderer.refresh).toHaveBeenCalledWith({schedule:true});
 mock.mockRestore();
});

it('默认无标题，仅悬停节点显示完整文件名，离开后消失',async()=>{
 setup();const name='一份非常长而且必须完整显示不能省略的文件名称-测试版本.markdown';
 invoke.mockImplementation(async command=>command==='explore_graph'?{...data,documents:[{...a,name},b,c]}:command==='read_document'?{body:'正文'}:{...data,total:3,indexing:false,partial:false});
 await enter();expect(rendererState.instances.at(-1)!.options.renderLabels).toBe(false);
 expect(document.querySelector('[role="tooltip"]')).toBeNull();
 graphEvent('enterNode',{node:'1'});await tick();
 expect(document.querySelector('[role="tooltip"]')?.textContent).toBe(name);
 select(1);await tick();graphEvent('leaveNode',{});await tick();
 expect(document.querySelector('[role="tooltip"]')).toBeNull();
});

it('节点逐帧靠近计算位置，连线共享端点，稳定时不改变视野',async()=>{
 vi.useFakeTimers();
 const matchMedia=window.matchMedia.bind(window);vi.spyOn(window,'matchMedia').mockImplementation(()=>matchMedia('not all'));
 setup();await enter();
 const renderer=rendererState.instances.at(-1)!,worker=rendererState.layouts.at(-1)!;
 const original=renderer.graph.getNodeAttribute('1','x'),camera={...renderer.camera};
 worker.graph.updateEachNodeAttributes((id:string,attributes:any)=>({...attributes,x:id==='1'?1000:attributes.x}));
 const target=graphLayoutTargets(worker.graph.mapNodes((id,attributes)=>({id,x:attributes.x,y:attributes.y}))).find(point=>point.id==='1')!;
 expect(renderer.graph.getNodeAttribute('1','x')).toBe(original);
 await vi.advanceTimersByTimeAsync(32);await tick();
 const moving=renderer.graph.getNodeAttribute('1','x');
 expect(Math.abs(moving-target.x)).toBeLessThan(Math.abs(original-target.x));
 expect(moving).not.toBe(target.x);
 expect(renderer.graph.extremities(renderer.graph.edges()[0])).toEqual(['1','2']);
 await vi.advanceTimersByTimeAsync(7000);await tick();
 expect(renderer.graph.getNodeAttribute('1','x')).toBeCloseTo(target.x,2);
 expect(renderer.camera).toEqual(camera);
});

it('排列未完成就离开，返回时重新排列而不保存中间位置为完成状态',async()=>{
 const {session}=setup();await enter();
 expect(session.graph.layoutReady).toBe(false);
 document.querySelector<HTMLButtonElement>('[data-files-mode]')!.click();await tick();
 expect(session.graph.layoutReady).toBe(false);
 const count=rendererState.layouts.length;await enter();
 expect(rendererState.layouts).toHaveLength(count+1);
 expect(session.graph.layoutReady).toBe(false);
});
