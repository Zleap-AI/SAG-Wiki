// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();listeners.clear();localStorage.clear()});
const failures=[{path:'/notes/broken.pdf',root:'/notes',error:'识别超时'},{path:'/notes/second.epub',root:'/notes',error:'索引写入失败'}];
function start(active=true,searchable=false,aggregate?:object){
 const task={id:'enhancement:/notes',kind:'enhancement',path:'/notes',stage:'enhancing',completed:1,total:2,files:['/notes/guide.pdf'],conversions:[{path:'/notes/guide.pdf',engine:'Docling',segment_progress:aggregate,segment:{current:4,total:8,start_page:151,end_page:200}}],summary:{checked:1,reused:0,updated:1,failed:0}};
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:
  command==='list_roots'?[{path:'/notes',name:'Notes',status:active?'indexing':'partial',count:1,error:active?null:'两个文件未能处理'}]:
  command==='get_index_status'?{paused:false,tasks:active?[task]:[],failed_files:active?0:2,failed_documents:active?[]:failures,conversion_states:active?[{source:'/notes/guide.pdf',root:'/notes',status:'enhancing',searchable,error:null}]:[]}:
  command==='retry_failed_indexes'?[]:[]);
 app=mount(App,{target:document.body});
}
it('keeps only title/current filename outside inline details and uses completed segment progress',async()=>{
 start();await vi.waitFor(()=>expect(document.querySelector('.index-status-card details summary')).not.toBeNull());
 const card=document.querySelector('.index-status-card')!;
 expect(card.querySelector('.index-status-primary')?.textContent).toContain('正在解析文档（1/2）');
 expect(card.querySelector('.index-status-current-file')?.textContent).toContain('guide.pdf');
 const details=card.querySelector('details')!;expect(details.open).toBe(false);
 expect(details.textContent).toContain('解析完成后可搜索');
 const progress=details.querySelector('progress')!;expect(progress.value).toBe(3);expect(progress.max).toBe(8);
 for(const removed of ['已检查','已复用','本批文件','隐藏进度'])expect(card.textContent).not.toContain(removed);
});
it('uses the explicit ready count and shows the pages of all active segments',async()=>{
 start(true,true,{total:8,completed:5,failed:0,cancelled:0,active:[
  {segment_id:4,start_page:151,end_page:200,engine:'MinerU',phase:'parsing'},
  {segment_id:7,start_page:301,end_page:350,engine:'Docling',phase:'running_local'}]});
 await vi.waitFor(()=>expect(document.querySelector('progress')).not.toBeNull());
 const details=document.querySelector('.index-status-card details')!;
 expect(details.querySelector('progress')?.value).toBe(5);
 expect(details.textContent).toContain('301–350');
 expect(details.textContent).toContain('MinerU');
});
it('shows actual partial search availability without claiming full indexing is finished',async()=>{
 start(true,true);await vi.waitFor(()=>expect(document.querySelector('.index-status-card details')?.textContent).toContain('已可搜索部分内容'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在解析文档');
});
it('lists all failed files after completion, omits a current filename, and retries only failures',async()=>{
 start(false);await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('部分文件处理失败（2）'));
 const card=document.querySelector('.index-status-card')!;
 expect(card.querySelector('.index-status-current-file')).toBeNull();
 const details=card.querySelector('details')!;expect(details.textContent).toContain('broken.pdf');expect(details.textContent).toContain('second.epub');expect(details.textContent).toContain('索引写入失败');
 const retry=Array.from(details.querySelectorAll('button')).find(button=>button.textContent?.includes('重试失败文件'))!;
 expect(retry).toBeTruthy();retry.click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('retry_failed_indexes',undefined));
 expect(invoke.mock.calls.some(([command])=>command==='reindex_all')).toBe(false);
});

it('shows newly indexed files in details while an older PDF keeps processing',async()=>{
 let files:any[]=[];
 const task={path:'/notes',kind:'enhancement',stage:'enhancing',completed:0,total:1,files:['/notes/long.pdf']};
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:[task]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:'indexing',count:files.length,error:null}]:command==='list_files'?files:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-current-file')?.textContent).toContain('long.pdf'));
 files=[{id:1,path:'/notes/new.md',name:'new.md'}];listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.recent-files')?.textContent).toContain('new.md'));
 expect(document.querySelector('.recent-files')?.textContent).toContain('新增文件已可搜索');
 expect(document.querySelector('.index-status-current-file')?.textContent).toContain('long.pdf');
});

it('keeps a submitted retry in progress across snapshots and clears it after completion',async()=>{
 let release!:(roots:any[])=>void;
 let running=false,finished=false;
 const root={path:'/notes',name:'Notes',status:'partial',count:1,error:'failed'};
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:running?[{path:root.path,stage:'converting',completed:0,total:1,files:['/notes/broken.pdf']}]:[],failed_files:finished?0:2,failed_documents:finished?[]:failures}:command==='list_roots'?[{...root,status:running?'indexing':finished?'ready':'partial',error:finished?null:root.error}]:command==='retry_failed_indexes'?new Promise(resolve=>{release=resolve}):[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.failed-files li')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.index-retry-button')!.click();
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在重试失败文件'));
 expect(document.querySelector<HTMLButtonElement>('.index-retry-button')?.disabled).toBe(true);
 running=true;release([{...root,status:'pending'}]);listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-current-file')?.textContent).toContain('broken.pdf'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（0/1）'); // Current task scope, not all initial failures.
 running=false;finished=true;listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('文档解析完成'));
 expect(document.querySelector('.index-status-current-file')).toBeNull();
 expect(document.querySelector('.index-retry-button')).toBeNull();
});

