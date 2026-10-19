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
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.resetAllMocks()});
async function setup(){
 invoke.mockResolvedValue([]);
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'privacy',onClose:()=>{},onSave:async()=>{}}});
 await tick();
}
const removeButton=()=>document.querySelector<HTMLButtonElement>('.remove-roots-settings button')!;
const confirmButton=()=>document.querySelector<HTMLButtonElement>('[aria-label="确认移除所有文件夹"]')!;
it('数据与隐私中移除全部文件夹，需要确认且请求期间不能重复提交',async()=>{
 await setup();
 expect(document.querySelector('.remove-roots-settings')).not.toBeNull();
 removeButton().click();await tick();
 expect(document.body.textContent).toContain('同时删除这些文件夹的索引');
 document.querySelector<HTMLButtonElement>('[aria-label="取消移除文件夹"]')!.click();await tick();
 expect(invoke.mock.calls.some(([command])=>command==='remove_all_roots')).toBe(false);
 let finish!:()=>void;
 invoke.mockImplementation(command=>command==='remove_all_roots'?new Promise<void>(resolve=>finish=resolve):Promise.resolve([]));
 removeButton().click();await tick();confirmButton().click();await tick();
 expect(confirmButton().disabled).toBe(true);confirmButton().click();
 expect(invoke.mock.calls.filter(([command])=>command==='remove_all_roots')).toHaveLength(1);
 finish();await vi.waitFor(()=>expect(confirmButton()).toBeNull());
 expect(document.body.textContent).toContain('所有文件夹及索引已移除，本地文件已保留。');
});
it('移除失败时保留确认窗口并显示错误',async()=>{
 await setup();
 invoke.mockImplementation(async command=>{if(command==='remove_all_roots')throw new Error('正在索引，请等待当前任务完成后再移除所有文件夹');return [];});
 removeButton().click();await tick();confirmButton().click();
 await vi.waitFor(()=>expect(document.querySelector('.remove-roots-error')?.textContent).toContain('正在索引'));
 expect(confirmButton().disabled).toBe(false);
 expect(document.body.textContent).not.toContain('所有文件夹及索引已移除，本地文件已保留。');
});
it('成功后文件夹列表立即清空，会话记录保留',async()=>{
 localStorage.setItem('sag.onboarding.seen','1');
 let roots=[{path:'/notes',name:'测试文件夹',status:'ready',count:1,error:null}];
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return value;
  if(command==='list_roots')return roots;
  if(command==='list_conversations')return[{id:'kept',title:'保留的会话',updated:1,deleted:false,messages:[]}];
  if(command==='remove_all_roots'){roots=[];return;}
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[title="/notes"]')).not.toBeNull());
 [...document.querySelectorAll<HTMLButtonElement>('.sidebar button')].find(button=>button.textContent==='设置')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent==='数据与隐私')!.click();await tick();
 removeButton().click();await tick();confirmButton().click();
 await vi.waitFor(()=>expect(document.querySelector('[title="/notes"]')).toBeNull());
 expect(document.querySelector('.folder-empty-state')?.textContent).toContain('添加文件夹');
 expect(document.querySelector('.conversation-item')?.textContent).toContain('保留的会话');
});
