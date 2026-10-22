// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import LibraryStats from '../src/components/LibraryStats.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {FileInfo,IndexStatus,Root} from '../src/lib/types';
const {api}=vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api}));
let app:ReturnType<typeof mount>|undefined;
let status:IndexStatus;
const roots:Root[]=[{path:'/notes',name:'Notes',status:'indexing',count:2,error:null}];
beforeEach(()=>{
 setLanguage('zh-CN');
 status={paused:false,tasks:[{path:'/notes',stage:'indexing',completed:2,total:4,files:['/notes/a.md']}]};
 api.mockImplementation(async(command,args)=>{
  if(command==='set_index_paused')status={...status,paused:args.paused,tasks:status.tasks.map(task=>({...task,files:[]}))};
  if(command==='get_index_status'||command==='set_index_paused')return status;
  if(command==='get_index_history')return[{id:1,path:'/notes',name:'Notes',started_at:Date.now()-11000,finished_at:Date.now()-10000,completed:2,total:2,status:'ready',error:null}];
  throw new Error(`Unexpected command: ${command}`);
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();vi.useRealTimers();setLanguage('zh-CN');});
const files:FileInfo[]=[
 {id:1,path:'/notes/a.md',name:'a.md'},
 {id:2,path:'/notes/.draft.markdown',name:'.draft.markdown'},
 {id:3,path:'/notes/page.HTML',name:'page.HTML'},
 {id:4,path:'/notes/.sag-converted/report.pdf.md',name:'report.pdf.md'},
 {id:5,path:'/notes/.sag-converted/deck.pptx.md',name:'deck.pptx.md'},
];
it('统计真实文件类型，转换文档按原格式计数，条状图覆盖全部文件',async()=>{
 app=mount(LibraryStats,{target:document.body,props:{files,roots}});await tick();
 expect(document.querySelector('.file-stats-heading strong')?.textContent).toBe('5');
 expect(document.querySelector('.file-type-legend')?.textContent).toContain('MD2');
 expect(document.querySelector('.file-type-legend')?.textContent).toContain('PDF1');
 expect(document.querySelector('.file-type-legend')?.textContent).toContain('PPTX1');
 const bars=[...document.querySelectorAll<HTMLElement>('.file-type-bar>span')];
 expect(bars).toHaveLength(4);
 expect(bars.reduce((sum,bar)=>sum+Number.parseFloat(bar.style.width),0)).toBeCloseTo(100);
});
it('在概览暂停和恢复索引，更新状态并打开索引记录',async()=>{
 const onDetails=vi.fn(),onPauseChange=vi.fn();
 app=mount(LibraryStats,{target:document.body,props:{files,roots,onDetails,onPauseChange}});
 await vi.waitFor(()=>expect(document.querySelector('.index-state')?.textContent).toBe('正在索引'));
 expect(document.querySelector('.index-updated')?.textContent).toContain('2 / 4');
 document.querySelector<HTMLButtonElement>('.library-index-pause')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('set_index_paused',{paused:true}));
 await vi.waitFor(()=>expect(document.querySelector('.index-state')?.textContent).toBe('索引已暂停'));
 expect(onPauseChange).toHaveBeenCalledWith(true);
 document.querySelector<HTMLButtonElement>('.library-index-pause')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('set_index_paused',{paused:false}));
 document.querySelector<HTMLButtonElement>('.library-index-details')!.click();
 expect(onDetails).toHaveBeenCalledOnce();
});
it('完成时显示记录中的更新时间，空库和暂停状态可分别识别',async()=>{
 status={paused:false,tasks:[]};
 app=mount(LibraryStats,{target:document.body,props:{files,roots:[{...roots[0],status:'ready'}]}});
 await vi.waitFor(()=>expect(document.querySelector('.index-updated time')).not.toBeNull());
 expect(document.querySelector('.index-state')?.textContent).toBe('索引已完成');
 expect(document.querySelector('.library-index-pause')).toBeNull();
 expect(document.querySelector('.index-history-hint')).toBeNull();
 await unmount(app);
 status={paused:false,tasks:[]};api.mockImplementation(async command=>command==='get_index_status'?status:[]);
 app=mount(LibraryStats,{target:document.body,props:{files:[],roots:[]}});
 await vi.waitFor(()=>expect(document.querySelector('.index-state')?.textContent).toBe('等待添加文件夹'));
 expect(document.querySelector('.index-updated')).toBeNull();
 expect(document.querySelectorAll('.file-type-bar>span')).toHaveLength(0);
});
it('接口失败显示错误，不将失败当作已完成',async()=>{
 api.mockRejectedValue(new Error('连接已断开'));
 app=mount(LibraryStats,{target:document.body,props:{files:[],roots:[]}});
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toBe('暂时无法更新，点击查看详情'));
 expect(document.querySelector('.library-index-details')?.getAttribute('title')).toContain('连接已断开');
 expect(document.body.textContent).not.toContain('连接已断开');
 expect(document.querySelector('.index-state')?.textContent).toBe('无法读取索引状态');
});
it('仅记录读取失败时保留真实索引状态，技术信息不占用卡片正文',async()=>{
 status={paused:false,tasks:[]};
 api.mockImplementation(async command=>{
  if(command==='get_index_status')return status;
  throw new Error('Command get_index_history not found');
 });
 app=mount(LibraryStats,{target:document.body,props:{files,roots:[{...roots[0],status:'ready'}]}});
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());
 expect(document.querySelector('.index-state')?.textContent).toBe('索引已完成');
 expect(document.body.textContent).not.toContain('Command get_index_history');
 expect(document.querySelector('.library-index-details')?.getAttribute('title')).toContain('Command get_index_history');
});
