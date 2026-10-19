// @vitest-environment happy-dom
import{describe,it,expect}from 'vitest';
import{textMatches,highlightedHtml,highlightedText,searchSnippet,searchText,editorMatches,documentMatchIndex}from '../src/lib/search-highlights';
import{readonlyHtml}from '../src/lib/domain';
import{getSchema}from '@tiptap/core';import StarterKit from '@tiptap/starter-kit';
describe('搜索高亮和位置',()=>{
 it.each([
  ['/notes/a.txt','早先 Token。\n\n目标 Token。','目标'],
  ['/notes/a.md','---\ntitle: Token\n---\n\n# 早先\n\nToken。\n\n# 目标\n\nToken。','# 目标'],
  ['/notes/a.html','<style>Token</style><p>早先 Token</p><svg><text>目标 Token</text></svg>','<svg>'],
 ])('按 UTF-8 原文位置定位 %s 的匹配',async(path,body,anchor)=>{
  const prefix=body.slice(0,body.indexOf(anchor)),start=new TextEncoder().encode(prefix).length;
  const candidate={id:1,document_id:1,path,title:'a',heading:'',content:body.slice(prefix.length),start_line:1,end_line:1,source_ranges:[],source_spans:[{start_byte:start,end_byte:new TextEncoder().encode(body).length,start_line:1,end_line:1,start_column:1,end_column:1}],bm25:0,score:0,order:0,hash:'same'};
  expect(await documentMatchIndex(body,path,['Token'],candidate)).toBe(1);
 });
 it('HTML 片段只保留可见文字，不显示图片编码、脚本和样式',()=>{
  const html='<h1>Token 路由器</h1><style>.noise{color:red}</style><script>noise()</script><img src="data:image/png;base64,'+'ABCD'.repeat(1_500_000)+'"><p>结束正文</p>';
  const text=searchText('/notes/product.HTM',html);
  expect(text).toContain('Token 路由器');expect(text).toContain('结束正文');
  expect(text).not.toContain('ABCD');expect(text).not.toContain('noise');expect(text.length).toBeLessThan(100);
  expect(searchText('/notes/product.html','@page{size:960px 540px;margin:0}}</style><p>正文</p>')).toBe('正文');
 });
 it('RTF 正文预览能高亮准确的位置',()=>{
  const result=highlightedHtml(readonlyHtml('/notes/a.rtf','星河\n验证码 LYRA-73492。'),['LYRA-73492']);
  const doc=new DOMParser().parseFromString(result.html,'text/html');
  expect(result.count).toBe(1);expect(doc.querySelector('mark')?.textContent).toBe('LYRA-73492');expect(doc.querySelector('pre')?.textContent).toBe('星河\n验证码 LYRA-73492。');
 });
 it('图片编码不参与高亮，原始图片地址保持完整',()=>{
  const image='data:image/png;base64,imagepayloadmarker';
  const html=`<p>beforemarker 正文</p><img src="${image}" alt="图片说明"><p>aftermarker 正文</p>`;
  const result=highlightedHtml(html,['imagepayloadmarker','aftermarker']);
  const doc=new DOMParser().parseFromString(result.html,'text/html');
  expect(result.count).toBe(1);
  expect(doc.querySelector('mark')?.textContent).toBe('aftermarker');
  expect(doc.querySelector('img')?.getAttribute('src')).toBe(image);
  expect(searchText('/notes/images.html',html)).not.toContain('imagepayloadmarker');
 });
 it('SVG 图形保留显示，但不参与正文搜索和高亮',()=>{
  const html='<p>正文marker</p><svg><text>svgmarker</text><path d="M0 0L10 10"/></svg>';
  const result=highlightedHtml(html,['正文marker','svgmarker']);
  const doc=new DOMParser().parseFromString(result.html,'text/html');
  expect(result.count).toBe(1);
  expect(doc.querySelector('svg text')?.textContent).toBe('svgmarker');
  expect(doc.querySelector('svg .search-hit')).toBeNull();
  expect(searchText('/notes/page.html',html)).not.toContain('svgmarker');
 });
 it('忽略大小写并保留中文、emoji 的原文位置',()=>{
  const text='😀 OCTX 中文 octx';const matches=textMatches(text,['octx','中文']);
  expect(matches.map(m=>text.slice(m.from,m.to))).toEqual(['OCTX','中文','octx']);
 });
 it('与索引一致地识别全角字符和组合字符',()=>{
  const text='ＯＣＴＸ e\u0301';expect(textMatches(text,['octx','é']).map(m=>text.slice(m.from,m.to))).toEqual(['ＯＣＴＸ','e\u0301']);
 });
 it('按字面匹配符号，重叠词不重复计数',()=>{
  expect(textMatches('C++ octx',['C++','octx','oct'])).toEqual([{from:0,to:3},{from:4,to:8}]);
  expect(textMatches('octx',[])).toEqual([]);
 });
 it('摘要显示远离片段开头的匹配',()=>{
  const html=searchSnippet('开头'.repeat(200)+'OCTX 文档',['octx']);
  expect(html).toContain('<mark class="search-hit">OCTX</mark>');expect(html.startsWith('…')).toBe(true);
 });
 it('保留原文及 HTML 转义，不把标记写入内容',()=>{
  expect(highlightedText('<octx>',['octx'])).toBe('&lt;<mark class="search-hit">octx</mark>&gt;');
 });
 it('跨行内格式匹配，保留链接并忽略属性值',()=>{
  const result=highlightedHtml('<p>O<strong>CT</strong>X <a href="/octx">其他</a></p><pre>octx()</pre>',['octx']);
  const doc=new DOMParser().parseFromString(result.html,'text/html');
  expect(result.count).toBe(2);expect(doc.querySelector('p')!.textContent).toBe('OCTX 其他');
  expect(doc.querySelectorAll('[data-search-match="0"]')).toHaveLength(3);
  expect(doc.querySelector('a')!.getAttribute('href')).toBe('/octx');expect(doc.querySelector('a mark')).toBeNull();
  expect(doc.querySelector('pre mark')!.textContent).toBe('octx');
 });
 it('不同段落之间不形成虚假的连续匹配',()=>{
  expect(highlightedHtml('<p>oc</p><p>tx</p>',['octx']).count).toBe(0);
 });
 it('编辑器位置跨加粗文本仍准确，不改变文档',()=>{
  const schema=getSchema([StarterKit]);const doc=schema.nodeFromJSON({type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'O'},{type:'text',text:'CT',marks:[{type:'bold'}]},{type:'text',text:'X'}]}]});
  const before=doc.toJSON();const matches=editorMatches(doc,['octx']);
  expect(matches).toEqual([{from:1,to:5}]);expect(doc.textBetween(matches[0].from,matches[0].to)).toBe('OCTX');expect(doc.toJSON()).toEqual(before);
 });
});
