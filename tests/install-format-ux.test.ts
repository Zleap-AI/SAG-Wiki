// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Harness from './helpers/OtherFormatsHarness.svelte';
import {doclingStatus,doclingDownloadOpen,doclingInstallRequest} from '../src/lib/docling';
import type {DoclingStatus} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[['PDF',['pdf']],['Word',['docx']],['音频',['mp3']],['视频',['mp4']]],profiles:[{id:'documents',installed:false,extensions:['pdf','docx']},{id:'media',installed:false,extensions:['mp3','mp4']}]};
const find=(label:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
const button=(label:string,scope:ParentNode=document)=>Array.from(scope.querySelectorAll<HTMLButtonElement>('button')).find(node=>node.textContent===label)!;
const confirmation=()=>document.querySelector<HTMLElement>('.docling-dialog:not(.docling-formats-dialog)')!;
const applied=()=>JSON.parse(document.querySelector('[aria-label="已应用转换设置"]')!.textContent!);
async function choose(extensions:string[]){
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();button('清空选择').click();await tick();
 for(const extension of extensions){find(`转换 .${extension}`).click();await tick();}
 button('安装并应用').click();await tick();
}
function ready(status:DoclingStatus):DoclingStatus{return {...status,installed:true,running:false,stage:'done',error:null,profiles:status.profiles?.map(profile=>({...profile,installed:true}))};}
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();doclingStatus.set(null);doclingDownloadOpen.set(false);doclingInstallRequest.set(null)});

it('格式选择是唯一入口，PDF安装确认只读且完成后仅应用本次草稿',async()=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,stage:'dependencies',requested_profiles:['documents']};return current;});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 expect(document.querySelectorAll('.other-formats-actions button')).toHaveLength(1);
 expect(find('安装 Docling')).toBeNull();
 find('选择其他格式').click();await tick();button('清空选择').click();await tick();find('转换 .pdf').click();await tick();
 button('安装并应用').click();await tick();
 expect(confirmation().textContent).toContain('安装文件解析支持');
 expect(confirmation().querySelector('[role="checkbox"]')).toBeNull();
 expect(confirmation().textContent).toContain('PDF');
 expect(confirmation().textContent).not.toMatch(/文档组件|音视频组件|Python|FFmpeg|\.sag-converted/);
 expect(invoke.mock.calls.some(([name])=>name==='install_docling_profiles'||name==='install_docling')).toBe(false);
 button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_docling_profiles',{profiles:['documents']}));
 expect(applied()).toEqual({enabled:false});
 current={...current,running:false,stage:'done',installed:true,profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))};doclingStatus.set(current);await tick();
 expect(applied()).toEqual({enabled:false});
 await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();
 await vi.waitFor(()=>expect(applied()).toEqual({enabled:true,formats:['pdf']}));
 expect(confirmation()).toBeNull();expect(document.querySelector('.docling-formats-dialog')).toBeNull();
 expect(invoke.mock.calls.some(([name])=>name==='save_settings'||name==='save_folder_index_options')).toBe(false);
});

it.each([
 {name:'仅音频',selected:['mp3'],documents:false,profiles:['media']},
 {name:'文档已就绪的混合选择',selected:['pdf','mp3'],documents:true,profiles:['media']},
 {name:'两个支持均缺失的混合选择',selected:['pdf','mp3'],documents:false,profiles:['documents','media']}
])('$name只安装所缺支持，摘要保留完整选择',async({selected,documents,profiles})=>{
 let current={...structuredClone(initial),installed:documents,profiles:initial.profiles!.map(profile=>({...profile,installed:documents&&profile.id==='documents'}))};
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true};return current;});
 app=mount(Harness,{target:document.body});await choose(selected);
 for(const extension of selected)expect(confirmation().querySelector('.selected-formats')?.textContent).toContain(extension.toUpperCase());
 button('安装',confirmation()).click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_docling_profiles',{profiles}));
});

it('取消安装确认不启用也不保存，保留格式草稿',async()=>{
 invoke.mockResolvedValue(initial);app=mount(Harness,{target:document.body});await choose(['mp3']);button('取消',confirmation()).click();await tick();
 expect(confirmation()).toBeNull();expect(find('转换 .mp3').getAttribute('aria-checked')).toBe('true');expect(applied()).toEqual({enabled:false});
 expect(invoke.mock.calls.every(([name])=>name==='get_docling_status')).toBe(true);
});

it('安装失败不应用，进度重开与重试仍使用原缺失目标',async()=>{
 let current:DoclingStatus={...structuredClone(initial),installed:true,error:'earlier document failure',requested_profiles:['documents'],profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))},attempts=0;
 invoke.mockImplementation(async command=>{
  if(command==='install_docling_profiles'){attempts++;if(attempts===1)throw Error('download interrupted');current={...current,running:true};}return current;
 });
 app=mount(Harness,{target:document.body});await choose(['pdf','mp3']);button('重试安装',confirmation()).click();
 await vi.waitFor(()=>expect(confirmation().querySelector('[role="alert"]')?.textContent).toContain('download interrupted'));
 expect(applied()).toEqual({enabled:false});find('查看 Docling 安装进度').click();await tick();expect(button('完成',confirmation())).toBeUndefined();button('重试安装',confirmation()).click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles')).toHaveLength(2));
 expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles').map(([,args])=>args)).toEqual([{profiles:['media']},{profiles:['media']}]);
 current=ready(current);doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();
 await vi.waitFor(()=>expect(applied()).toEqual({enabled:true,formats:['pdf','mp3']}));
});

