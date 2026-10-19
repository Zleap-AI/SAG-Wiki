// @vitest-environment happy-dom
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Settings} from '../src/lib/types';

const {invoke, listeners, onCloseRequested} = vi.hoisted(()=>({
 invoke:vi.fn(), listeners:new Map<string,()=>Promise<void>>(), onCloseRequested:vi.fn(async()=>()=>{}),
}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,fn)=>{listeners.set(name,fn);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));

const settings:Settings={language:'zh-CN',answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0};
let app:ReturnType<typeof mount>;
let restoreDraft=false;
beforeEach(()=>{
 restoreDraft=false;localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return settings;
  if(command==='list_drafts')return restoreDraft?[{path:'/temporary/note.md',name:'note.md',body:'未保存的测试内容',hash:'original',is_new:false}]:[];
  if(command==='save_document')return {path:'/temporary/note.md',name:'note.md',body:'未保存的测试内容',hash:'saved'};
  return [];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';listeners.clear();vi.clearAllMocks();});

async function ready(){
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome h1')).not.toBeNull());
 await vi.waitFor(()=>expect(listeners.has('quit-requested')).toBe(true));
}
function click(text:string){
 const button=[...document.querySelectorAll<HTMLButtonElement>('button')].filter(node=>node.textContent?.trim()===text).at(-1);
 expect(button,text).toBeDefined();button!.click();
}
async function draft(){restoreDraft=true;await ready();click('恢复');await vi.waitFor(()=>expect(document.querySelector('.editor')).not.toBeNull());}

it('关闭由原生托盘逻辑处理，前端不注册会销毁窗口的关闭监听',async()=>{
 await ready();expect(onCloseRequested).not.toHaveBeenCalled();
 await listeners.get('quit-requested')!();
 expect(invoke).toHaveBeenCalledWith('quit_application',undefined);
});
it('隐藏窗口后退出，未保存文档先显示确认，取消后继续运行',async()=>{
 await draft();const quit=listeners.get('quit-requested')!();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('保存文档？'));
 expect(invoke).toHaveBeenCalledWith('reveal_main_window',undefined);
 await listeners.get('quit-requested')!();
 click('取消');await quit;await tick();
 expect(invoke).not.toHaveBeenCalledWith('quit_application',undefined);
 const retry=listeners.get('quit-requested')!();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('保存文档？'));
 click('不保存');await retry;
 expect(invoke).toHaveBeenCalledWith('delete_draft',{path:'/temporary/note.md'});
 expect(invoke).toHaveBeenCalledWith('quit_application',undefined);
});
it('保存成功后才退出，沿用现有文档保存流程',async()=>{
 await draft();const quit=listeners.get('quit-requested')!();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('保存文档？'));
 click('保存');await quit;
 expect(invoke).toHaveBeenCalledWith('save_document',{path:'/temporary/note.md',body:'未保存的测试内容',expectedHash:'original'});
 expect(invoke.mock.calls.findIndex(([name])=>name==='save_document')).toBeLessThan(invoke.mock.calls.findIndex(([name])=>name==='quit_application'));
});
