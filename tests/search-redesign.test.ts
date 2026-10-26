// @vitest-environment happy-dom
import {afterEach,describe,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import {sourceFolder,sourceName} from '../src/lib/domain';
import {plainText} from '../src/lib/search-highlights';
import type {Candidate} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));

describe('结果项文字',()=>{
 it('摘录去掉 Markdown 符号，转换文件显示原始文件名和相对目录',()=>{
  expect(plainText('/a.md','### 1.1 实体 **KV Cache** 设计\n\n为什么 `KV` 对[前缀](http://x)敏感。\n\n| a | b |\n| --- | --- |\n- [x] 完成')).toBe('1.1 实体 KV Cache 设计 为什么 KV 对前缀敏感。 a b 完成');
  expect(plainText('/a.txt','第一行\n第二行')).toBe('第一行 第二行');
  expect(sourceName({path:'/r/.sag-converted/a.pdf.md',title:'a.pdf.md'})).toBe('a.pdf');
  expect(sourceName({path:'/r/a.md',title:'a.md'})).toBe('a.md');
  expect(sourceFolder('/r/资料/.sag-converted/a.pdf.md',[{path:'/r/资料',name:'资料'}])).toBe('资料');
  expect(sourceFolder('/r/资料/笔记/b.md',[{path:'/r',name:'r'},{path:'/r/资料',name:'资料'}])).toBe('资料 / 笔记');
 });
});

let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();localStorage.clear()});
const hit=(id:number,path:string,content:string):Candidate=>({id,document_id:id,path,title:path.split('/').at(-1)!,heading:`第 ${id} 节`,content,start_line:1,end_line:1,source_ranges:[],bm25:1,score:1,order:id,hash:String(id)});
const hits=[hit(1,'/files/a.md','甲 octx'),hit(2,'/files/a.md','乙 octx'),hit(3,'/files/b.md','丙 octx'),hit(4,'/files/c.md','丁 octx')];
async function search(candidates=hits,mode:'search'|'deep_search'='deep_search'){
 localStorage.setItem('sag.search.mode',mode);localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'search',editor_size:15,search_summary_enabled:false,answer:{model:'test',models:['test']}};
  if(command==='run_query'){const page=args.query.page??1;return{results:mode==='search'?candidates.slice((page-1)*20,page*20):candidates,terms:['octx'],elapsed_ms:24,pagination:mode==='search'?{total:candidates.length,page,page_size:20}:null,trace:[],conversation:null}}
  if(command==='read_document'){const found=candidates.find(c=>c.path===args.path)!;return{id:1,path:found.path,name:found.title,body:candidates.filter(c=>c.path===found.path).map(c=>c.content).join('\n\n'),hash:'same'}}
  if(command==='list_roots')return[{path:'/files',name:'资料',status:'ready',count:3,error:null}];
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='octx';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-result')).toHaveLength(Math.min(20,new Set(candidates.map(c=>c.path)).size)));
}
const title=()=>document.querySelector('.search-preview-title strong')?.textContent;
const key=(target:EventTarget,init:KeyboardEventInit)=>target.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,cancelable:true,...init}));

it('默认阅读第一文件，片段在阅读区打开且不扩展结果列表',async()=>{
 await search();
 await vi.waitFor(()=>expect(title()).toBe('a.md'));
 expect(document.querySelector('.search-summary')!.textContent).toBe('深入搜索 · 3 个文件 · 24 ms');
 expect(document.querySelector('.search-result-folder')!.textContent).toBe('资料');
 const list=document.querySelector<HTMLElement>('.search-results')!;
 const before=list.textContent;
 list.focus();key(list,{key:'ArrowRight'});await tick();
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-passage-picker .search-passage-option')).toHaveLength(2));
 expect(list.textContent).toBe(before);
 expect(list.querySelector('.search-passage-picker')).toBeNull();
 const passages=[...document.querySelectorAll<HTMLButtonElement>('.search-passage-picker .search-passage-option')];
 passages[1].click();await tick();
 expect(document.querySelector('.search-passage-picker')).toBeNull();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('2 / 2'));
 key(list,{key:'ArrowDown'});await tick();
 await vi.waitFor(()=>expect(title()).toBe('b.md'));
 key(list,{key:'ArrowUp'});await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-trigger')!.textContent).toContain('2 / 2'));
});

