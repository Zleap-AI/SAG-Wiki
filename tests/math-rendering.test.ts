// @vitest-environment jsdom
import {expect,it,describe} from 'vitest';
import {renderMarkdown,renderAnswer} from '../src/lib/markdown';
import {splitMarkdown} from '../src/lib/markdown-chunks';
import {renderMathHtml} from '../src/lib/math';
import type {Candidate} from '../src/lib/types';

const B=String.fromCharCode(92);
const render=(text:string)=>new DOMParser().parseFromString(renderMarkdown(text),'text/html');
const candidate=(id:number):Candidate=>({id,document_id:id,path:`/doc${id}.md`,title:`doc${id}`,heading:'h',content:'c',start_line:1,end_line:1,source_ranges:[],bm25:1,score:1,order:id,hash:`h${id}`} as Candidate);

describe('inline math', () => {
 it('renders $…$ as KaTeX with the TeX kept on the wrapper', () => {
  const doc=render('The threshold is $V_{th} = V_{GS} - V_T$ for the device.');
  const inline=doc.querySelector('.math-inline');
  expect(inline).not.toBeNull();
  expect(inline!.getAttribute('data-tex')).toBe('V_{th} = V_{GS} - V_T');
  expect(inline!.querySelector('.katex')).not.toBeNull();
  expect(inline!.querySelector('math')).not.toBeNull();
  // The visible layer holds rendered glyphs; MathML keeps the TeX for readers.
  const visible=inline!.querySelector('.katex-html')!;
  expect(visible.textContent).not.toContain('{th}');
  expect(visible.textContent).not.toContain('$');
  expect(doc.querySelector('em')).toBeNull();
 });

 it(`renders the ${B}(…${B}) form that models emit`, () => {
  const doc=render(`Paren style ${B}(E = mc^2${B}) here.`);
  expect(doc.querySelector('.math-inline')?.getAttribute('data-tex')).toBe('E = mc^2');
 });

 it('keeps currency, prompt and code samples as text', () => {
  for (const text of ['It costs $5 and $10 in total.','Run $HOME/bin/tool now.','Inline `$x$` stays code.','```\n$x = 1$\n```']) {
   expect(render(text).querySelector('.math-inline'), text).toBeNull();
  }
 });

 it('keeps adjacent shell variables literal inside inline code', () => {
  const doc=render('Use `$HOME/$USER` for the directory.');
  expect(doc.querySelector('.math-inline')).toBeNull();
  expect(doc.querySelector('code')?.textContent).toBe('$HOME/$USER');
 });

 it('keeps page-footers and bare numbers as text', () => {
  for (const text of ['|Figure 3.1| The bonding wire connection. *$.04$','It costs $1,000$ per wafer.']) {
   expect(render(text).querySelector('.math-inline'), text).toBeNull();
  }
  // Real math keeps working, including single symbols and expressions.
  expect(render('$2+2$').querySelector('.math-inline')).not.toBeNull();
  expect(render('$x$').querySelector('.math-inline')).not.toBeNull();
 });

 it('keeps escaped dollars literal', () => {
  const doc=render(`A literal ${B}$x${B}$ stays text.`);
  expect(doc.querySelector('.math-inline')).toBeNull();
  expect(doc.body.textContent).toContain('$x$');
 });

 it.each([String.raw`_{2}^{3}He`,String.raw`^{14}C`,String.raw`x\_`])('renders valid scripts and escaped symbols: %s', tex => {
  const doc=render(`$${tex}$`);
  expect(doc.querySelector('.math-inline')?.getAttribute('data-tex')).toBe(tex);
  expect(doc.querySelector('.katex')).not.toBeNull();
  expect(doc.querySelector('.katex-error')).toBeNull();
 });
});

