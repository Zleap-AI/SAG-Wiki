// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
const today=new Date(2026,9,19,9,5).getTime();
const yesterday=new Date(2026,9,18,23,59).getTime();
const message={question:'问题',answer:'回答',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'问题'};
const conversations=[
 {id:'today',title:'今天的会话',updated:today,deleted:false,messages:[{...message,created_at:today,context:{input_tokens:100,output_tokens:20,estimated_input:100,elapsed_ms:1500}},{...message,created_at:yesterday},{...message}]},
 {id:'yesterday',title:'昨天的会话',updated:yesterday,deleted:false,messages:[]},
 {id:'last-year',title:'去年的会话',updated:new Date(2025,11,31,15,30).getTime(),deleted:false,messages:[]},
];
beforeEach(async()=>{
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(new Date(2026,9,19,10,0));
 localStorage.setItem('sag.onboarding.seen','1');localStorage.setItem('sag.sidebar.tab','folders');
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return conversations;
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelectorAll('.history-row')).toHaveLength(3));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.useRealTimers();vi.clearAllMocks()});

it('会话列表今天显示时分，更早显示完整日期，搜索列表保持一致',async()=>{
 expect([...document.querySelectorAll('.history-row time')].map(node=>node.textContent)).toEqual(['09:05','2026-10-18','2025-12-31']);
 document.querySelector<HTMLButtonElement>('button[aria-label="搜索会话"]')!.click();await tick();
 expect([...document.querySelectorAll('.history-search-result time')].map(node=>node.textContent)).toEqual(['09:05','2026-10-18','2025-12-31']);
});

it('回答底部显示每条回答自己的时间，旧记录缺少时间时不补造',async()=>{
 document.querySelector<HTMLButtonElement>('[title="今天的会话"]')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.answer')).toHaveLength(3));
 expect([...document.querySelectorAll('.answer time')].map(node=>node.textContent)).toEqual(['09:05','2026-10-18']);
 expect(document.querySelector('.answer time')?.getAttribute('datetime')).toBe(new Date(today).toISOString());
 expect(document.querySelector('.answer-context')?.textContent).toContain('100 输入 tokens');
 expect(document.querySelector('.answer-context')?.textContent).toContain('20 输出 tokens');
 expect(document.querySelectorAll('.answer')[2].querySelector('time')).toBeNull();
});
