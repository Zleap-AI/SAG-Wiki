// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import MarkdownReader from '../src/components/MarkdownReader.svelte';
import {candidateRanges,clearMarkdownModels,markdownModel,modelCitationTerms,pieceIndex} from '../src/lib/document-model';
import {renderMarkdown} from '../src/lib/markdown';
import {citationTerms,highlightedHtml} from '../src/lib/search-highlights';
import type {Candidate} from '../src/lib/types';
import {citedSpan,fixture} from '../bench/fixtures';

vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(async()=>'data:image/png;base64,')}));
let app:ReturnType<typeof mount>|null=null;
afterEach(async()=>{if(app)await unmount(app);app=null;document.body.innerHTML='';clearMarkdownModels()});

const {body}=fixture(60_000,'KV Cache');
const doc={path:'/book.md',body,hash:'h'};
function candidate(position:number):Candidate{
 const cited=citedSpan(body,position);
 return {id:1,document_id:1,path:doc.path,title:'book.md',heading:'',content:cited.content,start_line:cited.startLine,end_line:cited.startLine,source_ranges:[],source_spans:[cited.span],bm25:0,score:0,order:0,hash:'h'};
}
const label=()=>document.querySelector('.match-navigation span')?.textContent??'';
async function open(props:Record<string,unknown>){
 app=mount(MarkdownReader,{target:document.body,props:{model:markdownModel(doc),path:doc.path,...props}});
 await tick();
}

it('文档按块渲染，计数和整篇高亮一致，命中可以跨块切换',async()=>{
 const model=markdownModel(doc);
 expect(model.pieces.length).toBeGreaterThan(5);
 const whole=highlightedHtml(renderMarkdown(body),['KV Cache']).count;
 const target=model.pieces[3].from;
 await open({terms:['KV Cache'],target});
 expect(document.querySelectorAll('.reader-piece')).toHaveLength(model.pieces.length);
 expect(document.querySelector('.search-hit.active')?.closest<HTMLElement>('.reader-piece')?.dataset.piece).toBe('3');
 await vi.waitFor(()=>expect(label()).toMatch(/处匹配$/));
 expect(label()).toMatch(new RegExp(`/ ${whole} 处匹配$`));
 expect(document.querySelectorAll('.reader-piece:not([data-state])')).toHaveLength(0);
 const before=label();
 const marks=document.querySelectorAll('.reader-piece[data-piece="3"] .search-hit');
 for(let step=0;step<marks.length;step++){document.querySelector<HTMLButtonElement>('.match-next')!.click();await tick()}
 expect(label()).not.toBe(before);
 expect(Number(document.querySelector('.search-hit.active')?.closest<HTMLElement>('.reader-piece')?.dataset.piece)).toBeGreaterThan(3);
});

it('搜索预览定位到候选位置之后的第一处命中，编号与整篇前缀一致',async()=>{
 const model=markdownModel(doc),hit=candidate(0.7),target=candidateRanges(model,hit)[0].from;
 await open({terms:['KV Cache'],target});
 await vi.waitFor(()=>expect(label()).toMatch(/处匹配$/));
 const prefix=highlightedHtml(renderMarkdown(body.slice(0,model.offset+target),body),['KV Cache']).count;
 expect(label()).toMatch(new RegExp(`^${prefix+1} / `));
});

it.each([0,400])('目标之后无匹配时仍可在前文命中之间导航（填充 %i 段）',async(paragraphs)=>{
 const body='needle first\n\nneedle second\n\n'+'普通段落用于分隔目标与命中。\n\n'.repeat(paragraphs)+'目标段落没有关键词\n';
 const model=markdownModel({path:'/trailing.md',body,hash:'trailing'});
 await open({model,terms:['needle'],target:body.indexOf('目标段落')});
 await vi.waitFor(()=>expect(label()).toBe('2 / 2 处匹配'));
 const matches=document.querySelectorAll('.search-hit');
 expect(matches).toHaveLength(2);
 expect(document.querySelector('.search-hit.active')).toBe(matches[1]);
 document.querySelector<HTMLButtonElement>('.match-previous')!.click();await tick();
 expect(label()).toBe('1 / 2 处匹配');
 expect(document.querySelector('.search-hit.active')).toBe(matches[0]);
 document.querySelector<HTMLButtonElement>('.match-next')!.click();await tick();
 expect(label()).toBe('2 / 2 处匹配');
 document.querySelector<HTMLButtonElement>('.match-next')!.click();await tick();
 expect(label()).toBe('1 / 2 处匹配');
});

