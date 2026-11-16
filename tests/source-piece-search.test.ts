// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Reader from '../src/components/LargeDocumentReader.svelte';
let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.replaceChildren();});
import {markdownModel,pieceDocument,pieceText} from '../src/lib/document-model';
import {SourcePieceSearch} from '../src/lib/source-piece-search';

it.each(['\r\n','\r'])('source repaint preserves %j and a last-line match at the cut',async newline=>{
 const line='content'+newline,base=line.repeat(Math.floor(3994/line.length));
 const text=base+' '.repeat(3994-base.length)+'needle'+'x'.repeat(100000);
 const model=markdownModel({body:text,path:'/newlines.md',hash:newline});
 const parsed=pieceDocument(model,0);
 expect(parsed.querySelector('pre')!.textContent).toBe(pieceText(model,0));
 const search=new SourcePieceSearch(model,['needle']);
 await search.prepare(0,()=>true);search.paint(parsed,0,0);
 expect(parsed.querySelector('pre')!.textContent).toBe(pieceText(model,0));
 expect(parsed.querySelector('mark')?.textContent).toBe('needle');
});
it('normalizes the complete grapheme on either side of an artificial source cut',async()=>{
 const body='x'.repeat(3999)+'A\u030A'+'q'.repeat(100000);
 const model=markdownModel({body,path:'/grapheme.md',hash:'h'});
 for(const [term,expected] of [['Å',[{from:3999,to:4001}]],['\u030A',[]]] as const){
  const search=new SourcePieceSearch(model,[term]);await search.prepare(1,()=>true);
  expect([...search.matches(0),...search.matches(1)]).toEqual(expected);
 }
});
it.each(['',' ','  ','   '])('keeps no-blank-line ATX sections with indentation %j rich and reaches the last heading',async indent=>{
 const body=Array.from({length:50},(_,i)=>indent+'# Section '+i+'\n'+'A plain paragraph. '.repeat(120)+'\n').join('');
 app=mount(Reader,{target:document.body,props:{doc:{id:1,path:'/sections.md',name:'sections.md',hash:'test',body}}});
 await vi.waitFor(()=>expect(document.querySelector('.reader-piece h1')?.textContent).toBe('Section 0'));
 document.querySelector<HTMLButtonElement>('.outline-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.outline-pages button:last-child')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.outline-pages button:last-child')!.click();await tick();
 document.querySelector<HTMLButtonElement>('button[title="Section 49"]')!.click();
 expect(document.querySelector('.reader-piece h1:last-of-type')?.closest('.reader-piece')?.textContent).toContain('Section 49');
 expect(document.querySelectorAll('.source-presentation')).toHaveLength(0);
});
it('shares normalization of a single giant grapheme across every display cut',async()=>{
 const body='A'+'\u0301'.repeat(109999)+' end';
 const model=markdownModel({body,path:'/giant-grapheme.md',hash:'h'});
 const original=String.prototype.normalize;let chars=0;
 const spy=vi.spyOn(String.prototype,'normalize').mockImplementation(function(this:string,form?:string){
  if(this.length>1000)chars+=this.length;
  return original.call(this,form);
 });
 try{
  for(const [term,expected] of [['absent',[]],['Á',[{from:0,to:110000}]],['\u0301',[{from:0,to:110000}]]] as const){
   chars=0;const search=new SourcePieceSearch(model,[term]),found=[];
   for(let i=0;i<model.pieces.length;i++){
    await search.prepare(i,()=>true);found.push(...search.matches(i));
   }
   expect(found).toEqual(expected);
   expect(chars).toBeLessThanOrEqual(body.length);
  }
 }finally{spy.mockRestore();}
});
it('terminates an exceptional search worker when its reader is disposed',async()=>{
 const body='A'+'\u0301'.repeat(105000);
 const model=markdownModel({body,path:'/cancel-worker.md',hash:'cancel'});
 const stop=vi.fn(),post=vi.fn();
 vi.stubGlobal('Worker',class{postMessage=post;terminate=stop;});
 try{
  const search=new SourcePieceSearch(model,['absent']),pending=search.prepare(0,()=>true);
  expect(post).toHaveBeenCalledTimes(1);search.dispose();
  expect(stop).toHaveBeenCalledTimes(1);expect(await pending).toBe(false);
 }finally{vi.unstubAllGlobals();}
});
it('finishes exceptional search when the host rejects a worker',async()=>{
 const body='A'+'\u0301'.repeat(105000)+' final';
 const model=markdownModel({body,path:'/unavailable-worker.md',hash:'fallback'});
 const stop=vi.fn();
 vi.stubGlobal('Worker',class{
  onerror!:(event:Event)=>void;terminate=stop;
  postMessage(){queueMicrotask(()=>this.onerror(new Event('error',{cancelable:true})));}
 });
 try{
  const search=new SourcePieceSearch(model,['final']);
  expect(await search.prepare(model.pieces.length-1,()=>true)).toBe(true);
  expect(stop).toHaveBeenCalledOnce();
  expect(search.matches(model.pieces.length-1)).toEqual([{from:body.length-5,to:body.length}]);
 }finally{vi.unstubAllGlobals();}
});

it.each(['worker','fallback'] as const)('continues nonoverlapping matches through the exceptional %s handoff in either lookup order',async engine=>{
 const body='x'+'a'.repeat(7900)+'A'+'\u0301'.repeat(100000)+' final';
 const model=markdownModel({body,path:'/handoff.md',hash:'h'});
 const originalHandler=self.onmessage;
 let engineCalls=0;
 try{
  if(engine==='worker'){
   // Exercise the production worker entry and its message protocol in-process.
   await import('../src/lib/source-search.worker');
   const workerHandler=self.onmessage!;
   vi.stubGlobal('Worker',class{
    onmessage!:(event:MessageEvent)=>void;
    postMessage(data:unknown){
     engineCalls++;
     vi.stubGlobal('postMessage',(pairs:Uint32Array)=>this.onmessage(new MessageEvent('message',{data:pairs})));
     queueMicrotask(()=>workerHandler.call(self,new MessageEvent('message',{data})));
    }
    terminate(){}
   });
  }else vi.stubGlobal('Worker',undefined);
  for(const first of [0,model.pieces.length-1]){
   const search=new SourcePieceSearch(model,['aa']);
   await search.prepare(first,()=>true);
   for(let i=0;i<model.pieces.length;i++)await search.prepare(i,()=>true);
   expect(search.matches(0).at(-1)).toEqual({from:3999,to:4001});
   const remaining=model.pieces.slice(1).flatMap((_,i)=>search.matches(i+1));
   expect(remaining[0]).toEqual({from:4001,to:4003});
   expect(remaining).toHaveLength(1950);
   expect(search.matches(0).length+remaining.length).toBe(3950);
   search.dispose();
  }
  if(engine==='worker')expect(engineCalls).toBe(2);
 }finally{self.onmessage=originalHandler;vi.unstubAllGlobals();}
});
