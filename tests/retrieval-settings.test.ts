// @vitest-environment happy-dom
import{it,expect,vi}from 'vitest';import{mount,unmount,tick}from 'svelte';import SettingsPanel from '../src/components/Settings.svelte';import type{Settings}from '../src/lib/types';
it('allows context budgets above 128000 without imposing a numeric maximum',async()=>{
 const value:Settings={answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:2,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 const onSave=vi.fn().mockResolvedValue(undefined);const app=mount(SettingsPanel,{target:document.body,props:{value,initial:'answer',onClose:()=>{},onSave}});
 try{
  await vi.waitFor(()=>expect(document.querySelector('#answer-context')).not.toBeNull());
  const input=document.querySelector<HTMLInputElement>('#answer-context')!;
  for(const budget of [131072,200000,1048576]){
   input.value=String(budget);input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
   expect(input.checkValidity()).toBe(true);
   document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
   expect(onSave.mock.calls.at(-1)![0].answer_context_tokens).toBe(budget);
  }
  expect(input.max).toBe('');
 }finally{await unmount(app);document.body.innerHTML=''}
});
it('恢复默认设置使用 V10 参数',async()=>{
 const value:Settings={answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:3,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:16000,answer_output_tokens:2048};
 const onSave=vi.fn();const app=mount(SettingsPanel,{target:document.body,props:{value,initial:'retrieval',onClose:()=>{},onSave}});
 try{
  await vi.waitFor(()=>expect([...document.querySelectorAll('button')].some(button=>button.textContent==='恢复默认参数')).toBe(true));
  [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='恢复默认参数')!.click();await tick();
  expect(document.querySelector<HTMLInputElement>('[aria-label="扩展轮数"]')!.value).toBe('1');
  expect(document.querySelector<HTMLInputElement>('[aria-label="每轮候选数量"]')!.value).toBe('50');
  expect(document.querySelector<HTMLInputElement>('[aria-label="片段目标字符数"]')!.value).toBe('512');
  document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();expect(onSave.mock.calls[0][0].retrieval.max_hops).toBe(1);expect(onSave.mock.calls[0][0].retrieval.candidate_k).toBe(50);
  expect(onSave.mock.calls[0][0].retrieval.chunk_chars).toBe(512);
 }finally{await unmount(app);document.body.innerHTML=''}
});
it('索引并行数默认八并保存用户设置',async()=>{
 const value:Settings={answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:16000,answer_output_tokens:2048};
 const onSave=vi.fn();const app=mount(SettingsPanel,{target:document.body,props:{value,initial:'files',onClose:()=>{},onSave}});
 try{
  await vi.waitFor(()=>expect(document.querySelector('[aria-label="索引并行数"]')).not.toBeNull());
  const input=document.querySelector<HTMLInputElement>('[aria-label="索引并行数"]')!;
  expect(input.value).toBe('8');
  input.value='14';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
  expect(onSave.mock.calls[0][0].index_workers).toBe(14);
  expect(onSave.mock.calls[0][0].retrieval).toEqual(value.retrieval);
 }finally{await unmount(app);document.body.innerHTML=''}
});
