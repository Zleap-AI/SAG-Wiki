// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import {doclingStatus,doclingDownloadOpen} from '../src/lib/docling';
import type {DoclingStatus} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.resetAllMocks();localStorage.clear();doclingStatus.set(null);doclingDownloadOpen.set(false)});
const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[['PDF',['pdf']]]};
async function start(current:()=>DoclingStatus){
 localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command)=>{
  if(command==='get_docling_status')return current();
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,watch:true,show_hidden:false,answer:{model:'test',models:['test']},jev:{model:'test'},retrieval:{chunk_chars:512},excluded:[]};
  if(command==='get_index_status')return {paused:false,tasks:[]};
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.ask-content')).not.toBeNull());
}
const button=(name:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${name}"]`)!;
it('从设置开始安装后可关闭详情和设置，进度继续更新，再次打开不重复下载',async()=>{
 let current={...initial};await start(()=>current);
 const base=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='install_docling'){current={...initial,running:true,stage:'dependencies',downloaded:38,total:100,message:'Downloading components'};return current;}
  return base(command,args);
 });
 [...document.querySelectorAll<HTMLButtonElement>('.sidebar-tools button')].find(node=>node.textContent==='设置')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(node=>node.textContent==='文件设置')!.click();await tick();
 await vi.waitFor(()=>expect(button('选择其他格式')?.disabled).toBe(false));
 button('选择其他格式').click();await tick();button('转换 .pdf').click();await tick();
 await vi.waitFor(()=>expect([...document.querySelectorAll<HTMLButtonElement>('.docling-formats-dialog button')].find(node=>node.textContent==='安装并应用')?.disabled).toBe(false));
 [...document.querySelectorAll<HTMLButtonElement>('.docling-formats-dialog button')].find(node=>node.textContent==='安装并应用')!.click();await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.docling-dialog:not(.docling-formats-dialog) button')].find(node=>node.textContent==='安装')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')?.textContent).toContain('38%'));
 [...document.querySelectorAll<HTMLButtonElement>('.docling-dialog button')].find(node=>node.textContent==='后台下载')!.click();await tick();
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog)')).toBeNull();
 button('关闭设置').click();await tick();
 expect(document.querySelector('.settings-dialog')).toBeNull();
 current={...current,downloaded:72};
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')?.textContent).toContain('72%'),{timeout:1800});
 button('查看 Docling 安装进度').click();await tick();
 expect(document.querySelector('.docling-dialog progress')?.getAttribute('value')).toBe('72');
 button('关闭下载详情').click();await tick();
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog)')).toBeNull();
 await new Promise(resolve=>setTimeout(resolve,850));
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog)')).toBeNull();
 expect(invoke.mock.calls.filter(([command])=>command==='install_docling')).toHaveLength(1);
 expect(invoke.mock.calls.some(([command])=>command==='cancel_docling_install')).toBe(false);
 current={...initial,installed:true,stage:'done'};
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')).toBeNull(),{timeout:1800});
});
it('已有后台下载不会自动弹窗，无百分比时显示安装中，失败后可查看原因与重试',async()=>{
 let current:DoclingStatus={...initial,running:true,stage:'models',percent:38};await start(()=>current);
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')?.textContent).toContain('38%'));
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog)')).toBeNull();
 current={...current,percent:null};
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')?.textContent).toBe('文件解析支持安装中'),{timeout:1800});
 current={...current,running:false,error:'下载连接失败'};
 await vi.waitFor(()=>expect(document.querySelector('.docling-progress')?.textContent).toBe('文件解析支持安装失败'),{timeout:1800});
 button('查看 Docling 安装进度').click();await tick();
 expect(document.querySelector('.docling-dialog [role="alert"]')?.textContent).toContain('下载连接失败');
 expect(document.querySelector('.docling-dialog')?.textContent).toContain('重试安装');
});
