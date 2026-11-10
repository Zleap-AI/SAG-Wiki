// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import LibraryWorkspace from '../src/components/LibraryWorkspace.svelte';
import {createLibrarySession} from '../src/lib/explore';
import {setLanguage} from '../src/lib/i18n';
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';setLanguage('zh-CN')});
const entry=(name:string,directory=false)=>({path:'/notes/'+name,name,directory,symlink:false});
const names=()=>[...document.querySelectorAll('.simple-file-entry')].map(row=>row.textContent);
async function next(label='按索引状态筛选'){
 document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click();await tick();
 expect(document.querySelector('[role=listbox]')).toBeNull();
}
it.each(['zh-CN','en'] as const)('%s 筛选索引状态，文件夹保持可进入，文件数量随筛选更新',async(locale)=>{
 setLanguage(locale);const browse=vi.fn();
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[entry('图片',true),entry('正文.md'),entry('cover.png')],indexedFiles:[{id:1,path:'/notes/正文.md',name:'正文.md'}],reading:false,revision:0,onbrowse:browse,onopen:vi.fn(),onmenu:vi.fn()}});
 const label=locale==='en'?'Filter by index status':'按索引状态筛选';
 expect(names()).toEqual(['图片','正文.md','cover.png']);
 expect(document.querySelector(`[aria-label="${label}"]`)?.textContent).toBe(locale==='en'?'All':'全部');
 await next(label);
 expect(names()).toEqual(['图片','正文.md']);
 expect(document.querySelector('.folder-count')?.textContent).toBe(locale==='en'?'1 folders · 1 files':'1 个文件夹 · 1 个文件');
 await next(label);
 expect(names()).toEqual(['图片','cover.png']);
 expect(document.querySelector('.simple-file-entry.unindexed')?.textContent).toBe('cover.png');
 document.querySelector<HTMLButtonElement>('.simple-file-entry .simple-file-row')!.click();
 expect(browse).toHaveBeenCalledWith('/notes/图片');
 await next(label);
 expect(names()).toEqual(['图片','正文.md','cover.png']);
});
it('筛选无匹配文件时显示提示，切回全部后恢复列表',async()=>{
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[entry('正文.md')],indexedFiles:[{id:1,path:'/notes/正文.md',name:'正文.md'}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn()}});
 await next();await next();
 expect(names()).toEqual([]);expect(document.querySelector('.empty')?.textContent).toBe('没有符合条件的文件');
 expect(document.querySelector('[aria-label="返回上层"]')).not.toBeNull();
 await next();expect(names()).toEqual(['正文.md']);expect(document.querySelector('.empty')).toBeNull();
});

it('转换后的文档原文件显示为已索引并可直接打开',async()=>{
 const browse=vi.fn();
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[entry('report.docx')],indexedFiles:[{id:1,path:'/notes/.sag-converted/report.docx.md',name:'report.docx.md'}],reading:false,revision:0,onbrowse:browse,onopen:vi.fn(),onmenu:vi.fn()}});
 expect(document.querySelector('.simple-file-entry.unindexed')).toBeNull();
 await next();
 expect(names()).toEqual(['report.docx']);
 document.querySelector<HTMLButtonElement>('.simple-file-entry .simple-file-row')!.click();
 expect(browse).toHaveBeenCalledWith('/notes/report.docx');
 expect(document.querySelector('[role=dialog]')).toBeNull();
});

it('shows selected folder conversion stage and waiting state persistently',async()=>{
 const details=vi.fn();app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[entry('a.pdf')],roots:[{path:'/notes',name:'Notes',status:'pending',count:0,error:null}],indexTasks:[{path:'/other',stage:'converting',completed:18,total:20,files:['/other/old.pdf']}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn(),onIndexDetails:details}});
 expect(document.querySelector('.folder-index-status')?.textContent).toContain('等待后台处理');expect(document.querySelector('.folder-index-status')?.textContent).toContain('其他文件夹');
 document.querySelector<HTMLButtonElement>('.folder-index-status button')!.click();expect(details).toHaveBeenCalled();
});
it('shows active conversion counts and filename without requiring a dialog',async()=>{
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[entry('a.pdf')],roots:[{path:'/notes',name:'Notes',status:'indexing',count:0,error:null}],indexTasks:[{path:'/notes',stage:'converting',completed:2,total:4,files:['/notes/a.pdf']}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn()}});
 expect(document.querySelector('.folder-index-status')?.textContent).toContain('正在转换文件');expect(document.querySelector('.folder-index-status')?.textContent).toContain('2 / 4');expect(document.querySelector('.folder-index-status')?.textContent).toContain('a.pdf');
 document.querySelector<HTMLButtonElement>('.simple-file-entry button')!.click();await tick();expect(document.querySelector('[role=dialog]')?.textContent).toContain('文件正在处理中');expect(document.querySelector('[role=dialog]')?.textContent).not.toContain('去设置');
});
