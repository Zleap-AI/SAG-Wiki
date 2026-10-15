// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import IndexSettings from '../src/components/IndexSettings.svelte';
import type {Root} from '../src/lib/types';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();listeners.clear()});
const button=()=>document.querySelector<HTMLButtonElement>('.reindex-settings button')!;
it('重新索引期间显示进度并禁止重复启动，完成后可查看失败详情',async()=>{
 let roots:Root[]=[{path:'/a',name:'A',status:'ready',count:2,error:null},{path:'/b',name:'B',status:'ready',count:3,error:null}];
 invoke.mockImplementation(async command=>{
  if(command==='reindex_all'){roots=roots.map(root=>({...root,status:'pending'}));return roots;}
  if(command==='list_roots')return roots;
 });
 app=mount(IndexSettings,{target:document.body});
 await vi.waitFor(()=>expect(button().disabled).toBe(false));
 button().click();
 await vi.waitFor(()=>expect(button().disabled).toBe(true));
 button().click();
 expect(invoke.mock.calls.filter(([command])=>command==='reindex_all')).toHaveLength(1);
 listeners.get('index-progress')!({payload:{path:'/a',count:1,total:2}});
 await tick();expect(document.querySelector('progress')?.value).toBe(1);
 expect(document.body.textContent).toContain('A：已处理 1 / 2 个文件');
 roots=[{...roots[0],status:'ready'},roots[1]];
 listeners.get('data-changed')!({payload:null});await tick();
 expect(button().disabled).toBe(true);
 roots=[roots[0],{...roots[1],status:'partial',error:'未能索引 1 个文件\n\n/b/broken.txt\n文件无法按 UTF-8 编码读取'}];
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(button().disabled).toBe(false));
 expect(document.body.textContent).toContain('重新索引完成，部分文件未能索引。');
 expect(document.body.textContent).toContain('已索引 5 个文件');
 const details=document.querySelector('details')!;details.querySelector('summary')!.click();await tick();
 expect(details.open).toBe(true);expect(details.textContent).toContain('/b/broken.txt');
});
it('未保存索引设置或未添加目录时禁用按钮，启动错误可见',async()=>{
 let roots:Root[]=[];
 invoke.mockImplementation(async command=>{
  if(command==='list_roots')return roots;
  if(command==='reindex_all')throw new Error('索引启动失败');
 });
 app=mount(IndexSettings,{target:document.body});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('添加文件夹后可重新索引。'));
 expect(button().disabled).toBe(true);
 await unmount(app);document.body.innerHTML='';
 roots=[{path:'/a',name:'A',status:'ready',count:1,error:null}];
 app=mount(IndexSettings,{target:document.body,props:{settingsChanged:true}});
 await tick();expect(button().disabled).toBe(true);
 expect(document.body.textContent).toContain('请先保存文件类型或排除目录的修改，再重新索引。');
 await unmount(app);document.body.innerHTML='';
 app=mount(IndexSettings,{target:document.body});
 await vi.waitFor(()=>expect(button().disabled).toBe(false));button().click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('索引启动失败'));
 expect(button().disabled).toBe(false);
});
