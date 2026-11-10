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
 api.mockResolvedValue('服务可达；未验证密钥有效性');app=mount(MineruSettings,{target:document.body,props:{value:{...value}}});await tick();
 expect(document.querySelector('input[type=url]')).toBeNull();
 const key=document.querySelector<HTMLInputElement>('[aria-label="MinerU API Key"]')!;key.value='test-key';key.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll('button')].find(b=>b.textContent==='测试连接')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_mineru',expect.objectContaining({key:'test-key',config:expect.objectContaining({provider:'302'})})));
 expect(api.mock.calls.some(([command])=>command==='save_settings')).toBe(false);
 await vi.waitFor(()=>expect(document.body.textContent).toContain('未验证密钥有效性'));
});
it('clears draft key on provider switch and requires the matching key',async()=>{
 app=mount(MineruSettings,{target:document.body,props:{value:{...value},apiKey:'302-secret'}});await tick();
 const select=document.querySelector<HTMLSelectElement>('[aria-label="MinerU 服务商"]')!;select.value='official';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 expect(document.querySelector<HTMLInputElement>('[aria-label="MinerU API Key"]')!.value).toBe('');
 [...document.querySelectorAll('button')].find(b=>b.textContent==='测试连接')!.click();await tick();
 expect(api).not.toHaveBeenCalledWith('test_mineru',expect.anything());expect(document.body.textContent).toContain('请填写');
});
