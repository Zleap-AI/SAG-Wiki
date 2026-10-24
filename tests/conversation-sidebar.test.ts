// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import ConversationSidebar from '../src/components/ConversationSidebar.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {Conversation} from '../src/lib/types';

const conversations:Conversation[]=[
 {id:'current',title:'第一篇会话',updated:new Date(2026,9,24,8,30).getTime(),deleted:false,messages:[{question:'如何导入文件？',answer:'选择知识库文件夹。',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'导入'}]},
 {id:'another',title:'架构讨论',updated:new Date(2026,9,23,9).getTime(),deleted:false,messages:[{question:'缓存如何处理？',answer:'Use a persistent cache for documents.',evidence:[],model:'test',scope:[],references:[],status:'complete',error:null,retrieval_question:'缓存'}]},
 {id:'empty',title:'没有消息的会话',updated:new Date(2026,9,22,9).getTime(),deleted:false,messages:[]},
];
const onSelect=vi.fn(),onMenu=vi.fn(),onStop=vi.fn();
let app:ReturnType<typeof mount>;
beforeEach(()=>setLanguage('zh-CN'));
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function start(props:Partial<{conversations:Conversation[];activeId:string;generating:boolean}>={}){app=mount(ConversationSidebar,{target:document.body,props:{conversations,activeId:'current',onSelect,onMenu,onStop,...props}});await tick()}
function input(){return document.querySelector<HTMLInputElement>('[aria-label="搜索会话"]')!}
function key(target:Element,key:string,options:KeyboardEventInit={}){target.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...options}))}
async function search(value:string){input().value=value;input().dispatchEvent(new Event('input',{bubbles:true}));await tick()}
function item(title:string){return document.querySelector<HTMLButtonElement>(`.conversation-sidebar-item[title="${title}"]`)!}

it('按日期分组展示会话、时间和当前会话',async()=>{
 await start();
 expect(document.querySelector('section[aria-label="会话列表"]')).not.toBeNull();
 expect(document.querySelector('[aria-modal]')).toBeNull();
 expect(document.querySelectorAll('.conversation-sidebar-item')).toHaveLength(3);
 expect(item('第一篇会话').getAttribute('aria-current')).toBe('page');
 expect(item('架构讨论').hasAttribute('aria-current')).toBe(false);
 expect(item('架构讨论').querySelector('time')?.getAttribute('datetime')).toBe(new Date(conversations[1].updated).toISOString());
 item('架构讨论').click();expect(onSelect).toHaveBeenCalledWith(conversations[1]);
});

it('搜索会话标题、问题和回答，支持多个关键词与大小写无关匹配',async()=>{
 await start();
 for(const query of ['架构','缓存','PERSISTENT documents']){
  await search(query);
  expect(document.querySelectorAll('.conversation-sidebar-item')).toHaveLength(1);
  expect(item('架构讨论')).not.toBeNull();
  expect(document.querySelector('.conversation-sidebar-snippet')?.textContent).toContain('persistent cache');
 }
 await search('导入');expect(item('第一篇会话')).not.toBeNull();
 await search('');expect(document.querySelectorAll('.conversation-sidebar-item')).toHaveLength(3);
});

it('区分没有历史记录与没有匹配结果的状态',async()=>{
 await start({conversations:[]});
 expect(document.querySelector('.conversation-sidebar-empty')?.textContent).toContain('还没有问答记录');
 expect(document.querySelector('.conversation-sidebar-empty')?.textContent).toContain('发送第一个问题后，会话会自动保存在这里。');
 await unmount(app);await start();await search('不存在的词');
 expect(document.querySelector('.conversation-sidebar-empty')?.textContent).toContain('没有找到相关会话');
 expect(document.querySelector('.conversation-sidebar-empty')?.textContent).not.toContain('发送第一个问题');
});

it('每行菜单使用独立按钮并传回触发事件，不会打开会话',async()=>{
 await start();
 const trigger=document.querySelector<HTMLButtonElement>('[aria-label="架构讨论 的选项"]')!;
 onMenu.mockImplementationOnce(event=>expect(event.currentTarget).toBe(trigger));
 trigger.click();
 expect(onMenu).toHaveBeenCalledWith(expect.any(MouseEvent),conversations[1]);
 expect(onSelect).not.toHaveBeenCalled();
});

it('搜索框和列表支持方向键真实焦点导航，Enter 打开搜索结果',async()=>{
 await start();input().focus();key(input(),'ArrowDown');await tick();
 expect(document.activeElement).toBe(item('第一篇会话'));
 key(document.activeElement!,'ArrowDown');await tick();expect(document.activeElement).toBe(item('架构讨论'));
 key(document.activeElement!,'ArrowUp');await tick();expect(document.activeElement).toBe(item('第一篇会话'));
 input().focus();key(input(),'ArrowUp');await tick();expect(document.activeElement).toBe(item('没有消息的会话'));
 await search('cache');input().focus();key(input(),'Enter');await tick();expect(onSelect).toHaveBeenCalledWith(conversations[1]);
});

it('中文输入法确认不会误打开会话',async()=>{
 await start();key(input(),'Enter',{isComposing:true});await tick();expect(onSelect).not.toHaveBeenCalled();
});

it('生成时仍可搜索，禁止切换其他会话及当前会话菜单，并可显式停止',async()=>{
 await start({generating:true});
 expect(document.querySelector('[role="status"]')?.textContent).toContain('正在生成回答');
 expect(document.querySelector('[role="status"]')?.textContent).toContain('停止后可切换会话');
 expect(item('架构讨论').disabled).toBe(true);
 expect(item('第一篇会话').disabled).toBe(false);
 expect(document.querySelector<HTMLButtonElement>('[aria-label="第一篇会话 的选项"]')?.disabled).toBe(true);
 expect(document.querySelector<HTMLButtonElement>('[aria-label="架构讨论 的选项"]')?.disabled).toBe(false);
 item('架构讨论').click();
 expect(onSelect).not.toHaveBeenCalled();
 item('第一篇会话').click();expect(onSelect).toHaveBeenCalledWith(conversations[0]);
 onSelect.mockClear();await search('cache');key(input(),'Enter');await tick();expect(onSelect).not.toHaveBeenCalled();
 expect(input().disabled).toBe(false);
 document.querySelector<HTMLButtonElement>('.conversation-sidebar-stop')!.click();expect(onStop).toHaveBeenCalledOnce();
});

it('生成中方向键跳过不可切换的会话，未保存的新会话不允许选择历史',async()=>{
 await start({generating:true});key(input(),'ArrowDown');await tick();expect(document.activeElement).toBe(item('第一篇会话'));
 key(document.activeElement!,'ArrowDown');await tick();expect(document.activeElement).toBe(item('第一篇会话'));
 await unmount(app);await start({generating:true,activeId:'not-yet-saved'});input().focus();key(input(),'ArrowDown');key(input(),'Enter');await tick();
 expect(document.activeElement).toBe(input());expect(onSelect).not.toHaveBeenCalled();
});
