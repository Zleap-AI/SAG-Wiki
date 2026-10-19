// @vitest-environment happy-dom
import {afterEach,beforeEach,describe,it,expect,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import App from '../src/App.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:string})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
beforeEach(async()=>{
 invoke.mockImplementation(async(command)=>command==='get_settings'?{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('app-error')).toBe(true));
 vi.useFakeTimers();
});
afterEach(async()=>{await unmount(app);vi.useRealTimers();document.body.innerHTML='';vi.clearAllMocks()});
async function notify(message:string){listeners.get('app-error')!({payload:message});await tick()}
describe('短暂通知',()=>{
 it('显示三秒后自动关闭',async()=>{
  await notify('测试提示');expect(document.querySelector('[role="status"]')?.textContent).toContain('测试提示');
  await vi.advanceTimersByTimeAsync(2999);expect(document.querySelector('#notice')).not.toBeNull();
  await vi.advanceTimersByTimeAsync(1);expect(document.querySelector('#notice')).toBeNull();
 });
 it('新提示有完整的展示时间',async()=>{
  await notify('第一条');await vi.advanceTimersByTimeAsync(2000);await notify('第二条');
  await vi.advanceTimersByTimeAsync(1000);expect(document.querySelector('#notice')?.textContent).toContain('第二条');
  await vi.advanceTimersByTimeAsync(2000);expect(document.querySelector('#notice')).toBeNull();
 });
 it('鼠标停在提示上时暂停关闭，移开后继续计时',async()=>{
  await notify('测试提示');document.querySelector('#notice')!.dispatchEvent(new MouseEvent('mouseenter'));
  await vi.advanceTimersByTimeAsync(4000);expect(document.querySelector('#notice')).not.toBeNull();
  document.querySelector('#notice')!.dispatchEvent(new MouseEvent('mouseleave'));
  await vi.advanceTimersByTimeAsync(3000);expect(document.querySelector('#notice')).toBeNull();
 });
 it('可以手动关闭',async()=>{
  await notify('测试提示');document.querySelector<HTMLButtonElement>('[aria-label="关闭提示"]')!.click();await tick();
  expect(document.querySelector('#notice')).toBeNull();
  await vi.advanceTimersByTimeAsync(3000);expect(document.querySelector('#notice')).toBeNull();
 });
});
