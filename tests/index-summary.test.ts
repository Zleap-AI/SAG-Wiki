// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import IndexDetails from '../src/components/IndexDetails.svelte';
import IndexHistoryEntry from '../src/components/IndexHistoryEntry.svelte';
import {setLanguage} from '../src/lib/i18n';

const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>{if(listeners.get(name)===handler)listeners.delete(name)}})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>|undefined;
const root={path:'/notes',name:'Notes',status:'indexing',count:42890,error:null};
const unchanged={checked:42890,reused:42890,updated:0,failed:0};
const record={id:1,path:root.path,name:root.name,started_at:1700000000000,finished_at:1700000001000,completed:42890,total:42890,status:'ready',error:null};
const settings=(language='zh-CN')=>({language,theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';listeners.clear();vi.clearAllMocks()});

it('unchanged reopen reports 42890 checked and reused, zero actual updates, and separate phase progress',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?settings():command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:[{path:root.path,stage:'indexing_converted',completed:0,total:2,files:[],summary:unchanged}]}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-summary')?.textContent).toBe('已检查 42890 · 已复用 42890 · 已更新 0 · 失败 0'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查转换结果');
 expect(document.querySelector('.index-status-meta')?.textContent).toContain('当前阶段：已处理 0 / 2 个文件');
 const bar=document.querySelector<HTMLProgressElement>('.index-status-bar');
 expect(bar?.tagName).toBe('PROGRESS');expect(bar?.value).toBe(0);expect(bar?.max).toBe(2);
 expect(bar?.getAttribute('aria-label')).toBe('当前阶段进度');
});

it.each([
 ['zh-CN','正在检查基础文本','复用已有基础文本','仅在需要时提取文字层'],
 ['en','Checking basic text','Reuse existing basic text','extract a text layer only when needed']
])('%s cached basic-text pass describes checking and reuse instead of repeat extraction',async(locale,label,reuse,extract)=>{
 invoke.mockImplementation(async command=>command==='get_settings'?settings(locale):command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:[{path:root.path,stage:'extracting_basic',completed:42890,total:42890,files:[],summary:unchanged}]}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain(label));
 expect(document.querySelector('.index-status-summary')?.textContent).toContain(locale==='en'?'Reused 42890 · Updated 0':'已复用 42890 · 已更新 0');
 document.querySelector<HTMLButtonElement>('.index-status-link')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-active .index-root-title')?.textContent).toContain(label));
 expect(document.querySelector('.index-stage-description')?.textContent).toContain(reuse);
 expect(document.querySelector('.index-stage-description')?.textContent).toContain(extract);
});

it('progress events show mixed outcomes in English without calling every checked file updated',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?settings('en'):command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:[]}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.documentElement.lang).toBe('en'));
 listeners.get('index-progress')!({payload:{path:root.path,stage:'indexing_native',count:10,total:12,summary:{checked:10,reused:7,updated:2,failed:1}}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-summary')?.textContent).toBe('Checked 10 · Reused 7 · Updated 2 · Failed 1'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('Checking text index');
 expect(document.querySelector('.index-status-meta')?.textContent).toContain('Current stage: processed 10 / 12 files');
});

