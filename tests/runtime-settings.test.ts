// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import RuntimeSettings from '../src/components/RuntimeSettings.svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import type {Settings} from '../src/lib/types';
const {api}=vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
it('loads backend defaults, edits only current group and resets controls',async()=>{
 api.mockResolvedValue(JSON.stringify({defaults:{docling_timeout_secs:1800,docling_threads:2,jev_concurrency:20},fields:[
 {key:'docling_timeout_secs',label:'Docling 转换时限',unit:'秒',help:'新任务生效',min:10,max:86400,group:'files',advanced:false},
 {key:'docling_threads',label:'Docling 推理线程',unit:'',help:'',min:1,max:64,group:'files',advanced:true},
 {key:'jev_concurrency',label:'JEV 请求并发',unit:'',help:'',min:1,max:64,group:'retrieval',advanced:true}]}));
 app=mount(RuntimeSettings,{target:document.body,props:{group:'files',value:{docling_timeout_secs:3600}}});
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="Docling 转换时限"]')?.value).toBe('3600'));
 expect(document.querySelector('[aria-label="JEV 请求并发"]')).toBeNull();
 expect(document.querySelector<HTMLDetailsElement>('details')?.open).toBe(false);
 const input=document.querySelector<HTMLInputElement>('[aria-label="Docling 转换时限"]')!;
 input.value='7200';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(input.value).toBe('7200');
 [...document.querySelectorAll('button')].find(b=>b.textContent?.includes('恢复本组默认'))!.click();await tick();expect(input.value).toBe('1800');
});
it('reports schema loading failure without discarding saved values',async()=>{
 api.mockRejectedValue(new Error('offline'));
 app=mount(RuntimeSettings,{target:document.body,props:{group:'files',value:{docling_timeout_secs:3600}}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('offline'));
});

it('saves OCR timeout and memory budgets from File settings and restores the saved values',async()=>{
 const value:Settings={answer:{url:'https://api.example.com/v1',model:'demo',models:['demo'],has_key:false},jev:{url:'https://api.example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 const defaults={docling_timeout_secs:1800,conversion_memory_percent:50,conversion_job_mib:0};
 const fields=[
  {key:'docling_timeout_secs',label:'Docling 转换超时',unit:'秒',help:'',min:1,max:86400,group:'files',advanced:false},
  {key:'conversion_memory_percent',label:'转换总内存预算',unit:'%',help:'',min:10,max:80,group:'files',advanced:true},
  {key:'conversion_job_mib',label:'单任务内存预算',unit:'MiB',help:'',min:0,max:262144,group:'files',advanced:true},
 ];
 api.mockImplementation(async command=>command==='runtime_schema'?JSON.stringify({defaults,fields}):command==='list_removed_directories'?[]:undefined);
 const onSave=vi.fn(async(_settings:Settings)=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'files',onClose:()=>{},onSave}});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="Docling 转换超时"]')).not.toBeNull());
 const change=(label:string,value:string)=>{const input=document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}))};
 change('Docling 转换超时','60');change('转换总内存预算','25');change('单任务内存预算','512');await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='保存设置')!.click();
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalled());
 const saved=onSave.mock.calls[0][0] as Settings;
 expect(saved.runtime).toMatchObject({docling_timeout_secs:60,conversion_memory_percent:25,conversion_job_mib:512});
 await unmount(app);document.body.innerHTML='';
 app=mount(SettingsPanel,{target:document.body,props:{value:saved,initial:'files',onClose:()=>{},onSave}});
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="单任务内存预算"]')?.value).toBe('512'));
 expect(document.querySelector<HTMLInputElement>('[aria-label="Docling 转换超时"]')?.value).toBe('60');
 expect(document.querySelector<HTMLInputElement>('[aria-label="转换总内存预算"]')?.value).toBe('25');
});

it('search runtime controls belong to Search settings rather than Web Clipper',async()=>{
 const value:Settings={answer:{url:'https://api.example.com/v1',model:'demo',models:['demo'],has_key:false},jev:{url:'https://api.example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 api.mockImplementation(async command=>command==='runtime_schema'?JSON.stringify({defaults:{jev_concurrency:20},fields:[{key:'jev_concurrency',label:'JEV 请求并发数',unit:'',help:'',min:1,max:64,group:'retrieval',advanced:true}]}):undefined);
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'retrieval',onClose:()=>{},onSave:async()=>{}}});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="JEV 请求并发数"]')).not.toBeNull());
 const clipper=[...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent?.includes('网页剪藏'))!;clipper.click();await tick();
 expect(document.querySelector('[aria-label="JEV 请求并发数"]')).toBeNull();
});
