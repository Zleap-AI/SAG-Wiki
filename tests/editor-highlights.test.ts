// @vitest-environment happy-dom
import{afterEach,it,expect,vi}from 'vitest';import{mount,unmount,tick}from 'svelte';import RichEditor from '../src/components/RichEditor.svelte';
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML=''});
it('打开和切换搜索标记不会触发保存或更改正文',async()=>{
 const onChange=vi.fn();app=mount(RichEditor,{target:document.body,props:{body:'O**CT**X\n\n第二处 octx。',path:'/notes.md',searchTerms:['octx'],onChange}});
 await vi.waitFor(()=>expect(document.querySelectorAll('.tiptap .search-hit')).toHaveLength(4));
 expect(document.querySelector('[aria-label="匹配位置"]')!.textContent).toContain('1 / 2');
 document.querySelector<HTMLButtonElement>('[aria-label="下一处匹配"]')!.click();await tick();
 expect(document.querySelector('[aria-label="匹配位置"]')!.textContent).toContain('2 / 2');
 expect(document.querySelector('.search-hit.active')!.textContent).toBe('octx');expect(onChange).not.toHaveBeenCalled();
});
