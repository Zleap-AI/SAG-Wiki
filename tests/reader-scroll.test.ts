// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import HighlightedDocument from '../src/components/HighlightedDocument.svelte';

let app:ReturnType<typeof mount>|null=null;
afterEach(async()=>{if(app)await unmount(app);app=null;document.body.innerHTML='';vi.restoreAllMocks()});
async function open(props:Record<string,unknown>={}){
 app=mount(HighlightedDocument,{target:document.body,props:{html:'<p>needle first</p><p>needle second</p>',terms:['needle'],...props}});await tick();
}
it('普通正文隐藏导航仍保留匹配高亮',async()=>{
 await open({showNavigation:false});
 expect(document.querySelector('.match-navigation')).toBeNull();
 expect(document.querySelector('.search-hit.active')?.textContent).toBe('needle');
});
it('普通正文只在显式返回时恢复位置，新目标和新key不复用旧滚动',async()=>{
 const memoryKey='plain-return:file:h:query-1',scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
 await open({memoryKey});
 const host=document.querySelector<HTMLElement>('.search-preview-body')!;
 host.scrollTop=420;host.scrollLeft=12;host.dispatchEvent(new Event('scroll'));
 await unmount(app!);app=null;scroll.mockClear();
 await open({memoryKey,restoreScroll:true});
 expect(document.querySelector<HTMLElement>('.search-preview-body')!.scrollTop).toBe(420);
 expect(document.querySelector<HTMLElement>('.search-preview-body')!.scrollLeft).toBe(12);
 expect(scroll).not.toHaveBeenCalled();
 await unmount(app!);app=null;scroll.mockClear();
 await open({memoryKey,restoreScroll:false,initialMatch:1});
 expect(document.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
 expect(scroll).toHaveBeenCalledWith({block:'center'});
 await unmount(app!);app=null;scroll.mockClear();
 await open({memoryKey:'plain-return:file:new-h:query-2',restoreScroll:true,initialMatch:1});
 expect(document.querySelector<HTMLElement>('.search-preview-body')!.scrollTop).toBe(0);
 expect(scroll).toHaveBeenCalledWith({block:'center'});
});
