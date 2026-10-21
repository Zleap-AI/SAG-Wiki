// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import OtherFormats from './helpers/OtherFormatsHarness.svelte';
import {doclingStatus,doclingDownloadOpen} from '../src/lib/docling';
import {setLanguage} from '../src/lib/i18n';
import type {DoclingStatus} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,formats:[['PDF',['pdf']],['Word',['docx','doc']],['PowerPoint',['pptx']]]};
const find=(label:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();vi.useRealTimers();setLanguage('zh-CN');doclingStatus.set(null);doclingDownloadOpen.set(false)});

it('未安装时只有选择入口，从所选格式确认后才安装',async()=>{
 invoke.mockResolvedValue(initial);
 app=mount(OtherFormats,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式').disabled).toBe(false));
 expect(find('选择其他格式').textContent).toBe('选择');
 expect(document.querySelector('.other-formats [role="checkbox"]')).toBeNull();
 expect(document.querySelector('.format-select')).toBeNull();
 expect(document.querySelector('.other-formats p')?.textContent).toBe('本地转换为 Markdown，原文件保留。');
 find('选择其他格式').click();await tick();
 await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='安装并应用')?.disabled).toBe(false));
 (Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='安装并应用')!).click();await tick();
 expect(document.body.textContent).toContain('安装文件解析支持');
 expect(invoke.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
 (Array.from(document.querySelectorAll<HTMLButtonElement>('.docling-dialog:not(.docling-formats-dialog) button')).find(button=>button.textContent==='取消')!).click();await tick();
 expect(find('选择其他格式').textContent).toBe('选择');
});

it('下载展示实际字节与阶段，成功后显示选择，失败可重试',async()=>{
 let current={...initial};
 invoke.mockImplementation(async command=>{
  if(command==='get_docling_status')return current;
  if(command==='install_docling'){current={...initial,running:true,stage:'runtime',downloaded:1048576,total:2097152,message:'download'};return current;}
 });
 app=mount(OtherFormats,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式').disabled).toBe(false));
 find('选择其他格式').click();await tick();
 await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='安装并应用')?.disabled).toBe(false));
 (Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='安装并应用')!).click();await tick();
 (Array.from(document.querySelectorAll<HTMLButtonElement>('.docling-dialog:not(.docling-formats-dialog) button')).find(button=>button.textContent==='安装')!).click();
 await vi.waitFor(()=>expect(document.querySelector('progress')?.value).toBe(1048576));
 expect(document.body.textContent).toContain('1.0 / 2.0 MB');
 current={...initial,stage:'error',error:'network unavailable'};
 await vi.waitFor(()=>expect(document.body.textContent).toContain('network unavailable'),{timeout:1800});
 expect(find('选择其他格式').textContent).toBe('选择');
 (Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='重试安装')!).click();
 await vi.waitFor(()=>expect(document.querySelector('progress')).not.toBeNull());
 current={...initial,stage:'done',installed:true,message:'Docling 已准备就绪'};
 await vi.waitFor(()=>expect(document.body.textContent).toContain('文件解析支持已就绪'),{timeout:1800});
 expect(find('选择其他格式')).not.toBeNull();
});

it('英文显示完整，已安装时无需重复下载',async()=>{
 setLanguage('en');invoke.mockResolvedValue({...initial,installed:true});
 app=mount(OtherFormats,{target:document.body});
 await vi.waitFor(()=>expect(find('Select other formats').disabled).toBe(false));
 find('Select other formats').click();await tick();
 expect(find('Select other formats').textContent).toBe('Select');
 expect(document.querySelector('.other-formats [role="checkbox"]')).toBeNull();
 expect(document.body.textContent).not.toMatch(/[\u3400-\u9fff]/);
 expect(invoke.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
});

it('状态连接中断一次后继续刷新，取消下载后退出运行状态',async()=>{
 let current={...initial,running:true,stage:'dependencies',message:'Downloading torch'};
 let reads=0;
 invoke.mockImplementation(async command=>{
  if(command==='get_docling_status'){
   if(++reads===2)throw new Error('Connection reset by peer (os error 54)');
   return current;
  }
  if(command==='cancel_docling_install'){current={...initial,stage:'cancelled'};return;}
 });
 app=mount(OtherFormats,{target:document.body});
 await vi.waitFor(()=>expect(find('查看 Docling 安装进度')).not.toBeNull());
 find('查看 Docling 安装进度').click();await tick();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('取消下载'));
 await vi.waitFor(()=>expect(reads).toBeGreaterThan(2),{timeout:2500});
 (Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='取消下载')!).click();
 await vi.waitFor(()=>expect(document.body.textContent).not.toContain('取消下载'),{timeout:1800});
 expect(find('选择其他格式').textContent).toBe('选择');
});

