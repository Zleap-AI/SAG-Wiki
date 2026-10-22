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
it('打开搜索片段时可定位到较后的匹配，保持正文不变',async()=>{
 const onChange=vi.fn();app=mount(RichEditor,{target:document.body,props:{body:'前面的 octx。\n\n目标区域里的 OCTX。',path:'/notes.md',searchTerms:['octx'],initialMatch:1,onChange}});
 await vi.waitFor(()=>expect(document.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1'));
 expect(document.querySelector('.search-hit.active')?.textContent).toBe('OCTX');
 expect(onChange).not.toHaveBeenCalled();
});
it.each(['# 标题','```js\nconst value = 1;\n```','- 第一项\n- 第二项','| 名称 | 值 |\n| --- | --- |\n| 一 | 1 |'])('初始化末尾结构不产生编辑记录：%s',async body=>{
 const onChange=vi.fn();app=mount(RichEditor,{target:document.body,props:{body,path:'/notes.md',onChange}});
 await tick();
 expect(onChange).not.toHaveBeenCalled();
 expect(document.querySelector<HTMLButtonElement>('[aria-label="撤销"]')!.disabled).toBe(true);
});
