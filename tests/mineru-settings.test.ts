// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import MineruSettings from '../src/components/MineruSettings.svelte';
import type {MineruConfig} from '../src/lib/types';
const {api}=vi.hoisted(()=>({api:vi.fn()}));vi.mock('../src/lib/api',()=>({api}));
let app:ReturnType<typeof mount>;
const value:MineruConfig={enabled:false,provider:'302',has_key:false,request_timeout_secs:120,task_timeout_secs:1800,poll_interval_secs:2,concurrency:2,model_version:'vlm',ocr:true,table:true,formula:true};
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
it('offers API key only and tests unsaved provider settings without saving',async()=>{
 api.mockImplementation((command)=>Promise.resolve(command==='mineru_schema'?JSON.stringify({defaults:value,keys:{},endpoints:{'302':'https://api.302ai.cn',official:'https://mineru.net/api/v4'}}):'服务可达；未验证密钥有效性')); app=mount(MineruSettings,{target:document.body,props:{value:{...value}}});await tick();
 expect(document.querySelector('input[type=url]')).toBeNull();expect(document.querySelector('[aria-label="启用 MinerU"]')).toBeNull();
 const key=document.querySelector<HTMLInputElement>('[aria-label="MinerU API Key"]')!;key.value='test-key';key.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll('button')].find(b=>b.textContent==='测试连接')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_mineru',expect.objectContaining({key:'test-key',config:expect.objectContaining({provider:'302'})})));
 expect(api.mock.calls.some(([command])=>command==='save_settings')).toBe(false);
 await vi.waitFor(()=>expect(document.querySelector('.mineru-test-area')?.textContent).toContain('未验证密钥有效性'));
 expect(document.body.textContent).toContain('https://api.302ai.cn');
});
it('clears draft key on provider switch and requires the matching key',async()=>{
 app=mount(MineruSettings,{target:document.body,props:{value:{...value},apiKey:'302-secret'}});await tick();
 const select=document.querySelector<HTMLButtonElement>('[aria-label="MinerU 服务商"]')!;expect(select.tagName).toBe('BUTTON');select.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));await tick();
 await vi.waitFor(()=>expect(document.querySelector('[role=option]')).not.toBeNull());
 [...document.querySelectorAll<HTMLElement>('[role=option]')].find(el=>el.textContent?.includes('MinerU 官方'))!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
 expect(document.querySelector<HTMLInputElement>('[aria-label="MinerU API Key"]')!.value).toBe('');
 [...document.querySelectorAll('button')].find(b=>b.textContent==='测试连接')!.click();await tick();
 expect(api).not.toHaveBeenCalledWith('test_mineru',expect.anything());expect(document.body.textContent).toContain('请填写');
});

it('shows failed connection beside the test action',async()=>{
 api.mockImplementation(command=>command==='test_mineru'?Promise.reject('HTTP 403'):Promise.resolve(JSON.stringify({defaults:value,keys:{}})));
 app=mount(MineruSettings,{target:document.body,props:{value:{...value},apiKey:'draft-key'}});await tick();
 [...document.querySelectorAll('button')].find(b=>b.textContent==='测试连接')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.mineru-test-area [role=alert]')?.textContent).toContain('HTTP 403'));
 expect(document.querySelector('details [role=alert]')).toBeNull();
});

it('exposes open-source version and parsing method without official model switches',async()=>{
 api.mockImplementation(()=>Promise.resolve(JSON.stringify({defaults:value,keys:{},endpoints:{'302':'https://api.302ai.cn'}})));
 app=mount(MineruSettings,{target:document.body,props:{value:{...value,provider:'302',version:'2.5',parse_method:'auto'}}});await tick();
 expect(document.querySelector('[aria-label="MinerU 版本"]')).not.toBeNull();expect(document.querySelector('[aria-label="解析方式"]')).not.toBeNull();expect(document.querySelector('[aria-label="MinerU 解析模型"]')).toBeNull();
 expect(document.body.textContent).not.toContain('官方免费版代理');
});
