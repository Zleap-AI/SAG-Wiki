// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import {Editor} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {Markdown} from '@tiptap/markdown';
import {renderMarkdown,renderAnswer} from '../src/lib/markdown';
import {MermaidCodeBlock,mermaidPreview} from '../src/lib/mermaid';
import RichEditor from '../src/components/RichEditor.svelte';
import HighlightedDocument from '../src/components/HighlightedDocument.svelte';
import {answerHtml} from '../src/lib/answer-html';
import {setLanguage} from '../src/lib/i18n';

Element.prototype.scrollIntoView=()=>{};
beforeEach(()=>vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}}));

const flow='flowchart LR\n  A[开始] --> B[完成]';
const markdown=(source:string,language='mermaid')=>'```'+language+'\n'+source+'\n```';
const parse=(html:string)=>new DOMParser().parseFromString(html,'text/html');
afterEach(()=>{document.body.innerHTML='';vi.unstubAllGlobals()});

it.each([
 flow,
 'sequenceDiagram\n  Alice->>Bob: 你好\n  Bob-->>Alice: 收到',
 'classDiagram\n  Animal <|-- Duck',
 'stateDiagram-v2\n  [*] --> Ready\n  Ready --> [*]',
 'erDiagram\n  CUSTOMER ||--o{ ORDER : places',
 'pie title 分类\n  "文档" : 60\n  "代码" : 40',
])('renders Mermaid diagrams with the real Pele renderer: %s',source=>{
 const doc=parse(renderMarkdown(markdown(source)));
 expect(doc.querySelector('.mermaid-diagram svg'),doc.body.innerHTML).not.toBeNull();
 expect(doc.querySelector('.mermaid-error')).toBeNull();
 expect(doc.querySelector('pre')).toBeNull();
});

it('leaves ordinary code and inline Mermaid text unchanged',()=>{
 const doc=parse(renderMarkdown(markdown('const mermaid = 1;','js')+'\n\n`mermaid`'));
 expect(doc.querySelector('pre code')!.textContent).toBe('const mermaid = 1;');
 expect(doc.querySelector('.mermaid-diagram')).toBeNull();
 expect(parse(renderMarkdown(markdown(flow,'MERMAID'))).querySelector('svg')).not.toBeNull();
});

it('keeps invalid source visible and recovers when streaming input becomes valid',()=>{
 const host=document.body.appendChild(document.createElement('div'));
 const action=answerHtml(host,renderMarkdown(markdown('flowchart LR\n  A[')));
 expect(host.querySelector('pre')?.textContent).toContain('A[');
 expect(host.querySelector('.mermaid-error')?.textContent).toContain('Mermaid 图表无法渲染');
 action.update(renderMarkdown(markdown(flow)));
 expect(host.querySelector('.mermaid-error')).toBeNull();
 expect(host.querySelector('svg')?.textContent,host.innerHTML).toContain('完成');
});

it('handles source limits and localizes errors',()=>{
 setLanguage('en');
 const preview=mermaidPreview('x'.repeat(50001));
 expect(preview.textContent).toContain('Unable to render Mermaid diagram');
 expect(preview.textContent).toContain('50000');
});

it('keeps SVG IDs distinct when multiple diagrams are displayed',()=>{
 const source='sequenceDiagram\n  accTitle: 示例\n  Alice->>Bob: Hello';
 const doc=parse(renderMarkdown(markdown(source)+'\n\n'+markdown(source))+renderMarkdown(markdown(source)));
 const ids=[...doc.querySelectorAll('[id]')].map(node=>node.id);
 expect(ids.length).toBeGreaterThan(0);
 expect(new Set(ids).size).toBe(ids.length);
 for(const node of doc.querySelectorAll('[aria-labelledby]'))expect(doc.getElementById(node.getAttribute('aria-labelledby')!)).not.toBeNull();
});

it('does not interpret diagram labels or errors as citation numbers',()=>{
 const evidence=[{id:1,document_id:1,path:'/notes.md',title:'notes',heading:'',content:'内容',start_line:1,end_line:1,source_ranges:[],source_spans:[],bm25:0,score:1,order:0,hash:'a'}];
 const result=renderAnswer(markdown('flowchart LR\n  A["资料 [1]"] --> B[完成]')+'\n\n见原文 [1]。',evidence);
 const doc=parse(result.html);
 expect(doc.querySelector('svg')?.textContent).toContain('[1]');
 expect(doc.querySelectorAll('.citation')).toHaveLength(1);
 expect(doc.querySelector('svg .citation')).toBeNull();
});

