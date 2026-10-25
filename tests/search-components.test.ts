// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SearchResultItem from '../src/components/SearchResultItem.svelte';
import SearchPassagePicker from '../src/components/SearchPassagePicker.svelte';
import type {Candidate} from '../src/lib/types';

let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.restoreAllMocks()});
const candidate=(id:number):Candidate=>({id,document_id:1,path:'/files/.sag-converted/report.pdf.md',title:'report.pdf.md',heading:`第 ${id} 节`,content:`第 ${id} 段 **needle** 原文。`,start_line:id,end_line:id,source_ranges:[],bm25:1,score:1,order:id,hash:'same'});

it.each([1,46])('%i 个片段都提供总数入口，打开选择器不在文件列表内追加内容',async count=>{
 const candidates=Array.from({length:count},(_,i)=>candidate(i+1)),onSelect=vi.fn(),onToggle=vi.fn();
 app=mount(SearchResultItem,{target:document.body,props:{id:'file-1',group:{first:candidates[0],matches:candidates},expanded:true,onSelect,onToggle}});
 const trigger=document.querySelector<HTMLButtonElement>('.search-result-more');
 expect(trigger).not.toBeNull();
 expect(trigger!.textContent).toContain(`${count} 处相关片段`);
 expect(document.querySelector('.search-passages')).toBeNull();
 trigger!.click();
 expect(onToggle).toHaveBeenCalledTimes(1);
 expect(onSelect).not.toHaveBeenCalled();
 expect(document.querySelectorAll('.search-result')).toHaveLength(1);
});

it('文件条目保留原始名称、目录、纯文本摘要和引用，并可选择原文',()=>{
 const first=candidate(1),onSelect=vi.fn();
 app=mount(SearchResultItem,{target:document.body,props:{id:'file-1',group:{first,matches:[first]},roots:[{path:'/files',name:'资料'}],terms:['needle'],cited:[1,3],selected:true,focused:first,onSelect,onToggle:()=>{}}});
 expect(document.querySelector('.search-result-title strong')?.textContent).toBe('report.pdf');
 expect(document.querySelector('.search-result-folder')?.textContent).toBe('资料');
 expect(document.querySelector('.search-result-format')?.textContent).toBe('PDF');
 expect(document.querySelector('.search-result p')?.textContent).toBe('第 1 段 needle 原文。');
 expect(document.querySelector('.search-result p .search-hit')?.textContent).toBe('needle');
 expect([...document.querySelectorAll('.search-result-cited span')].map(node=>node.textContent)).toEqual(['1','3']);
 expect(document.querySelector('.search-result-reading')?.textContent).toContain('阅读中');
 document.querySelector<HTMLElement>('.search-result')!.click();
 expect(onSelect).toHaveBeenCalledExactlyOnceWith(first);
});

const key=(target:EventTarget,value:string)=>target.dispatchEvent(new KeyboardEvent('keydown',{key:value,bubbles:true,cancelable:true}));
it('片段面板显示全部结果，聚焦当前片段，方向键定位并仅确认指定片段一次',async()=>{
 const candidates=Array.from({length:46},(_,i)=>candidate(i+1)),onSelect=vi.fn();
 app=mount(SearchPassagePicker,{target:document.body,props:{candidates,selectedId:23,terms:['needle'],onSelect,onClose:()=>{}}});
 await tick();
 const panel=document.querySelector<HTMLElement>('[role="dialog"]')!;
 expect(panel).not.toBeNull();
 expect(panel.getAttribute('aria-modal')).not.toBe('true');
 const options=[...panel.querySelectorAll<HTMLButtonElement>('.search-passage-option')];
 expect(options).toHaveLength(46);
 expect(document.activeElement).toBe(options[22]);
 expect(options[22].getAttribute('aria-pressed')).toBe('true');
 expect(options[22].textContent).toContain('第 23 节');
 expect(options[22].textContent).not.toContain('**');
 expect(options[22].querySelector('.search-hit')?.textContent).toBe('needle');
 key(options[22],'ArrowDown');await tick();expect(document.activeElement).toBe(options[23]);
 key(options[23],'ArrowUp');await tick();expect(document.activeElement).toBe(options[22]);
 key(options[22],'Home');await tick();expect(document.activeElement).toBe(options[0]);
 key(options[0],'End');await tick();expect(document.activeElement).toBe(options[45]);
 expect(onSelect).not.toHaveBeenCalled();
 expect(key(options[45],'Enter')).toBe(false);
 expect(onSelect).toHaveBeenCalledExactlyOnceWith(candidates[45]);
});

it('Esc 关闭片段面板而不冒泡触发页面返回，空列表也可关闭',async()=>{
 const onClose=vi.fn(),onSelect=vi.fn(),pageKeys=vi.fn();
 document.body.addEventListener('keydown',pageKeys);
 try{
  app=mount(SearchPassagePicker,{target:document.body,props:{candidates:[],selectedId:null,onSelect,onClose}});await tick();
  const close=document.querySelector<HTMLButtonElement>('[aria-label="关闭片段选择器"]')!;
  expect(document.activeElement).toBe(close);
  key(close,'ArrowDown');key(close,'Home');
  expect(onSelect).not.toHaveBeenCalled();
  expect(key(close,'Escape')).toBe(false);
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(pageKeys.mock.calls.some(([event])=>event.key==='Escape')).toBe(false);
  close.click();expect(onClose).toHaveBeenCalledTimes(2);
 }finally{document.body.removeEventListener('keydown',pageKeys)}
});