it('进度入口以服务当前失败任务为准，不能沿用较早完成的文档请求',async()=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,requested_profiles:['documents']};return current;});
 app=mount(Harness,{target:document.body});await choose(['pdf']);button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());current=ready(current);doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();await tick();
 current={...current,stage:'error',error:'media failed',requested_profiles:['media'],profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))};doclingStatus.set(current);await tick();find('查看 Docling 安装进度').click();await tick();
 expect(button('完成',confirmation())).toBeUndefined();button('重试安装',confirmation()).click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenLastCalledWith('get_docling_status',undefined));
 expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles').at(-1)?.[1]).toEqual({profiles:['media']});
});

it.each(['关闭格式选择','关闭发起设置','关闭下载详情'])('%s后完成不能回写旧草稿',async closeLabel=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true};return current;});
 app=mount(Harness,{target:document.body,props:{removable:true}});await choose(['pdf']);button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());
 if(closeLabel!=='关闭下载详情'){button('后台下载',confirmation()).click();await tick();}
 find(closeLabel).click();await tick();
 current=ready(current);doclingStatus.set(current);doclingDownloadOpen.set(true);await tick();
 expect(button('应用所选格式',confirmation())).toBeUndefined();button('完成',confirmation()).click();await tick();
 expect(applied()).toEqual({enabled:false});
});

it('安装RPC未返回时第二控件不能接管目标或完成回调，重复进度保持原请求',async()=>{
 let current=structuredClone(initial),accept!:(value:DoclingStatus)=>void;
 invoke.mockImplementation(async command=>{
  if(command==='install_docling_profiles')return new Promise<DoclingStatus>(resolve=>{accept=resolve;});return current;
 });
 app=mount(Harness,{target:document.body,props:{second:true}});await choose(['pdf']);button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(accept).toBeDefined());find('关闭下载详情').click();await tick();
 const selects=document.querySelectorAll<HTMLButtonElement>('[aria-label="选择其他格式"]');selects[1].click();await tick();
 const dialogs=document.querySelectorAll<HTMLElement>('.docling-formats-dialog'),second=dialogs[dialogs.length-1];button('清空选择',second).click();await tick();second.querySelector<HTMLButtonElement>('[aria-label="转换 .mp3"]')!.click();await tick();button('安装并应用',second).click();await tick();
 expect(confirmation().querySelector('.selected-formats')?.textContent).toContain('PDF');expect(confirmation().querySelector('.selected-formats')?.textContent).not.toContain('MP3');
 current={...current,running:true,requested_profiles:['documents']};accept(current);
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());find('查看 Docling 安装进度').click();await tick();
 expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles')).toHaveLength(1);
 current=ready(current);doclingStatus.set(current);await tick();
 // Closing the first confirmation explicitly abandoned its completion action.
 expect(button('应用所选格式',confirmation())).toBeUndefined();button('完成',confirmation()).click();await tick();
 expect(applied()).toEqual({enabled:false});expect(JSON.parse(document.querySelector('[aria-label="另一文件夹转换设置"]')!.textContent!)).toEqual({enabled:false});
});

it('后台下载的完成按钮只应用原发起控件，另一控件保持自己的未应用草稿',async()=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{if(command==='install_docling_profiles')current={...current,running:true,requested_profiles:['documents']};return current;});
 app=mount(Harness,{target:document.body,props:{second:true}});await choose(['pdf']);button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());button('后台下载',confirmation()).click();await tick();
 document.querySelectorAll<HTMLButtonElement>('[aria-label="选择其他格式"]')[1].click();await tick();
 const dialogs=document.querySelectorAll<HTMLElement>('.docling-formats-dialog'),second=dialogs[dialogs.length-1];button('清空选择',second).click();await tick();second.querySelector<HTMLButtonElement>('[aria-label="转换 .mp3"]')!.click();await tick();button('安装并应用',second).click();await tick();
 expect(confirmation().querySelector('.selected-formats')?.textContent).toContain('PDF');
 current={...current,running:false,installed:true,stage:'done',profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))};doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();
 await vi.waitFor(()=>expect(applied()).toEqual({enabled:true,formats:['pdf']}));
 expect(JSON.parse(document.querySelector('[aria-label="另一文件夹转换设置"]')!.textContent!)).toEqual({enabled:false});expect(find('转换 .mp3').getAttribute('aria-checked')).toBe('true');
 expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles')).toEqual([['install_docling_profiles',{profiles:['documents']}]]);
});