it('keeps watcher errors visible alongside actionable file failures',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:[],failed_files:2,failed_documents:failures}:command==='list_roots'?[{path:'/notes',name:'Notes',status:'partial',count:1,error:'file failures'},{path:'/watch',name:'Watch',status:'partial',count:1,error:'文件监听失败，自动更新受限'}]:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.card-details')?.textContent).toContain('文件监听失败，自动更新受限'));
 expect(document.querySelectorAll('.failed-files li')).toHaveLength(2);
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（2）');
});

it('active and paused cards can be dismissed and new results restore them',async()=>{
 let phase='partial',reason='first failure';
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:phase==='paused',tasks:phase==='indexing'?[{path:'/notes',stage:'converting',completed:0,total:1,files:['/notes/new.pdf']}]:[],failed_files:phase==='partial'?1:0,failed_documents:phase==='partial'?[{...failures[0],error:reason}]:[]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:phase==='paused'?'ready':phase,count:1,error:phase==='partial'?reason:null}]:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="隐藏索引状态"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="隐藏索引状态"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-status-card')).toBeNull());
 listeners.get('data-changed')!({payload:null});await new Promise(resolve=>setTimeout(resolve,20));
 expect(document.querySelector('.index-status-card')).toBeNull();
 reason='new failure';listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-card')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="隐藏索引状态"]')!.click();
 phase='indexing';listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-current-file')?.textContent).toContain('new.pdf'));
 expect(document.querySelector('[aria-label="隐藏索引状态"]')).not.toBeNull();
 phase='paused';listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('已暂停'));
 expect(document.querySelector('[aria-label="隐藏索引状态"]')).not.toBeNull();
 phase='ready';listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('文档解析完成'));
 document.querySelector<HTMLButtonElement>('[aria-label="隐藏索引状态"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-status-card')).toBeNull());
});

it('pauses and resumes from title controls without expanding details, and keeps active hiding across progress',async()=>{
 let paused=false,completed=0;
 const task=()=>({path:'/notes',stage:'indexing',completed,total:10,files:paused?[]:['/notes/a.md']});
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {language:'zh-CN',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'indexing',count:completed,error:null}];
  if(command==='get_index_status')return {paused,tasks:[task()]};
  if(command==='set_index_paused'){paused=args.paused;return {paused,tasks:[task()]};}
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.card-actions [aria-label="暂停索引"]')).not.toBeNull());
 expect(document.querySelector<HTMLDetailsElement>('.index-status-card details')?.open).toBe(false);
 document.querySelector<HTMLButtonElement>('.card-actions [aria-label="暂停索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:true}));
 await vi.waitFor(()=>expect(document.querySelector('.card-actions [aria-label="继续索引"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.card-actions [aria-label="继续索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:false}));
 await vi.waitFor(()=>expect(document.querySelector('.card-actions [aria-label="暂停索引"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.card-actions [aria-label="隐藏索引状态"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-status-card')).toBeNull());
 const pauseCalls=invoke.mock.calls.filter(([command])=>command==='set_index_paused').length;
 completed=1;listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='list_roots').length).toBeGreaterThan(2));
 expect(document.querySelector('.index-status-card')).toBeNull();
 expect(invoke.mock.calls.filter(([command])=>command==='set_index_paused')).toHaveLength(pauseCalls);
});

it('reports pause errors and allows retrying the title control',async()=>{
 start();
 await vi.waitFor(()=>expect(document.querySelector('.pause-button')).not.toBeNull());
 const original=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>{if(command==='set_index_paused')throw Error('无法暂停索引');return original(command,...args)});
 document.querySelector<HTMLButtonElement>('.pause-button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('无法暂停索引'));
 expect(document.querySelector<HTMLButtonElement>('.pause-button')?.disabled).toBe(false);
});

it('refreshes a stable snapshot when only another segment phase changes',async()=>{
 const {sameIndexStatus}=await import('../src/lib/stable-snapshot');
 const status=(phase:string)=>({paused:false,tasks:[{path:'/notes',stage:'enhancing',completed:0,total:1,files:['guide.pdf'],conversions:[{path:'guide.pdf',engine:'MinerU',segment_progress:{total:8,completed:1,failed:0,cancelled:0,active:[{segment_id:2,start_page:51,end_page:100,engine:'MinerU',phase}]}}]}]});
 expect(sameIndexStatus(status('queued'),status('parsing'))).toBe(false);
 expect(sameIndexStatus(status('parsing'),status('parsing'))).toBe(true);
});
