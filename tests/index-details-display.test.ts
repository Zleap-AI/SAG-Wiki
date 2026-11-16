// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
let saved:Settings;
beforeEach(()=>{
 saved={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return structuredClone(saved);
  if(command==='save_settings'){saved=structuredClone(args.settings);return;}
  if(command==='list_roots')return[{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='list_entries')return[{path:'/notes/a.md',name:'a.md',directory:false,symlink:false}];
  if(command==='list_files')return[{id:1,path:'/notes/a.md',root:'/notes',name:'a.md'}];
  return [];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
function click(label:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()===label||node.getAttribute('aria-label')===label);expect(button,label).toBeDefined();button!.click()}
async function settings(){click('设置');await tick();click('文件设置');await tick()}
const toggle=()=>document.querySelector<HTMLButtonElement>('[role="switch"][aria-label="显示索引详情"]')!;
it('旧配置默认隐藏文件夹索引详情，保存开关后立即显示，再次关闭并重启仍隐藏',async()=>{
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.tree-name[title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-entry')).not.toBeNull());
 expect(document.querySelector('.folder-index-status')).toBeNull();
 await settings();expect(toggle().getAttribute('aria-checked')).toBe('false');
 const group=toggle().closest('.settings-group')!;
 expect(group.querySelector('.index-history-settings')!.nextElementSibling).toBe(toggle().closest('.setting-row'));
 toggle().click();await tick();click('保存设置');
 await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
 expect(saved.show_index_details).toBe(true);
 expect(document.querySelector('.folder-index-status')?.textContent).toContain('索引已完成');
 await settings();expect(toggle().getAttribute('aria-checked')).toBe('true');toggle().click();await tick();click('保存设置');
 await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
 expect(document.querySelector('.folder-index-status')).toBeNull();expect(saved.show_index_details).toBe(false);
 await unmount(app);app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.tree-name[title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-entry')).not.toBeNull());
 expect(document.querySelector('.folder-index-status')).toBeNull();
 await settings();expect(toggle().getAttribute('aria-checked')).toBe('false');
 document.querySelector<HTMLButtonElement>('.settings-index-history')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')).not.toBeNull());
});
it('取消开关修改不影响已保存的显示状态',async()=>{
 saved.show_index_details=true;app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.tree-name[title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-index-status')).not.toBeNull());
 await settings();toggle().click();await tick();click('取消');
 await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
 expect(saved.show_index_details).toBe(true);expect(document.querySelector('.folder-index-status')).not.toBeNull();
});
