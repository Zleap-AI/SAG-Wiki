// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import IndexDetails from '../src/components/IndexDetails.svelte';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:any})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,handler)=>{listeners.set(name,handler);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();listeners.clear()});
it('失败详情单独显示完整文件名与路径，并可定位原文件',async()=>{
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
 await vi.waitFor(()=>expect(document.querySelector('.index-history-toggle .index-root-path')?.textContent).toBe(file.slice(4)));
 expect(document.querySelector('.index-history-toggle .index-root-path')?.getAttribute('title')).toBe(file.slice(4));
 expect(document.querySelector('.index-history-toggle')?.getAttribute('aria-expanded')).toBe('false');
 await vi.waitFor(()=>expect(document.querySelector('.index-failure')).not.toBeNull());
 const details=document.querySelector<HTMLDetailsElement>('.index-failure')!;details.open=true;details.dispatchEvent(new Event('toggle'));
 await vi.waitFor(()=>expect(document.querySelector('.index-failed-file strong')?.textContent).toBe('主表 生成式人工智能上线备案表.pdf'));
 expect(document.querySelector('.index-failed-file .index-file-path')?.textContent).toBe(file.slice(4));
 expect(document.querySelector('.index-failure pre')?.textContent).not.toContain('\\\\?\\');
 document.querySelector<HTMLButtonElement>('.index-failed-file')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('reveal_path',{path:file}));
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
 await vi.waitFor(()=>expect(document.querySelector('.index-current-files span')?.textContent).toBe(String.raw`D:\Documents\大模型算法备案\报告.pdf`));
 expect([...document.querySelectorAll('.index-root-path')].map(node=>[node.textContent,node.getAttribute('title')])).toEqual(Array(2).fill([String.raw`D:\Documents\大模型算法备案`,String.raw`D:\Documents\大模型算法备案`]));
 expect(document.querySelector('.index-issues-dialog')?.textContent).not.toContain('\\\\?\\');
 document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history-file')?.getAttribute('title')).toBe(String.raw`D:\Documents\大模型算法备案\报告.pdf`));
 document.querySelector<HTMLButtonElement>('.index-history-file')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_index_history_file',{id:42,path:file}));
});
it('部分文件索引失败时可查看具体路径，同时显示已索引文件数量',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='list_roots')return [{path:'/notes',name:'Notes',status:'partial',count:270,error:'未能索引 1 个文件\n\n/notes/broken.txt\n文件无法按 UTF-16LE 编码读取'}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toContain('SAG · 部分失败'));
 expect(document.querySelector('.index-status')?.textContent).not.toContain('读取失败');
 expect(document.querySelector('.index-status')?.textContent).not.toContain('Notes');
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('/notes/broken.txt'));
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('UTF-16LE');
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('已索引 270 个文件');
 expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('其他文件可正常检索');
});

it('索引百分比与部分失败在同一行，完成后隐藏进度',async()=>{
 let ready=false;
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='list_roots')return [
   {path:'/notes',name:'Notes',status:ready?'ready':'partial',count:3,error:ready?null:'无法读取原文件'},
   {path:'/other',name:'Other',status:ready?'ready':'indexing',count:4,error:null}
  ];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(listeners.has('data-changed')).toBe(true));
 listeners.get('index-progress')!({payload:{path:'/other',count:27,total:100}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引（27% 部分失败）'));
 expect(document.querySelector('.index-status')?.children).toHaveLength(1);
 expect(document.querySelector('.index-status-row')?.children).toHaveLength(2);
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')?.textContent).toContain('无法读取原文件'));
 document.querySelector<HTMLButtonElement>('[aria-label="关闭索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-issues-dialog')).toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="隐藏索引状态"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe(''));
 listeners.get('index-progress')!({payload:{path:'/other',count:30,total:100}});
 await new Promise(resolve=>setTimeout(resolve,0));
 expect(document.querySelector('.index-status')?.textContent).toBe('');
 listeners.get('index-progress')!({payload:{path:'/other',count:0,total:50}});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引（0% 部分失败）'));
 ready=true;
 listeners.get('data-changed')!({payload:null});
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe(''));
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
 document.querySelector<HTMLButtonElement>('[aria-label="暂停索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:true}));
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toContain('索引已暂停'));
 expect(document.querySelector('.index-active .index-root-title span')?.textContent).toBe('索引已暂停');
 expect(document.querySelector('.index-active')?.textContent).not.toContain('正在索引文件');
 document.querySelector<HTMLButtonElement>('[aria-label="继续索引"]')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('set_index_paused',{paused:false}));
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).not.toContain('索引已暂停'));
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

it('文件设置最下方的索引历史打开相同详情窗口，关闭后仍在文件设置中',async()=>{
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
 expect(document.querySelector('.settings-content')!.lastElementChild).toBe(document.querySelector('.index-history-settings'));
 expect(document.querySelector('.settings-nav')?.textContent).not.toContain('索引历史');
 expect(document.querySelector('.index-history-settings')?.textContent).toContain('索引历史');
 entry.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-history')?.textContent).toContain('10 / 10'));
 expect(document.querySelector('.index-history')?.textContent).toContain('Notes');
 expect(document.querySelector('[aria-label="暂停索引"]')?.hasAttribute('disabled')).toBe(true);
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
 await vi.waitFor(()=>expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引文本（25%）'));
 document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('文本索引'));
 expect(document.querySelector('.index-stage-description')?.textContent).toContain('完成后即可搜索');
 stage='converting';listeners.get('index-progress')!({payload:{path:'/notes',stage,count:2,total:4}});
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('文件转换'),{timeout:2000});
 expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在转换文件（50%）');
 expect(document.querySelector('.index-pipeline .done')?.textContent).toContain('文本索引');
 expect(document.querySelector('.index-stage-description')?.textContent).toContain('仍可搜索');
 stage='indexing_converted';listeners.get('index-progress')!({payload:{path:'/notes',stage,count:1,total:4}});
 await vi.waitFor(()=>expect(document.querySelector('.index-pipeline [aria-current="step"]')?.textContent).toContain('转换结果索引'),{timeout:2000});
 expect(document.querySelectorAll('.index-pipeline .done')).toHaveLength(2);
 expect(document.querySelector('.index-status')?.textContent).toBe('SAG 正在索引转换结果（25%）');
});
