import type {FoldResult,FoldSource} from './document-fold';

// Adapt the editor's DOM to TextFold's native reader payload. The rendering and
// motion engine itself lives, unchanged, in public/textfold (see its README).
type Token={t?:string;a?:string;s?:number;f?:string;m?:number;k?:number};
type Atom={kind:string;html:string;text:string;alt:string};
type Unit={id:string;role:string;list?:string;toks:Token[];sents:number[][];lad:{role:string;ev:number[];brief:Token[];tn:number;topic:Token[];model:string;glue?:string}};
type Block={k:string;chunk:string;u?:string;id?:string;html?:string;kind?:string;cap?:string;list?:string;ordered?:boolean;start?:number};
// JavaScript's \w is ASCII-only. Keep non-CJK words (and combining marks)
// together, using the same whitespace-after representation as tokens.py.
const pieces=/\s+|[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]|\d+(?:[.,]\d+)+%?|(?:(?![\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}])[\p{L}\p{M}\p{N}_])+(?:['’\-](?:(?![\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}])[\p{L}\p{M}\p{N}_])+)*|[^\s]/gu;
function tokenize(text:string,out:Token[]=[],font=''){
 for(const piece of text.match(pieces)??[]){
  if(/^\s+$/.test(piece)){if(out.length)out[out.length-1].s=1;}
  else out.push({t:piece,...font?{f:font}:{}});
 }
 return out;
}
const stop=new Set('the a an of to in is are was were be been and or that this these those it its for on with as by at from we our their they which while than then into can not but such also has have had does do more most less only both 的 了 是 在 和 与 或 这 那 就 也 都 而 及 其 之 对 中 为'.split(' '));
function norm(token:Token,atoms:Record<string,Atom>){
 if(token.a)return 'atom:'+atoms[token.a].alt.replace(/\s/g,'');
 let text=(token.t??'').toLowerCase().replace(/[^\p{L}\p{M}\p{N}_]/gu,'');
 if(stop.has(text))return '';
 if(text.length>3&&text.endsWith('s')&&!text.endsWith('ss'))text=text.slice(0,-1);
 return text;
}
function align(original:Token[],written:Token[],indices:number[],atoms:Record<string,Atom>){
 const a=indices.map(i=>norm(original[i],atoms)),b=written.map(t=>norm(t,atoms));
 const dp=Array.from({length:a.length+1},()=>new Uint32Array(b.length+1));
 for(let i=a.length-1;i>=0;i--)for(let j=b.length-1;j>=0;j--)dp[i][j]=a[i]&&a[i]===b[j]?1+dp[i+1][j+1]:Math.max(dp[i+1][j],dp[i][j+1]);
 let i=0,j=0;
 while(i<a.length&&j<b.length){if(a[i]&&a[i]===b[j])written[j++].m=indices[i++];else if(dp[i+1][j]>=dp[i][j+1])i++;else j++;}
}
function locate(text:string,tokens:Token[]){
 const clean=(s:string)=>s.toLowerCase().replace(/[^\p{L}\p{M}\p{N}_]/gu,'');
 const wanted=clean(text);if(!wanted)return [];
 for(let i=0;i<tokens.length;i++){
  let part='';
  for(let j=i;j<tokens.length;j++){
   part+=clean(tokens[j].t??'');
   if(part===wanted)return Array.from({length:j-i+1},(_,k)=>i+k);
   if(!wanted.startsWith(part))break;
  }
 }
 return [];
}

export function textfoldData(source:FoldSource,result:FoldResult,id:string){
 const root=document.createElement('div');root.innerHTML=source.html;
 const atoms:Record<string,Atom>={},units:Record<string,Unit>={},blocks:Block[]=[];
 const chunks:{id:string;title:string;html:string;num:string;level:number;units:string[];lad:{take:Token[];topic:number[]}}[]=[];
 const answers=new Map(result.units.map(row=>[row.id,row]));
 for(const section of source.document.sections){
  const sid=`s${section.id}`,wrapper=root.querySelector<HTMLElement>(`[data-fold-section="${section.id}"]`)!;
  const summary=result.sections.find(row=>row.id===section.id);
  const take=tokenize(summary?.takeaway??'');
  const header=wrapper.querySelector('h1,h2,h3,h4,h5,h6');
  // The reader draws the document title separately, as upstream's parser does.
  const title=header?.tagName==='H1'&&section.title===source.document.title?'':section.title;
  const chunk={id:sid,title,html:title?header?.innerHTML??title:'',num:'',level:Math.max(2,section.depth+1),units:section.units.map(u=>`u${u.id}`),lad:{take,topic:locate(summary?.topic??'',take)}};
  chunks.push(chunk);blocks.push({k:'h',chunk:sid});
  for(const unit of section.units){
   const uid=`u${unit.id}`,node=wrapper.querySelector<HTMLElement>(`[data-fold-unit="${unit.id}"]`)!;
   const toks:Token[]=[],offsets:{start:number;end:number}[]=[];let offset=0;
   const appendText=(text:string,font:string)=>{
    for(const piece of text.match(pieces)??[]){const start=offset;offset+=piece.length;
     if(/^\s+$/.test(piece)){if(toks.length)toks[toks.length-1].s=1;}
     else{toks.push({t:piece,...font?{f:font}:{}});offsets.push({start,end:offset});}
    }
   };
   const walk=(parent:Node,font='')=>{
    for(const child of parent.childNodes){
     if(child.nodeType===Node.TEXT_NODE){appendText(child.textContent??'',font);continue;}
     if(!(child instanceof HTMLElement)&&!(child instanceof SVGElement))continue;
     const tag=child.tagName.toLowerCase();
     if(['a','code','math','svg'].includes(tag)||child.classList.contains('katex')){
      const text=child.textContent??'',aid=`a${Object.keys(atoms).length}`;
      atoms[aid]={kind:tag==='math'||child.classList.contains('katex')?'math':'inline',html:child.outerHTML,text,alt:text};
      toks.push({a:aid});offsets.push({start:offset,end:offset+text.length});offset+=text.length;
     }else if(tag==='br')appendText('\n',font);
     else walk(child,[font,...['strong','b'].includes(tag)?['ltx_font_bold']:[],...['em','i'].includes(tag)?['ltx_font_italic']:[],...['s','del'].includes(tag)?['strike']:[]].filter(Boolean).join(' '));
    }
   };
   walk(node);
   const text=node.textContent??'';let sentenceOffset=0;
   const sents=unit.sentences.map(sentence=>{
    const start=text.indexOf(sentence,sentenceOffset),end=start+sentence.length;sentenceOffset=end;
    const first=offsets.findIndex(o=>o.end>start),after=offsets.findIndex(o=>o.start>=end);
    return [first,after<0?toks.length:after];
   });
   const answer=answers.get(unit.id)!;
   const brief=answer.suppressed?[]:tokenize(answer.key),tn=answer.hide_takeaway?0:tokenize(answer.takeaway).length;
   const evidence=answer.evidence.flatMap(s=>Array.from({length:sents[s][1]-sents[s][0]},(_,i)=>sents[s][0]+i));
   align(toks,brief,evidence,atoms);
   const located=locate(answer.topic,brief.slice(0,tn));
   const topic:Token[]=located.length?located.map(k=>({...brief[k],k})):tokenize(answer.topic);
   if(!located.length){
    align(brief.slice(0,tn),topic,Array.from({length:tn},(_,i)=>i),atoms);
    for(const token of topic){if(token.m!==undefined){token.k=token.m;delete token.m;}}
   }
   const list=node.closest('ul,ol');
   units[uid]={id:uid,role:unit.kind==='list item'?'item':unit.kind==='caption'?'caption':'p',...list?{list:`list-${section.id}-${Array.from(wrapper.querySelectorAll('ul,ol')).indexOf(list)}`}:{},toks,sents,
    lad:{role:answer.role,ev:answer.suppressed?[]:answer.evidence,brief,tn,topic:answer.suppressed?[]:topic,model:answer.local?'local':'model',glue:answer.glue}};
  }
  const visit=(node:Node)=>{
   if(!(node instanceof HTMLElement))return;
   if(/^H[1-6]$/.test(node.tagName))return;
   if(node.matches('figure,table,pre,img,.mermaid-code-block')){
    const cap=node.querySelector<HTMLElement>('[data-fold-unit]'),uid=cap?`u${cap.dataset.foldUnit}`:undefined;
    const content=node.cloneNode(true) as HTMLElement;content.querySelectorAll('figcaption').forEach(n=>n.remove());
    blocks.push({k:node.matches('pre,.mermaid-code-block')?'raw':'fig',chunk:sid,id:`block-${blocks.length}`,html:content.outerHTML,kind:node.tagName==='TABLE'?'table':'figure',cap:uid});return;
   }
   if(node.dataset.foldUnit!==undefined){
    const uid=`u${node.dataset.foldUnit}`,list=node.closest('ol,ul');
    blocks.push({k:'u',chunk:sid,u:uid,...list?{list:units[uid].list,ordered:list.tagName==='OL',start:Number(list.getAttribute('start')??1)}:{}});return;
   }
   if(node.querySelector('[data-fold-unit],table,pre,img,figure,.mermaid-code-block')){for(const child of node.childNodes)visit(child);}
   else if(node.outerHTML)blocks.push({k:'raw',chunk:sid,id:`block-${blocks.length}`,html:node.outerHTML});
  };
  for(const child of wrapper.childNodes)visit(child);
 }
 const cjk=/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u.test(source.units.flatMap(u=>u.sentences).join('').slice(0,4000));
 return {meta:{id,title:source.document.title,authors:[],lang:cjk?'zh':'en',langs:[cjk?'zh':'en'],cjk,source:{kind:'markdown'},label:source.document.title},units,atoms,chunks,blocks,bib:'',edges:[],alias:{},app:false};
}
