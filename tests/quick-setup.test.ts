// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import QuickSetup from '../src/components/QuickSetup.svelte';
import App from '../src/App.svelte';
import type {MineruConfig,Settings} from '../src/lib/types';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));

const mineru:MineruConfig={enabled:false,provider:'official',base_url:'https://mineru.net/api/v4',has_key:false,request_timeout_secs:120,task_timeout_secs:1800,poll_interval_secs:2,concurrency:2,model_version:'vlm',version:'2.5',parse_method:'auto',ocr:true,table:true,formula:true};
const settings:Settings={language:'zh-CN',theme:'light',start_page:'ask',editor_size:15,answer:{url:'https://api.302ai.cn/v1',model:'qwen3.6-flash',models:['qwen3.6-flash'],has_key:false},jev:{url:'https://api.zleap.com/v1',model:'jev-latest',models:['jev-latest'],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},excluded:[],show_hidden:false,watch:true,timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0,mineru,convert_other_formats:false,conversion_formats:null};
let app:ReturnType<typeof mount>|undefined;
const onSave=vi.fn(async(_settings:Settings,_answer:string,_jev:string,_credentials?:Record<string,unknown>,_mineruKey?:string)=>{});
beforeEach(()=>{onSave.mockClear();invoke.mockReset();localStorage.clear()});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks()});
const button=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()===label);
const password=()=>document.querySelector<HTMLInputElement>('input[type=password]')!;
async function type(text:string){const input=password();input.value=text;input.dispatchEvent(new Event('input',{bubbles:true}));await tick()}
function mountDialog(value:Settings=settings){app=mount(QuickSetup,{target:document.body,props:{settings:structuredClone(value),onClose:vi.fn(),onSave,onManual:vi.fn()}})}

it('tests the answer model, Jev and MinerU before saving one key',async()=>{
 invoke.mockResolvedValue('连接成功');
 mountDialog();await tick();
 await type('key-123');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalledTimes(1));
 const answer=invoke.mock.calls.find(([command,args])=>command==='test_model'&&args.kind==='answer');
 expect(answer![1].config).toMatchObject({url:'https://api.302ai.cn/v1',model:'qwen3.8-flash',models:['qwen3.8-flash','qwen3.6-flash'],auth_mode:'bearer'});
 expect(answer![1].key).toBe('key-123');
 const jev=invoke.mock.calls.find(([command,args])=>command==='test_model'&&args.kind==='jev');
 expect(jev![1].config).toMatchObject({url:'https://api.302ai.cn/typesafeai/v1',model:'jev-latest'});
 expect(invoke.mock.calls.some(([command,args])=>command==='test_mineru'&&args.config.provider==='302'&&args.key==='key-123')).toBe(true);
 const [saved,answerKey,jevKey,credentials,mineruKey]=onSave.mock.calls[0];
 expect(saved.answer.model).toBe('qwen3.8-flash');
 expect(saved.document_parser).toBe('mineru');
 expect(saved.convert_other_formats).toBe(true);
 expect(saved.conversion_formats).toEqual(['pdf']);
 expect(saved.mineru).toMatchObject({provider:'302',base_url:''});
 expect([answerKey,jevKey,mineruKey]).toEqual(['key-123','key-123','key-123']);
 expect(credentials).toEqual({});
});

it('keeps the key and saves nothing when a test fails',async()=>{
 invoke.mockImplementation(async command=>{if(command==='test_model')throw new Error('认证失败');return '连接成功'});
 mountDialog();await tick();
 await type('bad-key');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.form-error')?.textContent).toContain('认证失败'));
 expect(onSave).not.toHaveBeenCalled();
 expect(invoke.mock.calls.some(([command])=>command==='save_settings')).toBe(false);
 expect(password().value).toBe('bad-key');
});

it('shows one key field with a visibility toggle and lists what will be written',async()=>{
 invoke.mockResolvedValue('连接成功');
 mountDialog();await tick();
 expect(document.querySelector('.quick-setup-dialog h2')?.textContent).toBe('快速配置');
 expect(document.querySelector('.quick-setup-plan')?.textContent).toContain('MinerU · 302.AI');
 expect(document.querySelector('.quick-setup-plan')?.textContent).toContain('qwen3.8-flash');
 await type('key-123');
 expect(password().type).toBe('password');
 document.querySelector<HTMLElement>('[aria-label="显示密钥"]')!.click();await tick();
 expect(document.querySelector<HTMLInputElement>('.quick-setup-key input')!.type).toBe('text');
 expect(document.querySelector('[aria-label="隐藏密钥"]')).not.toBeNull();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes('获取 API Key'))!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_about_link',{target:'302-api-keys'}));
 expect(onSave).not.toHaveBeenCalled();
});

