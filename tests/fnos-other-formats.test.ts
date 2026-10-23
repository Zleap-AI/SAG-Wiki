// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import Harness from './helpers/OtherFormatsHarness.svelte';
import {doclingStatus,doclingDownloadOpen,doclingInstallRequest} from '../src/lib/docling';
import {setLanguage} from '../src/lib/i18n';
import type {DoclingStatus} from '../src/lib/types';

const {api}=vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api}));
let app:ReturnType<typeof mount>;
const initial:DoclingStatus={installed:false,running:false,stage:'',message:'',downloaded:0,total:null,error:null,
 formats:[['Apple iWork',['pages','key','numbers']],['图片',['jpg','png']],['音频',['mp3']]],
 builtin_formats:[['PDF',['pdf']],['Word',['docx','doc']],['Excel / CSV',['xlsx','csv']]]};
const find=(label:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
const button=(text:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(node=>node.textContent===text)!;
const applied=()=>JSON.parse(document.querySelector('[aria-label="已应用转换设置"]')!.textContent!);
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN');doclingStatus.set(null);doclingDownloadOpen.set(false);doclingInstallRequest.set(null)});

it('shows built-in formats as ready and offers only component formats for selection',async()=>{
 api.mockImplementation(async command=>{if(command==='get_docling_status')return initial;throw Error(command)});
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式').disabled).toBe(false));
 expect(document.querySelector('.other-formats p')?.textContent).toContain('内置转换');
 find('选择其他格式').click();await tick();
 const builtin=document.querySelector('.builtin-formats')!;
 expect(builtin.textContent).toContain('无需下载');
 expect(builtin.textContent).toContain('.pdf');
 expect(builtin.textContent).toContain('.docx');
 expect(document.querySelector('[aria-label="转换 .pdf"]')).toBeNull();
 expect(find('转换 .pages')).not.toBeNull();
 expect(find('转换 .pages').getAttribute('aria-checked')).toBe('false');
 expect(document.querySelector<HTMLButtonElement>('.uninstall-docling')?.disabled).toBe(true);
 expect(api.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
});

it('does not preselect component formats for a legacy all-formats preference and keeps it on apply',async()=>{
 api.mockImplementation(async command=>{if(command==='get_docling_status')return {...initial,installed:true};throw Error(command)});
 app=mount(Harness,{target:document.body,props:{enabled:true,formats:null}});
 await vi.waitFor(()=>expect(find('选择其他格式').disabled).toBe(false));
 find('选择其他格式').click();await tick();
 expect(find('转换 .pages').getAttribute('aria-checked')).toBe('false');
 expect(find('转换 .mp3').getAttribute('aria-checked')).toBe('false');
 expect(document.body.textContent).toContain('沿用原有全部格式范围');
 button('应用').click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.docling-formats-dialog')).toBeNull());
 expect(applied()).toEqual({enabled:true,formats:null});
});

it('asks to install Docling before applying component formats and applies them once installed',async()=>{
 let current:DoclingStatus={...initial};
 api.mockImplementation(async command=>{
  if(command==='get_docling_status')return current;
  if(command==='install_docling'){current={...initial,running:true,stage:'dependencies',downloaded:1048576,total:2097152,packages_done:1,packages_total:3,message:'Downloading torch (121.4MiB)'};return current;}
  throw Error(command);
 });
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('选择其他格式').disabled).toBe(false));
 find('选择其他格式').click();await tick();find('转换 .pages').click();await tick();
 await vi.waitFor(()=>expect(button('安装并应用')?.disabled).toBe(false));
 button('安装并应用').click();await tick();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('下载文档解析组件'));
 expect(document.querySelector('.selected-formats')?.textContent).toContain('PAGES');
 expect(api.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
 button('下载并启用').click();
 await vi.waitFor(()=>expect(document.querySelector('progress')?.value).toBe(1048576));
 expect(document.body.textContent).toContain('1.0 / 2.0 MB');
 expect(document.body.textContent).toContain('已完成 1 / 3 个包');
 expect(document.querySelector('.download-activity')?.textContent).toContain('Downloading torch');
 expect(button('查看安装日志')).toBeTruthy();
 current={...initial,installed:true,stage:'done',message:'Docling 已准备就绪'};
 await vi.waitFor(()=>expect(button('应用所选格式')).toBeTruthy(),{timeout:1800});
 button('应用所选格式').click();
 await vi.waitFor(()=>expect(applied()).toEqual({enabled:true,formats:['pages']}));
 await vi.waitFor(()=>expect(document.querySelector('.docling-formats-dialog')).toBeNull());
});

it('shows the install log on request and reports a missing log as an error',async()=>{
 const current:DoclingStatus={...initial,stage:'error',error:'network unavailable'};
 api.mockImplementation(async command=>{
  if(command==='get_docling_status')return current;
  if(command==='get_docling_install_log')throw Error('尚未生成安装日志');
  throw Error(command);
 });
 app=mount(Harness,{target:document.body});
 await vi.waitFor(()=>expect(find('查看 Docling 安装进度')).not.toBeNull());
 find('查看 Docling 安装进度').click();await tick();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('network unavailable'));
 button('查看安装日志').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('尚未生成安装日志'));
 api.mockImplementation(async command=>{
  if(command==='get_docling_status')return current;
  if(command==='get_docling_install_log')return 'Downloading uv\nResolved 111 packages';
  throw Error(command);
 });
 button('查看安装日志').click();
 await vi.waitFor(()=>expect(document.querySelector('.install-log')?.textContent).toContain('Resolved 111 packages'));
});