it('文档完全没有匹配时不显示命中导航',async()=>{
 const model=markdownModel({path:'/no-match.md',body:'目标段落没有关键词\n',hash:'none'});
 await open({model,terms:['needle']});
 await vi.waitFor(()=>expect(document.querySelectorAll('.reader-piece:not([data-state])')).toHaveLength(0));
 expect(document.querySelector('.match-navigation')).toBeNull();
 expect(document.querySelector('.search-hit.active')).toBeNull();
});

it('搜索正文可以隐藏命中导航，同时保留目标高亮',async()=>{
 await open({terms:['KV Cache'],showNavigation:false});
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
 expect(document.querySelector('.match-navigation')).toBeNull();
});

it('同文件返回恢复阅读块，后台匹配不抢位置；新片段优先新目标',async()=>{
 const tasks:(()=>void)[]=[];
 vi.stubGlobal('scheduler',{postTask:(task:()=>void)=>new Promise<void>(resolve=>tasks.push(()=>{task();resolve()}))});
 const height=vi.spyOn(HTMLElement.prototype,'offsetHeight','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('reader-piece')?(this.dataset.state?100:parseFloat(this.style.minHeight)||100):0});
 const top=vi.spyOn(HTMLElement.prototype,'offsetTop','get').mockImplementation(function(this:HTMLElement){
  if(!this.classList.contains('reader-piece'))return 0;
  let offset=0;for(let node=this.previousElementSibling;node;node=node.previousElementSibling)offset+=(node as HTMLElement).offsetHeight;return offset;
 });
 const scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
 const flush=async()=>{for(let turn=0;turn<100&&tasks.length;turn++){tasks.splice(0).forEach(task=>task());await tick()}};
 try{
  const model=markdownModel(doc),memoryKey='reader-return:book:h:query-1';
  await open({model,terms:['KV Cache'],memoryKey});await flush();
  const firstHost=document.querySelector<HTMLElement>('.markdown-reader')!;
  firstHost.scrollTop=625;firstHost.dispatchEvent(new Event('scroll'));
  await unmount(app!);app=null;
  scroll.mockClear();
  await open({model,terms:['KV Cache'],memoryKey,restoreScroll:true,showNavigation:false});
  expect([...document.querySelectorAll<HTMLElement>('.reader-piece[data-state="rendered"]')].map(piece=>piece.dataset.piece)).toEqual(['5','6','7']);
  await flush();
  expect(document.querySelector<HTMLElement>('.markdown-reader')!.scrollTop).toBe(625);
  expect(scroll).not.toHaveBeenCalled();
  await unmount(app!);app=null;scroll.mockClear();
  await open({model,terms:['KV Cache'],memoryKey,restoreScroll:false,target:model.pieces[2].from});
  expect(document.querySelector('.search-hit.active')?.closest<HTMLElement>('.reader-piece')?.dataset.piece).toBe('2');
  expect(scroll).toHaveBeenCalledWith({block:'center'});
 }finally{height.mockRestore();top.mockRestore();scroll.mockRestore();vi.unstubAllGlobals()}
});

it('远处首次命中在后台查找，首屏只渲染目标邻域且用户滚动后不抢视口',async()=>{
 const tasks:(()=>void)[]=[];
 vi.stubGlobal('scheduler',{postTask:(task:()=>void)=>new Promise<void>(resolve=>tasks.push(()=>{task();resolve()}))});
 const scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
 try{
  const body=('普通段落用于分隔目标与命中。'.repeat(60)+'\n\n').repeat(100)+'needle first\n\nneedle second\n';
  const model=markdownModel({path:'/distant-match.md',body,hash:'distant'});
  expect(model.pieces.length).toBeGreaterThan(10);
  await open({model,terms:['needle'],target:model.pieces[2].from});
  expect([...document.querySelectorAll<HTMLElement>('.reader-piece[data-state="rendered"]')].map(piece=>piece.dataset.piece)).toEqual(['1','2','3']);
  expect(document.querySelector('.search-hit.active')).toBeNull();
  expect(tasks.length).toBeGreaterThan(0);
  document.querySelector('.markdown-reader')!.dispatchEvent(new Event('wheel'));
  scroll.mockClear();
  await vi.waitFor(()=>{
   tasks.splice(0).forEach(task=>task());
   expect(label()).toBe('1 / 2 处匹配');
  });
  expect(document.querySelectorAll('.reader-piece:not([data-state])')).toHaveLength(0);
  expect(document.querySelector('.search-hit.active')?.textContent).toBe('needle');
  expect(scroll).not.toHaveBeenCalled();
  document.querySelector<HTMLButtonElement>('.match-next')!.click();await tick();
  expect(label()).toBe('2 / 2 处匹配');
 }finally{scroll.mockRestore();vi.unstubAllGlobals()}
});

it('用户滚动后，后台选中前文命中不会重新移动视口',async()=>{
 const tasks:(()=>void)[]=[];
 vi.stubGlobal('scheduler',{postTask:(task:()=>void)=>new Promise<void>(resolve=>tasks.push(()=>{task();resolve()}))});
 const scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
 try{
  const body='needle first\n\nneedle second\n\n'+'普通段落。\n\n'.repeat(1000)+'目标段落\n';
  const model=markdownModel({path:'/scroll.md',body,hash:'scroll'});
  await open({model,terms:['needle'],target:body.indexOf('目标段落')});
  expect(tasks.length).toBeGreaterThan(0);
  document.querySelector('.markdown-reader')!.dispatchEvent(new Event('wheel'));
  scroll.mockClear();
  await vi.waitFor(()=>{
   tasks.splice(0).forEach(task=>task());
   expect(label()).toBe('2 / 2 处匹配');
  });
  expect(document.querySelector('.search-hit.active')).not.toBeNull();
  expect(scroll).not.toHaveBeenCalled();
 }finally{scroll.mockRestore();vi.unstubAllGlobals()}
});

it('引用只高亮被引用的块，并按引用计数',async()=>{
 const model=markdownModel(doc),hit=candidate(0.5),ranges=candidateRanges(model,hit);
 const terms=await modelCitationTerms(model,hit,ranges);
 expect(terms).toEqual(await citationTerms(hit,body));
 await open({terms,target:ranges[0].from,within:ranges,label:'citation'});
 await vi.waitFor(()=>expect(label()).toBe('引用 1 / 1'));
 const pieces=new Set([...document.querySelectorAll<HTMLElement>('.search-hit')].map(mark=>mark.closest<HTMLElement>('.reader-piece')!.dataset.piece));
 expect([...pieces]).toEqual([String(pieceIndex(model,ranges[0].from))]);
 expect([...document.querySelectorAll('.search-hit.active')].map(mark=>mark.textContent).join('')).toBe(terms[0]);
});

it('同一文件再次打开复用缓存的模型和渲染结果',()=>{
 const first=markdownModel(doc);first.html[0]='<p>cached</p>';
 expect(markdownModel({...doc})).toBe(first);
 expect(markdownModel({...doc,hash:'other'})).not.toBe(first);
 for(let index=0;index<6;index++)markdownModel({...doc,path:`/other-${index}.md`});
 expect(markdownModel(doc)).not.toBe(first);
});

it('没有字节范围时按行号定位，末行引用到文件结尾',()=>{
 const model=markdownModel({path:'/one.md',body:'第一行\n第二行',hash:'1'});
 const base={...candidate(0.5),source_spans:undefined,path:'/one.md'};
 expect(candidateRanges(model,{...base,start_line:2,end_line:2})).toEqual([{from:4,to:7}]);
 expect(candidateRanges(model,{...base,start_line:1,end_line:1})).toEqual([{from:0,to:4}]);
});
