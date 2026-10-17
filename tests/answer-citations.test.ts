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
