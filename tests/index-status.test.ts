// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();listeners.clear()});
it('部分文件索引失败时可查看具体路径，同时显示已索引文件数量',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:270,error:'未能索引 1 个文件\n\n/notes/broken.txt\n文件无法按 UTF-16LE 编码读取'}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toContain('SAG · 部分失败'));
 expect(document.querySelector('.index-status')?.textContent).not.toContain('读取失败');
 expect(document.querySelector('.index-status')?.textContent).not.toContain('Notes');
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('/notes/broken.txt'));
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('UTF-16LE');
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('已索引 270 个文件');
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('其他文件可正常检索');
});

it('索引百分比与部分失败在同一行，完成后隐藏进度',async()=>{
 let ready=false;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [
   {path:'/notes',name:'Notes',status:ready?'ready':'partial',count:3,error:ready?null:'无法读取原文件'},
   {path:'/other',name:'Other',status:ready?'ready':'indexing',count:4,error:null}
  ];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('data-changed')).toBe(true));
 listeners.get('index-progress')!({payload:{path:'/other',count:27,total:100}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引（27% 部分失败）'));
 expect(document.querySelector('.index-status')?.children).toHaveLength(1);
 expect(document.querySelector('.index-status-row')?.children).toHaveLength(2);
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('无法读取原文件'));
 document.querySelector<HTMLButtonElement>('[aria-label="关闭索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')).toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="隐藏索引状态"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe(''));
 listeners.get('index-progress')!({payload:{path:'/other',count:30,total:100}});
 await new Promise(resolve=>setTimeout(resolve,0));
 expect(document.querySelector('.index-status')?.textContent).toBe('');
 listeners.get('index-progress')!({payload:{path:'/other',count:0,total:50}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引（0% 部分失败）'));
 ready=true;
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe(''));
});
