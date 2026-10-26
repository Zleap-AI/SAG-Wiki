// @vitest-environment happy-dom
import {expect,it} from 'vitest';
import {generateHTML} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {MarkdownManager} from '@tiptap/markdown';
import {TableKit} from '@tiptap/extension-table';
import {TaskList,TaskItem} from '@tiptap/extension-list';
import {LocalImage,markdownCitation,markdownLinks,renderMarkdown,renderMarkdownPiece,finishHtml} from '../src/lib/markdown';
import {chunkAt,splitMarkdown} from '../src/lib/markdown-chunks';
import {protectHtml,splitFrontmatter} from '../src/lib/markdown-source';
import {fixture} from '../bench/fixtures';

const tricky=[
 '---\ntitle: 前言\n---\n# 标题\n\n段落一，引用[链接][late]。\n\n- 宽松列表\n\n- 第二项\n\n  续段落\n\n段落二\n',
 '1. 一\n\n2. 二\n\n10) 十\n\n普通段落\n\n```js\nconst a = 1;\n\nconst b = 2;\n```\n\n~~~\n\n```\n\n~~~\n\n结束\n',
 '    缩进代码\n\n    仍是代码\n\n> 引用\n\n> 另一个引用\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n- [ ] 待办\n- [x] 完成\n\n后文\n',
 '<div>\n\n<p>html</p>\n</div>\n\n文本\n\n<!--\n\n注释\n\n-->\n\n标题\n===\n\n***\n\n```mermaid\nflowchart LR\n  A-->B\n```\n\n[late]: https://example.com\n',
 '多个空行\n\n\n标题后\n\n  \n\n空白行之后\n\n\n\n\n尾部空行\n\n',
 '段落\r\n\r\n- 列表\r\n  1. 嵌套\r\n\r\n     继续\r\n\r\n另一段\r\n',
];

function wholeHtml(markdown:string,extensions:unknown[]){
 const parser=new MarkdownManager({extensions:extensions as never});
 return finishHtml(generateHTML(parser.parse(protectHtml(splitFrontmatter(markdown).body)),extensions as never));
}

const originalExtensions=[StarterKit,TableKit,LocalImage,TaskList,TaskItem.configure({nested:true})];

it('CRLF 文档仍按段落切块，并保留原文偏移和整篇渲染结果',()=>{
 const short='one\r\n\r\ntwo\r\n\r\nthree';
 expect(splitMarkdown(short,1)).toEqual([{from:0,to:7},{from:7,to:14},{from:14,to:19}]);
 const body=('paragraph '.repeat(40)+'\r\n\r\n').repeat(30),pieces=splitMarkdown(body);
 expect(pieces.length).toBeGreaterThan(2);
 expect(pieces.map(piece=>body.slice(piece.from,piece.to)).join('')).toBe(body);
 expect(renderMarkdown(body)).toBe(wholeHtml(body,originalExtensions));
});

