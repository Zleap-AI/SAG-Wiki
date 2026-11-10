// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Download from '../src/components/DoclingDownload.svelte';
import {doclingStatus,doclingDownloadOpen,doclingInstallRequest,openDoclingDownload} from '../src/lib/docling';
import {setLanguage} from '../src/lib/i18n';
const {invoke,open}=vi.hoisted(()=>({invoke:vi.fn(),open:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open}));
let app:ReturnType<typeof mount>;
const initial={installed:false,running:false,stage:'error',message:'',downloaded:0,total:null,error:'connect timeout',formats:[['PDF',['pdf']]] as [string,string[]][],profiles:[{id:'documents' as const,installed:false,extensions:['pdf']}]};
const button=(name:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent===name);
async function start(){
 invoke.mockResolvedValue(initial);app=mount(Download,{target:document.body});
 openDoclingDownload(['documents'],['pdf'],async()=>{});
 await vi.waitFor(()=>expect(button('导入离线组件')?.disabled).toBe(false));
}
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML='';vi.resetAllMocks();doclingStatus.set(null);doclingDownloadOpen.set(false);doclingInstallRequest.set(null);setLanguage('zh-CN')});
it('下载失败后选择解压目录，启动离线安装而不调用在线安装或自动应用格式',async()=>{
 await start();open.mockResolvedValue('/offline/component');
 invoke.mockImplementation(async command=>command==='get_docling_status'?{...initial,running:true,error:null}:undefined);
 button('导入离线组件')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_docling_component',{path:'/offline/component'}));
 expect(open).toHaveBeenCalledWith(expect.objectContaining({directory:true,multiple:false}));
 expect(invoke.mock.calls.some(([name])=>name==='install_docling'||name==='install_docling_profiles')).toBe(false);
 await vi.waitFor(()=>expect(document.querySelector('.download-indeterminate')).not.toBeNull());
 expect(document.querySelector('.docling-dialog')).not.toBeNull();
});
it('取消目录选择不发起安装',async()=>{
 await start();open.mockResolvedValue(null);button('导入离线组件')!.click();await tick();
 await vi.waitFor(()=>expect(button('导入离线组件')?.disabled).toBe(false));
 expect(invoke.mock.calls.every(([name])=>name==='get_docling_status')).toBe(true);
});
it('无效组件显示错误，允许重新选择且不回退联网下载',async()=>{
 await start();open.mockResolvedValue('/offline/bad');
 invoke.mockImplementation(async command=>{if(command==='import_docling_component')throw Error('invalid component');return initial});
 button('导入离线组件')!.click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('invalid component'));
 expect(button('导入离线组件')?.disabled).toBe(false);
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles')).toBe(false);
});
it('音视频不展示仅支持文档的离线导入入口',async()=>{
 await start();openDoclingDownload(['media'],['mp3'],async()=>{});await tick();expect(button('导入离线组件')).toBeUndefined();
});

it('导入就绪后仍需用户点击应用，安装期间不允许重复导入',async()=>{
 await start();const apply=vi.fn(async()=>{});openDoclingDownload(['documents'],['pdf'],apply);await tick();
 open.mockResolvedValue('/offline/component');
 let current={...initial,running:true,error:null as string|null};
 invoke.mockImplementation(async command=>command==='get_docling_status'?current:undefined);
 button('导入离线组件')!.click();await vi.waitFor(()=>expect(button('导入离线组件')).toBeUndefined());
 expect(apply).not.toHaveBeenCalled();
 doclingStatus.set({...initial,installed:true,stage:'done',error:null,profiles:[{...initial.profiles[0],installed:true}]});await tick();
 expect(apply).not.toHaveBeenCalled();button('应用所选格式')!.click();await vi.waitFor(()=>expect(apply).toHaveBeenCalledOnce());
});
it('英文导入说明与按钮没有遗漏中文',async()=>{
 await start();setLanguage('en');await tick();expect(button('Import offline component')).toBeDefined();expect(document.querySelector('.docling-dialog')!.textContent).not.toMatch(/[\u3400-\u9fff]/);
});