it('片段面板 Escape 关闭并归还焦点，选择不会改变文件列表',async()=>{
 await search();
 await vi.waitFor(()=>expect(title()).toBe('a.md'));
 const list=document.querySelector<HTMLElement>('.search-results')!;
 list.focus();key(list,{key:'ArrowRight'});await tick();
 await vi.waitFor(()=>expect(document.querySelector('.search-passage-picker')).not.toBeNull());
 key(document.activeElement!,{key:'Escape'});await tick();
 expect(document.querySelector('.search-passage-picker')).toBeNull();
 expect(document.activeElement).toBe(list);
 expect(title()).toBe('a.md');
});

it.each(['鼠标','键盘'])('%s 切换回文件时定位高亮，不恢复偏离高亮的阅读位置',async method=>{
 const scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
 try{
  await search();
  await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')?.textContent).toBe('甲 octx'));
  const reader=document.querySelector<HTMLElement>('.markdown-reader')!;
  reader.scrollTop=640;reader.dispatchEvent(new Event('scroll'));
  const list=document.querySelector<HTMLElement>('.search-results')!;
  if(method==='鼠标')document.querySelectorAll<HTMLElement>('.search-result')[1].click();
  else key(list,{key:'ArrowDown'});
  await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')?.textContent).toBe('丙 octx'));
  scroll.mockClear();
  if(method==='鼠标')document.querySelectorAll<HTMLElement>('.search-result')[0].click();
  else key(list,{key:'ArrowUp'});
  await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')?.textContent).toBe('甲 octx'));
  expect(document.querySelector<HTMLElement>('.markdown-reader')!.scrollTop).not.toBe(640);
  expect(scroll.mock.contexts).toContain(document.querySelector('.search-hit.active'));
 }finally{scroll.mockRestore()}
});

it('正文查找与专注阅读按需展开，Escape 先关闭查找再退出专注',async()=>{
 await search();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')).not.toBeNull());
 expect(document.querySelector('.search-preview .match-navigation')).toBeNull();
 document.querySelector<HTMLButtonElement>('.search-focus-toggle')!.click();await tick();
 expect(document.querySelector('.search-layout.focus-mode')).not.toBeNull();
 key(window,{key:'f',metaKey:true});await tick();
 await vi.waitFor(()=>expect(document.activeElement?.id).toBe('preview-find'));
 const input=document.querySelector<HTMLInputElement>('#preview-find')!;
 input.value='乙';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.search-preview .search-hit.active')?.textContent).toBe('乙'));
 key(input,{key:'Escape'});await tick();
 expect(document.querySelector('.search-find-bar')).toBeNull();
 expect(document.querySelector('.search-layout.focus-mode')).not.toBeNull();
 key(window,{key:'Escape'});await tick();
 expect(document.querySelector('.search-layout.focus-mode')).toBeNull();
});

it('拖动或用方向键调整列表宽度并记住',async()=>{
 await search();
 const handle=document.querySelector<HTMLElement>('.search-resizer')!;
 expect(handle.getAttribute('aria-valuenow')).toBe('340');
 key(handle,{key:'ArrowRight'});await tick();
 expect(handle.getAttribute('aria-valuenow')).toBe('356');
 expect(localStorage.getItem('sag.search.listWidth')).toBe('356');
 for(let step=0;step<30;step++)key(handle,{key:'ArrowLeft'});await tick();
 expect(handle.getAttribute('aria-valuenow')).toBe('260');
});

