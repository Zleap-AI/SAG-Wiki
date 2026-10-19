// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import RemovedDirectories from '../src/components/RemovedDirectories.svelte';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
it('restores a removed folder immediately and keeps it listed if restoration fails',async()=>{
 setLanguage('zh-CN');let removed=['/project/docs'];let fail=true;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='list_removed_directories')return removed;
  if(command==='add_root'){
   expect(args).toEqual({path:'/project/docs'});
   if(fail)throw Error('目录不存在');
   removed=[];return '/project';
  }
 });
 app=mount(RemovedDirectories,{target:document.body});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('/project/docs'));
 document.querySelector<HTMLButtonElement>('button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('目录不存在'));
 expect(document.body.textContent).toContain('/project/docs');
 fail=false;document.querySelector<HTMLButtonElement>('button')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('暂无已移除的文件和文件夹。'));
 expect(document.querySelector('[role=status]')?.textContent).toContain('正在重新索引');
});
