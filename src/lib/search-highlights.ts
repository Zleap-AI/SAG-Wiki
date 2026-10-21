import type{Node as ProseMirrorNode}from '@tiptap/pm/model';
import {escaped,isHtml,isMarkdown} from './domain';
import type {Candidate} from './types';
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
export function searchText(path:string,content:string):string{
 if(!isHtml(path))return content;
 const tail=/<\/(style|script)\s*>/i.exec(content);
 if(tail&&!new RegExp(`<${tail[1]}(?:\\s|>)`,'i').test(content.slice(0,tail.index)))content=content.slice(tail.index+tail[0].length);
 const doc=new DOMParser().parseFromString(content,'text/html');
 doc.querySelectorAll('script,style,template,noscript,svg').forEach(node=>node.remove());
 const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT),parts:string[]=[];
 while(walker.nextNode())parts.push(walker.currentNode.textContent!);
 return parts.join(' ');
}
// Match rendered text across inline formatting, while keeping paragraphs separate.
export function highlightedHtml(html:string,terms:string[]):{html:string;count:number}{
 if(!terms.length||!html)return {html,count:0};
 const doc=new DOMParser().parseFromString(html,'text/html');
 const count=highlightDocument(doc,terms);
 return {html:doc.body.innerHTML,count};
}
export function highlightDocument(doc:Document,terms:string[]):number{
 if(!terms.length)return 0;
 let count=0;
 for(const nodes of textBlocks(doc)){
  const matches=textMatches(nodes.map(node=>node.data).join(''),terms).map(match=>({...match,id:count++}));
  let offset=0;
  for(const node of nodes){
   const end=offset+node.length,parts=matches.filter(match=>match.from<end&&match.to>offset);
   if(parts.length){
    const fragment=doc.createDocumentFragment();let cursor=0;
    for(const match of parts){
     const from=Math.max(match.from-offset,0),to=Math.min(match.to-offset,node.length);
     fragment.append(node.data.slice(cursor,from));
     const mark=node.parentElement!.namespaceURI==='http://www.w3.org/2000/svg'?doc.createElementNS('http://www.w3.org/2000/svg','tspan'):doc.createElement('mark');
     mark.setAttribute('class','search-hit');mark.setAttribute('data-search-match',String(match.id));mark.textContent=node.data.slice(from,to);fragment.append(mark);cursor=to;
    }
    fragment.append(node.data.slice(cursor));node.replaceWith(fragment);
   }
   offset=end;
  }
 }
 return count;
}
function textBlocks(doc:Document):Text[][]{
 const walker=doc.createTreeWalker(doc.body,NodeFilter.SHOW_TEXT),groups=new Map<Element,Text[]>();
 while(walker.nextNode()){
  const node=walker.currentNode as Text;
  if(node.parentElement!.closest('script,style,textarea,svg'))continue;
  const block=node.parentElement!.closest('p,li,pre,td,th,h1,h2,h3,h4,h5,h6,div,blockquote,text')||doc.body;
  const nodes=groups.get(block)||[];nodes.push(node);groups.set(block,nodes);
 }
 return [...groups.values()];
}
export async function documentMatchIndex(body:string,path:string,terms:string[],candidate:Candidate):Promise<number>{
 const start=candidate.source_spans?.[0]?.start_byte;
 const prefix=start===undefined?body.split('\n').slice(0,candidate.start_line-1).join('\n'):new TextDecoder().decode(new TextEncoder().encode(body).subarray(0,start));
 if(isMarkdown(path)){const {renderMarkdown}=await import('./markdown');return highlightedHtml(renderMarkdown(prefix,body),terms).count;}
 if(isHtml(path))return highlightDocument(new DOMParser().parseFromString(prefix,'text/html'),terms);
 return textMatches(prefix,terms).length;
}

export async function citationTerms(candidate:Candidate,body:string):Promise<string[]>{
 let html:string;
 if(isMarkdown(candidate.path)){
  const {renderMarkdown,markdownCitation}=await import('./markdown');
  const bytes=new TextEncoder().encode(body),decoder=new TextDecoder(),lines=body.split('\n');
  const ranges=candidate.source_spans?.length?candidate.source_spans.map(span=>({from:decoder.decode(bytes.subarray(0,span.start_byte)).length,to:decoder.decode(bytes.subarray(0,span.end_byte)).length})):[{from:lines.slice(0,candidate.start_line-1).join('\n').length+(candidate.start_line>1?1:0),to:lines.slice(0,candidate.end_line).join('\n').length}];
  html=renderMarkdown(markdownCitation(body,ranges));
 }else html=isHtml(candidate.path)?candidate.content:`<pre>${escaped(candidate.content)}</pre>`;
 const doc=new DOMParser().parseFromString(html,'text/html');
 doc.querySelectorAll('script,style,template,noscript,svg').forEach(node=>node.remove());
 return [...new Set(textBlocks(doc).map(nodes=>nodes.map(node=>node.data).join('').trim()).filter(Boolean))];
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
