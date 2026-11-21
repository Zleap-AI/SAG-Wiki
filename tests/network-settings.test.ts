// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {McpCredentials,Settings} from '../src/lib/types';
type SaveArgs=[Settings,string,string,Record<string,McpCredentials>?,string?];
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
const value:Settings={answer:{url:'https://api.302ai.cn/v1',model:'test',models:['test'],has_key:true},jev:{url:'https://api.zleap.com/v1',model:'jev',models:[],has_key:true},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:1000},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN')});
const field=(id:string)=>document.querySelector<HTMLInputElement|HTMLTextAreaElement>(`#network-proxy-${id}`)!;
const setInput=(element:HTMLInputElement|HTMLTextAreaElement,text:string)=>{element.value=text;element.dispatchEvent(new Event('input',{bubbles:true}));};
async function pickMode(mode:string){
 const control=document.querySelector<HTMLButtonElement>('.network-mode-select')!;
 control.focus();control.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${mode}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${mode}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
}
const submit=async()=>{document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();};
const customProxy={mode:'custom' as const,url:'http://proxy.corp:3128',username:'alice',bypass:'',has_password:true,password:null};

it('默认跟随系统代理；自定义时显示地址、认证和直连地址并随设置保存',async()=>{
 const onSave=vi.fn<(...args:SaveArgs)=>Promise<void>>(async()=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'network',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(document.querySelector('.network-settings')).not.toBeNull());
 expect(document.querySelector('.network-mode-select')?.textContent).toContain('跟随系统代理');
 expect(field('url')).toBeNull();
 expect(field('bypass')).not.toBeNull();
 await pickMode('custom');
 await vi.waitFor(()=>expect(field('url')).not.toBeNull());
 setInput(field('url'),'http://proxy.corp:3128 ');await tick();
 expect(document.querySelector('#network-proxy-url-help')?.textContent).toContain('HTTP 代理');
 setInput(field('bypass'),'.corp.example, 10.0.0.0/8');await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="代理需要认证"]')!.click();await tick();
 setInput(field('username'),'alice');setInput(field('password'),'secret');await tick();
 await submit();
 expect(onSave).toHaveBeenCalledTimes(1);
 expect(onSave.mock.calls[0][0].model_proxy).toEqual({mode:'custom',url:'http://proxy.corp:3128',username:'alice',bypass:'.corp.example, 10.0.0.0/8',has_password:false,password:'secret'});
 expect(value.model_proxy).toBeUndefined();
});

it('旧版只保存地址的设置按自定义代理显示；不使用代理时隐藏地址与直连地址',async()=>{
 const onSave=vi.fn<(...args:SaveArgs)=>Promise<void>>(async()=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,model_proxy:{url:'http://legacy.example:8080',bypass:'.office.example'} as any},initial:'network',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(field('url')).not.toBeNull());
 expect(document.querySelector('.network-mode-select')?.textContent).toContain('自定义代理');
 expect(field('url').value).toBe('http://legacy.example:8080');
 expect(field('bypass').value).toBe('.office.example');
 await pickMode('none');
 await vi.waitFor(()=>expect(field('bypass')).toBeNull());
 await submit();
 expect(onSave.mock.calls[0][0].model_proxy).toEqual({mode:'none',url:'http://legacy.example:8080',username:'',bypass:'.office.example',has_password:false,password:null});
});

it('粘贴带账号密码的地址时拆分到认证字段，关闭认证会清除',async()=>{
 const onSave=vi.fn<(...args:SaveArgs)=>Promise<void>>(async()=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,model_proxy:{...customProxy,url:'',username:'',has_password:false}},initial:'network',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(field('url')).not.toBeNull());
 setInput(field('url'),'socks5://bob:p%40ss@10.0.0.9:1080');await tick();
 expect(field('url').value).toBe('socks5://10.0.0.9:1080');
 expect(field('username').value).toBe('bob');
 expect(field('password').value).toBe('p@ss');
 expect(document.querySelector('#network-proxy-url-help')?.textContent).toContain('SOCKS5');
 document.querySelector<HTMLButtonElement>('[aria-label="代理需要认证"]')!.click();await tick();
 expect(field('username')).toBeNull();
 await submit();
 expect(onSave.mock.calls[0][0].model_proxy).toMatchObject({username:'',password:''});
});

it('已保存密码时留空保持原密码，并按草稿测试连接',async()=>{
 invoke.mockImplementation(async(command:string,args:any)=>{if(command==='test_proxy_connection')return `已通过代理 ${new URL(args.proxy.url).host} 连接 api.302ai.cn（HTTP 401）`;});
 const onSave=vi.fn<(...args:SaveArgs)=>Promise<void>>(async()=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,model_proxy:customProxy},initial:'network',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(field('password')).not.toBeNull());
 expect(field('password').getAttribute('placeholder')).toContain('留空保持原密码');
 document.querySelector<HTMLButtonElement>('.network-test-button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.network-test-result')?.textContent).toContain('已通过代理 proxy.corp:3128 连接 api.302ai.cn'));
 expect(invoke).toHaveBeenCalledWith('test_proxy_connection',{proxy:customProxy});
 await submit();
 expect(onSave.mock.calls[0][0].model_proxy?.password).toBeNull();
});

it('自定义模式缺少地址时阻止保存；改动输入会丢弃进行中的测试结果并显示本次错误',async()=>{
 let complete:(value:string)=>void=()=>{};
 invoke.mockImplementationOnce(()=>new Promise<string>(resolve=>{complete=resolve}));
 const onSave=vi.fn<(...args:SaveArgs)=>Promise<void>>(async()=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,model_proxy:{...customProxy,url:'',username:'',has_password:false}},initial:'network',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(field('url')).not.toBeNull());
 await submit();
 expect(onSave).not.toHaveBeenCalled();
 expect(document.querySelector('.settings-feedback .form-error')?.textContent).toContain('请填写代理地址');
 setInput(field('url'),'http://first.example:8080');await tick();
 document.querySelector<HTMLButtonElement>('.network-test-button')!.click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('.network-test-button')!.disabled).toBe(true));
 setInput(field('url'),'http://second.example:8080');await tick();
 complete('已通过代理 first.example:8080 连接 api.302ai.cn（HTTP 200）');await tick();
 expect(document.querySelector('.network-test-result')).toBeNull();
 invoke.mockRejectedValueOnce(new Error('无法连接代理 second.example:8080'));
 document.querySelector<HTMLButtonElement>('.network-test-button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.network-test-result.failed')?.textContent).toContain('无法连接代理 second.example:8080'));
});

it('英文界面不显示中文并提供三种模式',async()=>{
 setLanguage('en');
 app=mount(SettingsPanel,{target:document.body,props:{value:{...value,language:'en'},initial:'network',onSave:async()=>{},onClose:()=>{}}});
 await vi.waitFor(()=>expect(document.querySelector('.network-settings')).not.toBeNull());
 expect(document.querySelector('.network-settings')?.textContent).not.toMatch(/[㐀-鿿]/);
 await pickMode('custom');
 await vi.waitFor(()=>expect(field('url')).not.toBeNull());
 expect(document.querySelector('.network-settings')?.textContent).not.toMatch(/[㐀-鿿]/);
});