it.each(['```','~~~'])('带正文的 %s 行不能关闭代码围栏',fence=>{
 const code='inside\n'.repeat(600)+`${fence}not-a-close\n\n# still code`;
 const body=`${fence}text\n${code}\n${fence}\n`;
 const html=renderMarkdown(body),doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('code')?.textContent).toBe(code);
 expect(doc.querySelector('h1')).toBeNull();
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it.each(['```','~~~'])('尾随制表符的 %s 行与整篇解析一样保持围栏打开',fence=>{
 const code='inside\n'.repeat(600)+`${fence}\t\n\n# still code`;
 const body=`${fence}text\n${code}\n${fence}\n`;
 const html=renderMarkdown(body),doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('code')?.textContent).toBe(code);
 expect(doc.querySelector('h1')).toBeNull();
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it.each(['> [ref]: https://first.example\n\n','- item\n\n  [ref]: https://first.example\n\n','- [ ] task\n\n  [ref]: https://first.example\n\n'])('保留嵌套定义的跨块链接及先出现优先：%s',definition=>{
 const body=definition+'padding line\n\n'.repeat(400)+'[ref]: https://later.example\n\n[title][ref]\n';
 const html=renderMarkdown(body),doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://first.example');
 expect(doc.querySelector('a')?.textContent).toBe('title');
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it.each(['constructor','__proto__'])('跨块引用标签 %s 不受对象原型属性影响',tag=>{
 const body=`[title][${tag}]\n\n`+'padding line\n\n'.repeat(400)+`[${tag}]: https://example.com\n`;
 const html=renderMarkdown(body),doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://example.com');
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it.each(['\n','\r\n'])('HTML 原文保护后的引用定义跨块保持一致（换行 %j）',newline=>{
 const body=('[dup]: https://first.example\n\n[dup]: https://ignored.example\n\n<div>\n[ref]: https://inside.example\n</div>\n\n'+'padding line\n\n'.repeat(400)+'[ref]: https://later.example\n\n[title][ref]\n').replaceAll('\n',newline);
 expect(markdownLinks(body).find(link=>link.tag==='ref')?.offset).toBe(body.indexOf('<div>'));
 const html=renderMarkdown(body),doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://inside.example');
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it('CRLF 的嵌套定义与重复定义按原文块位置播种',()=>{
 const body='[dup]: https://first.example\r\n\r\n[dup]: https://ignored.example\r\n\r\n> [nested]: https://nested.example\r\n\r\n[title][nested] and [first][dup]\r\n\r\n[dup]: https://last.example\r\n';
 const links=markdownLinks(body),nested=links.find(link=>link.tag==='nested');
 expect(nested?.offset).toBe(body.indexOf('> [nested]'));
 const pieces=splitMarkdown(body,1);
 const html=finishHtml(pieces.map((piece,index)=>renderMarkdownPiece(body.slice(piece.from,piece.to),links,{offset:piece.from,last:index===pieces.length-1})).join(''));
 const doc=new DOMParser().parseFromString(html,'text/html');
 expect([...doc.querySelectorAll('a')].map(link=>link.getAttribute('href'))).toEqual(['https://nested.example','https://first.example']);
 expect(html).toBe(wholeHtml(body,originalExtensions));
});

it('切块渲染与整篇渲染结果一致',()=>{
 const documents=[...tricky,fixture(150_000).body];
 for(const markdown of documents){
  const content=splitFrontmatter(markdown).body,pieces=splitMarkdown(content,1);
  expect(pieces.map(piece=>content.slice(piece.from,piece.to)).join('')).toBe(content);
  const links=markdownLinks(markdown);
  const chunked=finishHtml(pieces.map((piece,index)=>renderMarkdownPiece(content.slice(piece.from,piece.to),links,{offset:piece.from,last:index===pieces.length-1})).join(''));
  expect(chunked).toBe(renderMarkdown(markdown));
  expect(chunked).toBe(wholeHtml(markdown,[StarterKit,TableKit,LocalImage,TaskList,TaskItem.configure({nested:true})]));
 }
});

it('只在空行后、零缩进且不是列表项的位置切块',()=>{
 const content='a\n\n- b\n\n  c\n\n1. d\n\ne\n\n```\nf\n\ng\n```\n\nh\n';
 const starts=splitMarkdown(content,1).map(piece=>content.slice(piece.from).split('\n')[0]);
 expect(starts).toEqual(['a','e','```','h']);
 expect(splitMarkdown('a\n\n\nb\n\n  \nc\n',1)).toHaveLength(1);
 const pieces=splitMarkdown(content,1);
 expect(chunkAt(pieces,0)).toBe(0);
 expect(chunkAt(pieces,content.indexOf('h'))).toBe(3);
 expect(chunkAt(pieces,content.length)).toBe(3);
});

it('引用片段只取被引用的块，并保留全文链接定义',()=>{
 const body=fixture(60_000).body,from=body.indexOf('\n\n',body.length/2)+2,to=body.indexOf('\n\n',from);
 const citation=markdownCitation(body,[{from,to}]);
 expect(citation).toContain(body.slice(from,to));
 expect(citation).toContain('[ref0]: https://example.com/chapter-0');
 expect(citation.length).toBeLessThan(body.length/10);
});

it('大文档渲染保持线性耗时',()=>{
 const small=fixture(100_000).body,large=fixture(400_000).body;
 renderMarkdown(small);
 let start=performance.now();renderMarkdown(small);const smallTime=performance.now()-start;
 start=performance.now();renderMarkdown(large);const largeTime=performance.now()-start;
 expect(largeTime/smallTime).toBeLessThan(8);
});
