// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import LibraryWorkspace from '../src/components/LibraryWorkspace.svelte';
import {createLibrarySession} from '../src/lib/explore';
import {setLanguage} from '../src/lib/i18n';
import {fileIndexState,indexStage,indexPipeline} from '../src/lib/index-stages';
let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';setLanguage('zh-CN')});
it.each(['zh-CN','en'] as const)('%s preserves access to basic text and explains enhancement failure',async(locale)=>{
 setLanguage(locale);const browse=vi.fn();
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[{path:'/notes/a.pdf',name:'a.pdf',directory:false,symlink:false}],indexedFiles:[{id:1,path:'/notes/.sag-converted/a.pdf.md',name:'a.pdf.md',index_status:'enhancement_failed',index_error:'OCR timeout'}],reading:false,revision:0,onbrowse:browse,onopen:vi.fn(),onmenu:vi.fn()}});
 const badge=document.querySelector('.file-index-state');
 expect(badge?.textContent).toContain(locale==='en'?'Basic text searchable · Enhancement failed':'基础文本可搜索 · 增强失败');
 expect(badge?.getAttribute('title')).toContain('OCR timeout');
 expect(document.querySelector('.simple-file-entry.unindexed')).toBeNull();
 document.querySelector<HTMLButtonElement>('.simple-file-entry button')!.click();await tick();expect(browse).toHaveBeenCalledWith('/notes/a.pdf');
});
it('does not describe scans as already searchable during OCR',()=>{
 expect(indexStage('awaiting_ocr')?.description).toContain('尚不可搜索');
 expect(indexStage('enhancing')?.description).toContain('已有基础文本');
});
it.each([['basic','基础文本可搜索 · 等待增强'],['enhancing','基础文本可搜索 · 增强中']] as const)('%s basic text remains open during background work',async(status,label)=>{
 const browse=vi.fn();app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[{path:'/notes/mixed.pdf',name:'mixed.pdf',directory:false,symlink:false}],indexedFiles:[{id:1,path:'/notes/.sag-converted/mixed.pdf.md',name:'mixed.pdf.md',index_status:status}],reading:false,revision:0,onbrowse:browse,onopen:vi.fn(),onmenu:vi.fn()}});
 expect(document.querySelector('.file-index-state')?.textContent).toBe(label);
 document.querySelector<HTMLButtonElement>('.simple-file-entry button')!.click();await tick();expect(browse).toHaveBeenCalledWith('/notes/mixed.pdf');
});
it('shows a pure scan waiting for OCR without making it searchable',()=>{
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[{path:'/notes/scan.pdf',name:'scan.pdf',directory:false,symlink:false}],indexedFiles:[],conversionStates:[{source:'/notes/scan.pdf',root:'/notes',status:'awaiting_ocr',searchable:false,error:null}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn()}});
 expect(document.querySelector('.simple-file-entry.unindexed')).not.toBeNull();
 expect(document.querySelector('.file-index-state')?.textContent).toBe('等待 OCR');
 expect(document.querySelector('.file-index-state')?.getAttribute('title')).toContain('尚不可搜索');
});
it('does not claim retained basic text when pure-scan recognition fails',()=>{
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[{path:'/notes/scan.pdf',name:'scan.pdf',directory:false,symlink:false}],indexedFiles:[],conversionStates:[{source:'/notes/scan.pdf',root:'/notes',status:'enhancement_failed',searchable:false,error:'OCR timeout'}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn()}});
 expect(document.querySelector('.file-index-state')?.textContent).toBe('识别失败 · 尚不可搜索');
 expect(document.querySelector('.file-index-state')?.textContent).not.toContain('基础文本可搜索');
});

it('keeps enhancement retry in the progressive pipeline',()=>{
 expect(indexStage('retrying_enhancement')?.description).toContain('基础文本');
 expect(indexPipeline('retrying_enhancement').map(item=>item.stage)).toContain('enhancing');
});
it('uses a generic parsing label for sources without extracted text',()=>{
 const {label,description}=fileIndexState({index_status:'enhancing',searchable:false})!;
 expect(label).toBe('内容解析中 · 尚不可搜索');
 expect(description).not.toContain('没有可用文字层');
});
it('does not claim OCR is required when extraction has not diagnosed the source',()=>{
 const state=fileIndexState({index_status:'awaiting_conversion',searchable:false});
 expect(state?.label).toBe('等待解析');
 expect(state?.description).toContain('尚不可搜索');
 expect(state?.description).not.toContain('OCR');
});
it('shows both same-folder indexing lanes without losing enhancement progress',()=>{
 app=mount(LibraryWorkspace,{target:document.body,props:{scope:'/notes',session:createLibrarySession(),entries:[],indexedFiles:[],roots:[{path:'/notes',name:'Notes',status:'indexing',count:1,error:null}],indexTasks:[{id:'enhance:/notes',kind:'enhancement',path:'/notes',stage:'enhancing',completed:1,total:3,files:['/notes/old.pdf']},{id:'primary:/notes',kind:'primary',path:'/notes',stage:'indexing_native',completed:2,total:5,files:['/notes/new.md']}],reading:false,revision:0,onbrowse:vi.fn(),onopen:vi.fn(),onmenu:vi.fn()}});
 const status=document.querySelector('.folder-index-status');
 expect(status?.textContent).toContain('2 / 5');expect(status?.textContent).toContain('1 / 3');
 expect(status?.textContent).toContain('new.md');expect(status?.textContent).toContain('old.pdf');
});

it('falls back to distinct legacy keys when a Rust bridge defaults missing ids to empty strings',async()=>{
 const {indexTaskKey}=await import('../src/lib/stable-snapshot');
 const task={id:'',path:'/notes',stage:'indexing_native',completed:0,total:1,files:[]};
 expect(indexTaskKey(task)).toBe('/notes:primary');
 expect(indexTaskKey({...task,path:'/other'})).toBe('/other:primary');
});
