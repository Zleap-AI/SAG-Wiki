import {describe,it,expect}from 'vitest';import{inside,normalizeScope,fileName,documentResults,sourceHtml}from '../src/lib/domain';import type{Candidate}from '../src/lib/types';
describe('范围和文件操作',()=>{
 it('按目录组件比较，不混入相似名称',()=>{expect(inside('/notes/one.md','/notes')).toBe(true);expect(inside('/notes2/one.md','/notes')).toBe(false);expect(inside('C:\\notes\\one.md','C:\\notes')).toBe(true)});
 it('父目录覆盖子目录并去重',()=>expect(normalizeScope(['/a','/a/b','/a','/x'])).toEqual(['/a','/x']));
 it('禁止越界和 Windows 保留文件名',()=>{expect(fileName('笔记')).toBe('笔记.md');expect(()=>fileName('../x')).toThrow();expect(()=>fileName('CON.md')).toThrow()});
 it('引用中的不可信 HTML 不执行',()=>{expect(sourceHtml('<script>x</script> [1]',[{id:1}as Candidate])).toContain('&lt;script&gt;');expect(sourceHtml('[2]',[{id:1}as Candidate])).toBe('[2]')});
 it('同一文件命中合并但保留片段',()=>{const a={path:'/a',id:1}as Candidate,b={path:'/a',id:2}as Candidate;expect(documentResults([a,b])[0].matches).toHaveLength(2)});
});
import{protectHtml,restoreHtml,splitFrontmatter}from '../src/lib/markdown-source';
import{compactNumber,durationLabel,readonlyHtml,sourceLocation}from '../src/lib/domain';
describe('回答统计格式',()=>{
 it.each([[523,'523'],[12002,'12K'],[1187,'1.2K'],[999950,'1M'],[1500000,'1.5M'],[2100000000,'2.1B'],[23.46,'23.5']])('紧凑显示 %s', (value,label)=>expect(compactNumber(value as number)).toBe(label));
 it.each([[15320,'15.3s'],[60000,'1m 0s'],[119950,'2m 0s']])('显示耗时 %s',(value,label)=>expect(durationLabel(value as number)).toBe(label));
});
describe('Markdown 原始内容',()=>{
 it('保留 YAML 元数据',()=>{const d=splitFrontmatter('---\ntitle: Test\n---\n# 正文');expect(d.prefix).toBe('---\ntitle: Test\n---\n');expect(d.body).toBe('# 正文')});
 it('HTML 往返不丢内容',()=>{const source='<p align="center">\n<img src="logo.png"/>\n</p>\n\n# Title';expect(restoreHtml(protectHtml(source))).toBe(source)});
 it('不改写代码块中的 HTML',()=>{const source='```html\n<div>Example</div>\n```';expect(protectHtml(source)).toBe(source)});
});

describe('RTF 正文与引用',()=>{
 it('正文保留换行并转义 HTML',()=>{expect(readonlyHtml('/notes/a.RTF','星河\n<正文>')).toBe('<pre class="rtf-preview">星河\n&lt;正文&gt;</pre>')});
 it('引用位置明确对应 RTF 正文',()=>{const source={path:'/notes/a.rtf',start_line:2,end_line:2,source_spans:[{start_byte:4,end_byte:10,start_line:2,start_column:1,end_line:2,end_column:3}]} as Candidate;expect(sourceLocation(source)).toBe('RTF 正文 · 第 2 行 1 列 → 第 2 行 3 列')});
});
