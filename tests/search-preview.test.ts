// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Candidate} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const hits:Candidate[]=[1,2].map(id=>({id,document_id:id,path:`/files/file-${id}.txt`,title:`file-${id}.txt`,heading:'',content:`文件 ${id} 的 octx 正文。`,start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:-1,score:0,order:id,hash:'same'}));
async function setup(candidates=hits,bodies:Record<string,string>={}){
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'search',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='run_query')return{results:candidates,terms:['octx'],elapsed_ms:24,trace:[],conversation:null};
  if(command==='read_document'){const hit=candidates.find(hit=>hit.path===args.path)!;return{id:hit.id,path:hit.path,name:hit.title,body:bodies[hit.path]??hit.content,hash:'same'}}
  return[];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='octx';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-result')).toHaveLength(Math.min(candidates.length,20)));
}
function openDocument(){[...document.querySelectorAll<HTMLButtonElement>('.search-preview button')].find(button=>button.textContent==='打开文档')!.click()}
it('Windows 搜索预览和原文页显示普通路径，读取和打开使用原始路径',async()=>{
 const path=String.raw`\\?\D:\资料\file.txt`,shown=String.raw`D:\资料\file.txt`;
 await setup([{...hits[0],path}]);
 await vi.waitFor(()=>expect(document.querySelector('.search-preview .source-path')?.textContent).toBe(shown));
 expect(invoke).toHaveBeenCalledWith('read_document',{path});
 openDocument();
 await vi.waitFor(()=>expect(document.querySelector('.readonly-document .source-path')?.textContent).toBe(shown));
 document.querySelector<HTMLButtonElement>('.readonly-document .editor-head .outline')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('reveal_path',{path}));
});
it('右侧直接渲染 Markdown 全文及高亮，切换文件读取相应文档',async()=>{
 const target={...hits[0],path:'/files/notes.md',title:'notes.md'};
 await setup([target,hits[1]],{[target.path]:'引言\n\n# 标题\n\n可渲染的 **octx** 正文。\n\n全文末尾'});
 await vi.waitFor(()=>expect(document.querySelector('.search-preview h1')?.textContent).toBe('标题'));
 expect(document.querySelector('.search-preview strong .search-hit')?.textContent).toBe('octx');
 expect(document.querySelector('.search-preview-body')?.textContent).toContain('全文末尾');
 document.querySelectorAll<HTMLButtonElement>('.search-result')[1].click();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')?.textContent).toContain('文件 2 的 octx'));
 expect(invoke.mock.calls.filter(([command])=>command==='read_document')).toHaveLength(2);
});
it('HTML 右侧预览和打开文档都渲染页面，保留样式、图片、高亮及目录链接',async()=>{
 const target={...hits[0],path:'/files/product.html',title:'product.html',content:'<h1>产品 octx</h1>'};
 const body='<style>h1{color:red}</style><a href="#content">目录</a><h1 id="content">产品 octx</h1><img src="data:image/png;base64,QUJD"><script>window.parent.postMessage("test","*")</script>';
 await setup([target],{[target.path]:body});
 await vi.waitFor(()=>expect(document.querySelector('.search-preview iframe')).not.toBeNull());
 for(const selector of ['.search-preview iframe','.readonly-document iframe']){
  if(selector.startsWith('.readonly')){openDocument();await vi.waitFor(()=>expect(document.querySelector(selector)).not.toBeNull())}
  const frame=document.querySelector<HTMLIFrameElement>(selector)!;
  const rendered=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
  expect(rendered.querySelector('h1 .search-hit')?.textContent).toBe('octx');
  expect([...rendered.querySelectorAll('style')].some(style=>style.textContent==='h1{color:red}')).toBe(true);
  expect(rendered.querySelector('img')!.getAttribute('src')).toBe('data:image/png;base64,QUJD');
  expect(rendered.querySelector('a')!.getAttribute('href')).toBe('about:srcdoc#content');
  expect(frame.getAttribute('sandbox')).toBe('allow-same-origin');expect(frame.title).toBe('HTML 文档');
 }
});
it('预览和打开文档均定位到检索片段，而非全文较早的相同词',async()=>{
 const target={...hits[0],content:'目标段落里的 octx。',start_line:3};
 const body='前面的 octx。\n\n目标段落里的 octx。';
 await setup([target],{[target.path]:body});
 await vi.waitFor(()=>expect(document.querySelector('.search-preview .search-hit.active')?.getAttribute('data-search-match')).toBe('1'));
 openDocument();
 await vi.waitFor(()=>expect(document.querySelector('.readonly-document .search-hit.active')?.getAttribute('data-search-match')).toBe('1'));
});
it('返回搜索保留页码、查询输入、选中文件和列表滚动，不重新查询',async()=>{
 const candidates=Array.from({length:25},(_,index)=>({...hits[0],id:index+1,path:`/files/${index+1}.txt`,title:`${index+1}.txt`}));
 await setup(candidates);document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click();await tick();
 document.querySelectorAll<HTMLButtonElement>('.search-result')[1].click();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')).not.toBeNull());
 document.querySelector('.search-results')!.scrollTop=220;
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value='未提交的查询';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 openDocument();await vi.waitFor(()=>expect(document.querySelector('.readonly-document')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[title="返回搜索"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.search-page')).not.toBeNull());
 expect(document.querySelector('.search-result.selected')?.textContent).toContain('22.txt');
 expect(document.querySelector('.search-page-range')?.textContent).toContain('第 21–25 个');
 expect(document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!.value).toBe('未提交的查询');
 expect(document.querySelector('.search-results')!.scrollTop).toBe(220);
 expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(1);
});
it('打开全文失败时保留预览并显示错误',async()=>{
 await setup();await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')).not.toBeNull());
 const base=invoke.getMockImplementation()!;invoke.mockImplementation((command,args)=>command==='read_document'?Promise.reject(new Error('文件暂时不可读')):base(command,args));
 openDocument();await vi.waitFor(()=>expect(document.body.textContent).toContain('文件暂时不可读'));
 expect(document.querySelector('.search-preview-body')!.textContent).toContain('文件 1 的 octx');
});
it('切换文档后，较早的读取结果不会覆盖当前预览',async()=>{
 await setup();await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')).not.toBeNull());
 const base=invoke.getMockImplementation()!;let finish:(value:unknown)=>void=()=>{};
 invoke.mockImplementation((command,args)=>command==='read_document'&&args.path===hits[1].path?new Promise(resolve=>finish=resolve):base(command,args));
 document.querySelectorAll<HTMLButtonElement>('.search-result')[1].click();await tick();
 expect(document.querySelector('.search-preview')!.textContent).toContain('正在读取文件');
 document.querySelectorAll<HTMLButtonElement>('.search-result')[0].click();
 await vi.waitFor(()=>expect(document.querySelector('.search-preview-body')?.textContent).toContain('文件 1 的 octx'));
 finish({id:2,path:hits[1].path,name:hits[1].title,body:'迟到的错误文档',hash:'same'});await tick();await tick();
 expect(document.querySelector('.search-preview-body')!.textContent).toContain('文件 1 的 octx');
});
