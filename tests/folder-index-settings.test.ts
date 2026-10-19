// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import FolderIndexSettings from '../src/components/FolderIndexSettings.svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.resetAllMocks()});
const options={file_types:{markdown:true,text:true,code:false,config:false},convert_other_formats:false};
it('重新索引提交当前文件夹和选择，显示清除范围说明',async()=>{
 invoke.mockImplementation(async command=>command==='get_folder_index_options'?structuredClone(options):{installed:true,running:false});
 const onSaved=vi.fn(),onClose=vi.fn();
 app=mount(FolderIndexSettings,{target:document.body,props:{path:'/notes',onSaved,onClose}});
 await vi.waitFor(()=>expect(document.querySelector('#folder-type-code')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('#folder-type-code')!.click();await tick();
 expect(document.querySelector('.rebuild-note')?.textContent).toContain('原文件和其他文件夹不受影响');
 document.querySelector<HTMLButtonElement>('.rebuild')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('save_folder_index_options',{path:'/notes',options:{...options,file_types:{...options.file_types,code:true}},rebuild:true}));
 await vi.waitFor(()=>expect(onSaved).toHaveBeenCalledWith(true));
 expect(onClose).toHaveBeenCalledOnce();
});
it('保存失败会保留选择并显示原因',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_folder_index_options')return structuredClone(options);
  if(command==='save_folder_index_options')throw Error('此文件夹正在索引');
  return {installed:true,running:false};
 });
 const onClose=vi.fn();
 app=mount(FolderIndexSettings,{target:document.body,props:{path:'/notes',onSaved:vi.fn(),onClose}});
 await vi.waitFor(()=>expect(document.querySelector('#folder-type-code')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('#folder-type-code')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.dialog-footer .primary')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('此文件夹正在索引'));
 expect(document.querySelector('#folder-type-code')?.getAttribute('aria-checked')).toBe('true');
 expect(onClose).not.toHaveBeenCalled();
});
it('已添加文件夹的菜单打开对应索引设置',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_folder_index_options')return structuredClone(options);
  if(command==='get_docling_status')return {installed:true,running:false};
  return [];
 });
 localStorage.setItem('sag.onboarding.seen','1');
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="Notes 的文件夹选项"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="Notes 的文件夹选项"]')!.click();await tick();
 const entry=[...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(node=>node.textContent?.includes('索引设置'))!;
 entry.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('get_folder_index_options',{path:'/notes'}));
 await vi.waitFor(()=>expect(document.querySelector('.folder-index-settings')).not.toBeNull());
});

it('子文件夹的文件列表菜单也能打开自身索引设置',async()=>{
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='list_files')return [{id:1,path:'/notes/papers/article.md',root:'/notes',name:'article.md',hash:'a'}];
  if(command==='list_entries')return [{path:'/notes/papers',name:'papers',directory:true,symlink:false}];
  if(command==='get_folder_index_options')return structuredClone(options);
  if(command==='get_docling_status')return {installed:true,running:false};
  return [];
 });
 localStorage.setItem('sag.onboarding.seen','1');
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.tree-name[title="/notes"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.simple-file-entry .simple-file-row[title="/notes/papers"]')).not.toBeNull());
 document.querySelector('.simple-file-entry')!.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true}));await tick();
 const entry=[...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(node=>node.textContent?.includes('索引设置'))!;
 expect(entry).toBeDefined();entry.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('get_folder_index_options',{path:'/notes/papers'}));
 await vi.waitFor(()=>expect(document.querySelector('.folder-index-settings .folder-path')?.textContent).toBe('/notes/papers'));
 expect(document.querySelector('.folder-index-intro')?.textContent).toContain('未单独设置的子文件夹');
});

it('已安装后可选择具体扩展名，保存后重新打开保留选择',async()=>{
 let saved={...structuredClone(options),convert_other_formats:true,conversion_formats:null as string[]|null};
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {conversion_workers:4};
  if(command==='get_folder_index_options')return structuredClone(saved);
  if(command==='get_docling_status')return {installed:true,running:false,formats:[['PDF',['pdf']],['Word',['docx','doc']],['音频',['mp3','wav']]]};
  if(command==='save_folder_index_options'){saved=structuredClone(args.options);return;}
 });
 app=mount(FolderIndexSettings,{target:document.body,props:{path:'/notes',onSaved:vi.fn(),onClose:vi.fn()}});
 const button=(label:string)=>document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
 await vi.waitFor(()=>expect(button('选择其他格式')?.disabled).toBe(false));
 button('选择其他格式').click();await tick();
 expect(button('转换 .pdf').getAttribute('aria-checked')).toBe('true');
 for(const extension of ['docx','doc','mp3','wav'])button('转换 .'+extension).click();
 await tick();
 [...document.querySelectorAll<HTMLButtonElement>('.docling-dialog .dialog-footer button')].find(button=>button.textContent==='应用')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.folder-index-settings>.dialog-footer .primary')!.click();
 await vi.waitFor(()=>expect(saved.conversion_formats).toEqual(['pdf']));
 expect(saved.convert_other_formats).toBe(true);
 await unmount(app);document.body.innerHTML='';
 app=mount(FolderIndexSettings,{target:document.body,props:{path:'/notes',onSaved:vi.fn(),onClose:vi.fn()}});
 await vi.waitFor(()=>expect(button('选择其他格式')?.disabled).toBe(false));
 button('选择其他格式').click();await tick();
 expect(button('转换 .pdf').getAttribute('aria-checked')).toBe('true');
 expect(button('转换 .docx').getAttribute('aria-checked')).toBe('false');
 expect(button('转换 .mp3').getAttribute('aria-checked')).toBe('false');
 expect(invoke.mock.calls.some(([command])=>command==='install_docling')).toBe(false);
});
