// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import App from '../src/App.svelte';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const value:Settings={answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.resetAllMocks();localStorage.removeItem('sag.search.history')});
it('数据与隐私中的清空操作须确认，成功后清除界面历史',async()=>{
 const cleared=vi.fn();invoke.mockResolvedValue(undefined);
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'privacy',onClose:()=>{},onSave:async()=>{},onConversationsCleared:cleared}});await tick();
 const button=()=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()==='清空所有对话')!;
 expect(button()).toBeDefined();expect(document.querySelector('.clear-conversations-settings')).not.toBeNull();button().click();await tick();expect(invoke).not.toHaveBeenCalled();
 document.querySelector<HTMLButtonElement>('[aria-label="取消清空对话"]')!.click();await tick();expect(invoke).not.toHaveBeenCalled();
 button().click();await tick();document.querySelector<HTMLButtonElement>('[aria-label="确认清空所有对话"]')!.click();
 await vi.waitFor(()=>expect(cleared).toHaveBeenCalledOnce());
 expect(invoke).toHaveBeenCalledWith('clear_conversations',undefined);
 expect(document.querySelector('[aria-label="确认清空所有对话"]')).toBeNull();
});
it('清空失败时显示错误，不报告成功',async()=>{
 const cleared=vi.fn();invoke.mockRejectedValue(new Error('请先停止正在进行的问答'));
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'privacy',onClose:()=>{},onSave:async()=>{},onConversationsCleared:cleared}});await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()==='清空所有对话')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="确认清空所有对话"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.clear-history-error')?.textContent).toContain('请先停止正在进行的问答'));
 expect(cleared).not.toHaveBeenCalled();
});

it('清空后当前会话和列表立即移除，并可开始新对话',async()=>{
 let history=[{id:'old',title:'清空前的对话',updated:0,deleted:false,messages:[{question:'旧问题',answer:'旧回答',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'旧问题'}]}];
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return value;
  if(command==='list_conversations')return history;
  if(command==='clear_conversations'){history=[];return;}
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[title="清空前的对话"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[title="清空前的对话"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('旧回答'));
 [...document.querySelectorAll<HTMLButtonElement>('.sidebar button')].find(button=>button.textContent==='设置')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent==='数据与隐私')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='清空所有对话')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="确认清空所有对话"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')).toBeNull());
 expect(document.querySelector('.history-row')).toBeNull();
 expect(document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')?.value).toBe('');
});

it('清除所有数据需确认，同时清空文件夹、会话和持久化搜索记录',async()=>{
 localStorage.setItem('sag.onboarding.seen','1');
 localStorage.setItem('sag.search.history',JSON.stringify(['旧搜索']));
 let cleared=false;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return cleared?value:{...value,answer:{...value.answer,model:'old-model',models:['old-model'],has_key:true}};
  if(command==='list_roots')return cleared?[]:[{path:'/notes',name:'待清理文件夹',status:'ready',count:1,error:null}];
  if(command==='list_conversations')return cleared?[]:[{id:'old',title:'待清理会话',updated:0,deleted:false,messages:[]}];
  if(command==='clear_all_data'){cleared=true;return;}
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[title="/notes"]')).not.toBeNull());
 [...document.querySelectorAll<HTMLButtonElement>('.sidebar button')].find(button=>button.textContent==='设置')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent==='数据与隐私')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.clear-data-settings button')!.click();await tick();
 expect(document.querySelector('[aria-label="清除所有数据"]')?.textContent).toContain('模型设置、密钥和未保存草稿将被清除');
 expect(invoke.mock.calls.some(([command])=>command==='clear_all_data')).toBe(false);
 document.querySelector<HTMLButtonElement>('[aria-label="取消清除所有数据"]')!.click();await tick();
 expect(localStorage.getItem('sag.search.history')).toContain('旧搜索');
 document.querySelector<HTMLButtonElement>('.clear-data-settings button')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="确认清除所有数据"]')!.click();
 await vi.waitFor(()=>expect(localStorage.getItem('sag.search.history')).toBeNull());
 expect(document.querySelector('[title="/notes"]')).toBeNull();
 expect(document.querySelector('[title="待清理会话"]')).toBeNull();
 expect(invoke.mock.calls.filter(([command])=>command==='clear_all_data')).toHaveLength(1);
 expect(document.querySelector('[aria-label="关闭设置"]')).toBeNull();
 expect(document.querySelector('[aria-label="模型与思考强度"]')?.textContent).toContain('test');
 expect(document.querySelector('[aria-label="模型与思考强度"]')?.textContent).not.toContain('old-model');
 expect(invoke.mock.calls.some(([command])=>command==='save_settings')).toBe(false);
 document.querySelector<HTMLButtonElement>('.workspace-switch button[title^="搜索"]')!.click();await tick();
 expect(document.querySelector('[aria-label="最近搜索"]')).toBeNull();
 await unmount(app);app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome')).not.toBeNull());
 expect(document.querySelector('[title="待清理会话"]')).toBeNull();
 expect(localStorage.getItem('sag.search.history')).toBeNull();
});
it('清除所有数据失败时保留搜索历史，不清空界面',async()=>{
 localStorage.setItem('sag.search.history',JSON.stringify(['保留搜索']));
 const cleared=vi.fn();invoke.mockRejectedValue(new Error('正在索引'));
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'privacy',onClose:()=>{},onSave:async()=>{},onDataCleared:cleared}});await tick();
 document.querySelector<HTMLButtonElement>('.clear-data-settings button')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="确认清除所有数据"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.clear-data-error')?.textContent).toContain('正在索引'));
 expect(cleared).not.toHaveBeenCalled();
 expect(localStorage.getItem('sag.search.history')).toContain('保留搜索');
});
it('文件和问答设置不再包含批量清理操作',async()=>{
 invoke.mockImplementation(async command=>command==='get_docling_status'?{installed:false,running:false,formats:[]}:command==='get_index_status'?{paused:false,tasks:[]}:[]);
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'files',onClose:()=>{},onSave:async()=>{}}});await tick();
 expect(document.querySelector('.remove-roots-settings')).toBeNull();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent==='问答设置')!.click();await tick();
 expect(document.querySelector('.clear-conversations-settings')).toBeNull();
});