it('keeps an already configured MinerU and Jev untouched',async()=>{
 invoke.mockResolvedValue('连接成功');
 mountDialog({...settings,jev:{...settings.jev,has_key:true},mineru:{...mineru,provider:'302',has_key:true,enabled:true}});await tick();
 expect([...document.querySelectorAll('.quick-setup-plan dd.kept')].map(node=>node.textContent)).toEqual(['已配置，保持不变','已配置，保持不变']);
 await type('key-123');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalledTimes(1));
 expect(invoke.mock.calls.filter(([command,args])=>command==='test_model'&&args.kind==='jev')).toEqual([]);
 expect(invoke.mock.calls.some(([command])=>command==='test_mineru')).toBe(false);
 const [saved,,jevKey,,mineruKey]=onSave.mock.calls[0];
 expect(saved.jev.url).toBe('https://api.zleap.com/v1');
 expect(jevKey).toBe('');
 expect(mineruKey).toBe('');
});

it('configures MinerU again when a key is stored but parsing is not routed to it',async()=>{
 invoke.mockResolvedValue('连接成功');
 mountDialog({...settings,mineru:{...mineru,provider:'302',has_key:true,enabled:false}});await tick();
 expect(document.querySelector('.quick-setup-plan')?.textContent).toContain('MinerU · 302.AI');
 await type('key-123');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalledTimes(1));
 expect(invoke.mock.calls.some(([command])=>command==='test_mineru')).toBe(true);
 const [saved,,,,mineruKey]=onSave.mock.calls[0];
 expect(saved.document_parser).toBe('mineru');
 expect(mineruKey).toBe('key-123');
});

it('does not save when the dialog is closed while a test is still running',async()=>{
 let finish!:(value:string)=>void;
 invoke.mockImplementation(async command=>command==='test_model'?new Promise<string>(resolve=>{finish=resolve}):'连接成功');
 mountDialog();await tick();
 await type('key-123');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'answer'})));
 expect(document.querySelector<HTMLButtonElement>('[aria-label="关闭"]')!.disabled).toBe(true);
 await unmount(app!);app=undefined;
 finish('连接成功');await tick();await tick();
 expect(onSave).not.toHaveBeenCalled();
});

it('uses Quick setup in the model slot until an answer model is configured',async()=>{
 const load=(value:Settings)=>invoke.mockImplementation(async command=>{
  if(command==='get_settings')return value;
  if(command==='list_conversations')return [];
  return [];
 });
 localStorage.setItem('sag.onboarding.seen','1');
 load(settings);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome')).not.toBeNull());
 expect(document.querySelector('.composer-model-controls .quick-setup-action')?.textContent).toBe('快速配置');
 expect(document.querySelector('.composer-model-controls .reasoning-button')).toBeNull();
 expect(document.querySelector('.welcome p, .welcome button')).toBeNull();
 expect(document.querySelectorAll('.quick-setup-action')).toHaveLength(1);
 expect(document.querySelector('.folder-empty-state button')?.textContent?.trim()).toBe('添加文件夹');
 await unmount(app);document.body.innerHTML='';
 load({...settings,answer:{...settings.answer,has_key:true}});
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome')).not.toBeNull());
 expect(document.querySelector('.quick-setup-action')).toBeNull();
 expect(document.querySelector('.composer-model-controls .reasoning-trigger-model')?.textContent).toBe(settings.answer.model);
 expect(document.querySelector('.setup-hint')?.textContent).toContain('未配置 Jev');
});

it('opens the dialog from the model slot and shows the configured model after saving',async()=>{
 localStorage.setItem('sag.onboarding.seen','1');
 let savedSettings=settings;
 invoke.mockImplementation(async (command,args)=>{
  if(command==='get_settings')return savedSettings;
  if(command==='list_conversations')return [];
  if(command==='save_settings'){
   const saved:Settings=args.settings;
   savedSettings={...saved,answer:{...saved.answer,has_key:true},jev:{...saved.jev,has_key:true}};
  }
  if(command==='test_model'||command==='test_mineru'||command==='save_settings')return '连接成功';
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.quick-setup-action')).not.toBeNull());
 document.querySelector<HTMLElement>('.quick-setup-action')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.quick-setup-dialog')).not.toBeNull());
 await type('key-123');
 button('测试并保存')!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='save_settings').length).toBe(1));
 const [,args]=invoke.mock.calls.find(([command])=>command==='save_settings')!;
 expect(args).toMatchObject({answerKey:'key-123',jevKey:'key-123',mineruKey:'key-123'});
 await vi.waitFor(()=>expect(document.querySelector('.quick-setup-dialog')).toBeNull());
 expect(document.querySelector('#notice')?.textContent).toContain('快速配置已完成');
 expect(document.querySelector('.quick-setup-action')).toBeNull();
 expect(document.querySelector('.composer-model-controls .reasoning-trigger-model')?.textContent).toBe('qwen3.8-flash');
});
