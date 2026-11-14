// Math rendering for answers, reader previews and search summaries.
//
// Formulas are kept as TeX on the node (`data-tex`) and rendered with KaTeX at
// render time. `htmlAndMathml` output keeps the visual layer identical across
// WebViews and gives screen readers the MathML layer. Invalid TeX never throws:
// KaTeX renders its own `.katex-error` box, and the source stays selectable.
import katex from 'katex';
import {Node} from '@tiptap/core';
import type {MarkdownToken, MarkdownTokenizer} from '@tiptap/core';
import type {DOMOutputSpec} from '@tiptap/pm/model';
import 'katex/dist/katex.min.css';
import './math.css';

// One answer can stream hundreds of distinct formulas; cap the cache so a long
// session cannot keep every rendered formula alive.
const RENDER_CACHE_LIMIT=256;
const rendered=new Map<string,string>();

export function renderMathHtml(tex:string,display:boolean):string{
 const key=(display?'b':'i')+tex;
 const cached=rendered.get(key);
 if(cached!==undefined)return cached;
 const html=katex.renderToString(tex,{displayMode:display,output:'htmlAndMathml',throwOnError:false,strict:'ignore',trust:false});
 if(rendered.size>=RENDER_CACHE_LIMIT)rendered.delete(rendered.keys().next().value!);
 rendered.set(key,html);
 return html;
}

function mathElement(tex:string,display:boolean):HTMLElement{
 // A block wrapper keeps KaTeX's own display spacing instead of inheriting none.
 const element=document.createElement(display?'div':'span');
 element.className=display?'math-block':'math-inline';
 element.dataset.tex=tex;
 element.setAttribute('role','math');
 const output=document.createElement('span');
 output.className='math-output';
 output.innerHTML=renderMathHtml(tex,display);
 element.append(output);
 return element;
}

function mathToken(type:string,raw:string,tex:string):MarkdownToken{
 return {type,raw,text:tex};
}

// `$…$` stays prose when the closing `$` is followed by a digit (prices) or the
// body starts/ends with whitespace, which keeps "$5 and $10" and "$HOME" text.
function inlineDollar(src:string):MarkdownToken|undefined{
 if(src[0]!=='$'||src[1]==='$')return;
 if(!/\S/.test(src[1]??''))return;
 for(let index=1;index<src.length;index++){
  const char=src[index];
  if(char==='\\'){index++;continue;}
  if(char==='\n')return;
  if(char!=='$')continue;
  const tex=src.slice(1,index);
  if(!/\S$/.test(tex))return;
  if(/\d/.test(src[index+1]??''))return;
  return mathToken('inlineMath',src.slice(0,index+1),tex);
 }
 return;
}

const inlineMath:MarkdownTokenizer={
 name:'inlineMath',
 level:'inline',
 start(src){
  const candidates=[src.indexOf('$'),src.indexOf('\\(')].filter(index=>index>=0);
  return candidates.length?Math.min(...candidates):-1;
 },
 tokenize(src){
  if(src.startsWith('\\(')){
   const end=src.indexOf('\\)',2);
   const tex=end>2?src.slice(2,end):'';
   if(tex&&!tex.includes('\n\n'))return mathToken('inlineMath',src.slice(0,end+2),tex);
  }
  return inlineDollar(src);
 },
};

// `\[…\]` also appears in prose as escaped citation brackets (`\[1\]`, `\[2,3\]`).
// Only a body that carries math syntax or a symbol becomes display math.
function bracketMath(tex:string):boolean{
 return !/^[0-9\s,.\u2013\u2014-]+$/.test(tex);
}

// Display math owns whole blocks so blank lines inside `aligned`/`cases` stay
// inside one node instead of becoming separate paragraphs.
const blockMath:MarkdownTokenizer={
 name:'blockMath',
 level:'block',
 start(src){
  const candidates=[src.search(/^ {0,3}\$\$/m),src.search(/^ {0,3}\\\[/m)].filter(index=>index>=0);
  return candidates.length?Math.min(...candidates):-1;
 },
 tokenize(src){
  const dollars=/^ {0,3}\$\$[ \t]*\n?([\s\S]*?)\n?[ \t]*\$\$[ \t]*(?:\n+|$)/.exec(src);
  if(dollars)return mathToken('blockMath',dollars[0],dollars[1].trim());
  const brackets=/^ {0,3}\\\[[ \t]*\n?([\s\S]*?)\n?[ \t]*\\\][ \t]*(?:\n+|$)/.exec(src);
  if(brackets&&bracketMath(brackets[1]))return mathToken('blockMath',brackets[0],brackets[1].trim());
  return;
 },
};

function mathNode(name:string,display:boolean){
 return Node.create({
  name,
  group:display?'block':'inline',
  inline:!display,
  atom:true,
  selectable:true,
  draggable:false,
  addAttributes(){
   return {
    tex:{
     default:'',
     parseHTML:element=>element.getAttribute('data-tex')??'',
     renderHTML:attributes=>({'data-tex':attributes.tex}),
    },
   };
  },
  parseHTML(){
   return [{tag:display?'div.math-block':'span.math-inline'}];
  },
  renderHTML({node}){
   return mathElement(String(node.attrs.tex??''),display) as unknown as DOMOutputSpec;
  },
  parseMarkdown(this:{name:string},token,helpers){
   return helpers.createNode(this.name,{tex:String(token.text??'')});
  },
  markdownTokenizer:display?blockMath:inlineMath,
 });
}

export const InlineMath=mathNode('inlineMath',false);
export const BlockMath=mathNode('blockMath',true);
export const mathExtensions=[InlineMath,BlockMath];
