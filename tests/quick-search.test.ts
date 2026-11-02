// @vitest-environment happy-dom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import QuickSearch from '../src/QuickSearch.svelte';
import QuickSearchSettings from '../src/components/QuickSearchSettings.svelte';
import {shortcutFromKey} from '../src/lib/quick-search';
const {invoke,events}=vi.hoisted(()=>({invoke:vi.fn(),events:new Map<string,()=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{events.set(name,handler);return()=>events.delete(name)})}));
const hit=(id:number)=>({id,document_id:id,path:`/notes/${id}.md`,title:`${id}.md`,heading:'',content:`匹配内容 ${id}`,start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:0,score:1,order:id,hash:'same'});
const output=(id:number)=>({results:[hit(id),hit(id+1)],terms:[],trace:[],conversation:null});
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 invoke.mockReset();events.clear();
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {language:'zh-CN',theme:'light',answer:{model:'answer'},jev:{model:'jev',url:'http://localhost',has_key:true}};
  if(command==='list_roots')return [{path:'/notes',name:'笔记',status:'ready'}];
  if(command==='get_quick_search')return {preferences:{enabled:true,shortcut:'CommandOrControl+Shift+Space'},error:null};
  if(command==='run_query')return output(1);
  if(command==='read_document'){const id=Number(args.path.match(/(\d+)\.md$/)[1]);return {path:args.path,name:`${id}.md`,body:`匹配内容 ${id}`,hash:'same'};}
 });
});
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.useRealTimers();});
async function setup(){app=mount(QuickSearch,{target:document.body});await vi.waitFor(()=>expect(document.activeElement?.getAttribute('aria-label')).toBe('快捷搜索关键词'));}
function type(text:string){const input=document.querySelector('input')!;input.value=text;input.dispatchEvent(new Event('input',{bubbles:true}));return input;}
function key(key:string,target:EventTarget=document.querySelector('input')!,options:KeyboardEventInit={}){target.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...options}));}
async function search(text='测试'){const input=type(text);key('Enter',input);await vi.waitFor(()=>expect(document.querySelectorAll('[role=option]')).toHaveLength(2));}
describe('快捷搜索',()=>{
 it('最多展示十个文件，点击或方向键只更新右侧高亮预览',async()=>{
  await setup();const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((c,a)=>c==='run_query'?Promise.resolve({results:[...Array.from({length:12},(_,i)=>hit(i+1)),hit(1)],terms:['匹配'],trace:[],conversation:null}):base(c,a));
  type('匹配');key('Enter');await vi.waitFor(()=>expect(document.querySelectorAll('[role=option]')).toHaveLength(10));
  document.querySelectorAll<HTMLButtonElement>('[role=option]')[8].click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('.quick-preview .search-hit')?.textContent).toContain('匹配内容 9'));
  expect(invoke.mock.calls.some(([c])=>c==='open_quick_search_result')).toBe(false);
  key('ArrowDown');await tick();
  await vi.waitFor(()=>expect(document.querySelector('.quick-preview .search-hit')?.textContent).toContain('匹配内容 10'));
  expect(invoke.mock.calls.filter(([c])=>c==='run_query')).toHaveLength(1);
 });
 it('默认只调用本地检索，去重展示并用方向键及回车打开原文',async()=>{
  await setup();await search();
  expect(invoke.mock.calls.find(([c])=>c==='run_query')![1].query).toMatchObject({kind:'search',question:'测试',scope:[]});
  expect(invoke.mock.calls.some(([c])=>c==='summarize_search')).toBe(false);
  key('ArrowDown');await tick();key('Enter');await tick();
  expect(invoke).toHaveBeenCalledWith('open_quick_search_result',{request:{query:'测试',candidate:hit(2),deep:false}});
 });
 it('深入检索只在点击后执行，完整搜索交给主窗口',async()=>{
  await setup();await search();
  [...document.querySelectorAll('button')].find(b=>b.textContent==='深入检索')!.click();
  await vi.waitFor(()=>expect(invoke.mock.calls.filter(([c])=>c==='run_query').at(-1)![1].query.kind).toBe('deep_search'));
  key('Enter',undefined,{ctrlKey:true});await tick();
  expect(invoke).toHaveBeenCalledWith('open_quick_search_result',{request:{query:'测试',candidate:null,deep:true}});
 });
 it('新查询清除旧结果，晚到结果不能覆盖新查询',async()=>{
  await setup();let resolve!:(value:unknown)=>void;const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((c,a)=>c==='run_query'&&a.query.question==='旧问题'?new Promise(r=>resolve=r):base(c,a));
  type('旧问题');key('Enter');await vi.waitFor(()=>expect(resolve).toBeDefined());
  type('新问题');key('Enter');await vi.waitFor(()=>expect(document.querySelectorAll('[role=option]')).toHaveLength(2));
  resolve(output(9));await tick();await tick();expect(document.querySelector('[role=option]')!.textContent).toContain('1.md');
  expect(invoke.mock.calls.some(([c])=>c==='cancel_query')).toBe(true);
 });
 it('中文组合输入的回车不触发搜索或打开',async()=>{
  await setup();const input=type('拼音');input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));key('Enter',input,{isComposing:true});await tick();
  expect(invoke.mock.calls.some(([c])=>c==='run_query'||c==='open_quick_search_result')).toBe(false);
  input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));key('Enter',input);await tick();
  expect(invoke.mock.calls.some(([c])=>c==='open_quick_search_result')).toBe(false);
 });
 it('Esc 隐藏；失焦通知取消查询并屏蔽迟到结果；再打开重新聚焦',async()=>{
  await setup();let resolve!:(value:unknown)=>void;const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((c,a)=>c==='run_query'?new Promise(r=>resolve=r):base(c,a));
  type('测试');key('Enter');await vi.waitFor(()=>expect(resolve).toBeDefined());key('Escape');await tick();expect(invoke).toHaveBeenCalledWith('hide_quick_search',undefined);
  events.get('quick-search-hidden')!();resolve(output(8));await tick();await tick();expect(document.querySelector('[role=option]')).toBeNull();
  events.get('quick-search-shown')!();await vi.waitFor(()=>expect(document.activeElement?.tagName).toBe('INPUT'));
 });
});
describe('快捷键设置',()=>{
 it('记录组合键并保存，冲突错误明确显示且保留用户选择',async()=>{
  app=mount(QuickSearchSettings,{target:document.body});await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('#quick-search-shortcut')!.disabled).toBe(false));
  const input=document.querySelector<HTMLInputElement>('#quick-search-shortcut')!;input.focus();key('k',input,{code:'KeyK',ctrlKey:true,shiftKey:true});await tick();
  const base=invoke.getMockImplementation()!;invoke.mockImplementation((c,a)=>c==='save_quick_search'?Promise.reject(new Error('快捷键已占用')):base(c,a));
  [...document.querySelectorAll('button')].find(b=>b.textContent==='应用快捷键')!.click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')!.textContent).toContain('快捷键已占用'));
  expect(invoke).toHaveBeenCalledWith('save_quick_search',{preferences:{enabled:true,shortcut:'Control+Shift+KeyK'}});
  expect(input.value).toContain('K');
 });
 it('忽略纯修饰键和中文组合输入',()=>{
  expect(shortcutFromKey(new KeyboardEvent('keydown',{key:'Control',code:'ControlLeft',ctrlKey:true}))).toBeNull();
  expect(shortcutFromKey(new KeyboardEvent('keydown',{key:'k',code:'KeyK',ctrlKey:true,isComposing:true}))).toBeNull();
 });
});
