// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import IndexDetails from '../src/components/IndexDetails.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
beforeEach(()=>localStorage.removeItem('sag.index.dismissed-failures'));
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();listeners.clear();localStorage.removeItem('sag.index.dismissed-failures')});
it('completed partial failures keep reasons in details and preserve indexed history',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:[],failed_files:1,failed_documents:[{root:'/notes',path:'/notes/broken.txt',error:'文件无法按 UTF-16LE 编码读取'}]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:'partial',count:270,error:'未能索引 1 个文件\n\n/notes/broken.txt\n文件无法按 UTF-16LE 编码读取'}]:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('部分文件处理失败（1）'));
 expect(document.querySelector('.index-status-current-file')).toBeNull();
 expect(document.querySelector('details')?.open).toBe(false);
 expect(document.querySelector('.failed-files')?.textContent).toContain('broken.txt');
 expect(document.querySelector('.failed-files')?.textContent).toContain('UTF-16LE');
 expect(document.querySelector('.index-status-summary')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('/notes/broken.txt'));
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('已索引 270 个文件');
});

it('failed results remain discoverable after reopen and update when the reason changes',async()=>{
 let reason='first failure';
 invoke.mockImplementation(async command=>command==='get_index_status'?{paused:false,tasks:[],failed_files:1,failed_documents:[{root:'/notes',path:'/notes/broken.pdf',error:reason}]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:'partial',count:2,error:reason}]:command==='get_settings'?{theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.failed-files')?.textContent).toContain(reason));
 expect(document.querySelector('[aria-label="隐藏索引状态"]')).not.toBeNull();
 await unmount(app);app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.failed-files')?.textContent).toContain(reason));
 reason='new failure';listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.failed-files')?.textContent).toContain(reason));
});

it('active conversion stays primary and completion clears the current filename',async()=>{
 let ready=false;
 invoke.mockImplementation(async command=>command==='get_settings'?{theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:ready?[]:[{path:'/notes',stage:'converting',completed:27,total:100,files:['/notes/report.pdf']}]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:ready?'ready':'indexing',count:27,error:null}]:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在解析文档（27/100）'));
 expect(document.querySelector('.index-status-current-file')?.textContent).toBe('report.pdf');
 expect(document.querySelector('.index-status-meta')).toBeNull();
 ready=true;listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('文档解析完成'));
 expect(document.querySelector('.index-status-current-file')).toBeNull();
});

it('English zero progress remains a file count and does not promote a separate failure',async()=>{
 invoke.mockImplementation(async command=>command==='get_settings'?{language:'en',theme:'light',start_page:'ask',answer:{model:'test',models:['test']}}:command==='get_index_status'?{paused:false,tasks:[{path:'/active',stage:'converting',completed:0,total:40,files:['/active/report.pdf']}],failed_files:1,failed_documents:[{root:'/failed',path:'/failed/broken.pdf',error:'read failed'}]}:command==='list_roots'?[{path:'/active',name:'Active',status:'indexing',count:0,error:null},{path:'/failed',name:'Failed',status:'partial',count:270,error:'read failed'}]:[]);
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('Processing documents（0/40）'));
 expect(document.querySelector('.index-status-primary')?.textContent).not.toContain('failure');
 expect(document.querySelector('.failed-files')?.textContent).toContain('read failed');
 expect(document.querySelector<HTMLButtonElement>('.index-retry-button')?.disabled).toBe(true);
});

it('运行中的索引可查看当前文件并暂停、继续，暂停状态显示在侧栏',async()=>{
 let paused=false;
 const root={path:'/notes',name:'Notes',status:'indexing',count:12,error:null};
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [root];
  if(command==='set_index_paused')paused=args.paused;
  if(command==='get_index_status'||command==='set_index_paused')return {paused,tasks:[{path:'/notes',stage:'indexing',completed:12,total:100,files:paused?[]:['/notes/meeting.md']}]};
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="查看索引详情"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('/notes/meeting.md'));
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('12 / 100');
 document.querySelector<HTMLButtonElement>('.index-issues-dialog [aria-label="暂停索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:true}));
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toContain('文档处理已暂停'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（12/100）');
 expect(document.querySelector('.index-active .index-root-title span')?.textContent).toBe('索引已暂停');
 expect(document.querySelector('.index-active')?.textContent).not.toContain('正在索引文件');
 document.querySelector<HTMLButtonElement>('.index-issues-dialog [aria-label="继续索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:false}));
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).not.toContain('文档处理已暂停'));
 expect(document.querySelector('.index-active .index-root-title span')?.textContent).toBe('正在索引文件');
});

