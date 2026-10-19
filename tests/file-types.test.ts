// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {Settings} from '../src/lib/types';
const value:Settings={answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:1000},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';setLanguage('zh-CN')});
it('默认只勾选 Markdown 和文本，保存后记住手动开启的类别',async()=>{
 const onSave=vi.fn(async(_settings:Settings)=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'files',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(document.querySelectorAll('.file-types [role=checkbox]')).toHaveLength(4));
 expect(document.querySelector('.file-types .settings-group-heading button')?.textContent).toBe('重新索引');
 for(const kind of ['markdown','text'])expect(document.querySelector(`#file-type-${kind}`)?.getAttribute('aria-checked')).toBe('true');
 for(const kind of ['code','config'])expect(document.querySelector(`#file-type-${kind}`)?.getAttribute('aria-checked')).toBe('false');
 (document.querySelector('#file-type-code') as HTMLButtonElement).click();await tick();
 expect(document.querySelector('#file-type-code')?.getAttribute('aria-checked')).toBe('true');
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].file_types).toEqual({markdown:true,text:true,code:true,config:false});
 expect(value.file_types).toBeUndefined();
});
it('恢复已保存的选择，取消不会保存，并支持英文',async()=>{
 setLanguage('en');
 const onSave=vi.fn(async()=>{}),onClose=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,language:'en',file_types:{markdown:true,text:true,code:false,config:true}},initial:'files',onSave,onClose}});
 await vi.waitFor(()=>expect(document.querySelector('#file-type-code')?.getAttribute('aria-checked')).toBe('false'));
 expect(document.querySelector('#file-type-config')?.getAttribute('aria-checked')).toBe('true');
 expect(document.querySelector('.file-types')?.textContent).toContain('File types to import');
 expect(document.querySelector('.file-types')?.textContent).not.toMatch(/[\u3400-\u9fff]/);
 (document.querySelector('#file-type-code') as HTMLButtonElement).click();await tick();
 (document.querySelector('.settings-cancel') as HTMLButtonElement).click();await tick();
 expect(onClose).toHaveBeenCalled();expect(onSave).not.toHaveBeenCalled();
});
