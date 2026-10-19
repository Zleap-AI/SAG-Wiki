// @vitest-environment happy-dom
import {expect,it} from 'vitest';
import {renderAnswer} from '../src/lib/markdown';
import type {Candidate} from '../src/lib/types';

const source:Candidate={id:1,document_id:1,path:'/notes/a.md',title:'a.md',heading:'',content:'原文',start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:0,score:1,order:0,hash:'a'};
const evidence=[source,{...source,id:2,path:'/notes/b.md',title:'b.md',hash:'b'},{...source,id:3,path:'/archive/a.md'}, {...source,id:4,path:'/notes/unused.md',title:'unused.md',hash:'unused'}];

it('numbers only cited documents in first-appearance order and preserves paragraph targets',()=>{
 const result=renderAnswer('先引用 [3]，再引用 [2]，同一文件 [1]，重复 [3]。',evidence);
 const doc=new DOMParser().parseFromString(result.html,'text/html');
 expect(result.sources.map(item=>[item.number,item.first.path])).toEqual([[1,'/archive/a.md'],[2,'/notes/b.md']]);
 expect([...doc.querySelectorAll('.citation')].map(button=>[button.textContent,button.getAttribute('data-citation')])).toEqual([['1','2'],['2','1'],['1','0'],['1','2']]);
});

it('does not turn examples, code or unavailable source numbers into citations',()=>{
 const result=renderAnswer('`[2]`\n\n```text\n[3]\n```\n\n正文 [4]，无效编号 [0] [99]。',evidence);
 const doc=new DOMParser().parseFromString(result.html,'text/html');
 expect(result.sources.map(item=>item.first.path)).toEqual(['/notes/unused.md']);
 expect(doc.querySelectorAll('.citation')).toHaveLength(1);
 expect(doc.querySelector('.citation')!.textContent).toBe('1');
 expect(doc.querySelector('code')!.textContent).toBe('[2]');
 expect(doc.body.textContent).toContain('[99]');
});

it('leaves the source list empty when the answer does not cite any evidence',()=>{
 expect(renderAnswer('没有足够证据。',evidence).sources).toEqual([]);
});

it('keeps citation numbering across text separated by multiple tool calls',()=>{
 const first='先查🙂。';
 const middle='**第一条证据** [2]。';
 const last='第二条证据 [1]，再次引用 [2]。';
 const result=renderAnswer(first+middle+last,evidence,[
  {id:'first',query:'查询一',source_count:1,answer_offset:first.length},
  {id:'second',query:'查询二',source_count:1,answer_offset:first.length+middle.length},
 ]);
 const parse=(html:string)=>new DOMParser().parseFromString(html,'text/html');
 expect(parse(result.events[0].html).body.textContent).toBe(first);
 expect(parse(result.events[1].html).querySelector('strong')!.textContent).toBe('第一条证据');
 expect(parse(result.events[1].html).querySelector('.citation')!.textContent).toBe('1');
 expect([...parse(result.html).querySelectorAll('.citation')].map(node=>node.textContent)).toEqual(['2','1']);
 expect(result.sources.map(item=>item.first.path)).toEqual(['/notes/b.md','/notes/a.md']);
});

it('resolves Markdown links defined after a tool call',()=>{
 const before='请查看 [资料][ref]。\n\n';
 const result=renderAnswer(before+'检索完成。\n\n[ref]: https://example.com/guide',[],[
  {id:'search',query:'检索资料',source_count:0,answer_offset:before.length},
 ]);
 const doc=new DOMParser().parseFromString(result.events[0].html,'text/html');
 expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://example.com/guide');
 expect(doc.querySelector('a')?.textContent).toBe('资料');
});