it('暂停过程中分别显示仍在收尾和已经暂停的任务，并保留进度',async()=>{
 let files=['/active/test.md'];
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:true,tasks:[{path:'/active',stage:'indexing',completed:12,total:100,files:[...files]}]};
  if(command==='get_index_history')return [];
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[{path:'/active',name:'Active',status:'indexing',count:12,error:null},{path:'/pending',name:'Pending',status:'pending',count:0,error:null}],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-summary-state')?.textContent).toContain('正在暂停…'));
 expect([...document.querySelectorAll('.index-active .index-root-title>span')].map(node=>node.textContent)).toEqual(['正在暂停…','索引已暂停']);
 files=[];
 await vi.waitFor(()=>expect(document.querySelector('.index-summary-state')?.textContent).toContain('索引已暂停'),{timeout:2000});
 expect([...document.querySelectorAll('.index-active .index-root-title>span')].map(node=>node.textContent)).toEqual(['索引已暂停','索引已暂停']);
 expect(document.querySelector('.index-current-files')).toBeNull();
 expect(document.querySelector('.index-count')?.textContent).toContain('12 / 100');
});

it('文件设置中的索引历史打开相同详情窗口，关闭后仍在文件设置中',async()=>{
 const settings={language:'zh-CN',theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048,answer:{url:'http://localhost',model:'test',models:['test'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512}};
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return settings;
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return [{id:1,path:'/notes',name:'Notes',started_at:1700000000000,finished_at:1700000001000,completed:10,total:10,status:'ready',error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('data-changed')).toBe(true));
 document.querySelector<HTMLButtonElement>('.sidebar-tools button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.settings-nav')).not.toBeNull());
 expect(document.querySelector('.settings-index-history')).toBeNull();
 [...document.querySelectorAll<HTMLButtonElement>('.settings-nav button')].find(button=>button.textContent==='文件设置')!.click();await tick();
 const entry=document.querySelector<HTMLButtonElement>('.settings-index-history')!;
 expect(entry.closest('.settings-group')?.previousElementSibling?.textContent).toBe('索引与显示');
 expect(document.querySelector('.settings-nav')?.textContent).not.toContain('索引历史');
 expect(document.querySelector('.index-history-settings')?.textContent).toContain('索引历史');
 entry.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('10 / 10'));
 expect(document.querySelector('.index-history')?.textContent).toContain('Notes');
 expect(document.querySelector('[aria-label="暂停索引"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="关闭索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')).toBeNull());
 expect(document.querySelector('.settings-nav')).not.toBeNull();
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('文件设置');
});

it('索引完成后在已打开的详情窗口更新历史结果',async()=>{
 let finished=false;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:finished?[]:[{path:'/notes',stage:'indexing',completed:0,total:1,files:['/notes/test.md']}]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:finished?'ready':'indexing',count:finished?1:0,error:null}];
  if(command==='get_index_history')return finished?[{id:1,path:'/notes',name:'Notes',started_at:1700000000000,finished_at:1700000001000,completed:1,total:1,status:'partial',error:'/notes/broken.md\n读取失败'}]:[];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="查看索引详情"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('暂无索引历史'));
 finished=true;
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('1 / 1'),{timeout:2000});
 expect(document.querySelector('.index-history')?.textContent).toContain('部分失败');
 expect(document.querySelector('.index-history')?.textContent).toContain('/notes/broken.md');
 expect(document.querySelector('.index-active')).toBeNull();
 expect(document.querySelectorAll('.index-root-title strong')).toHaveLength(1);
});


