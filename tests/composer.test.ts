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
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.clearAllMocks();vi.restoreAllMocks()});
async function composer(files:{id:number;path:string;name:string}[]=[]){
 invoke.mockImplementation(async(command:string)=>command==='get_settings'?{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}}:command==='list_files'?files:command==='run_query'?new Promise(()=>{}):[]);
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
 it('输入 @ 后没有匹配文件时，回车关闭引用候选并发送问题',async()=>{const input=await composer([{id:1,path:'/docs/first.pdf',name:'first.pdf'}]);input.focus();input.value='@78度的智慧 这本书讲了什么';input.setSelectionRange(input.value.length,input.value.length);input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(requests()).toHaveLength(1));expect(requests()[0][1].query.question).toBe('@78度的智慧 这本书讲了什么')});
 it('生成回答时输入框仍可编辑，回车不会重复发送',async()=>{const input=await composer();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(requests()).toHaveLength(1));await tick();expect(input.disabled).toBe(false);expect(input.placeholder).toBe('回答生成中，可以先输入下一个问题…');expect(input.closest('form')?.classList.contains('busy')).toBe(true);input.value='下一个问题';input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await tick();expect(requests()).toHaveLength(1);expect(input.value).toBe('下一个问题')});
 it('正常回车发送，Shift+Enter 换行',async()=>{const input=await composer();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true,cancelable:true}));expect(requests()).toHaveLength(0);input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));expect(requests()).toHaveLength(1)});
});

const mentionFiles=[{id:1,path:'D:\\docs\\first.pdf',name:'first.pdf'},{id:2,path:'D:\\docs\\second.pdf',name:'second.pdf'}];
async function openMentions(input:HTMLTextAreaElement){
 input.focus();input.value='@';input.setSelectionRange(1,1);input.dispatchEvent(new Event('input',{bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('.mention-options [role="option"]')).toHaveLength(2));
}
describe('document mentions',()=>{
 it('shows an actionable empty state when the library has no files',async()=>{
  const input=await composer();input.focus();input.value='@';input.setSelectionRange(1,1);input.dispatchEvent(new Event('input',{bubbles:true}));
  await vi.waitFor(()=>expect(document.querySelector('.mention-empty strong')?.textContent).toBe('知识库还没有可引用的文件'));
  expect(document.querySelector('.mention-empty p')?.textContent).toContain('添加文件夹并完成索引');
  expect(document.querySelector('.mention-add-folder')?.textContent).toContain('添加文件夹');
  expect(document.querySelector('.mention-options')).toBeNull();
 });
 it('explains when the search term has no matching files',async()=>{
  const input=await composer(mentionFiles);input.focus();input.value='@not-found';input.setSelectionRange(input.value.length,input.value.length);input.dispatchEvent(new Event('input',{bubbles:true}));
  await vi.waitFor(()=>expect(document.querySelector('.mention-empty strong')?.textContent).toBe('没有匹配的文件'));
  expect(document.querySelector('.mention-empty p')?.textContent).toContain('试试其他文件名');
  expect(document.querySelector('.mention-add-folder')).toBeNull();
 });
 it('keeps the selected file visible without moving input focus or submitting',async()=>{
  const scroll=vi.spyOn(HTMLElement.prototype,'scrollIntoView');
  const input=await composer(mentionFiles);await openMentions(input);
  expect(document.activeElement).toBe(input);
  input.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(scroll).toHaveBeenCalledWith({block:'nearest',inline:'nearest',behavior:'instant'}));
  expect(scroll.mock.instances[0]).toBe(document.querySelector('.mention-options [aria-selected="true"]'));
  expect((scroll.mock.instances[0] as HTMLElement).textContent).toContain('second.pdf');
  expect(document.activeElement).toBe(input);
  input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(document.querySelector('.reference-chip')?.textContent).toBe('second.pdf'));
  expect(document.querySelector('.mention-picker')).toBeNull();expect(document.activeElement).toBe(input);expect(requests()).toHaveLength(0);
 });
 it('selects with the pointer and closes with Escape',async()=>{
  const input=await composer(mentionFiles);await openMentions(input);
  input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));await tick();
  await vi.waitFor(()=>expect(document.querySelector('.mention-picker')).toBeNull());expect(input.value).toBe('@');
  await openMentions(input);
  document.querySelector('.mention-options [role="option"]')!.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(document.querySelector('.reference-chip')?.textContent).toBe('first.pdf'));
  expect(document.querySelector('.mention-picker')).toBeNull();expect(document.activeElement).toBe(input);expect(requests()).toHaveLength(0);
 });
});
