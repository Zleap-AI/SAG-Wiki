// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import type {Settings} from '../src/lib/types';
let app:ReturnType<typeof mount>;
const onSave=vi.fn(),onClose=vi.fn();
const value:Settings={answer:{url:'http://localhost',model:'first',models:['first','second'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
const query=<T extends HTMLElement>(selector:string)=>document.querySelector<T>(selector)!;
const key=(target:Element,key:string)=>target.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));
beforeEach(()=>{onSave.mockClear();onClose.mockClear()});
afterEach(async()=>{await unmount(app);document.body.innerHTML=''});
it('下拉框选择同步设置值，Esc 先收起下拉框再关闭设置',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'appearance',onSave,onClose}});
 await vi.waitFor(()=>expect(query('[aria-label="文档字号"]')).not.toBeNull());
 const trigger=query('[aria-label="文档字号"]');trigger.focus();key(trigger,'ArrowDown');
 await vi.waitFor(()=>expect(query('[role=listbox]')).not.toBeNull());
 query('[role=option][data-value="18"]').dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await vi.waitFor(()=>expect(trigger.textContent).toContain('18 px'));
 key(trigger,'ArrowDown');await vi.waitFor(()=>expect(query('[role=listbox]')).not.toBeNull());
 key(trigger,'Escape');await vi.waitFor(()=>expect(query('[role=listbox]')).toBeNull());expect(onClose).not.toHaveBeenCalled();
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].editor_size).toBe(18);
});
it('开关支持空格操作，排除目录输入保持保存内容',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'files',onSave,onClose}});
 await vi.waitFor(()=>expect(query('[role=switch]')).not.toBeNull());
 const toggle=query('[aria-label="自动监听文件变化"]');expect(toggle.getAttribute('aria-checked')).toBe('true');key(toggle,' ');await tick();expect(toggle.getAttribute('aria-checked')).toBe('false');
 const text=query<HTMLTextAreaElement>('textarea');text.value='node_modules\n.cache';text.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].watch).toBe(false);expect(onSave.mock.calls[0][0].excluded).toEqual(['node_modules','.cache']);
});
it('数字输入保持数值类型并保留原生范围验证',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'retrieval',onSave,onClose}});
 await vi.waitFor(()=>expect(query('[aria-label="片段目标字符数"]')).not.toBeNull());
 const input=query<HTMLInputElement>('[aria-label="片段目标字符数"]');input.value='3200';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(input.min).toBe('400');expect(input.max).toBe('12000');
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].retrieval.chunk_chars).toBe(3200);
});