it('已完成的文件夹只显示在历史中，当前任务只保留索引与等待项',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[{path:'/active',stage:'scanning',completed:0,total:0,files:[]}]};
  if(command==='get_index_history')return [{id:1,path:'/done',name:'Completed',started_at:1700000000000,finished_at:1700000001000,completed:6,total:6,status:'ready',error:null}];
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[{path:'/done',name:'Completed',status:'ready',count:6,error:null},{path:'/active',name:'Active',status:'indexing',count:0,error:null},{path:'/pending',name:'Pending',status:'pending',count:0,error:null}],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('6 / 6'));
 expect([...document.querySelectorAll('.index-active .index-root-title strong')].map(node=>node.textContent)).toEqual(['Active','Pending']);
 expect([...document.querySelectorAll('.index-root-title strong')].filter(node=>node.textContent==='Completed')).toHaveLength(1);
 expect(document.querySelector('.index-active')?.textContent).toContain('等待处理');
 expect(document.querySelector('.index-history .index-root-title span')?.textContent).toBe('已完成');
});

it('索引历史按文件夹只突出最新运行，更早记录可展开且失败原因优先显示可读说明',async()=>{
 const records=[
  {id:3,path:'/notes',name:'Notes',started_at:1700000003000,finished_at:1700000004000,completed:3,total:4,status:'partial',error:'permission denied: /notes/secret.md'},
  {id:2,path:'/notes',name:'Notes',started_at:1700000002000,finished_at:1700000002500,completed:4,total:4,status:'ready',error:null},
  {id:1,path:'/other',name:'Other',started_at:1700000001000,finished_at:1700000001500,completed:2,total:2,status:'ready',error:null}
 ];
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return records;
  if(command==='get_index_history_log')return {raw:JSON.stringify({path:'/notes/secret.md',status:'failed',error:'permission denied'})};
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('Notes'));
 expect(document.querySelectorAll('.index-history-list>.index-root')).toHaveLength(2);
 expect(document.querySelector('.index-history')?.textContent).toContain('查看更早记录（1）');
 expect(document.querySelector('.index-history')?.textContent).toContain('无法读取文件：请检查文件或文件夹的访问权限。');
 const older=document.querySelector<HTMLDetailsElement>('.older-history')!;
 older.open=true;
 await tick();
 expect(document.querySelectorAll('.older-history-list .index-root')).toHaveLength(1);
 expect(older.textContent).toContain('已完成');
});

it('清空索引记录后仅移除历史，保留当前任务且再次打开仍为空',async()=>{
 let records=[{id:1,path:'/done',name:'Completed',started_at:1700000000000,finished_at:1700000001000,completed:6,total:6,status:'ready',error:null}];
 const roots=[{path:'/done',name:'Completed',status:'ready',count:6,error:null},{path:'/active',name:'Active',status:'indexing',count:0,error:null}];
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[{path:'/active',stage:'scanning',completed:0,total:0,files:[]}]};
  if(command==='get_index_history')return structuredClone(records);
  if(command==='clear_index_history'){records=[];return;}
  throw Error(command);
 });
 const props={roots,onClose:vi.fn(),onPauseChange:vi.fn()};
 app=mount(IndexDetails,{target:document.body,props});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('Completed'));
 document.querySelector<HTMLButtonElement>('[aria-label="清空索引记录"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('暂无索引历史'));
 expect(invoke).toHaveBeenCalledWith('clear_index_history',undefined);
 expect(document.querySelector('.index-active')?.textContent).toContain('Active');
 expect(document.querySelector('.index-history')?.textContent).not.toContain('Completed');
 expect(document.querySelector('[aria-label="清空索引记录"]')?.hasAttribute('disabled')).toBe(true);
 await unmount(app);app=mount(IndexDetails,{target:document.body,props});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('暂无索引历史'));
 expect(document.querySelector('.index-history .index-root')).toBeNull();
});

it('清空索引记录失败时保留历史并显示原因',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return [{id:1,path:'/done',name:'Completed',started_at:1700000000000,finished_at:1700000001000,completed:6,total:6,status:'ready',error:null}];
  if(command==='clear_index_history')throw Error('无法写入数据库');
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('Completed'));
 document.querySelector<HTMLButtonElement>('[aria-label="清空索引记录"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('无法写入数据库'));
 expect(document.querySelector('.index-history')?.textContent).toContain('Completed');
});