it('取消正在下载的支持后不能再应用旧草稿，重试仍只安装原目标',async()=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{
  if(command==='install_docling_profiles')current={...current,running:true,requested_profiles:['media']};
  if(command==='cancel_docling_install')current={...current,running:false,stage:'cancelled'};return current;
 });
 app=mount(Harness,{target:document.body});await choose(['mp3']);button('安装',confirmation()).click();await vi.waitFor(()=>expect(button('取消下载',confirmation())).toBeDefined());button('取消下载',confirmation()).click();
 await vi.waitFor(()=>expect(button('重试安装',confirmation())).toBeDefined());expect(applied()).toEqual({enabled:false});button('重试安装',confirmation()).click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles')).toHaveLength(2));
 current=ready(current);doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('完成',confirmation())?.disabled).toBe(false));button('完成',confirmation()).click();await tick();
 expect(applied()).toEqual({enabled:false});expect(invoke.mock.calls.filter(([name])=>name==='install_docling_profiles').map(([,args])=>args)).toEqual([{profiles:['media']},{profiles:['media']}]);
});

it('应用所选格式等待并行数提交时关闭确认，返回后不能再回写格式',async()=>{
 let current=structuredClone(initial),finishWorkers!:()=>void;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {conversion_workers:4};
  if(command==='set_conversion_workers')return new Promise<void>(resolve=>{finishWorkers=resolve;});
  if(command==='install_docling_profiles')current={...current,running:true};return current;
 });
 app=mount(Harness,{target:document.body,props:{globalWorkers:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();find('转换 .pdf').click();
 await vi.waitFor(()=>expect(button('安装并应用')?.disabled).toBe(false));
 const input=find('转换并行数') as unknown as HTMLInputElement;input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('安装并应用').click();await tick();button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());current=ready(current);doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();
 await vi.waitFor(()=>expect(finishWorkers).toBeDefined());find('关闭下载详情').click();await tick();finishWorkers();await tick();await Promise.resolve();await tick();
 expect(applied()).toEqual({enabled:false});expect(document.querySelector('.docling-formats-dialog')).not.toBeNull();
});

it('旧完成动作等待并行数返回时，不能关闭另一控件的新安装确认',async()=>{
 let current=structuredClone(initial),finishWorkers!:()=>void;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {conversion_workers:4};
  if(command==='set_conversion_workers')return new Promise<void>(resolve=>{finishWorkers=resolve;});
  if(command==='install_docling_profiles')current={...current,running:true};return current;
 });
 app=mount(Harness,{target:document.body,props:{globalWorkers:true,second:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();find('转换 .pdf').click();
 await vi.waitFor(()=>expect(button('安装并应用')?.disabled).toBe(false));
 const input=find('转换并行数') as unknown as HTMLInputElement;input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('安装并应用').click();await tick();button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());current={...current,installed:true,running:false,stage:'done',profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))};doclingStatus.set(current);await tick();await vi.waitFor(()=>expect(button('应用所选格式',confirmation())?.disabled).toBe(false));button('应用所选格式',confirmation()).click();
 await vi.waitFor(()=>expect(finishWorkers).toBeDefined());find('关闭下载详情').click();await tick();
 document.querySelectorAll<HTMLButtonElement>('[aria-label="选择其他格式"]')[1].click();await tick();
 const dialogs=document.querySelectorAll<HTMLElement>('.docling-formats-dialog'),second=dialogs[dialogs.length-1];button('清空选择',second).click();await tick();second.querySelector<HTMLButtonElement>('[aria-label="转换 .mp3"]')!.click();await tick();button('安装并应用',second).click();await tick();
 expect(confirmation()?.querySelector('.selected-formats')?.textContent).toContain('MP3');finishWorkers();await vi.waitFor(()=>expect(input.disabled).toBe(false));await new Promise(resolve=>setTimeout(resolve,0));await tick();
 expect(confirmation()?.querySelector('.selected-formats')?.textContent).toContain('MP3');expect(applied()).toEqual({enabled:false});
});

it('后台安装时修改并行数会撤销旧完成回写，保留当前草稿',async()=>{
 let current=structuredClone(initial);
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {conversion_workers:4};
  if(command==='install_docling_profiles')current={...current,running:true,requested_profiles:['documents']};
  return current;
 });
 app=mount(Harness,{target:document.body,props:{globalWorkers:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));find('选择其他格式').click();await tick();find('转换 .pdf').click();
 await vi.waitFor(()=>expect(button('安装并应用')?.disabled).toBe(false));button('安装并应用').click();await tick();button('安装',confirmation()).click();
 await vi.waitFor(()=>expect(button('后台下载',confirmation())).toBeDefined());button('后台下载',confirmation()).click();await tick();
 const input=find('转换并行数') as unknown as HTMLInputElement;input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 current={...current,running:false,installed:true,stage:'done',profiles:initial.profiles!.map(profile=>({...profile,installed:profile.id==='documents'}))};doclingStatus.set(current);await tick();
 doclingDownloadOpen.set(true);await tick();
 expect(button('应用所选格式',confirmation())).toBeUndefined();
 button('完成',confirmation()).click();await tick();
 expect(applied()).toEqual({enabled:false});expect(input.value).toBe('2');
 expect(invoke.mock.calls.some(([name])=>name==='set_conversion_workers')).toBe(false);
 button('应用').click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_conversion_workers',{workers:2}));
});
