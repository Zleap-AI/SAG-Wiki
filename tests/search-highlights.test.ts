// @vitest-environment happy-dom
import{describe,it,expect}from 'vitest';
import{textMatches,highlightedHtml,highlightedText,searchSnippet,editorMatches}from '../src/lib/search-highlights';
import{readonlyHtml}from '../src/lib/domain';
import{getSchema}from '@tiptap/core';import StarterKit from '@tiptap/starter-kit';
describe('搜索高亮和位置',()=>{
 it('RTF 正文预览能高亮准确的位置',()=>{
  const result=highlightedHtml(readonlyHtml('/notes/a.rtf','星河\n验证码 LYRA-73492。'),['LYRA-73492']);
  const doc=new DOMParser().parseFromString(result.html,'text/html');
  expect(result.count).toBe(1);expect(doc.querySelector('mark')?.textContent).toBe('LYRA-73492');expect(doc.querySelector('pre')?.textContent).toBe('星河\n验证码 LYRA-73492。');
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