describe('display math', () => {
 it('keeps a $$ block with blank lines as one node', () => {
  const tex=`${B}begin{aligned}\nx &= 1\n\ny &= 2\n${B}end{aligned}`;
  const doc=render(`$$\n${tex}\n$$`);
  const blocks=doc.querySelectorAll('.math-block');
  expect(blocks).toHaveLength(1);
  expect(blocks[0].getAttribute('data-tex')).toBe(tex);
  expect(blocks[0].querySelector('.katex-display')).not.toBeNull();
  expect(blocks[0].querySelector('.katex-error')).toBeNull();
 });

 it(`renders the ${B}[…${B}] form and same-line $$…$$`, () => {
  expect(render(`${B}[x^2${B}]`).querySelector('.math-block')).not.toBeNull();
  expect(render('$$x^2$$').querySelector('.math-block')).not.toBeNull();
 });

 it(`keeps escaped citation brackets as text (${B}[1${B}], ${B}[2,3${B}])`, () => {
  expect(render(`${B}[1${B}]`).querySelector('.math-block')).toBeNull();
  expect(render(`${B}[2,3${B}]`).querySelector('.math-block')).toBeNull();
  expect(render(`${B}[1${B}]`).body.textContent).toContain('[1]');
 });

 it('keeps an unclosed $$ as text while an answer streams', () => {
  const doc=render('The cost is $$x = 1 and the rest');
  expect(doc.querySelector('.math-block')).toBeNull();
  expect(doc.body.textContent).toContain('$$x = 1');
 });

 it('keeps invalid TeX visible instead of throwing', () => {
  const doc=render(`$$${B}frac{1}{$$`);
  const block=doc.querySelector('.math-block');
  expect(block).not.toBeNull();
  expect(block!.getAttribute('data-tex')).toBe(`${B}frac{1}{`);
  expect(block!.querySelector('.katex-error')).not.toBeNull();
 });

 it('never renders links from TeX (trust disabled)', () => {
  const html=renderMathHtml(B+'href{javascript:alert(1)}{click}',false);
  // The TeX source may appear as annotation text, but never as a link target.
  expect(html).not.toMatch(/href\s*=\s*"javascript:/);
  expect(html).not.toContain('<a ');
  // KaTeX shows the rejected command instead of executing it.
  expect(html).toContain('mtext');
 });
});

describe('answers', () => {
 it('cites text but never inside formulas', () => {
  const html=renderAnswer(`Current is $I_D = ${B}text{see [1]}$ and the fact [1].`,[candidate(1)]).html;
  const doc=new DOMParser().parseFromString(html,'text/html');
  expect(doc.querySelectorAll('button.citation')).toHaveLength(1);
  expect(doc.querySelector('.math-inline')).not.toBeNull();
 });
});

describe('converted document runs', () => {
 it('keeps sub/sup runs instead of dropping or code-fencing them', () => {
  const inline=render('*e <sup>jωt</sup>dt* and V<sub>in</sub>');
  expect(inline.querySelector('sup')?.textContent).toBe('jωt');
  expect(inline.querySelector('sub')?.textContent).toBe('in');
  const block=render('<sup>j</sup> <u>C<sub>I</sub></u> (25.27)');
  expect(block.querySelector('pre')).toBeNull();
  expect(block.querySelector('sup')?.textContent).toBe('j');
  expect(block.querySelector('sub')?.textContent).toBe('I');
 });

 it('keeps an orphan script fragment literal', () => {
  expect(render('V$_{0}$ ¼ A sin ω t').querySelector('.math-inline')).toBeNull();
 });

 it.each(['_{0}','^2','_0^{2}'])('keeps script-only fragments literal: %s', tex => {
  const doc=render(`V$${tex}$`);
  expect(doc.querySelector('.math-inline')).toBeNull();
  expect(doc.body.textContent).toBe(`V$${tex}$`);
 });
});

describe('streaming chunks', () => {
 it('never splits a display formula across render pieces', () => {
  for (const [open,close] of [['$$','$$'],[`${B}[`,`${B}]`]]) {
   const text=`intro\n\n${open}\na = 1\n\nb = 2\n${close}\n\noutro`;
   const chunks=splitMarkdown(text,1);
   const holder=chunks.find(chunk=>text.slice(chunk.from,chunk.to).includes('a = 1'));
   expect(holder, text).toBeDefined();
   const piece=text.slice(holder!.from,holder!.to);
   expect(piece).toContain('b = 2');
  }
 });
});
