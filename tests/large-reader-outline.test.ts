// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Reader from '../src/components/LargeDocumentReader.svelte';

const padding=('Ordinary paragraph. '.repeat(220)+'\n\n').repeat(24);
const selector='.reader-piece :is(h1,h2,h3,h4,h5,h6)';
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.replaceChildren();vi.restoreAllMocks();});

it.each([
 ['list heading','- # Only **list** heading',['Only list heading']],
 ['empty heading','#\n\n# Named heading',['','Named heading']],
 ['protected HTML','<div>\n<h2>Example</h2>\n</div>\n\n# Real Head',['Real Head']]
] as const)('outlines %s and scrolls to its visible heading',async(name,fixture,expected)=>{
 app=mount(Reader,{target:document.body,props:{doc:{id:1,path:'/outline.md',name:'outline.md',hash:name,body:padding+fixture},target:padding.length}});
 await vi.waitFor(()=>expect([...document.querySelectorAll(selector)].map(el=>el.textContent)).toEqual(expected));
 const visibleTitles=[...document.querySelectorAll(selector)].map(el=>el.textContent).filter(Boolean);
 const host=document.querySelector<HTMLElement>('.markdown-reader')!;
 Object.defineProperty(host,'clientHeight',{value:500});
 host.scrollTop=0;host.dispatchEvent(new Event('scroll'));
 await vi.waitFor(()=>expect(document.querySelector(selector)).toBeNull());
 document.querySelector<HTMLButtonElement>('.outline-toggle')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.reader-outline [role=status]')).toBeNull());
 const rows=[...document.querySelectorAll<HTMLButtonElement>('.reader-outline .outline-heading')];
 expect(rows.map(row=>row.textContent)).toEqual(visibleTitles);

 // jsdom does not lay out headings. Give each rendered heading a distinct position on the page.
 const rect=HTMLElement.prototype.getBoundingClientRect;
 vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(function(this:HTMLElement){
  if(!this.matches(selector))return rect.call(this);
  const ordinal=[...document.querySelectorAll(selector)].indexOf(this);
  return new DOMRect(0,100000+ordinal*100-host.scrollTop,300,24);
 });
 rows[0].click();
 const atTop=[...document.querySelectorAll(selector)].find(el=>{
  const {top}=el.getBoundingClientRect();return top>=0&&top<50;
 });
 expect(atTop?.textContent).toBe(visibleTitles[0]);
});
