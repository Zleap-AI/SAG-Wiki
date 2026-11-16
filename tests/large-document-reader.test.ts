// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Reader from '../src/components/LargeDocumentReader.svelte';
import {clearMarkdownModels,documentCacheUsage,candidateRanges,prepareCandidateRanges,markdownModel,pieceHtml,pieceText,contentOffset} from '../src/lib/document-model';
import {copyText} from '../src/lib/web-connection';
import {setLanguage} from '../src/lib/i18n';
vi.mock('../src/lib/web-connection',async(importOriginal)=>({...await importOriginal<typeof import('../src/lib/web-connection')>(),copyText:vi.fn(async()=>{})}));
const source=(body:string,hash='h')=>({id:1,path:'/book.md',name:'book.md',body,hash});
let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.replaceChildren();clearMarkdownModels();vi.clearAllMocks()});
const body=Array.from({length:32},(_,i)=>'# Chapter '+i+'\n\n'+(i===0||i===31?'The **formatted** result.\n\n':'')+'Ordinary paragraph. '.repeat(240)+'\n\n').join('');
it('finds distant formatted matches, wraps, and reaches unmounted outline anchors without growing body DOM',async()=>{
 app=mount(Reader,{target:document.body,props:{doc:source(body),terms:['the formatted result']}});
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('1 / 2'),{timeout:4000});
 document.querySelector<HTMLButtonElement>('.match-next')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('2 / 2'));
 expect(document.querySelector('.search-hit.active')?.closest('section')?.textContent).toContain('Chapter 31');
 expect(document.querySelectorAll('.reader-piece').length).toBeLessThanOrEqual(12);
 document.querySelector<HTMLButtonElement>('.match-next')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('1 / 2'));
 document.querySelector<HTMLButtonElement>('.outline-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('button[title="Chapter 25"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('button[title="Chapter 25"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('button[aria-current=location]')?.textContent).toBe('Chapter 25'));
 expect(document.querySelector('.markdown-reader[tabindex="0"]')?.getAttribute('aria-label')).toBeTruthy();
});
it('uses bounded source for a giant semantic block and copies the complete unmodified file',async()=>{
 const full='---\ntitle: book\n---\n'+'\x60\x60\x60text\n'+'a'.repeat(150000)+'\n\x60\x60\x60';
 const model=markdownModel(source(full));
 expect(model.pieces.length).toBeGreaterThan(30);
 expect(Math.max(...model.pieces.map(piece=>piece.to-piece.from))).toBeLessThanOrEqual(4000);
 expect(model.pieces.map((_,index)=>pieceText(model,index)).join('')).toBe(model.content);
 app=mount(Reader,{target:document.body,props:{doc:source(full)}});
 await vi.waitFor(()=>expect(document.querySelector('.source-presentation')).not.toBeNull());
 expect(document.querySelectorAll('.reader-piece').length).toBeLessThanOrEqual(12);
 document.querySelector<HTMLButtonElement>('.copy-full-document')!.click();await tick();
 expect(copyText).toHaveBeenCalledWith(full);
});
it('accounts for allocated UTF-8 bytes and evicts old models under shared byte pressure',()=>{
 const full='中'.repeat(1800000);
 const first=markdownModel(source(full,'a'));contentOffset(first,3);
 const second=markdownModel(source(full,'b'));contentOffset(second,3);
 expect(markdownModel(source(full,'a'))).not.toBe(first);
 // Rendering through an evicted model remains valid for its active reader.
 expect(pieceHtml(first,0)).toContain('中');
});
it('find crosses artificial source cuts and counts nonoverlapping matches',async()=>{
 const full='a'.repeat(3998)+'XY'+'Z'+'b'.repeat(100000)+'XYZ';
 app=mount(Reader,{target:document.body,props:{doc:source(full),terms:['XYZ']}});
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('1 / 2'));
 expect(document.querySelector('.search-hit.active')?.textContent).toBe('XY');
 document.querySelector<HTMLButtonElement>('.match-next')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('2 / 2'));
 expect(document.querySelector('.search-hit.active')?.textContent).toBe('XYZ');
});
it('restores a source anchor for the same version and starts over after a hash change',async()=>{
 const doc=source(body),memoryKey='saved-library';
 app=mount(Reader,{target:document.body,props:{doc,memoryKey,restoreScroll:true}});
 await vi.waitFor(()=>expect(document.querySelector('.reader-piece')).not.toBeNull());
 const scroll=document.querySelector<HTMLElement>('.markdown-reader')!;
 scroll.scrollTop=65000;scroll.dispatchEvent(new Event('scroll'));
 await vi.waitFor(()=>expect(Number(document.querySelector<HTMLElement>('.reader-piece')?.dataset.piece)).toBeGreaterThan(10));
 const previous=scroll.scrollTop;await unmount(app);app=undefined;
 app=mount(Reader,{target:document.body,props:{doc,memoryKey,restoreScroll:true}});
 await vi.waitFor(()=>expect(document.querySelector<HTMLElement>('.markdown-reader')?.scrollTop).toBe(previous));
 await unmount(app);app=undefined;
 app=mount(Reader,{target:document.body,props:{doc:{...doc,hash:'updated'},memoryKey,restoreScroll:true}});
 await vi.waitFor(()=>expect(document.querySelector('.reader-piece[data-piece="0"]')).not.toBeNull());
 expect(document.querySelector<HTMLElement>('.markdown-reader')?.scrollTop).toBe(0);
});
it('enforces the HTML byte budget while an active oversized model remains readable',()=>{
 const doc=source('a'.repeat(9000000)),model=markdownModel(doc);
 expect(markdownModel(doc)).not.toBe(model);
 expect(pieceHtml(model,0)).toContain('aaaaa');
 expect(documentCacheUsage()).toMatchObject({modelBytes:0,htmlPieces:0});
 const book=markdownModel(source('b'.repeat(3000000),'cached'));
 for(let i=0;i<600;i++)pieceHtml(book,i);
 const usage=documentCacheUsage();
 expect(usage.htmlPieces).toBeGreaterThan(0);expect(usage.htmlBytes).toBeLessThanOrEqual(4*1024*1024);
 expect(usage.htmlPieces).toBeLessThan(600);
 expect(pieceHtml(model,0)).toContain('aaaaa');
});
it('bounds densely expanded Markdown even when the source is below the rich block ceiling',async()=>{
 const dense=Array.from({length:1900},()=>'<i>x</i>').join('');
 const doc=source(dense+'\n\n'+('Readable paragraph. '.repeat(200)+'\n\n').repeat(25));
 app=mount(Reader,{target:document.body,props:{doc}});
 await vi.waitFor(()=>expect(document.querySelector('.source-presentation')).not.toBeNull());
 expect(document.querySelectorAll('.reader-piece i')).toHaveLength(0);
 expect([...document.querySelectorAll('.source-subpiece')].every(span=>span.textContent!.length<=4000)).toBe(true);
 expect(document.querySelectorAll('.source-subpiece').length).toBeLessThanOrEqual(12);
});