it('⌘G 和 ⇧⌘G 在原文命中之间切换',async()=>{
 await search();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview .search-hit.active')).not.toBeNull());
 const label=()=>document.querySelector('.search-preview .match-navigation span')!.textContent;
 expect(document.querySelector('.search-preview .match-navigation')).toBeNull();
 key(window,{key:'f',metaKey:true});await tick();
 await vi.waitFor(()=>expect(label()).toBe('1 / 2 处匹配'));
 key(window,{key:'g',metaKey:true});await tick();
 await vi.waitFor(()=>expect(label()).toBe('2 / 2 处匹配'));
 key(window,{key:'g',metaKey:true,shiftKey:true});await tick();
 await vi.waitFor(()=>expect(label()).toBe('1 / 2 处匹配'));
});

it.each(['search','deep_search'] as const)('%s 翻到较短末页后，键盘从当前原文继续选择，也跟随鼠标选择',async mode=>{
 const candidates=Array.from({length:22},(_,i)=>hit(i+1,`/files/${i}.txt`,'octx'));
 await search(candidates,mode);
 const list=document.querySelector<HTMLElement>('.search-results')!;
 list.focus();key(list,{key:'End'});
 await vi.waitFor(()=>expect(title()).toBe('19.txt'));
 document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-result')).toHaveLength(2));
 await vi.waitFor(()=>expect(title()).toBe('20.txt'));
 list.focus();key(list,{key:'ArrowDown'});
 await vi.waitFor(()=>expect(title()).toBe('21.txt'));
 document.querySelector<HTMLElement>('.search-result')!.click();
 await vi.waitFor(()=>expect(title()).toBe('20.txt'));
 list.focus();key(list,{key:'ArrowDown'});
 await vi.waitFor(()=>expect(title()).toBe('21.txt'));
});

it('跨文件连续打开片段时把键盘焦点移入新文件的面板',async()=>{
 await search();await vi.waitFor(()=>expect(title()).toBe('a.md'));
 document.querySelectorAll<HTMLButtonElement>('.search-result-more')[0].click();await tick();
 await vi.waitFor(()=>expect(document.activeElement?.classList.contains('search-passage-option')).toBe(true));
 const other=document.querySelectorAll<HTMLButtonElement>('.search-result-more')[1];other.focus();other.click();await tick();
 await vi.waitFor(()=>expect(title()).toBe('b.md'));
 await vi.waitFor(()=>expect(document.activeElement?.closest('.search-passage-picker')).not.toBeNull());
 expect(document.querySelectorAll('.search-passage-option')).toHaveLength(1);
});

it('阅读区的上一个/下一个片段按钮在同一文件内切换，到边界时禁用',async()=>{
 await search();await vi.waitFor(()=>expect(title()).toBe('a.md'));
 const previous=()=>document.querySelector<HTMLButtonElement>('.search-passage-previous')!,next=()=>document.querySelector<HTMLButtonElement>('.search-passage-next')!;
 const counter=()=>document.querySelector('.search-passage-trigger b')!.textContent;
 expect(counter()).toBe('1 / 2');expect(previous().disabled).toBe(true);expect(next().disabled).toBe(false);
 next().click();await tick();
 await vi.waitFor(()=>expect(counter()).toBe('2 / 2'));
 expect(next().disabled).toBe(true);expect(previous().disabled).toBe(false);
 expect(title()).toBe('a.md');
 expect(document.querySelector('.search-passage-picker')).toBeNull();
 previous().click();await tick();
 await vi.waitFor(()=>expect(counter()).toBe('1 / 2'));
 // Clicking the passage count inside a file row opens the picker without changing which file is read.
 document.querySelectorAll<HTMLButtonElement>('.search-result-more')[1].click();await tick();
 await vi.waitFor(()=>expect(title()).toBe('b.md'));
 expect(document.querySelector('.search-passage-picker')).not.toBeNull();
});
