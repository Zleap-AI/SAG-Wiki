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
 setLanguage('zh-CN');status={enabled:false,running:false,web_enabled:false,web_running:false,host:'0.0.0.0',port:24681,local_url:'http://127.0.0.1:24681',lan_urls:['http://192.168.1.10:24681'],key_count:0,autostart:false,last_error:null};keys=[];
 invoke.mockImplementation(async(command,{action}={})=>{
  if(command!=='manage_api')return;
  if(action.action==='status')return structuredClone(status);
  if(action.action==='list_keys')return structuredClone(keys);
  if(action.action==='configure'||action.action==='start'||action.action==='web_start'){
   status.host=action.host;status.port=action.port;
   status.lan_urls=action.host==='0.0.0.0'?[`http://192.168.1.10:${action.port}`]:[];
  }
  if(action.action==='start'){status.enabled=true;status.running=true;}
  if(action.action==='stop'){status.enabled=false;status.running=false;}
  if(action.action==='web_start'){status.web_enabled=true;status.web_running=true;}
  if(action.action==='web_stop'){status.web_enabled=false;status.web_running=false;}
  if(action.action==='create_key'){keys=[{id:'key-1',name:action.name,prefix:'sag_demo…0001',created_at:new Date(2026,10,14,12).getTime(),copyable:true}];status.key_count=1;return {...keys[0],key:'test-api-secret'};}
  if(action.action==='copy_key')return {key:'test-api-secret'};
  if(action.action==='revoke_key'){keys=[];status.key_count=0;}
  if(action.action==='autostart')status.autostart=action.enabled;
  return structuredClone(status);
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();vi.restoreAllMocks();});
async function ready(){await vi.waitFor(()=>expect(document.querySelector('[aria-label="API 和 MCP"]')).not.toBeNull());}
function click(text:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text);expect(button,text).toBeDefined();button!.click();}
it('网页可在 API 关闭时独立启用，并提供浏览器地址和访问码',async()=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 const toggle=document.querySelector<HTMLButtonElement>('[aria-label="网页服务"]')!;
 toggle.click();
 await vi.waitFor(()=>expect(document.querySelector('.web-service-settings [aria-label="打开网页"]')).not.toBeNull());
 expect(document.querySelector('[aria-label="API 和 MCP"]')?.getAttribute('aria-checked')).toBe('false');
 expect(document.querySelector('.api-endpoints .api-address')).toBeNull();
 expect(document.querySelector('.web-service-settings')?.textContent).toContain('复制访问码');
 toggle.click();
 await vi.waitFor(()=>expect(document.querySelector('.web-service-settings [aria-label="打开网页"]')).toBeNull());
});
it('启用无 Key 的 API，并即时保存自启状态',async()=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 document.querySelector<HTMLButtonElement>('[aria-label="API 和 MCP"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'start',host:'0.0.0.0',port:24681}}));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('运行中'));
 expect(document.body.textContent).toContain('未授权（401）');
 document.querySelector<HTMLButtonElement>('[aria-label="开机自启"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'autostart',enabled:true}}));
});
it('确认后更换网页访问码，不关闭网页服务',async()=>{
 status.web_enabled=status.web_running=true;
 app=mount(ApiSettings,{target:document.body});await ready();
 click('更换访问码');await tick();
 expect(document.querySelector('[role=dialog]')?.textContent).toContain('旧访问码将立即失效');
 expect(invoke).not.toHaveBeenCalledWith('manage_api',{action:{action:'web_rotate_code'}});
 click('确认更换');
 await vi.waitFor(()=>expect(document.body.textContent).toContain('访问码已更换'));
 expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'web_rotate_code'}});
 expect(document.querySelector('.web-service-settings [aria-label="打开网页"]')).not.toBeNull();
});
it('保存连接范围后，API 和 MCP 地址随下一次启动更新',async()=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 const scope=document.querySelector<HTMLButtonElement>('[aria-label="连接范围"]')!;
 const toggle=document.querySelector<HTMLButtonElement>('[aria-label="API 和 MCP"]')!;
 const addresses=()=>[...document.querySelectorAll('.api-endpoints .api-address code')].map(el=>el.textContent);
 expect(scope.textContent).toContain('本机与局域网');
 for(const [host,label,expected] of [
  ['127.0.0.1','仅本机',['http://127.0.0.1:24681','http://127.0.0.1:24681/mcp']],
  ['0.0.0.0','本机与局域网',['http://127.0.0.1:24681','http://192.168.1.10:24681','http://127.0.0.1:24681/mcp','http://192.168.1.10:24681/mcp']],
 ] as const){
  scope.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,pointerType:'mouse'}));
  const option=`[role=option][data-value="${host}"]`;
  await vi.waitFor(()=>expect(document.querySelector(option)?.textContent).toContain(label));
  document.querySelector(option)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
  click('保存连接配置');
  await vi.waitFor(()=>expect(document.body.textContent).toContain('连接配置已保存'));
  expect(invoke).toHaveBeenCalledWith('manage_api',{action:{action:'configure',host,port:24681}});
  expect(addresses()).toEqual([]);
  toggle.click();await vi.waitFor(()=>expect(addresses()).toEqual(expected));
  toggle.click();await vi.waitFor(()=>expect(addresses()).toEqual([]));
 }
});
it('创建后可复制 Key 和本机、局域网的完整 MCP 配置，撤销不关闭服务',async()=>{
 const clipboard=vi.fn();
 vi.spyOn(navigator.clipboard,'write').mockImplementation(async ([item])=>{clipboard(await (await item.getType('text/plain')).text());});
 status.enabled=status.running=true;
 app=mount(ApiSettings,{target:document.body});await ready();click('创建 Key');await tick();
 const input=document.querySelector<HTMLInputElement>('[aria-label="Key 名称"]')!;input.value='业务程序';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(b=>b.textContent==='创建 Key')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('test-api-secret'));
 [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(b=>b.textContent==='完成')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).not.toContain('test-api-secret'));
 expect(document.body.textContent).toContain('sag_demo…0001');
 expect(document.querySelector('.api-key-table tbody td:nth-child(3)')?.textContent).toBe('2026/11/14');
 document.querySelector<HTMLButtonElement>('[aria-label="复制 Key：业务程序"]')!.click();
 await vi.waitFor(()=>expect(clipboard).toHaveBeenLastCalledWith('test-api-secret'));
 for(const [label,url] of [['复制本机 MCP 配置','http://127.0.0.1:24681/mcp'],['复制局域网 MCP 配置','http://192.168.1.10:24681/mcp']]){
  const button=document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
  expect(button.closest('.api-address')?.querySelector('code')?.textContent).toBe(url);
  button.click();
  await vi.waitFor(()=>expect(JSON.parse(clipboard.mock.lastCall![0])).toEqual({mcpServers:{'sag-wiki':{type:'http',url,headers:{Authorization:'Bearer test-api-secret'}}}}));
 }
 click('撤销 Key');await tick();click('确认撤销');await vi.waitFor(()=>expect(document.body.textContent).toContain('未授权（401）'));
 expect(status.enabled).toBe(true);expect(status.running).toBe(true);expect(invoke.mock.calls.some(([,args])=>args?.action?.action==='stop')).toBe(false);
});
it('没有可复制的 Key 时，两种 MCP 配置都提示先创建 Key',async()=>{
 const clipboard=vi.spyOn(navigator.clipboard,'writeText').mockResolvedValue(undefined);
 const richClipboard=vi.spyOn(navigator.clipboard,'write').mockResolvedValue(undefined);
 status.enabled=status.running=true;
 app=mount(ApiSettings,{target:document.body});await ready();
 for(const label of ['复制本机 MCP 配置','复制局域网 MCP 配置']){
  document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click();
  await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toBe('请先创建 API Key'));
 }
 expect(clipboard).not.toHaveBeenCalled();
 expect(richClipboard).not.toHaveBeenCalled();
});
it('监听失败后显示错误，开关保持原状态',async()=>{
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,args)=>args?.action?.action==='start'?Promise.reject('端口已被占用'):original(command,args));
 app=mount(ApiSettings,{target:document.body});await ready();document.querySelector<HTMLButtonElement>('[aria-label="API 和 MCP"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('端口已被占用'));
 expect(document.querySelector('[aria-label="API 和 MCP"]')?.getAttribute('data-state')).toBe('unchecked');
});
it('服务运行时锁定地址和端口，文档入口不依赖启用状态',async()=>{
 status.enabled=status.running=true;app=mount(ApiSettings,{target:document.body});await ready();
 expect(document.querySelector<HTMLButtonElement>('[aria-label="连接范围"]')!.disabled).toBe(true);
 expect(document.querySelector<HTMLInputElement>('[aria-label="端口"]')!.disabled).toBe(true);
 const docs=document.querySelector<HTMLAnchorElement>('.api-docs-link a')!;expect(docs.href).toBe('http://sag-wiki.zleap.com/zh/docs/api');docs.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url:'http://sag-wiki.zleap.com/zh/docs/api'}));
});
it('设置侧栏与完成按钮连接 API 页面，不触发普通设置保存',async()=>{
 const value:Settings={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:2,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 const onSave=vi.fn(),onClose=vi.fn();app=mount(SettingsPanel,{target:document.body,props:{value,initial:'api',onSave,onClose}});await ready();
 expect(document.querySelector('.settings-nav [aria-current=page]')?.textContent).toContain('远程服务');
 expect(document.body.textContent).toContain('更改即时生效');expect(document.querySelector('.settings-footer')?.textContent).not.toContain('保存设置');
 setLanguage('en');await tick();expect(document.querySelector('.settings-footer')?.textContent).toContain('Done');
 click('Done');expect(onClose).toHaveBeenCalledOnce();expect(onSave).not.toHaveBeenCalled();
});
it('英文界面显示 API 控件、英文文档链接与已有 Key 的创建日期',async()=>{
 keys=[{id:'english-key',name:'Custom label',prefix:'sag_demo…0001',created_at:new Date(2026,10,13,12).getTime()}];
 setLanguage('en');app=mount(ApiSettings,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('[aria-label="API and MCP"]')).not.toBeNull());
 expect(document.querySelector<HTMLAnchorElement>('.api-docs-link a')!.href).toBe('http://sag-wiki.zleap.com/en/docs/api');
 expect(document.body.textContent).toContain('Create key');
 expect(document.querySelector('.api-key-table th:first-child')?.textContent).toBe('Name');
 expect(document.querySelector('.api-key-table th:last-child')?.textContent).toBe('Actions');
 expect(document.querySelector('.api-key-table tbody td:nth-child(3)')?.textContent).toBe('11/13/2026');
});

