export type FoldUnit={id:number;kind:string;sentences:string[]};
export type FoldSection={id:number;title:string;depth:number;units:FoldUnit[];tables:string[]};
export type FoldDocument={title:string;sections:FoldSection[]};
export type FoldedUnit={id:number;evidence:number[];key:string;takeaway:string;topic:string;role:string;local:boolean;suppressed:boolean;hide_takeaway:boolean;glue?:string};
export type FoldedSection={id:number;takeaway:string;topic:string};
export type FoldResult={units:FoldedUnit[];sections:FoldedSection[];warnings:string[]};
export type FoldProgress={id:string;stage:string;percent:number};
export type FoldSource={html:string;document:FoldDocument;units:FoldUnit[]};

// Preserve Markdown blocks and inline markup. Tables supply chapter context;
// their cells are not treated as unrelated paragraphs to summarize.
export function foldSource(editor:HTMLElement,title=''):FoldSource{
 const copy=editor.cloneNode(true) as HTMLElement;
 copy.removeAttribute('contenteditable');copy.removeAttribute('role');copy.removeAttribute('aria-label');
 copy.querySelectorAll('[contenteditable]').forEach(node=>node.removeAttribute('contenteditable'));
 const sections:FoldSection[]=[];let section:FoldSection;let container:HTMLElement;let nextId=0;
 function begin(title:string,depth:number){
  section={id:sections.length,title,depth,units:[],tables:[]};sections.push(section);
  container=document.createElement('section');container.dataset.foldSection=String(section.id);copy.append(container);
 }
 const children=Array.from(copy.childNodes);copy.replaceChildren();begin('',1);
 for(const child of children){
  if(child instanceof HTMLElement&&/^H[1-6]$/.test(child.tagName))begin(child.textContent??'',Number(child.tagName[1]));
  container!.append(child);
 }
 for(const wrapper of Array.from(copy.children) as HTMLElement[]){
  const current=sections[Number(wrapper.dataset.foldSection)];
  if(!wrapper.childNodes.length){wrapper.remove();continue;}
  current.tables=Array.from(wrapper.querySelectorAll('table'),table=>Array.from(table.rows,row=>Array.from(row.cells,cell=>cell.textContent?.trim()??'').join(' | ')).join('\n'));
  for(const node of wrapper.querySelectorAll<HTMLElement>('p,li,figcaption')){
   if(node.closest('table,pre,.mermaid-code-block')||node.querySelector('p,li,img')||!node.textContent?.trim())continue;
   const sentences=Array.from(new Intl.Segmenter(undefined,{granularity:'sentence'}).segment(node.textContent.trim()),part=>part.segment.trim()).filter(Boolean);
   node.dataset.foldUnit=String(nextId);
   current.units.push({id:nextId++,kind:node.closest('li')?'list item':node.tagName==='FIGCAPTION'?'caption':'paragraph',sentences});
  }
 }
 const populated=sections.filter(s=>s.units.length||s.tables.length||s.title);
 const doc={title:copy.querySelector('h1')?.textContent?.trim()||title||populated[0]?.title||'Document',sections:populated};
 return {html:copy.innerHTML,document:doc,units:populated.flatMap(section=>section.units)};
}