it('清空时不会被较早发出的历史查询重新填回旧记录',async()=>{
 let finished=false,cleared=false,historyReads=0;
 let release:((value:any)=>void)|undefined;
 const record={id:1,path:'/done',name:'Completed',started_at:1700000000000,finished_at:1700000001000,completed:6,total:6,status:'ready',error:null};
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:finished?[]:[{path:'/active',stage:'scanning',completed:0,total:0,files:[]}]};
  if(command==='get_index_history'){
   historyReads++;
   if(historyReads===2)return await new Promise(resolve=>release=resolve);
   return cleared?[]:[record];
  }
  if(command==='clear_index_history'){cleared=true;return;}
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('Completed'));
 finished=true;
 await vi.waitFor(()=>expect(release).toBeDefined(),{timeout:2000});
 document.querySelector<HTMLButtonElement>('[aria-label="清空索引记录"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('暂无索引历史'));
 release!([record]);
 await vi.waitFor(()=>expect(historyReads).toBeGreaterThan(2),{timeout:2000});
 expect(document.querySelector('.index-history')?.textContent).not.toContain('Completed');
});

it('历史展开先显示十个原文件，查看更多显示当次原始日志并按记录打开原文件',async()=>{
 const record={id:42,path:'/notes',name:'Notes',started_at:1700000000000,finished_at:1700000001000,completed:120,total:120,status:'ready',error:null};
 const rows=Array.from({length:100},(_,i)=>({time:1700000000500+i,path:`/notes/document-${i+1}.pdf`,stage:'converting',status:'completed',error:null}));
 const raw=rows.map(row=>JSON.stringify(row)).join('\n');
 invoke.mockImplementation(async(command)=>{
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return [record];
  if(command==='get_index_history_log')return {raw};
  if(command==='open_index_history_file')return;
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history-toggle')).not.toBeNull());
 expect(document.querySelector('.index-file-preview')).toBeNull();
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.index-history-file')).toHaveLength(10));
 expect(document.querySelector('.index-file-preview')?.textContent).toContain('document-10.pdf');
 expect(document.querySelector('.index-file-preview')?.textContent).not.toContain('document-11.pdf');
 document.querySelector<HTMLButtonElement>('.index-history-file')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_index_history_file',{id:42,path:'/notes/document-1.pdf'}));
 document.querySelector<HTMLButtonElement>('.index-view-log')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-log-dialog')).not.toBeNull());
 expect(document.querySelector('[aria-label="原始索引日志内容"]')?.textContent).toBe(raw+'\n');
 expect(document.querySelectorAll('.index-log-path')).toHaveLength(100);
 [...document.querySelectorAll<HTMLButtonElement>('.index-log-path')].at(-1)!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_index_history_file',{id:42,path:'/notes/document-100.pdf'}));
 document.querySelector<HTMLButtonElement>('[aria-label="关闭索引日志"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-log-dialog')).toBeNull());
 expect(document.querySelectorAll('.index-history-file')).toHaveLength(10);
 expect(document.querySelector('.index-history-toggle')?.getAttribute('aria-expanded')).toBe('true');
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await tick();
 expect(document.querySelector('.index-file-preview')).toBeNull();
});

it('旧历史提示没有文件明细，空任务与读取失败分别显示，失败后可重试',async()=>{
 let reads=0;
 invoke.mockImplementation(async(command)=>{
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return [{id:1,path:'/notes',name:'Notes',started_at:1700000000000,finished_at:1700000001000,completed:6,total:6,status:'ready',error:null}];
  if(command==='get_index_history_log'){
   reads++;if(reads===1)throw Error('无法读取历史文件记录');
   return {raw:null};
  }
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history-toggle')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('无法读取历史文件记录'));
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-file-preview')?.textContent).toContain('此记录未保存文件明细'));
 expect(document.querySelector('.index-view-log')).toBeNull();
 expect(document.querySelectorAll('.index-history-file')).toHaveLength(0);
});

it('三阶段显示当前步骤及说明，侧栏百分比明确属于当前阶段',async()=>{
 let stage='indexing_native';
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'indexing',count:1,error:null}];
  if(command==='get_index_status')return {paused:false,tasks:[{path:'/notes',stage,completed:1,total:4,files:[]}]};
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('index-progress')).toBe(true));
 listeners.get('index-progress')!({payload:{path:'/notes',stage,count:1,total:4}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在检查文档（1/4）'));
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('文本索引'));
 expect(document.querySelector('.index-stage-description')?.textContent).toContain('完成后即可搜索');
 stage='converting';listeners.get('index-progress')!({payload:{path:'/notes',stage,count:2,total:4}});
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('文件转换'),{timeout:2000});
 expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在解析文档（2/4）');
 expect(document.querySelector('.index-pipeline .done')?.textContent).toContain('文本索引');
 expect(document.querySelector('.index-stage-description')?.textContent).toContain('仍可搜索');
 stage='indexing_converted';listeners.get('index-progress')!({payload:{path:'/notes',stage,count:1,total:4}});
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('转换结果索引'),{timeout:2000});
 expect(document.querySelectorAll('.index-pipeline .done')).toHaveLength(2);
 expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在检查文档（1/4）');
});

