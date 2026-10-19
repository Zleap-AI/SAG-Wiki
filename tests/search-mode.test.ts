// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function setup(){
 invoke.mockImplementation(async(command)=>command==='get_settings'?{theme:'light',start_page:'search',editor_size:15,answer:{model:'test',models:['test']}}:command==='run_query'?{results:[],terms:[],trace:[],conversation:null}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
 input.value='octx';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 return input;
}
it('搜索默认直接使用深入模式',async()=>{
 const input=await setup();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(invoke.mock.calls.find(([command])=>command==='run_query')![1].query.kind).toBe('deep_search');
});
it('搜索框发送按钮前切换快速模式，已有查询自动重新搜索',async()=>{
 const input=await setup();
 const mode=document.querySelector<HTMLButtonElement>('.search-composer [aria-label="搜索模式"]');
 expect(mode).not.toBeNull();expect(mode!.textContent).toContain('深入');
 expect(mode!.parentElement?.querySelector('[aria-label="搜索文件"]')).not.toBeNull();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="停止搜索"]')).toBeNull());
 mode!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="search"]')).not.toBeNull());
 document.querySelector('[role=option][data-value="search"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(2));
 expect(invoke.mock.calls.filter(([command])=>command==='run_query')[1][1].query.kind).toBe('search');
 expect(document.querySelector('.search-summary [aria-label="深入搜索"]')).toBeNull();
});
it('一百个文件分五页展示，合并同一文件的命中并保持后端排序',async()=>{
 const input=await setup();
 const hits=Array.from({length:100},(_,id)=>({id,document_id:id,path:`/files/${id}.md`,title:`file-${id}.md`,heading:'',content:'octx',start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:-1,score:1-id/100,order:id,hash:'same'}));
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,args)=>command==='run_query'?Promise.resolve({results:[...hits,{...hits[0],id:101}],terms:['octx'],trace:[],conversation:null}):command==='read_document'?Promise.resolve({path:args.path,name:'file.md',body:'octx',hash:'same'}):base(command,args));
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-result')).toHaveLength(20));
 expect(document.querySelector('.search-result')!.textContent).toContain('2 个片段');
 const titles:string[]=[];
 for(let page=0;page<5;page++){
  await vi.waitFor(()=>expect(document.querySelector('.search-result strong')!.textContent).toBe(`file-${page*20}.md`));
  const rows=[...document.querySelectorAll('.search-result strong')].map(row=>row.textContent!);
  expect(rows).toEqual(hits.slice(page*20,(page+1)*20).map(hit=>hit.title));
  titles.push(...rows);
  if(page<4){document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click();await tick();}
 }
 expect(titles).toEqual(hits.map(hit=>hit.title));
 expect(document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.disabled).toBe(true);
 expect(document.querySelector('.search-summary')!.textContent).toContain('100 个文件');
 expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(1);
});
