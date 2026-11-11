// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import ApiSettings from '../src/components/ApiSettings.svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
let status:any,keys:any[];
beforeEach(()=>{
 setLanguage('zh-CN');status={enabled:false,running:false,host:'0.0.0.0',port:24681,local_url:'http://127.0.0.1:24681',lan_urls:['http://192.168.1.10:24681'],key_count:0,autostart:false,last_error:null};keys=[];
 invoke.mockImplementation(async(command,{action}={})=>{
  if(command!=='manage_api')return;
  if(action.action==='status')return structuredClone(status);
  if(action.action==='list_keys')return structuredClone(keys);
  if(action.action==='start'){status.enabled=true;status.running=true;}
  if(action.action==='stop'){status.enabled=false;status.running=false;}
  if(action.action==='create_key'){keys=[{id:'key-1',name:action.name,prefix:'sag_demo…0001',created_at:1700000000}];status.key_count=1;return {...keys[0],key:'test-ephemeral-secret'};}
  if(action.action==='revoke_key'){keys=[];status.key_count=0;}
  if(action.action==='autostart')status.autostart=action.enabled;
  return structuredClone(status);
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();vi.restoreAllMocks();});
async function ready(){await vi.waitFor(()=>expect(document.querySelector('[aria-label="启用 API"]')).not.toBeNull());}
function click(text:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text);expect(button,text).toBeDefined();button!.click();}
it('启用无 Key 的 API，并即时保存自启状态',async()=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 document.querySelector<HTMLButtonElement>('[aria-label="启用 API"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'start',host:'0.0.0.0',port:24681}}));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('运行中'));
 expect(document.body.textContent).toContain('未授权（401）');
 document.querySelector<HTMLButtonElement>('[aria-label="开机自启"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'autostart',enabled:true}}));
});
it('完整 Key 仅创建后展示，撤销最后一个 Key 保留 API 启用状态',async()=>{
 status.enabled=status.running=true;
 app=mount(ApiSettings,{target:document.body});await ready();click('创建 Key');await tick();
 const input=document.querySelector<HTMLInputElement>('[aria-label="Key 名称"]')!;input.value='业务程序';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(b=>b.textContent==='创建 Key')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('test-ephemeral-secret'));
 click('我已保存');await vi.waitFor(()=>expect(document.body.textContent).not.toContain('test-ephemeral-secret'));
 expect(document.body.textContent).toContain('sag_demo…0001');
 click('撤销 Key');await tick();click('确认撤销');await vi.waitFor(()=>expect(document.body.textContent).toContain('未授权（401）'));
 expect(status.enabled).toBe(true);expect(status.running).toBe(true);expect(invoke.mock.calls.some(([,args])=>args?.action?.action==='stop')).toBe(false);
});
it('监听失败后显示错误，开关保持原状态',async()=>{
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,args)=>args?.action?.action==='start'?Promise.reject('端口已被占用'):original(command,args));
 app=mount(ApiSettings,{target:document.body});await ready();document.querySelector<HTMLButtonElement>('[aria-label="启用 API"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('端口已被占用'));
 expect(document.querySelector('[aria-label="启用 API"]')?.getAttribute('data-state')).toBe('unchecked');
});
it('服务运行时锁定地址和端口，文档入口不依赖启用状态',async()=>{
 status.enabled=status.running=true;app=mount(ApiSettings,{target:document.body});await ready();
 expect(document.querySelector<HTMLInputElement>('[aria-label="监听地址"]')!.disabled).toBe(true);
 expect(document.querySelector<HTMLInputElement>('[aria-label="端口"]')!.disabled).toBe(true);
 const docs=document.querySelector<HTMLAnchorElement>('.api-docs-link a')!;expect(docs.href).toBe('http://sag-wiki.zleap.com/zh/docs/api');docs.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url:'http://sag-wiki.zleap.com/zh/docs/api'}));
});
it('设置侧栏与完成按钮连接 API 页面，不触发普通设置保存',async()=>{
 const value:Settings={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:2,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 const onSave=vi.fn(),onClose=vi.fn();app=mount(SettingsPanel,{target:document.body,props:{value,initial:'api',onSave,onClose}});await ready();
 expect(document.querySelector('.settings-nav [aria-current=page]')?.textContent).toContain('API 服务');
 expect(document.body.textContent).toContain('更改即时生效');expect(document.querySelector('.settings-footer')?.textContent).not.toContain('保存设置');
 setLanguage('en');await tick();expect(document.querySelector('.settings-footer')?.textContent).toContain('Done');
 click('Done');expect(onClose).toHaveBeenCalledOnce();expect(onSave).not.toHaveBeenCalled();
});
it('英文界面显示 API 控件与英文文档链接',async()=>{
 keys=[{id:'english-key',name:'Custom label',prefix:'sag_demo…0001',created_at:1700000000}];
 setLanguage('en');app=mount(ApiSettings,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('[aria-label="Enable API"]')).not.toBeNull());
 expect(document.querySelector<HTMLAnchorElement>('.api-docs-link a')!.href).toBe('http://sag-wiki.zleap.com/en/docs/api');
 expect(document.body.textContent).toContain('Create key');
 expect(document.querySelector('.api-key-table th:first-child')?.textContent).toBe('Name');
 expect(document.querySelector('.api-key-table th:last-child')?.textContent).toBe('Actions');
});

it('端口为空时不以旧端口启动服务',async()=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 const port=document.querySelector<HTMLInputElement>('[aria-label="端口"]')!;port.value='';port.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="启用 API"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('有效端口'));
 expect(invoke.mock.calls.some(([,args])=>args?.action?.action==='start')).toBe(false);
 expect(document.querySelector('[aria-label="启用 API"]')?.getAttribute('data-state')).toBe('unchecked');
});

it.each([
 {enabled:true,running:true,label:'Running'},
 {enabled:true,running:false,label:'Enabled, not running'},
 {enabled:false,running:false,label:'Stopped'},
])('英文状态显示 $label',async({enabled,running,label})=>{
 setLanguage('en');status.enabled=enabled;status.running=running;
 app=mount(ApiSettings,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.api-settings .setting-row [role=status]')?.textContent).toBe(label));
});
