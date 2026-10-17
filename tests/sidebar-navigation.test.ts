// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const conversations=Array.from({length:50},(_,i)=>({id:`chat-${i}`,title:`会话 ${i}`,updated:i,deleted:false,messages:[{question:`问题 ${i}`,answer:i===41?'Deep connection tracing':'回答 '+i,model:'test',evidence:[],scope:[],references:[],status:'complete',error:null,retrieval_question:`问题 ${i}`}]}));
const keys=['sag.history.scroll','sag.sidebar.library.scroll','sag.sidebar.expanded'];
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 keys.forEach(key=>localStorage.removeItem(key));localStorage.setItem('sag.onboarding.seen','1');
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return conversations;
  if(command==='list_roots')return[{path:'/notes',name:'Knowledge',status:'ready',count:80,error:null}];
  if(command==='list_entries')return Array.from({length:80},(_,i)=>({path:`/notes/file-${i}.md`,name:`file-${i}.md`,directory:false,symlink:false}));
  return[];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';keys.forEach(key=>localStorage.removeItem(key));vi.restoreAllMocks();vi.clearAllMocks()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelectorAll('.history-row')).toHaveLength(50));await tick()}
it('搜索历史标题和内容，键盘打开结果，并保留原列表位置',async()=>{
 await start();
 const list=document.querySelector<HTMLElement>('.ask-content')!;list.scrollTop=520;list.dispatchEvent(new Event('scroll'));
 document.querySelector<HTMLButtonElement>('[aria-label="最近问答"]')!.click();await tick();
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索历史会话"]')!;
 input.value='TRACING';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 const result=document.querySelector<HTMLButtonElement>('.history-search-result')!;
 expect(result.tabIndex).toBe(-1);expect(result.textContent).toContain('会话 41');expect(document.querySelectorAll('.history-search-result')).toHaveLength(1);
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.history-search-dialog')).toBeNull());
 expect(document.querySelector('.answer-text')?.textContent).toBe('Deep connection tracing');
 document.querySelector<HTMLButtonElement>('[aria-label="新问答"]')!.click();await tick();
 expect(document.querySelector<HTMLElement>('.ask-content')!.scrollTop).toBe(520);
 document.querySelector<HTMLButtonElement>('[aria-label="最近问答"]')!.click();await tick();
 const titleInput=document.querySelector<HTMLInputElement>('[aria-label="搜索历史会话"]')!;
 titleInput.value='会话 23';titleInput.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelectorAll('.history-search-result')).toHaveLength(1);
 expect(document.querySelector('.history-search-result')!.textContent).toContain('会话 23');
 titleInput.value='完全不存在';titleInput.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(document.querySelector('.history-search-empty')?.textContent).toContain('没有找到相关会话');
});
it('最近问答放在右侧问答首页，左侧只保留知识库',async()=>{
 await start();
 expect(document.querySelector('.sidebar .recent-conversations')).toBeNull();
 expect(document.querySelectorAll('.workspace .recent-conversations .history-row')).toHaveLength(50);
 expect(document.querySelector('[aria-label="调整会话与知识库区域大小"]')).toBeNull();
 document.querySelector<HTMLButtonElement>('.workspace .conversation-item')!.click();await tick();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text')?.textContent).toBe('回答 0'));
 expect(document.querySelector('.workspace .recent-conversations')).toBeNull();
 expect(document.querySelector('.workspace [aria-label="最近问答"]')?.textContent).toContain('最近问答');
});
it('会话和知识库独立保存滚动位置，收起侧栏与重开应用后恢复',async()=>{
 await start();
 document.querySelector<HTMLButtonElement>('[aria-label="展开Knowledge"]')!.click();await tick();await tick();
 const list=document.querySelector<HTMLElement>('.ask-content')!;
 const tree=document.querySelector<HTMLElement>('.tree')!;
 list.scrollTop=430;list.dispatchEvent(new Event('scroll'));
 tree.scrollTop=170;tree.dispatchEvent(new Event('scroll'));
 document.querySelector<HTMLButtonElement>('[aria-label="收起侧边栏"]')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="展开侧边栏"]')!.click();await tick();
 expect(list.scrollTop).toBe(430);expect(tree.scrollTop).toBe(170);
 await unmount(app);await start();
 expect(document.querySelector<HTMLElement>('.ask-content')!.scrollTop).toBe(430);
 expect(document.querySelector<HTMLElement>('.tree')!.scrollTop).toBe(170);
 expect(document.querySelector('[aria-label="折叠Knowledge"]')).not.toBeNull();
});