it('行内只显示文件夹路径，失败文件在失败详情中单独显示完整文件名与路径，并可定位原文件',async()=>{
 const path=String.raw`\\?\D:\资料`,file=path+'\\'+('很长的子目录\\'.repeat(12))+'主表 生成式人工智能上线备案表.pdf';
 const record={id:43,path,name:'资料',started_at:1700000000000,finished_at:1700000001000,completed:1,total:1,status:'partial',error:'未能索引 1 个文件\n\n'+file+'\nDocling 未生成 Markdown'};
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return{paused:false,tasks:[]};
  if(command==='get_index_history')return[record];
  if(command==='get_index_history_log')return{raw:JSON.stringify({time:1700000000000,path:file,stage:'converting',status:'failed',error:'Docling 未生成 Markdown'})};
  if(command==='reveal_path')return;
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-history-toggle .index-root-path')?.textContent).toBe(path.slice(4)));
 expect(document.querySelector('.index-history-toggle .index-root-path')?.getAttribute('title')).toBe(path.slice(4));
 await vi.waitFor(()=>expect(document.querySelector('.index-entry-notice')?.textContent).toContain('1 个文件未进入索引'));
 expect(document.querySelector('.index-history-toggle')?.getAttribute('aria-expanded')).toBe('false');
 await vi.waitFor(()=>expect(document.querySelector('.index-failure')).not.toBeNull());
 const details=document.querySelector<HTMLDetailsElement>('.index-failure')!;details.open=true;details.dispatchEvent(new Event('toggle'));
 await vi.waitFor(()=>expect(document.querySelector('.index-failed-file strong')?.textContent).toBe('主表 生成式人工智能上线备案表.pdf'));
 expect(document.querySelector('.index-failed-file .index-file-path')?.textContent).toBe(file.slice(4));
 expect(document.querySelector('.index-failure pre')?.textContent).not.toContain('\\\\?\\');
 document.querySelector<HTMLButtonElement>('.index-failed-file')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('reveal_path',{path:file}));
});

it('文件编码异常与监听配额不足同时发生时，索引详情仍显示配额指引',async()=>{
 const error='未能索引 1 个文件\n\n/notes/old.md\n文件无法按 UTF-8 编码读取\n\n文件监听失败：已达到 Linux inotify 监听上限，自动更新受限。请调大 fs.inotify.max_user_watches（例如 sudo sysctl fs.inotify.max_user_watches=524288），然后重启 SAG 后台服务。';
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_index_history')return [];
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[{path:'/notes',name:'Notes',status:'partial',count:1,error}],onClose:vi.fn(),onPauseChange:vi.fn()}});
 const summary=()=>document.querySelector('.index-root .small.muted:nth-of-type(3)')?.textContent;
 await vi.waitFor(()=>expect(summary()).toContain('重启 SAG 后台服务'));
 expect(summary()).not.toContain('重建索引');
});