it.each([
 ['API 和 MCP','start',''],['API 和 MCP','start','0'],['API 和 MCP','start','65536'],['API 和 MCP','start','12.5'],
 ['网页服务','web_start',''],['网页服务','web_start','0'],['网页服务','web_start','65536'],['网页服务','web_start','12.5'],
])('%s 拒绝无效端口 %s/%s',async(label,action,value)=>{
 app=mount(ApiSettings,{target:document.body});await ready();
 const port=document.querySelector<HTMLInputElement>('[aria-label="端口"]')!;port.value=value;port.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('有效端口'));
 expect(invoke.mock.calls.some(([,args])=>args?.action?.action===action)).toBe(false);
 expect(document.querySelector(`[aria-label="${label}"]`)?.getAttribute('data-state')).toBe('unchecked');
});

it.each([
 {enabled:true,running:true,label:'Running'},
 {enabled:true,running:false,label:'Enabled, not running'},
 {enabled:false,running:false,label:'Stopped'},
])('英文状态显示 $label',async({enabled,running,label})=>{
 setLanguage('en');status.enabled=enabled;status.running=running;
 app=mount(ApiSettings,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.api-endpoints [role=status]')?.textContent).toBe(label));
 expect(document.querySelectorAll('.api-endpoints .api-address')).toHaveLength(running?4:0);
});
