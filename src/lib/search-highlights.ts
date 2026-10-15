import type{Node as ProseMirrorNode}from '@tiptap/pm/model';
import {escaped} from './domain';
export type TextMatch={from:number;to:number};
const segmenter=new Intl.Segmenter(undefined,{granularity:'grapheme'});
export function textMatches(text:string,terms:string[]):TextMatch[]{
 const words=[...new Set(terms.map(term=>term.normalize('NFKC').toLowerCase()).filter(Boolean))].sort((a,b)=>b.length-a.length);
 if(!words.length)return [];
 const pattern=new RegExp(words.map(word=>word.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')).join('|'),'gu');
 let normalized=text.normalize('NFKC').toLowerCase();
 let positions:{from:number;to:number}[]|null=null;
 if(normalized!==text.toLowerCase()||normalized.length!==text.length){
  positions=[];normalized='';
  for(const {segment,index} of segmenter.segment(text)){
   const folded=segment.normalize('NFKC').toLowerCase();normalized+=folded;
   for(let i=0;i<folded.length;i++)positions.push({from:index,to:index+segment.length});
  }
 }
 const matches:TextMatch[]=[];
 for(const match of normalized.matchAll(pattern)){
  const from=positions?positions[match.index].from:match.index;
  const to=positions?positions[match.index+match[0].length-1].to:match.index+match[0].length;
  if(matches.length&&from<matches[matches.length-1].to)continue;
  matches.push({from,to});
 }
 return matches;
}
export function highlightedText(text:string,terms:string[]):string{
 let html='',end=0;
 for(const match of textMatches(text,terms)){html+=escaped(text.slice(end,match.from))+`<mark class="search-hit">${escaped(text.slice(match.from,match.to))}</mark>`;end=match.to;}
 return html+escaped(text.slice(end));
}
export function searchSnippet(text:string,terms:string[]):string{
 const first=textMatches(text,terms)[0];const start=Math.max(0,(first?.from??0)-60),end=Math.min(text.length,start+180);
 return (start?'…':'')+highlightedText(text.slice(start,end),terms)+(end<text.length?'…':'');
}
// Match rendered text across inline formatting, while keeping paragraphs separate.
export function highlightedHtml(html:string,terms:string[]):{html:string;count:number}{
 if(!terms.length||!html)return {html,count:0};
 const doc=new DOMParser().parseFromString(html,'text/html');
 const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT),groups=new Map<Element,Text[]>();
 while(walker.nextNode()){
  const node=walker.currentNode as Text;
  if(node.parentElement!.closest('script,style,textarea'))continue;
  const block=node.parentElement!.closest('p,li,pre,td,th,h1,h2,h3,h4,h5,h6,div,blockquote')||doc.body;
  const nodes=groups.get(block)||[];nodes.push(node);groups.set(block,nodes);
 }
 let count=0;
 for(const nodes of groups.values()){
  const matches=textMatches(nodes.map(node=>node.data).join(''),terms).map(match=>({...match,id:count++}));
  let offset=0;
  for(const node of nodes){
   const end=offset+node.length,parts=matches.filter(match=>match.from<end&&match.to>offset);
   if(parts.length){
    const fragment=doc.createDocumentFragment();let cursor=0;
    for(const match of parts){
     const from=Math.max(match.from-offset,0),to=Math.min(match.to-offset,node.length);
     fragment.append(node.data.slice(cursor,from));const mark=doc.createElement('mark');
     mark.className='search-hit';mark.dataset.searchMatch=String(match.id);mark.textContent=node.data.slice(from,to);fragment.append(mark);cursor=to;
    }
    fragment.append(node.data.slice(cursor));node.replaceWith(fragment);
   }
   offset=end;
  }
 }
 return {html:doc.body.innerHTML,count};
}

export function editorMatches(doc:ProseMirrorNode,terms:string[]):TextMatch[]{
 const matches:TextMatch[]=[];
 doc.descendants((node,pos)=>{
  if(!node.isTextblock)return;
  for(const match of textMatches(node.textBetween(0,node.content.size,'','\ufffc'),terms))matches.push({from:pos+1+match.from,to:pos+1+match.to});
  return false;
 });
 return matches;
}