it('选择窗口说明转换位置，主页通过系统浏览器打开，卸载需确认且失败保留格式选择',async()=>{
 let current={...initial,installed:true},fail=true;
 invoke.mockImplementation(async(command)=>{
  if(command==='get_docling_status')return current;
  if(command==='uninstall_docling'){
   if(fail)throw Error('Docling 正在使用');
   current={...initial,stage:'uninstalled'};return current;
  }
 });
 app=mount(OtherFormats,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 find('选择其他格式').click();await tick();
 expect(document.querySelector('.conversion-info')?.textContent).toContain('已添加文件夹根目录的 .sag-converted');
 expect(document.querySelector('.conversion-info code')?.textContent).toContain('.sag-converted/资料/报告.pdf.md');
 document.querySelector<HTMLAnchorElement>('.docling-component a')!.click();await tick();
 expect(invoke).toHaveBeenCalledWith('open_about_link',{target:'docling'});
 find('转换 .docx').click();await tick();
 document.querySelector<HTMLButtonElement>('.uninstall-docling')!.click();await tick();
 expect(invoke.mock.calls.some(([command])=>command==='uninstall_docling')).toBe(false);
 expect(document.querySelector('.docling-uninstall-dialog')?.textContent).toContain('原文件、.sag-converted 中的转换文件及已有索引都会保留');
 find('确认卸载 Docling').click();
 await vi.waitFor(()=>expect(document.querySelector('.docling-uninstall-dialog [role="alert"]')?.textContent).toContain('Docling 正在使用'));
 expect(find('转换 .docx').getAttribute('aria-checked')).toBe('false');
 fail=false;find('确认卸载 Docling').click();
 await vi.waitFor(()=>expect(find('选择其他格式')?.textContent).toBe('选择'));
 await vi.waitFor(()=>expect(document.querySelector('.docling-formats-dialog')).toBeNull());
});

it('其他格式可保留并保存已配置的转换并行数，输入错误后可修正',async()=>{
 let workers=4;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_docling_status')return {...initial,installed:true};
  if(command==='get_settings')return {conversion_workers:workers};
  if(command==='set_conversion_workers'){workers=args.workers;return;}
  throw Error(command);
 });
 app=mount(OtherFormats,{target:document.body,props:{globalWorkers:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 find('选择其他格式').click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')?.disabled).toBe(false));
 const input=document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')!;
 expect(input.value).toBe('4');
 expect(document.querySelector('.conversion-info')?.textContent).toContain('先索引 Markdown');
 input.value='0';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='应用')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('1–16'));
 expect(invoke.mock.calls.some(([command])=>command==='set_conversion_workers')).toBe(false);
 input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='应用')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_conversion_workers',{workers:2}));
 await vi.waitFor(()=>expect(document.querySelector('.docling-formats-dialog')).toBeNull());
 find('选择其他格式').click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')?.value).toBe('2'));
});

it('旧设置缺少转换并行数时使用单个转换，并说明实际并行受资源预算约束',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_docling_status')return {...initial,installed:true};
  if(command==='get_settings')return {};
  throw Error(command);
 });
 app=mount(OtherFormats,{target:document.body,props:{globalWorkers:true}});
 await vi.waitFor(()=>expect(find('选择其他格式')?.disabled).toBe(false));
 find('选择其他格式').click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')?.disabled).toBe(false));
 expect(document.querySelector<HTMLInputElement>('[aria-label="转换并行数"]')?.value).toBe('1');
 expect(document.querySelector('.conversion-workers')?.textContent).toContain('资源预算');
});