it('Windows 索引路径显示常规格式，打开历史文件仍使用原始路径',async()=>{
 const path=String.raw`\\?\D:\Documents\大模型算法备案`,file=path+String.raw`\报告.pdf`;
 const root={path,name:'大模型算法备案',status:'indexing',count:1,error:null};
 const record={id:42,path,name:root.name,started_at:1700000000000,finished_at:1700000001000,completed:1,total:1,status:'ready',error:null};
 invoke.mockImplementation(async command=>{
  if(command==='get_index_status')return {paused:false,tasks:[{path,stage:'converting',completed:1,total:2,files:[file]}]};
  if(command==='get_index_history')return [record];
  if(command==='get_index_history_log')return {raw:JSON.stringify({path:file,stage:'converting',status:'completed',error:null})};
  if(command==='open_index_history_file')return;
  throw Error(command);
 });
 app=mount(IndexDetails,{target:document.body,props:{roots:[root],onClose:vi.fn(),onPauseChange:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('.index-current-file-path')?.textContent).toBe(String.raw`D:\Documents\大模型算法备案\报告.pdf`));
 expect([...document.querySelectorAll('.index-root-path')].map(node=>[node.textContent,node.getAttribute('title')])).toEqual(Array(2).fill([String.raw`D:\Documents\大模型算法备案`,String.raw`D:\Documents\大模型算法备案`]));
 expect(document.querySelector('.index-issues-dialog')?.textContent).not.toContain('\\\\?\\');
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history-file')?.getAttribute('title')).toBe(String.raw`D:\Documents\大模型算法备案\报告.pdf`));
 document.querySelector<HTMLButtonElement>('.index-history-file')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_index_history_file',{id:42,path:file}));
});

it('监听准备阶段恢复已隐藏的状态并展示不确定进度',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[{path:'/notes',stage:'preparing_watch',completed:0,total:0,files:[]}]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'pending',count:0,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-link')).not.toBeNull());
 expect(document.querySelector('[aria-label="隐藏索引状态"]')).not.toBeNull();
 listeners.get('index-progress')!({payload:{path:'/notes',count:0,total:0,stage:'preparing_watch'}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在准备文档'));
 expect(document.querySelector('.index-status-bar')).toBeNull();
 const reads=invoke.mock.calls.filter(([command])=>command==='list_roots').length;
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='list_roots').length).toBeGreaterThan(reads));
 await new Promise(resolve=>setTimeout(resolve,50));
 await tick();
 expect(document.querySelector('.index-status-primary')?.textContent).toBe('正在准备文档');
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('正在准备文件监听'));
});

