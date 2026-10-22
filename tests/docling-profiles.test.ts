// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Harness from './helpers/OtherFormatsHarness.svelte';
import {doclingStatus,doclingDownloadOpen,doclingInstallRequest,updateDoclingStatus} from '../src/lib/docling';
import {setLanguage} from '../src/lib/i18n';
import type {DoclingStatus} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
const initial={installed:true,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[['PDF',['pdf']],['音频',['mp3','wav']],['视频',['mp4']]] as [string,string[]][],profiles:[{id:'documents' as const,installed:true,extensions:['pdf']},{id:'media' as const,installed:false,extensions:['mp3','wav','mp4']}]};
const find=(label:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
const button=(label:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(node=>node.textContent===label)!;
const applied=()=>JSON.parse(document.querySelector('output')!.textContent!);
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN');doclingStatus.set(null);doclingDownloadOpen.set(false);doclingInstallRequest.set(null)});

it('音频选择先确认媒体组件，不把文档已安装当作媒体已就绪',async()=>{
 let current:DoclingStatus={...initial};
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,stage:'models',requested_profiles:['media']};return current;});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 find('选择其他格式').click();await tick();
 button('清空选择').click();await tick();find('转换 .mp3').click();await tick();
 button('安装并应用').click();await tick();
 expect(document.querySelector('.selected-formats')?.textContent).toContain('MP3');
 expect(applied().enabled).toBe(false);
 expect(invoke.mock.calls.some(([name])=>name==='install_docling'||name==='install_docling_profiles')).toBe(false);
 button('安装').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_docling_profiles',{profiles:['media']}));
 current={...initial,profiles:initial.profiles.map(profile=>({...profile,installed:true}))};doclingStatus.set(current);await tick();
 expect(applied().enabled).toBe(false);await vi.waitFor(()=>expect(button('应用所选格式')?.disabled).toBe(false));button('应用所选格式').click();await tick();
 expect(applied()).toEqual({enabled:true,formats:['mp3']});
});

it('首次未安装默认不选任何格式，勾选PDF只安装documents且完成按钮应用本次选择',async()=>{
 let current={...initial,installed:false,profiles:initial.profiles.map(profile=>({...profile,installed:false}))};
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,stage:'runtime'};return current;});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 find('选择其他格式').click();await tick();
 expect(find('转换 .pdf').getAttribute('aria-checked')).toBe('false');
 expect(button('应用').disabled).toBe(true);
 find('转换 .pdf').click();await tick();
 expect(find('转换 .pdf').getAttribute('aria-checked')).toBe('true');
 expect(button('安装并应用').disabled).toBe(false);
 const pdfChoice=find('转换 .pdf');
 expect(find('转换 .mp3').getAttribute('aria-checked')).toBe('false');
 expect(find('转换 .mp4').getAttribute('aria-checked')).toBe('false');
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles')).toBe(false);
 button('安装并应用').click();await tick();
 expect(document.querySelector('.selected-formats')?.textContent).toContain('PDF');
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog) [role="checkbox"]')).toBeNull();
 button('安装').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_docling_profiles',{profiles:['documents']}));
 expect(invoke.mock.calls.some(([name])=>name==='install_docling')).toBe(false);
 current={...current,running:false,stage:'done',installed:true,profiles:initial.profiles};doclingStatus.set(current);await tick();
 expect(applied().enabled).toBe(false);expect(find('转换 .pdf')).toBe(pdfChoice);await vi.waitFor(()=>expect(button('应用所选格式')?.disabled).toBe(false));button('应用所选格式').click();await tick();
 expect(applied()).toEqual({enabled:true,formats:['pdf']});
});

it('取消媒体确认保留选择且不写入并行数或启用转换',async()=>{
 invoke.mockResolvedValue(initial);app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();
 find('转换 .mp3').click();await tick();button('安装并应用').click();await tick();
 const dialog=document.querySelector('.docling-dialog:not(.docling-formats-dialog)')!;
 Array.from(dialog.querySelectorAll('button')).find(node=>node.textContent==='取消')!.click();await tick();
 expect(find('转换 .mp3').getAttribute('aria-checked')).toBe('true');
 expect(applied().enabled).toBe(false);
 expect(invoke.mock.calls.every(([name])=>name==='get_docling_status')).toBe(true);
});