it('current details and completed history distinguish reuse and updates and accept summary-only changes',async()=>{
 let summary={checked:9,reused:7,updated:1,failed:1};
 invoke.mockImplementation(async command=>command==='get_index_status'?{paused:false,tasks:[{path:root.path,stage:'indexing_native',completed:9,total:10,files:[],summary:{...summary}}]}:command==='get_index_history'?[{...record,summary:unchanged}]:[]);
 app=mount(IndexDetails,{target:document.body,props:{roots:[root],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-active .index-outcomes')?.textContent).toBe('已检查 9 · 已复用 7 · 已更新 1 · 失败 1'));
 expect(document.querySelector('.index-history .index-outcomes')?.textContent).toBe('已检查 42890 · 已复用 42890 · 已更新 0 · 失败 0');
 summary={checked:10,reused:8,updated:1,failed:1};
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-active .index-outcomes')?.textContent).toBe('已检查 10 · 已复用 8 · 已更新 1 · 失败 1'));
 expect(document.querySelector('.index-active .index-count')?.textContent).toContain('当前阶段：已处理 9 / 10 个文件');
});

it('old history keeps processed totals and omits unknown outcomes instead of inventing zero updates',async()=>{
 setLanguage('en');
 app=mount(IndexHistoryEntry,{target:document.body,props:{record}});
 expect(document.querySelector('.index-count')?.textContent).toBe('Processed 42890 / 42890 files');
 expect(document.querySelector('.index-outcomes')).toBeNull();
 expect(document.querySelector('.index-history-toggle')?.textContent).not.toMatch(/Reused|Updated|Failed 0/);
});

it('legacy live progress keeps honest processed counts without unknown reuse or update totals',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?settings():command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:[{path:root.path,stage:'converting',completed:4,total:10,files:[]}]}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-meta')?.textContent).toContain('已处理 4 / 10 个文件'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文件转换');
 expect(document.querySelector('.index-status-summary')).toBeNull();
});

it.each([true,false])('parser and document parts belong to the displayed lane and phase even when events arrive before snapshots (ids %s)',async(withIds)=>{
 const primary={id:withIds?'primary:/notes':undefined,kind:'primary',path:root.path,stage:'indexing_native',completed:2,total:5,files:['/notes/a.md'],summary:{checked:2,reused:2,updated:0,failed:0}};
 const enhancement={id:withIds?'enhancement:/notes':undefined,kind:'enhancement',path:root.path,stage:'enhancing',completed:1,total:3,files:['/notes/scan.pdf'],conversions:[{path:'/notes/scan.pdf',engine:'Docling',segment:{current:2,total:3,start_page:51,end_page:100}}],summary:{checked:1,reused:0,updated:1,failed:0}};
 invoke.mockImplementation(async command=>command==='get_settings'?settings():command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:structuredClone([primary,enhancement])}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文本索引'));
 expect(document.querySelector('.index-status-parser')).toBeNull();expect(document.querySelector('.index-status-segment')).toBeNull();
 listeners.get('index-progress')!({payload:{...enhancement,count:1}});await tick();
 expect(document.querySelector('.index-status-parser')?.textContent).toBe('解析方式：Docling');
 expect(document.querySelector('.index-status-segment')?.textContent).toContain('第 2 / 3 段');
 listeners.get('index-progress')!({payload:{...enhancement,stage:'indexing_converted',count:0,total:1}});await tick();
 expect(document.querySelector('.index-status-parser')).toBeNull();expect(document.querySelector('.index-status-segment')).toBeNull();
 listeners.get('index-progress')!({payload:{...primary,count:3}});await tick();
 expect(document.querySelector('.index-status-parser')).toBeNull();expect(document.querySelector('.index-status-segment')).toBeNull();
 expect(document.querySelector('.index-status-summary')?.textContent).toBe('已检查 2 · 已复用 2 · 已更新 0 · 失败 0');
});

it('an ambiguous legacy same-folder event does not borrow a parser or stage from either lane',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?settings():command==='list_roots'?[root]:command==='get_index_status'?{paused:false,tasks:[
  {kind:'enhancement',path:root.path,stage:'enhancing',completed:1,total:3,files:['/notes/scan.pdf'],conversions:[{path:'/notes/scan.pdf',engine:'Docling'}]},
  {kind:'primary',path:root.path,stage:'indexing_native',completed:2,total:5,files:['/notes/a.md']}
 ]}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文本索引'));
 listeners.get('index-progress')!({payload:{path:root.path,count:3,total:5}});await tick();
 expect(document.querySelector('.index-status-primary')?.textContent).toBe('SAG 正在索引（60%）');
 expect(document.querySelector('.index-status-parser')).toBeNull();
});