it('restores active backend work even when its root is no longer listed',async()=>{
 invoke.mockImplementation(async command=>{if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test'}};if(command==='get_index_status')return {paused:false,tasks:[{path:'/removed',stage:'converting',completed:18,total:20,files:['/removed/a.pdf']}]};if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:4,error:null}];return [];});
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在解析文档'));expect(document.querySelector('.index-status-primary')?.textContent).toContain('（18/20）');
});

it('recovers changed stages and completion when progress events are missed',async()=>{
 let task:any={path:'/notes',stage:'converting',completed:1,total:4,files:['/notes/a.pdf']};let rootState='indexing';
 invoke.mockImplementation(async command=>{if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test'}};if(command==='get_index_status')return {paused:false,tasks:task?[{...task}]:[]};if(command==='list_roots')return [{path:'/notes',name:'Notes',status:rootState,count:1,error:null}];return [];});
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在解析文档'));
 task={...task,stage:'indexing_converted',completed:2};await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文档'),{timeout:2500});
 task=null;rootState='ready';await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('文档解析完成'),{timeout:2500});
});

it.each(['converting','enhancing','retrying_enhancement'])('%s shows concurrent actual parsers, updates parser-only changes and clears them outside conversion',async(stage)=>{
 let task={path:'/notes',stage,completed:1,total:4,files:['/notes/a.pdf','/notes/b.docx'],conversions:[{path:'/notes/a.pdf',engine:'MinerU'},{path:'/notes/b.docx',engine:'anydoc'}]};
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[structuredClone(task)]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'indexing',count:1,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-parser')?.textContent).toBe('解析方式：MinerU'));
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-current-files')?.textContent).toContain('解析方式：MinerU'));
 task={...task,conversions:[{path:'/notes/a.pdf',engine:'Docling'},{path:'/notes/b.docx',engine:'anydoc'}]};
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-parser')?.textContent).toBe('解析方式：Docling'));
 await vi.waitFor(()=>expect(document.querySelector('.index-current-files')?.textContent).toContain('解析方式：Docling'));
 task={...task,stage:'indexing_converted',conversions:[]};
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-parser')).toBeNull());
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文档');
});

it('shows English preparation and retry parser labels while keeping stage and count separate',async()=>{
 let task={path:'/notes',stage:'converting',completed:0,total:4,files:['/notes/a.pdf'],conversions:[] as {path:string;engine:string}[]};
 let paused=false;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {language:'en',theme:'dark',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused,tasks:[structuredClone(task)]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'indexing',count:1,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('Processing documents'));
 expect(document.querySelector('.index-status-parser')).toBeNull();
 task={...task,stage:'retrying_conversion',conversions:[{path:'/notes/a.pdf',engine:'Docling'}]};
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-parser')?.textContent).toBe('Parser: Docling'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（0/4）');
 paused=true;
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toBe('Document processing paused（0/4）'));
 expect(document.querySelector('.index-status-parser')?.textContent).toBe('Parser: Docling');
 task={...task,files:[],conversions:[]};
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-parser')).toBeNull());
});


it('keeps the same card across PDF parts and stale ready root snapshots',async()=>{
 let task:any={path:'/notes',stage:'converting',completed:2,total:3,files:['/notes/long.pdf'],conversions:[{path:'/notes/long.pdf',engine:'MinerU',segment:{current:1,total:20,start_page:1,end_page:50}}]};
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {language:'en',theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:task?[structuredClone(task)]:[]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:2,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-segment')?.textContent).toContain('part 1/20'));
 const card=document.querySelector('.index-status-card');
 task.conversions=[{path:'/notes/long.pdf',engine:'',segment:{current:8,total:20,start_page:351,end_page:400}}];
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-segment')?.textContent).toContain('part 8/20'));
 expect(document.querySelector('.index-status-card')).toBe(card);
 expect(document.querySelector('.index-status-segment')?.textContent).toContain('Pages 351–400');
 expect(document.querySelector('.index-status-parser')).toBeNull();
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（2/3）');
 document.querySelector<HTMLButtonElement>('[aria-label="View index details"]')?.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-file-segment')?.textContent).toContain('part 8 / 20'));
 document.querySelector<HTMLButtonElement>('[aria-label="Close index details"]')?.click();
 expect(document.querySelector('[aria-label="Hide indexing status"]')).not.toBeNull();
 task.conversions[0].segment.current=9;listeners.get('data-changed')!({payload:null});
 await new Promise(resolve=>setTimeout(resolve,50));expect(document.querySelector('.index-status-card')).toBe(card);
});

it('ignores a ready snapshot that predates a newer index progress event',async()=>{
 let pending=false,release:(()=>void)|undefined;
 const gate=new Promise<void>(resolve=>release=resolve);
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test'}};
  if(command==='get_index_status'){if(pending)await gate;return {paused:false,tasks:[]};}
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'ready',count:1,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('data-changed')).toBe(true));
 await new Promise(resolve=>setTimeout(resolve,50));
 pending=true;listeners.get('data-changed')!({payload:null});
 await tick();
 listeners.get('index-progress')!({payload:{path:'/notes',count:1,total:3,stage:'converting'}});
 await tick();const card=document.querySelector('.index-status-card');expect(card).not.toBeNull();
 // Keep a genuinely fresh task active; only the in-flight old response is empty.
 invoke.mockImplementation(async command=>command==='get_index_status'?{paused:false,tasks:[{path:'/notes',stage:'converting',completed:1,total:3,files:['/notes/a.pdf']}]}:command==='list_roots'?[{path:'/notes',name:'Notes',status:'indexing',count:1,error:null}]:[]);
 release!();await new Promise(resolve=>setTimeout(resolve,100));await tick();
 expect(document.querySelector('.index-status-card')).toBe(card);
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（1/3）');
});


