// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import CitationDocument from '../src/components/CitationDocument.svelte';
import type {Candidate} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
function setup(path:string,body:string,content:string,ranges?:{from:number;to:number}[]){
 const start=body.lastIndexOf(content);
 const source:Candidate={id:1,document_id:1,path,title:path.split('/').at(-1)!,heading:'',content,start_line:1,end_line:1,source_ranges:[],source_spans:(ranges??[{from:start,to:start+content.length}]).map(({from,to})=>({start_byte:new TextEncoder().encode(body.slice(0,from)).length,end_byte:new TextEncoder().encode(body.slice(0,to)).length,start_line:1,end_line:1,start_column:1,end_column:1})),bm25:0,score:0,order:0,hash:'same'};
 const onOpen=vi.fn();
 invoke.mockResolvedValue({path,name:source.title,body,hash:'same'});
 app=mount(CitationDocument,{target:document.body,props:{source,onOpen}});
 return onOpen;
}
it('HTML 全文保留样式和图片，定位到实际引用位置',async()=>{
 const paragraph='<p>引用的<strong>完整段落</strong>。</p>';
 const body='<style>p{color:red}</style><h1>文档标题</h1>'+paragraph+'<p>未引用的中间内容</p>'+paragraph+'<p>全文结尾</p><img src="data:image/png;base64,QUJD">';
 setup('/files/article.html',body,paragraph);
 await vi.waitFor(()=>expect(document.querySelector('iframe')).not.toBeNull());
 const frame=document.querySelector('iframe')!;
 const doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
 expect(doc.body.textContent).toContain('全文结尾');
 expect(doc.body.textContent).toContain('未引用的中间内容');
 expect([...doc.querySelectorAll('style')].some(style=>style.textContent==='p{color:red}')).toBe(true);
 expect(doc.querySelector('img')?.getAttribute('src')).toBe('data:image/png;base64,QUJD');
 expect(frame.getAttribute('sandbox')).toBe('allow-same-origin');
 const scroll=vi.fn();doc.querySelectorAll('.search-hit').forEach(mark=>Object.defineProperty(mark,'scrollIntoView',{value:scroll}));
 Object.defineProperty(frame,'contentDocument',{value:doc});frame.dispatchEvent(new Event('load'));await tick();
 expect(doc.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
 expect([...doc.querySelectorAll('.search-hit.active')].map(mark=>mark.textContent).join('')).toBe('引用的完整段落。');
 expect(scroll).toHaveBeenCalledWith({block:'center'});
});
it('Markdown 全文保留格式并定位到后一次相同的引用段落',async()=>{
 const paragraph='引用的 **完整段落**。';
 const onOpen=setup('/files/article.md','引言\n\n# 文档标题\n\n'+paragraph+'\n\n未引用的中间内容\n\n'+paragraph+'\n\n全文结尾',paragraph);
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
 expect(document.querySelector('h1')?.textContent).toBe('文档标题');
 expect(document.querySelector('.search-preview-body')?.textContent).toContain('全文结尾');
 expect(document.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
 expect(document.querySelector('strong .search-hit.active')?.textContent).toBe('完整段落');
 document.querySelector<HTMLButtonElement>('.panel-footer button')!.click();
 expect(onOpen).toHaveBeenCalledWith(['引用的 完整段落。']);
});
it.each([
 '<div>第一段</div><div>引用段落</div>',
 '<div>第一段</div><p>引用段落</p>',
])('HTML 引用按实际文本块高亮：%s',async body=>{
 setup('/files/blocks.html',body,body);
 await vi.waitFor(()=>expect(document.querySelector('iframe')).not.toBeNull());
 const doc=new DOMParser().parseFromString(document.querySelector('iframe')!.getAttribute('srcdoc')!,'text/html');
 expect([...doc.querySelectorAll('.search-hit')].map(mark=>mark.textContent)).toEqual(['第一段','引用段落']);
});
it('Markdown 引用从格式标记中间开始时高亮完整段落',async()=>{
 setup('/files/partial.md','引言\r\n\r\n**第一句。第二句。**\r\n\r\n文档结束','第二句。**');
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
 expect(document.querySelector('strong .search-hit.active')?.textContent).toBe('第一句。第二句。');
 expect(document.querySelector('.search-preview-body')?.textContent).toContain('文档结束');
});
it('Markdown 的多个引用范围分别保留段落边界',async()=>{
 const body='引言\n\n第一段。\n\n第二段。';
 setup('/files/ranges.md',body,'第一段。\n\n第二段。',['第一段。','第二段。'].map(text=>({from:body.indexOf(text),to:body.indexOf(text)+text.length})));
 await vi.waitFor(()=>expect(document.querySelectorAll('.search-hit')).toHaveLength(2));
 expect([...document.querySelectorAll('.search-hit')].map(mark=>mark.textContent)).toEqual(['第一段。','第二段。']);
});
it.each([
 '引言\n\n引用 [部署指南][ref]。\n\n[ref]: https://example.com',
 '[ref]: https://example.com\n\n引言\n\n引用 [部署指南][ref]。',
])('Markdown 引用保留块外链接定义：%s',async body=>{
 setup('/files/reference.md',body,'引用 [部署指南][ref]。');
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
 expect([...document.querySelectorAll('.search-hit.active')].map(mark=>mark.textContent).join('')).toBe('引用 部署指南。');
 expect(document.querySelector('a')?.getAttribute('href')).toBe('https://example.com');
});
it('重复的引用式链接段落定位到被引用的后一次',async()=>{
 const paragraph='引用 [部署指南][ref]。';
 setup('/files/reference.md','引言\n\n'+paragraph+'\n\n'+paragraph+'\n\n[ref]: https://example.com',paragraph);
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
 expect(document.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
});
it('全文读取失败时显示错误',async()=>{
 invoke.mockRejectedValue(new Error('文件已被移走'));
 app=mount(CitationDocument,{target:document.body,props:{source:{id:1,document_id:1,path:'/missing.md',title:'missing.md',content:'旧片段',heading:'',start_line:1,end_line:1,source_ranges:[],source_spans:[],bm25:0,score:0,order:0,hash:'same'},onOpen:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.form-error')?.textContent).toContain('文件已被移走'));
 expect(document.querySelector('.search-preview-body')).toBeNull();
});