it('全选和旧null选择应用后写显式扩展名数组',async()=>{
 invoke.mockResolvedValue({...initial,profiles:initial.profiles.map(profile=>({...profile,installed:true}))});
 app=mount(Harness,{target:document.body,props:{enabled:true,formats:null}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();
 button('全选').click();await tick();button('应用').click();await tick();
 expect(applied()).toEqual({enabled:true,formats:['pdf','mp3','wav','mp4']});
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles')).toBe(false);
});

it.each([true,false])('旧null范围只改并行数时保留全部格式且不安装，媒体已就绪=%s',async mediaReady=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_docling_status')return {...initial,profiles:initial.profiles.map(profile=>({...profile,installed:profile.id==='documents'||mediaReady}))};
  if(command==='get_settings')return {conversion_workers:4};
 });
 app=mount(Harness,{target:document.body,props:{enabled:true,formats:null,globalWorkers:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')?.disabled).toBe(false));
 const input=document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')!;input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 button('应用').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_conversion_workers',{workers:2}));await tick();
 expect(applied()).toEqual({enabled:true,formats:null});
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles'||name==='install_docling')).toBe(false);
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog)')).toBeNull();
});

it('旧服务没有profiles时媒体选择安全失败，不回退到文档安装',async()=>{
 const {profiles,...legacy}=initial;invoke.mockResolvedValue(legacy);app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();
 find('转换 .mp4').click();await tick();button('安装并应用').click();await tick();
 expect(document.querySelector('[role="alert"]')?.textContent).toContain('更新后台服务');
 expect(applied().enabled).toBe(false);
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles'||name==='install_docling')).toBe(false);
});

it('后台安装完成只更新就绪状态，不自动启用或保存其他格式',async()=>{
 let current={...initial,installed:false,running:true,stage:'runtime',profiles:initial.profiles.map(profile=>({...profile,installed:false}))};
 invoke.mockImplementation(async()=>current);app=mount(Harness,{target:document.body,props:{second:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 expect(applied().enabled).toBe(false);
 expect(JSON.parse(document.querySelector('[aria-label="另一文件夹转换设置"]')!.textContent!).enabled).toBe(false);
 current={...current,running:false,stage:'done',installed:true};doclingStatus.set(current);await tick();
 expect(applied().enabled).toBe(false);
 expect(JSON.parse(document.querySelector('[aria-label="另一文件夹转换设置"]')!.textContent!).enabled).toBe(false);
 expect(invoke.mock.calls.some(([name])=>name==='save_settings'||name==='set_conversion_workers')).toBe(false);
});

it('媒体后台失败后重新打开保持媒体重试目标，文档就绪不会显示完成',async()=>{
 let current:DoclingStatus={...initial,stage:'error',error:'model connection interrupted',requested_profiles:['media']};
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,stage:'models',error:null};return current;});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('查看 Docling 安装进度')).not.toBeNull());find('查看 Docling 安装进度').click();await tick();
 expect(document.querySelector('.docling-dialog [role="checkbox"]')).toBeNull();
 button('重试安装').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_docling_profiles',{profiles:['media']}));
 expect(invoke.mock.calls.some(([name])=>name==='install_docling')).toBe(false);
});

it('英文媒体确认完整且首次不会为音视频预选下载',async()=>{
 setLanguage('en');invoke.mockResolvedValue({...initial,installed:false,profiles:initial.profiles.map(profile=>({...profile,installed:false}))});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('Select other formats')?.disabled).toBe(false));find('Select other formats').click();await tick();find('Convert .pdf').click();await tick();button('Install and apply').click();await tick();
 expect(document.querySelector('.docling-dialog:not(.docling-formats-dialog) [role="checkbox"]')).toBeNull();
 expect(document.body.textContent).not.toMatch(/[\u3400-\u9fff]/);
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles'||name==='install_docling')).toBe(false);
});

it('旧状态读取晚于卸载结果返回时，不能把组件重新显示为已安装',async()=>{
 let finish!:(status:DoclingStatus)=>void;
 invoke.mockImplementation(()=>new Promise<DoclingStatus>(resolve=>{finish=resolve}));
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('get_docling_status',undefined));
 updateDoclingStatus({...initial,installed:false,stage:'uninstalled',profiles:initial.profiles.map(profile=>({...profile,installed:false}))});await tick();
 finish(initial);await tick();await Promise.resolve();await tick();
 find('选择其他格式').click();await tick();expect(document.querySelector<HTMLButtonElement>('.uninstall-docling')?.disabled).toBe(true);
 expect(applied().enabled).toBe(false);
});