it('does not mix another root PDF parts with the displayed root file progress',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test'}};
  if(command==='get_index_status')return {paused:false,tasks:[
   {path:'/text',stage:'indexing_native',completed:1,total:5,files:['/text/a.md']},
   {path:'/pdf',stage:'converting',completed:2,total:3,files:['/pdf/long.pdf'],conversions:[{path:'/pdf/long.pdf',engine:'MinerU',segment:{current:8,total:20,start_page:351,end_page:400}}]}
  ]};
  if(command==='list_roots')return [{path:'/text',name:'Text',status:'indexing',count:1,error:null},{path:'/pdf',name:'PDF',status:'indexing',count:2,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文档'));
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（1/5）');
 expect(document.querySelector('.index-status-segment')).toBeNull();
});

it('失败卡片直接重建失败文件，提交期间阻止重复点击并保留详情入口',async()=>{
 let resolveRetry:(value:unknown)=>void=()=>{};
 const pending=new Promise(resolve=>resolveRetry=resolve);
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[],failed_files:2};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:8,error:'未能索引 2 个文件'}];
  if(command==='retry_failed_indexes')return pending;
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('.index-retry-button')).not.toBeNull());
 const button=document.querySelector<HTMLButtonElement>('.index-retry-button')!;
 expect(button.textContent).toContain('重试失败文件');
 expect(document.querySelector('.index-status-primary')?.textContent).toContain('（2）');
 button.click();button.click();
 await vi.waitFor(()=>expect(button.disabled).toBe(true));
 expect(button.textContent).toContain('正在重试');
 expect(invoke.mock.calls.filter(([command])=>command==='retry_failed_indexes')).toHaveLength(1);
 expect(invoke.mock.calls.some(([command])=>command==='reindex_all')).toBe(false);
 expect(document.querySelector('[aria-label="查看索引详情"]')).not.toBeNull();
 resolveRetry([{path:'/notes',name:'Notes',status:'pending',count:8,error:null}]);
 await tick();
});

it('仅有目录监听失败时不提供文件重建入口',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[],failed_files:0};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:8,error:'文件监听失败，自动更新受限'}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('部分文件处理失败'));
 expect(document.querySelector('.index-retry-button')).toBeNull();
});

it('失败文件重建提交错误留在卡片中并允许再次尝试',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[],failed_files:1};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:8,error:'previous failure'}];
  if(command==='retry_failed_indexes')throw new Error('服务暂时不可用');
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-retry-button')).not.toBeNull());
 const button=document.querySelector<HTMLButtonElement>('.index-retry-button')!;
 button.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('服务暂时不可用'));
 expect(button.disabled).toBe(false);
 button.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='retry_failed_indexes')).toHaveLength(2));
});

it('暂停时保留失败重建入口但禁止启动',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:true,tasks:[],failed_files:1};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:8,error:'previous failure'}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('.index-retry-button')?.disabled).toBe(true));
 document.querySelector<HTMLButtonElement>('.index-retry-button')!.click();
 expect(invoke.mock.calls.some(([command])=>command==='retry_failed_indexes')).toBe(false);
});

it.each([true,false])('keeps same-root primary and enhancement jobs with independent counts and parsers (ids: %s)',async(withIds)=>{
 const tasks=[{id:withIds?'primary:/notes':undefined,kind:'primary',path:'/notes',stage:'indexing_native',completed:2,total:5,files:['/notes/new.md']},{id:withIds?'enhancement:/notes':undefined,kind:'enhancement',path:'/notes',stage:'enhancing',completed:0,total:1,files:['/notes/scan.pdf'],conversions:[{path:'/notes/scan.pdf',engine:'Docling'}]}];
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:structuredClone(tasks)};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'indexing',count:2,error:null}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status-primary')?.textContent).toContain('正在检查文档'));
 expect(document.querySelector('.index-status-parser')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.index-active .index-root')).toHaveLength(2));
 expect(document.querySelector('.index-active')?.textContent).toContain('已处理 2 / 5 个文件');
 expect(document.querySelector('.index-active')?.textContent).toContain('已处理 0 / 1 个文件');
 expect(document.querySelector('.index-active')?.textContent).toContain('解析方式：Docling');
 tasks[1].completed=1;listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-active')?.textContent).toContain('已处理 1 / 1 个文件'));
 expect(document.querySelector('.index-active')?.textContent).toContain('已处理 2 / 5 个文件');
});
