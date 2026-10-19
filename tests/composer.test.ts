// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {mount, unmount, tick} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app: ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function composer(){
 invoke.mockImplementation(async(command:string)=>command==='get_settings'?{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}}:command==='run_query'?new Promise(()=>{}):[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('textarea')).not.toBeNull());
 const input=document.querySelector('textarea')!;
 input.value='中文测试';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 return input;
}
const requests=()=>invoke.mock.calls.filter(([command])=>command==='run_query');
describe('输入法回车',()=>{
 it('候选确认事件中 isComposing 为 false、keyCode 为 229 时不发送',async()=>{const input=await composer();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',keyCode:229,bubbles:true,cancelable:true}));expect(requests()).toHaveLength(0)});
 it('compositionend 与确认回车相邻时不发送，下一次正常回车才发送',async()=>{const input=await composer();input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));expect(requests()).toHaveLength(0);input.dispatchEvent(new KeyboardEvent('keyup',{key:'Enter',bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));expect(requests()).toHaveLength(1)});
 it('搜索框确认拼音时阻止表单提交',async()=>{const input=await composer();const searchButton=[...document.querySelectorAll('nav button')].find(b=>b.textContent?.includes('搜索'))! as HTMLButtonElement;searchButton.click();await vi.waitFor(()=>expect(document.querySelector('input[aria-label="搜索关键词"]')).not.toBeNull());const search=document.querySelector('input[aria-label="搜索关键词"]')!;search.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));search.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));const event=new KeyboardEvent('keydown',{key:'Enter',keyCode:229,bubbles:true,cancelable:true});search.dispatchEvent(event);expect(event.defaultPrevented).toBe(true);expect(requests()).toHaveLength(0)});
 it('组合输入期间保留输入法默认行为，同时阻止提交',async()=>{const input=await composer();input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));const event=new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true,cancelable:true});input.dispatchEvent(event);expect(event.defaultPrevented).toBe(false);input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));expect(requests()).toHaveLength(0)});
 it('正常回车发送，Shift+Enter 换行',async()=>{const input=await composer();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true,cancelable:true}));expect(requests()).toHaveLength(0);input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));expect(requests()).toHaveLength(1)});
});
