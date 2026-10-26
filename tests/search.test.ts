// @vitest-environment happy-dom
import {afterEach,describe,it,expect,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import App from '../src/App.svelte';
const {invoke,events}=vi.hoisted(()=>({invoke:vi.fn(),events:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{events.set(name,handler);return()=>events.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const hit={id:1,document_id:1,path:'/files/open-octx.md',title:'open-octx.md',heading:'',content:'OCTX 是一种格式。第二处 octx。',start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:-1,score:0,order:0,hash:'same'};
async function setup(pending=false,mode='deep_search',paginated=false,deepResults?:typeof hit[]){
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {theme:'light',start_page:'search',editor_size:15,answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:true},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0};
  if(command==='run_query'){
   if(pending)return new Promise(()=>{});
   const page=args.query.page??1;
   return {results:args.query.kind==='deep_search'&&deepResults?deepResults:[paginated?{...hit,id:page,path:`/files/page-${page}.md`,title:`page-${page}.md`}:hit],terms:['octx'],trace:[],conversation:null,pagination:paginated&&args.query.kind==='search'?{total:125,page,page_size:20}:null};
  }
  if(command==='read_document')return {path:hit.path,name:hit.title,body:'# 测试\n\nOCTX 是一种格式。\n\n第二处 octx。',hash:hit.hash};
  return [];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 if(mode==='search')await selectMode('search');
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='octx';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
}
async function selectMode(value:string){
 const trigger=document.querySelector<HTMLButtonElement>('[aria-label="搜索模式"]')!;
 trigger.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${value}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${value}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
}
const click=async(label:string)=>{await vi.waitFor(()=>expect(document.querySelector(`[aria-label="${label}"]`)).not.toBeNull());const button=document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`);expect(button).not.toBeNull();button!.click();await tick()};
describe('深入搜索分页',()=>{
 it('按已有文件顺序每页显示 20 个，翻页只更新片段，不重复搜索或重排',async()=>{
  const ranked=Array.from({length:45},(_,index)=>{
   const id=(index*17)%45+1;
   return {...hit,id,path:`/files/ranked-${id}.md`,title:`ranked-${id}.md`,score:(id%7)/7};
  });
  const results=[...ranked.slice(0,21),{...ranked[0],id:101,content:'同一文件的其他片段'},...ranked.slice(21)];
  const original=JSON.stringify(results);
  await setup(false,'deep_search',false,results);
  const visible=()=>[...document.querySelectorAll('.search-result-title strong')].map(node=>node.textContent);
  await vi.waitFor(()=>expect(visible()).toEqual(ranked.slice(0,20).map(c=>c.title)));
  expect(document.querySelector('.search-summary')!.textContent).toContain('45 个文件');
  expect(document.querySelector('.search-result-more')!.textContent).toContain('2 处相关片段');
  const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
  input.value='未提交的新问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  document.querySelector('.search-results')!.scrollTop=300;
  await click('下一页');await vi.waitFor(()=>expect(visible()).toEqual(ranked.slice(20,40).map(c=>c.title)));
  expect(document.querySelector('.search-page-range')!.textContent).toContain('第 21–40 个');
  await vi.waitFor(()=>expect(document.querySelector('.search-results')!.scrollTop).toBe(0));
  await click('前往第 3 页');await vi.waitFor(()=>expect(visible()).toEqual(ranked.slice(40).map(c=>c.title)));
  expect(document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.disabled).toBe(true);
  await click('前往第 1 页');await vi.waitFor(()=>expect(visible()).toEqual(ranked.slice(0,20).map(c=>c.title)));
  expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(1);
  expect(JSON.stringify(results)).toBe(original);
 });
 it('重新提交查询才重新搜索，并回到第一页',async()=>{
  const results=Array.from({length:25},(_,index)=>({...hit,id:index+1,path:`/files/${index+1}.md`,title:`${index+1}.md`}));
  await setup(false,'deep_search',false,results);await vi.waitFor(()=>expect(document.querySelector('.search-pagination')).not.toBeNull());
  await click('下一页');await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 21–25 个'));
  const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
  input.value='新的查询';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
  await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 1–20 个'));
  const queries=invoke.mock.calls.filter(([command])=>command==='run_query');expect(queries).toHaveLength(2);
  expect(queries[1][1].query).toMatchObject({kind:'deep_search',question:'新的查询',page:null});
 });
});
describe('快速搜索分页',()=>{
 it('显示全部数量，按原搜索词翻页，并在新查询或切换模式后重置',async()=>{
  await setup(false,'search',true);
  await vi.waitFor(()=>expect(document.querySelector('.search-summary')!.textContent).toContain('125 个文件'));
  expect(document.querySelector<HTMLButtonElement>('[aria-label="上一页"]')!.disabled).toBe(true);
  const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
  input.value='另一条查询';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  await click('下一页');await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 21–40 个'));
  await click('前往第 7 页');await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 121–125 个'));
  expect(document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.disabled).toBe(true);
  await click('上一页');await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 101–120 个'));
  await click('前往第 1 页');await vi.waitFor(()=>expect(document.querySelector('.search-page-range')!.textContent).toContain('第 1–20 个'));
  expect(invoke.mock.calls.filter(([command])=>command==='run_query').map(([,args])=>[args.query.page,args.query.question])).toEqual([[1,'octx'],[2,'octx'],[7,'octx'],[6,'octx'],[1,'octx']]);
  input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
  await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='run_query').at(-1)![1].query).toMatchObject({page:1,question:'另一条查询'}));
  await selectMode('deep_search');await vi.waitFor(()=>expect(document.querySelector('.search-pagination')).toBeNull());
 });
 it('翻页期间禁用控件，失败保留原页，重试成功后回到列表顶部',async()=>{
  await setup(false,'search',true);await vi.waitFor(()=>expect(document.querySelector('.search-pagination')).not.toBeNull());
  const base=invoke.getMockImplementation()!;let reject:(reason:Error)=>void=()=>{};
  invoke.mockImplementation((command,args)=>command==='run_query'&&args.query.page===2?new Promise((_,no)=>reject=no):base(command,args));
  await click('下一页');expect(document.querySelector('.search-page-range')!.textContent).toContain('正在翻页');
  expect(document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.disabled).toBe(true);
  expect(document.querySelector('.search-result')!.textContent).toContain('page-1.md');
  reject(new Error('分页读取失败'));await vi.waitFor(()=>expect(document.querySelector('[role=alert]')!.textContent).toContain('分页读取失败'));
  expect(document.querySelector('[aria-label="前往第 1 页"]')!.getAttribute('aria-current')).toBe('page');
  invoke.mockImplementation(base);document.querySelector('.search-results')!.scrollTop=300;
  await click('下一页');await vi.waitFor(()=>expect(document.querySelector('.search-result')!.textContent).toContain('page-2.md'));
  await vi.waitFor(()=>expect(document.querySelector('.search-results')!.scrollTop).toBe(0));
 });
 it('停止翻页后保留原页，迟到的结果不能替换列表',async()=>{
  await setup(false,'search',true);await vi.waitFor(()=>expect(document.querySelector('.search-pagination')).not.toBeNull());
  const base=invoke.getMockImplementation()!;let resolve:(value:unknown)=>void=()=>{};
  invoke.mockImplementation((command,args)=>command==='run_query'&&args.query.page===2?new Promise(yes=>resolve=yes):base(command,args));
  await click('下一页');await click('停止搜索');
  resolve({results:[{...hit,id:2,title:'late.md'}],terms:['octx'],trace:[],conversation:null,pagination:{total:125,page:2,page_size:20}});
  await tick();await tick();
  expect(document.querySelector('.search-result')!.textContent).toContain('page-1.md');
  expect(document.querySelector('[aria-label="前往第 1 页"]')!.getAttribute('aria-current')).toBe('page');
  expect(document.querySelector('.search-page-range')!.textContent).toContain('第 1–20 个');
 });
});
describe('搜索的本地结果和深度分析',()=>{
 it('停止搜索不能显示为没有匹配文件',async()=>{await setup(true);await click('停止搜索');await vi.waitFor(()=>expect(document.body.textContent).toContain('已停止'));expect(document.body.textContent).not.toContain('没有找到相关文件')});
 it('从快速切到深入后清空旧结果，显示阶段和动画，停止后显示停止状态',async()=>{
  await setup(false,'search');await vi.waitFor(()=>expect(document.body.textContent).toContain('open-octx.md'));
  const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((command,args)=>command==='run_query'&&args.query.kind==='deep_search'?new Promise(()=>{}):base(command,args));
  await selectMode('deep_search');
  const calls=invoke.mock.calls.filter(([command])=>command==='run_query');
  expect(calls.map(([,args])=>args.query.kind)).toEqual(['search','deep_search']);
  expect(document.querySelector('.search-result')).toBeNull();
  expect(document.querySelector('.search-preview')).toBeNull();
  expect(document.querySelector('.search-loading[role="status"]')).not.toBeNull();
  expect(document.querySelector('.search-loading .search-skeleton')).not.toBeNull();
  events.get('query-progress')!({payload:{id:calls[1][1].query.id,stage:'正在加载搜索索引',count:0,delta:null}});await tick();
  expect(document.querySelector('.search-loading')!.textContent).toContain('正在加载搜索索引');
  events.get('query-progress')!({payload:{id:calls[1][1].query.id,stage:'正在扩展检索',count:1,delta:null}});await tick();
  expect(document.querySelector('.search-loading')!.textContent).toContain('正在扩展搜索范围');
  events.get('query-progress')!({payload:{id:calls[1][1].query.id,stage:'Jev 正在判断相关原文',count:100,delta:null}});await tick();
  expect(document.querySelector('.search-loading')!.textContent).toContain('正在按相关性排序');
  expect(document.querySelector('.search-summary')!.textContent).not.toContain('本地匹配');
  expect(document.querySelector<HTMLButtonElement>('[aria-label="搜索模式"]')!.disabled).toBe(true);
  await click('停止搜索');await vi.waitFor(()=>expect(document.body.textContent).toContain('已停止'));
  expect(document.querySelector('.search-loading')).toBeNull();expect(document.querySelector('.search-result')).toBeNull();expect(document.body.textContent).not.toContain('没有找到相关文件');
 });
});

describe('深度搜索返回状态',()=>{
 it('接口失败后结束加载，显示错误和重试提示',async()=>{
  await setup(false,'search');await vi.waitFor(()=>expect(document.body.textContent).toContain('open-octx.md'));
  const base=invoke.getMockImplementation()!;let reject:(error:Error)=>void=()=>{};
  invoke.mockImplementation((command,args)=>command==='run_query'&&args.query.kind==='deep_search'?new Promise((_,no)=>reject=no):base(command,args));
  await selectMode('deep_search');reject(new Error('Jev 暂时不可用'));await vi.waitFor(()=>expect(document.body.textContent).toContain('Jev 暂时不可用'));expect(document.body.textContent).not.toContain('open-octx.md');expect(document.querySelector('.search-loading')).toBeNull();expect(document.body.textContent).toContain('搜索未完成');expect(document.body.textContent).not.toContain('没有找到相关文件');
 });
 it('停止后的迟到结果不能覆盖停止状态',async()=>{
  await setup(false,'search');await vi.waitFor(()=>expect(document.body.textContent).toContain('open-octx.md'));
  const base=invoke.getMockImplementation()!;let resolve:(value:unknown)=>void=()=>{};
  invoke.mockImplementation((command,args)=>command==='run_query'&&args.query.kind==='deep_search'?new Promise(yes=>resolve=yes):base(command,args));
  await selectMode('deep_search');await click('停止搜索');resolve({results:[],trace:[],conversation:null});await tick();await tick();
  expect(document.body.textContent).not.toContain('open-octx.md');expect(document.body.textContent).toContain('已停止');expect(document.querySelector('.search-loading')).toBeNull();
 });
});

describe('搜索结果里的原文高亮',()=>{
 it('摘要和片段高亮，下一处移动当前位置，修改输入不影响已完成的搜索',async()=>{
  await setup();await vi.waitFor(()=>expect(document.querySelectorAll('.search-preview .search-hit')).toHaveLength(2));
  expect(document.querySelector('.search-result mark')!.textContent).toBe('octx');
  expect(document.querySelector('.search-preview .search-hit.active')!.textContent).toBe('OCTX');
  await click('在文档中查找');await click('下一处匹配');expect(document.querySelector('.search-preview .search-hit.active')!.textContent).toBe('octx');
  const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='新问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  expect(document.querySelectorAll('.search-preview .search-hit')).toHaveLength(2);
 });
});


it('缺少 Jev 密钥时跳转模型设置并定位密钥输入，关闭后保留搜索',async()=>{
 await setup(false,'search');
 await vi.waitFor(()=>expect(document.querySelector('.search-result')).not.toBeNull());
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='run_query')throw '请在设置中填写 Jev API 密钥';
  return base(command,args);
 });
 await selectMode('deep_search');
 await vi.waitFor(()=>expect(document.querySelector('.search-page .settings-error-link')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.search-page .settings-error-link')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.settings-nav [aria-current="page"]')?.textContent).toBe('模型设置'));
 const key=document.querySelector('.model-settings-card[data-model=jev] input[data-autofocus]');
 expect(key).not.toBeNull();
 expect(key?.getAttribute('type')).toBe('password');
 document.querySelector<HTMLButtonElement>('[aria-label="关闭设置"]')!.click();await tick();
 expect(document.querySelector('.settings-dialog')).toBeNull();
 expect(document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!.value).toBe('octx');
});
