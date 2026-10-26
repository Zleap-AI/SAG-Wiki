// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
const query=<T extends HTMLElement>(selector:string)=>document.querySelector<T>(selector)!;
beforeEach(async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return [{id:'chat-1',title:'已有问答',messages:[],updated:0,deleted:false}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.history-more')).not.toBeNull());
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
function key(target:Element,key:string,shiftKey=false){target.dispatchEvent(new KeyboardEvent('keydown',{key,shiftKey,bubbles:true,cancelable:true}))}
it('侧边栏和内容空白区不弹出浏览器原生右键菜单',()=>{
 for(const selector of ['.sidebar','.workspace']){
  const event=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2});
  query(selector).dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(document.querySelector('[role=menu]')).toBeNull();
 }
});
it('菜单打开时保留组件库的鼠标交互样式',async()=>{
 query<HTMLButtonElement>('.history-more').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=menu]')).not.toBeNull());
 const menu=query('[role=menu]');
 expect(getComputedStyle(menu).pointerEvents).toBe('auto');
});
it('菜单内再次右键不会打开浏览器原生菜单',async()=>{
 query<HTMLButtonElement>('.history-more').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=menuitem]')).not.toBeNull());
 const event=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2});
 query('[role=menuitem]').dispatchEvent(event);
 expect(event.defaultPrevented).toBe(true);
 expect(document.querySelectorAll('[role=menu]')).toHaveLength(1);
});
it('按钮通过键盘打开菜单，方向键导航并用 Escape 返回触发按钮',async()=>{
 const trigger=query<HTMLButtonElement>('.history-more');trigger.focus();trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=menu]')).not.toBeNull());
 const menu=query('[role=menu]');menu.focus();key(menu,'ArrowDown');
 await vi.waitFor(()=>expect(document.activeElement?.textContent).toContain('重命名'));
 key(document.activeElement!,'ArrowDown');
 await vi.waitFor(()=>expect(document.activeElement?.textContent).toContain('删除'));
 key(document.activeElement!,'Escape');
 await vi.waitFor(()=>expect(document.querySelector('[role=menu]')).toBeNull());
 await vi.waitFor(()=>expect(document.activeElement).toBe(trigger));
});
it('菜单选择重命名后，弹窗聚焦名称，取消不修改记录',async()=>{
 query<HTMLButtonElement>('.history-more').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=menuitem]')).not.toBeNull());
 query('[role=menuitem]').click();
 await vi.waitFor(()=>expect(document.activeElement).toBe(query('[aria-label="问答名称"]')));
 key(document.activeElement!,'Escape');
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).toBeNull());
 expect(invoke.mock.calls.some(([command])=>command==='edit_conversation')).toBe(false);
});
it('菜单重命名和删除生效，删除后可以恢复',async()=>{
 const conversation={id:'chat-1',title:'已有问答',messages:[],updated:0,deleted:false};
 invoke.mockImplementation(async(command,payload)=>{
  if(command==='edit_conversation'){
   if(payload.title!==null)conversation.title=payload.title;
   if(payload.deleted!==null)conversation.deleted=payload.deleted;
  }
  if(command==='list_conversations')return conversation.deleted?[]:[{...conversation}];
  return [];
 });
 query<HTMLButtonElement>('.history-more').click();
 await vi.waitFor(()=>expect(query('[role=menuitem]')).not.toBeNull());
 query('[role=menuitem]').click();
 await vi.waitFor(()=>expect(query('[aria-label="问答名称"]')).not.toBeNull());
 const name=query<HTMLInputElement>('[aria-label="问答名称"]');
 name.value='新的问答名称';name.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 name.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(query('[title="新的问答名称"]')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'chat-1',title:'新的问答名称',deleted:null});
 query<HTMLButtonElement>('.history-more').click();
 await vi.waitFor(()=>expect(document.querySelectorAll('[role=menuitem]')).toHaveLength(2));
 (document.querySelectorAll<HTMLElement>('[role=menuitem]')[1]).click();
 await vi.waitFor(()=>expect(query('.history-more')).toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'chat-1',title:null,deleted:true});
 const restore=[...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(button=>button.textContent==='恢复')!;
 restore.click();
 await vi.waitFor(()=>expect(query('[title="新的问答名称"]')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('edit_conversation',{id:'chat-1',title:null,deleted:false});
});
it('范围弹窗由组件库关闭并恢复焦点',async()=>{
 const trigger=query<HTMLButtonElement>('.composer .scope-button');trigger.focus();trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).not.toBeNull());
 const dialog=query('[role=dialog]');
 expect(dialog.getAttribute('aria-modal')).toBe('true');
 query('.modal-backdrop').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerType:'mouse',clientX:1000,clientY:1000}));
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).toBeNull());
 await vi.waitFor(()=>expect(document.activeElement).toBe(trigger));
});

it('弹窗打开时阻止焦点进入背景，关闭后恢复原控件',async()=>{
 const trigger=query<HTMLButtonElement>('.composer .scope-button');trigger.focus();trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).not.toBeNull());
 const dialog=query('[role=dialog]');dialog.focus();await tick();trigger.focus();
 expect(dialog.contains(document.activeElement)).toBe(true);
 key(document.activeElement!,'Escape');
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog]')).toBeNull());
 await vi.waitFor(()=>expect(document.activeElement).toBe(trigger));
});