it('does not create executable markup or external requests from diagrams',()=>{
 const source='flowchart LR\n  A["<img src=x onerror=alert(1)>"] --> B[完成]\n  click B "https://example.com"';
 const doc=parse(renderMarkdown(markdown(source)));
 expect(doc.querySelector('svg')).not.toBeNull();
 expect(doc.querySelector('script,img,image,a,[onerror],[onclick]')).toBeNull();
 const image=mermaidPreview('flowchart LR\n  A@{ img: "https://example.com/image.png", label: "图片" }');
 expect(image.querySelector('svg')).not.toBeNull();
 expect(image.querySelector('image')).toBeNull();
});

it('updates the editable diagram, preserves Markdown source and supports undo',()=>{
 const host=document.body.appendChild(document.createElement('div'));
 const editor=new Editor({element:host,extensions:[StarterKit.configure({codeBlock:false}),MermaidCodeBlock,Markdown],content:markdown(flow),contentType:'markdown'});
 try{
  expect(host.querySelector('pre code')?.textContent).toBe(flow);
  expect(host.querySelector('pre')!.hidden).toBe(true);
  expect(host.querySelector('svg')?.textContent,host.innerHTML).toContain('完成');
  const start=flow.indexOf('完成')+1;
  editor.commands.insertContentAt({from:start,to:start+2},'成功');
  expect(host.querySelector('svg')?.textContent).toContain('成功');
  expect(editor.getMarkdown()).toContain(markdown(flow.replace('完成','成功')));
  expect(editor.getMarkdown()).not.toContain('<svg');
  editor.commands.undo();
  expect(host.querySelector('svg')?.textContent,host.innerHTML).toContain('完成');
  editor.commands.setNodeSelection(0);
  editor.commands.updateAttributes('codeBlock',{language:'js'});
  expect(host.querySelector('svg')).toBeNull();
  expect(host.querySelector('pre code')?.textContent).toBe(flow);
 }finally{editor.destroy()}
});

it('switches between the SVG and source without changing the saved Markdown',async()=>{
 const onChange=vi.fn();
 const app=mount(RichEditor,{target:document.body,props:{body:markdown(flow),path:'/notes.md',onChange}});
 try{
  await tick();
  const block=document.querySelector('.mermaid-code-block')!;
  const pre=block.querySelector('pre')!;
  const preview=block.querySelector<HTMLDivElement>('.mermaid-diagram')!;
  expect(pre.hidden).toBe(true);expect(preview.hidden).toBe(false);
  expect(block.getAttribute('contenteditable')).toBe('false');
  setLanguage('en');await tick();
  expect(block.querySelector('button')!.textContent).toBe('Edit');
  setLanguage('zh-CN');await tick();
  block.querySelector<HTMLButtonElement>('button')!.click();await tick();
  expect(pre.hidden).toBe(false);expect(preview.hidden).toBe(true);
  expect(block.querySelector('button')!.textContent).toBe('预览');
  block.querySelector<HTMLButtonElement>('button')!.click();await tick();
  expect(pre.hidden).toBe(true);expect(preview.hidden).toBe(false);
  expect(onChange).not.toHaveBeenCalled();
  expect(document.querySelector<HTMLButtonElement>('[aria-label="撤销"]')!.disabled).toBe(true);
 }finally{await unmount(app)}
});

it('opens the real editor with search highlights without changing the document',async()=>{
 const onChange=vi.fn();
 const app=mount(RichEditor,{target:document.body,props:{body:markdown(flow),path:'/notes.md',searchTerms:['完成'],onChange}});
 try{
  await tick();
  expect(document.querySelector('.mermaid-code-block svg')?.textContent).toContain('完成');
  expect(document.querySelector('pre .search-hit')?.textContent).toBe('完成');
  expect(onChange).not.toHaveBeenCalled();
  expect(document.querySelector<HTMLButtonElement>('[aria-label="撤销"]')!.disabled).toBe(true);
 }finally{await unmount(app)}
});

it('renders document previews with surrounding search highlights',async()=>{
 const app=mount(HighlightedDocument,{target:document.body,props:{html:renderMarkdown('说明文字\n\n'+markdown(flow)),terms:['说明']}});
 try{
  await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')).not.toBeNull());
  expect(document.querySelector('.search-preview-body svg')?.textContent).toContain('完成');
  expect(document.querySelector('.search-hit.active')?.textContent).toBe('说明');
 }finally{await unmount(app)}
});
