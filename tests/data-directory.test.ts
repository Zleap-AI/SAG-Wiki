// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import DataDirectory from '../src/components/DataDirectory.svelte';
import {setLanguage} from '../src/lib/i18n';
const {invoke,open}=vi.hoisted(()=>({invoke:vi.fn(),open:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open}));
let app:ReturnType<typeof mount>;
const button=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===label)!;
beforeEach(()=>{
 setLanguage('zh-CN');
 invoke.mockImplementation(async command=>{
  if(command==='get_data_directory')return {directory:'C:/SAG data',environment_override:false};
 });
 open.mockResolvedValue('D:/SAG data');
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.resetAllMocks()});
async function show(props:Record<string,unknown>={}){
 app=mount(DataDirectory,{target:document.body,props});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('C:/SAG data'));
}

it('displays the actual path, opens it, and cancelling the picker does not migrate',async()=>{
 await show();
 button('打开目录').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_data_directory',undefined));
 open.mockResolvedValue(null);
 button('更改位置').click();await tick();
 await vi.waitFor(()=>expect(open).toHaveBeenCalled());
 expect(button('迁移并重启')).toBeUndefined();
 expect(invoke.mock.calls.some(([name])=>name==='migrate_data_directory')).toBe(false);
});
it('requires confirmation and preserves the editor before migration, blocking repeats while pending',async()=>{
 const protect=vi.fn(async()=>true);
 let finish!:()=>void;
 invoke.mockImplementation(async command=>command==='get_data_directory'?{directory:'C:/SAG data',environment_override:false}:new Promise<void>(resolve=>finish=resolve));
 await show({onBeforeRestart:protect});
 button('更改位置').click();
 await vi.waitFor(()=>expect(button('迁移并重启')).toBeDefined());
 expect(document.body.textContent).toContain('旧数据目录保留为备份');
 expect(invoke.mock.calls.some(([name])=>name==='migrate_data_directory')).toBe(false);
 button('迁移并重启').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('migrate_data_directory',{directory:'D:/SAG data'}));
 expect(protect).toHaveBeenCalledOnce();
 expect(button('迁移并重启').disabled).toBe(true);
 expect(button('取消').disabled).toBe(true);
 finish();await tick();await tick();
 expect(button('迁移并重启').disabled).toBe(true);
});
it('allows cancelling confirmation and honours an editor cancellation',async()=>{
 const protect=vi.fn(async()=>false);
 await show({onBeforeRestart:protect});
 button('更改位置').click();
 await vi.waitFor(()=>expect(button('取消')).toBeDefined());
 button('取消').click();await tick();
 expect(button('迁移并重启')).toBeUndefined();
 button('更改位置').click();
 await vi.waitFor(()=>expect(button('迁移并重启')).toBeDefined());
 button('迁移并重启').click();
 await vi.waitFor(()=>expect(protect).toHaveBeenCalledOnce());
 await tick();expect(button('迁移并重启').disabled).toBe(false);
 expect(invoke.mock.calls.some(([name])=>name==='migrate_data_directory')).toBe(false);
});
it('shows migration errors and makes retry/cancel available',async()=>{
 await show();button('更改位置').click();
 await vi.waitFor(()=>expect(button('迁移并重启')).toBeDefined());
 invoke.mockRejectedValue('请选择空文件夹，目标目录已有内容');
 button('迁移并重启').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('请选择空文件夹'));
 expect(button('迁移并重启').disabled).toBe(false);
 expect(button('取消').disabled).toBe(false);
});
it('explains environment overrides and disables the conflicting picker',async()=>{
 invoke.mockResolvedValue({directory:'C:/SAG data',environment_override:true});
 await show();expect(button('更改位置').disabled).toBe(true);
 expect(document.body.textContent).toContain('SAG_DATA_DIR / SAG_DESKTOP_DATA_DIR');
});
it('reports a location read failure and supports retry',async()=>{
 invoke.mockRejectedValueOnce('无法读取应用数据位置配置');
 app=mount(DataDirectory,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('无法读取'));
 button('重试').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('C:/SAG data'));
});