it('maps UTF-8 spans including partial emoji, CRLF and frontmatter to the same source offsets',async()=>{
 const full='---\r\nt: test\r\n---\r\n😀中\r\nk'.repeat(5),model=markdownModel(source(full));
 const source_spans=Array.from({length:70},(_,i)=>({start_byte:i,end_byte:i+2,start_line:1,end_line:1,start_column:1,end_column:1}));
 const candidate={id:1,document_id:1,path:'/book.md',title:'book',content:'',heading:'',hash:'h',start_line:1,end_line:2,source_ranges:[],source_spans,bm25:0,score:0,order:0};
 expect(await prepareCandidateRanges(model,candidate,()=>true)).toEqual(candidateRanges(model,candidate));
 expect(await prepareCandidateRanges(model,candidate,()=>false)).toEqual([]);
});
it('replacing a find during indexing cancels its pending selection and count',async()=>{
 app=mount(Reader,{target:document.body,props:{doc:source(body),showFind:true}});
 await vi.waitFor(()=>expect(document.querySelector('.reader-piece[data-state=rendered]')).not.toBeNull());
 [...document.querySelectorAll('button')].find(button=>button.textContent?.includes('正文查找'))!.click();await tick();
 const input=document.querySelector<HTMLInputElement>('.large-find input')!;
 for(const term of ['Ordinary','formatted result']){
  input.value=term;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  input.form!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 }
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('1 / 2'));
 expect([...document.querySelectorAll('.search-hit.active')].map(mark=>mark.textContent).join('')).toBe('formatted result');
});
it.each(['',' ','  ','   '])('retains a real reference definition with indentation %j following a giant paragraph and ignores the fenced imitation',indent=>{
 const code='\x60\x60\x60\n'+indent+'[guide]: https://fake.invalid\n'+'x'.repeat(25000)+'\n\x60\x60\x60\n\n';
 const body=code+'q'.repeat(25001)+'\n\n'+indent+'[guide]: https://real.example\n\n'+('Padding. '.repeat(50)+'\n\n').repeat(120)+'A [distant title][guide].';
 const model=markdownModel(source(body));
 expect(model.definitions.get('guide')?.href).toBe('https://real.example');
 expect(pieceHtml(model,model.pieces.length-1)).toContain('href="https://real.example"');
});
it('disables find until asynchronous preparation finishes',async()=>{
 const models=await import('../src/lib/document-model');
 const doc=source(body);let finish!:()=>void;
 const spy=vi.spyOn(models,'prepareMarkdownModel').mockImplementationOnce(()=>new Promise(resolve=>{finish=()=>resolve(models.markdownModel(doc));}));
 try{
  app=mount(Reader,{target:document.body,props:{doc,showFind:true}});await tick();
  const button=[...document.querySelectorAll('button')].find(el=>el.textContent?.includes('正文查找'))!;
  expect(button.disabled).toBe(true);button.click();await tick();expect(document.querySelector('.large-find')).toBeNull();
  finish();await vi.waitFor(()=>expect(button.disabled).toBe(false));
  button.click();await tick();expect(document.querySelector('.large-find input')).not.toBeNull();
  expect(document.querySelector('.match-navigation')).toBeNull();
 }finally{spy.mockRestore();}
});
it('a definition-heavy model resolves only relevant labels when rendering individual rich pieces',()=>{
 const defs=Array.from({length:2200},(_,i)=>'[ref'+i+']: https://example.com/'+i+'\n\nA paragraph near definition '+i+'.\n\n').join('');
 const doc=source(defs+'\n[forward][ref2199] and [back][ref1].');
 const model=markdownModel(doc);expect(model.definitions.size).toBe(2200);
 let reads=0;
 model.definitions=new Proxy(model.definitions,{get(map,key){
  if(key==='get')return (tag:string)=>{reads++;return map.get(tag);};
  if(key==='size')return map.size;
  throw new Error('Rendering a piece must not iterate the global definition index: '+String(key));
 }});
 for(let i=0;i<model.pieces.length;i+=4)pieceHtml(model,i);
 expect(pieceHtml(model,model.pieces.length-1)).toContain('href="https://example.com/2199"');
 expect(reads).toBeLessThan(model.definitions.size);
});
it('selects formatted matches beyond the bounded marks and completes a later zero-hit find',async()=>{
 const paragraph='hit '.repeat(320);
 const doc=source(paragraph+'\n\n'+('Regular text. '.repeat(320)+'\n\n').repeat(26));
 app=mount(Reader,{target:document.body,props:{doc,terms:['hit'],target:paragraph.lastIndexOf('hit'),showFind:true}});
 await vi.waitFor(()=>expect(document.querySelector('.match-navigation')?.textContent).toContain('320 / 320'));
 expect(document.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('319');
 [...document.querySelectorAll('button')].find(el=>el.textContent?.includes('正文查找'))!.click();await tick();
 const field=document.querySelector<HTMLInputElement>('.large-find input')!;
 field.value='missingword';field.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 field.form!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 const status=document.querySelector('.match-navigation [aria-live=polite]')!;
 expect(status.textContent).toBe('正在统计匹配…');
 await vi.waitFor(()=>expect(status.textContent).toBe('未找到匹配项'));
 expect(status.isConnected).toBe(true);
 expect(document.querySelector('.match-next')).toBeNull();
 expect(document.querySelector('.search-hit.active')).toBeNull();
 setLanguage('en');await tick();expect(status.textContent).toBe('No matches');
});
it('keeps adjacent complete top-level fences rich without a separating blank line',()=>{
 const fence='\x60\x60\x60',block=fence+'txt\n'+'Line of code. '.repeat(120)+'\n'+fence+'\n';
 const model=markdownModel(source(block.repeat(75)));
 expect(model.pieces.every(p=>!p.source)).toBe(true);
 expect(pieceHtml(model,model.pieces.length-1)).toContain('<code');
});
it('keeps an indented standalone definition immediately after a closed giant fence',()=>{
 const fence='\x60\x60\x60';
 const body=fence+'\n[ref]: https://fake.invalid\n'+'z'.repeat(30000)+'\n'+fence+'\n   [ref]: https://real.example\n\n'+('padding paragraph. '.repeat(80)+'\n\n').repeat(60)+'[title][ref]';
 const model=markdownModel(source(body));
 expect(model.definitions.get('ref')?.href).toBe('https://real.example');
});
