// @vitest-environment happy-dom
import{afterEach,beforeEach,describe,it,expect,vi}from 'vitest';
import{mount,unmount,tick}from 'svelte';import App from '../src/App.svelte';
import type{Conversation,Candidate}from '../src/lib/types';
import{setLanguage,translate}from '../src/lib/i18n';
const {invoke,listeners}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>,resolveQuery:(value:unknown)=>void,rejectQuery:(reason:unknown)=>void;
const previous={question:'上次问题',answer:'上次回答',evidence:[],model:'test',scope:[],references:[],status:'done',error:null,retrieval_question:'上次问题'};
const conversation:Conversation={id:'existing',title:'已有对话',messages:[previous],updated:0,deleted:false};
let savedConversation=conversation;
beforeEach(async()=>{
 savedConversation=conversation;
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{url:'https://api.302.ai/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://api.302ai.cn/typesafeai/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0};
  if(command==='list_conversations')return [savedConversation];
  if(command==='run_query')return new Promise((resolve,reject)=>{resolveQuery=resolve;rejectQuery=reject});
  return [];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
 document.querySelector<HTMLButtonElement>('[title="已有对话"]')!.click();await tick();
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;input.value='这次问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.useRealTimers();vi.clearAllMocks()});
const progress=async(stage:string,delta:string|null=null,id?:string,replace=false)=>{
 const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
 listeners.get('query-progress')!({payload:{id:id??query.id,stage,delta,count:0,replace}});await tick();
};
describe('回答中的处理进度',()=>{
 it('缺少问答密钥时直接打开模型设置，关闭后保留当前会话',async()=>{
  const error='请在设置中填写问答模型 API 密钥';
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',status:'failed',answer:'',error}]};
  rejectQuery(error);
  await vi.waitFor(()=>expect(document.querySelector('.settings-error-link')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('.settings-error-link')!.click();await tick();
  expect(document.querySelector('.settings-nav [aria-current="page"]')?.textContent).toBe('模型设置');
  expect(document.querySelector('.model-settings-card input[type=password]')).not.toBeNull();
  document.querySelector<HTMLButtonElement>('[aria-label="关闭设置"]')!.click();await tick();
  expect(document.querySelector('.settings-dialog')).toBeNull();
  expect(document.querySelector('.chat-log')?.textContent).toContain('这次问题');
 });
 it('问答失败时只在回答内显示一次错误',async()=>{
  const error='模型未执行本题所需的工具调用，请重试或更换支持工具调用的模型';
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',status:'failed',answer:'',error}]};
  rejectQuery(error);
  await vi.waitFor(()=>expect(document.querySelector('.answer .form-error')?.textContent).toBe(error));
  expect([...document.querySelectorAll('.form-error')].filter(node=>node.textContent===error)).toHaveLength(1);
  expect(document.querySelector('.content > .form-error')).toBeNull();
  expect(document.querySelector('[aria-label="重新生成回答"]')).not.toBeNull();
  expect(document.querySelector('.settings-error-link')).toBeNull();
  setLanguage('en');await tick();
  expect([...document.querySelectorAll('.form-error')].filter(node=>node.textContent===translate(error,'en'))).toHaveLength(1);
  expect(document.querySelector('.content > .form-error')).toBeNull();
 });
 it('思考穿插在对应文字和工具之间，只有当前思考展开，重开会话仍保持顺序',async()=>{
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  const blocks=[
   {answer_offset:0,tool_count:0,content:'先确定查找范围。'},
   {answer_offset:8,tool_count:0,content:'决定调用搜索。'},
   {answer_offset:8,tool_count:1,content:'核对工具返回的证据。'},
  ];
  const thought=async(index:number)=>{listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:blocks[index].content,reasoning_block:blocks[index],count:0}});await tick()};
  const order=()=>[...document.querySelectorAll('.answer')[1].querySelectorAll('.answer-text,.reasoning,.tool-call')].map(node=>node.matches('.reasoning')?'思考':node.matches('.tool-call')?'工具':node.textContent).filter(Boolean);
  await thought(0);await progress('正在生成回答','我先查询文档🙂');
  await thought(1);
  await vi.waitFor(()=>expect(order()).toEqual(['思考','我先查询文档🙂','思考']));
  expect([...document.querySelectorAll('.reasoning button')].map(node=>node.getAttribute('aria-expanded'))).toEqual(['false','true']);
  const call={id:'reasoning-order',query:'查询资料',source_count:1,answer_offset:8};
  listeners.get('query-progress')!({payload:{id:query.id,stage:'检索完成',delta:null,count:1,tool:{call,finished:true}}});await tick();
  await thought(2);await progress('正在生成回答','这是最终回答。');
  const expected=['思考','我先查询文档🙂','思考','工具','思考','这是最终回答。'];
  await vi.waitFor(()=>expect(order()).toEqual(expected));
  expect([...document.querySelectorAll('.reasoning button')].every(node=>node.getAttribute('aria-expanded')==='false')).toBe(true);
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer:'我先查询文档🙂这是最终回答。',reasoning:blocks.map(block=>block.content).join(''),reasoning_blocks:blocks,tool_calls:[call],status:'complete'}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  document.querySelector<HTMLButtonElement>('[aria-label="返回会话列表"]')!.click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('[title="已有对话"]')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[title="已有对话"]')!.click();await tick();
  await vi.waitFor(()=>expect(order()).toEqual(expected));
 });
 it('思考过程按 Markdown 渲染，流式追加后保持段落和代码结构',async()=>{
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  const content='### 核对步骤\n\n先核对 **真实来源**。\n\n1. 搜索文件\n2. 读取原文\n\n```ts\nconst source = "证据";\n```';
  listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:content,reasoning_block:{answer_offset:0,tool_count:0,content},count:0}});await tick();
  await vi.waitFor(()=>expect(document.querySelector('.reasoning-text strong')?.textContent).toBe('真实来源'));
  const thought=document.querySelector('.reasoning-text')!;
  // Happy DOM's sanitizer drops heading tags; heading structure is also checked in the real-browser preview.
  expect(thought.textContent).toContain('核对步骤');
  expect(thought.querySelectorAll('ol li')).toHaveLength(2);
  expect(thought.querySelector('pre code')?.textContent).toContain('const source = "证据";');
  const first=thought.querySelector('p');
  const addition='\n\n然后核对 `版本`。';
  listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:addition,reasoning_block:{answer_offset:0,tool_count:0,content:addition},count:0}});await tick();
  await vi.waitFor(()=>expect(thought.querySelector('p code')?.textContent).toBe('版本'));
  expect(thought.querySelector('p')).toBe(first);
  await progress('正在生成回答','核对完成。');
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer:'核对完成。',reasoning:content+addition}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  const toggle=document.querySelector<HTMLButtonElement>('.reasoning-toggle')!;
  toggle.click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('.reasoning-text strong')?.textContent).toBe('真实来源'));
  expect(document.querySelector('.reasoning-text pre code')?.textContent).toContain('const source');
 });
 it('修正回答草稿后，相关思考仍留在新回答前面',async()=>{
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:'先检查。',reasoning_block:{answer_offset:0,tool_count:0,content:'先检查。'},count:0}});await tick();
  await progress('正在生成回答','错误草稿。');
  listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:'重新核对。',reasoning_block:{answer_offset:5,tool_count:0,content:'重新核对。'},count:0}});await tick();
  await progress('正在修正引用','',undefined,true);
  listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:'继续检查。',reasoning_block:{answer_offset:0,tool_count:0,content:'继续检查。'},count:0}});await tick();
  expect([...document.querySelectorAll('.reasoning button')].map(node=>node.getAttribute('aria-expanded'))).toEqual(['false','true']);
  await progress('正在生成回答','已修正。');
  const order=()=>[...document.querySelectorAll('.answer')[1].querySelectorAll('.answer-text,.reasoning')].map(node=>node.matches('.reasoning')?'思考':node.textContent).filter(Boolean);
  await vi.waitFor(()=>expect(order()).toEqual(['思考','思考','已修正。']));
 });
 it('思考时默认展开，生成回答时折叠，完成后仍可手动展开',async()=>{
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  const emitThought=async(text:string)=>{listeners.get('query-progress')!({payload:{id:query.id,stage:'思考中',delta:null,reasoning_delta:text,count:0}});await tick()};
  await emitThought('先确认问题涉及的文件。');
  const thought=document.querySelector<HTMLElement>('.reasoning')!;
  expect(thought).not.toBeNull();expect(thought.textContent).toContain('思考中');
  expect(thought.querySelector('button')!.getAttribute('aria-expanded')).toBe('true');
  expect(thought.textContent).toContain('先确认问题涉及的文件。');
  expect(document.querySelectorAll('.answer-text')[1].textContent).toBe('');
  await emitThought('\n再核对来源。');
  expect(thought.textContent).toContain('再核对来源。');
  await progress('正在生成回答','已经找到结果。');
  expect(thought.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
  expect(document.querySelectorAll('.answer-text')[1].textContent).toBe('已经找到结果。');
  await progress('思考中');
  expect(thought.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
  expect(thought.querySelector('button')!.textContent).toContain('思考过程');
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer:'已经找到结果。',reasoning:'先确认问题涉及的文件。\n再核对来源。'}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  expect(document.querySelector('.reasoning')!.textContent).toContain('思考过程');
  expect(document.querySelector('.answer-progress')).toBeNull();
  const savedThought=document.querySelector<HTMLElement>('.reasoning')!;
  expect(savedThought.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
  savedThought.querySelector<HTMLButtonElement>('button')!.click();await tick();
  expect(savedThought.querySelector('button')!.getAttribute('aria-expanded')).toBe('true');
  expect(savedThought.textContent).toContain('再核对来源。');
 });
 it('放在当前回答内，随检索和生成更新，不显示在输入框底部',async()=>{
  await progress('SAG2 正在检索');
  const answers=document.querySelectorAll('.answer');expect(answers).toHaveLength(2);
  expect(answers[0].querySelector('[role="status"]')).toBeNull();
  expect(answers[1].querySelector('[role="status"]')!.textContent).toContain('SAG2 正在检索');
  expect(document.querySelector('.composer-wrap [role="status"]')).toBeNull();
  expect(document.querySelector('.composer-note')).toBeNull();
  await progress('正在生成回答','开始回答');
  expect(answers[1].textContent).toContain('正在生成回答');expect(answers[1].textContent).toContain('开始回答');
 });
 it('完成后移除处理进度',async()=>{
  await progress('正在生成回答');savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer:'完成回答'}]};resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});
  await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
  expect(document.body.textContent).toContain('完成回答');
 });
 it('停止后移除进度，其他任务的事件不能更改当前进度',async()=>{
  await progress('SAG2 正在检索');await progress('其他任务',null,'different-query');
  expect(document.querySelector('.answer-progress')!.textContent).toContain('SAG2 正在检索');
  document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();await tick();await tick();
  expect(document.querySelector('.answer-progress')).toBeNull();
 });
});

describe('SAG2 工具调用卡片',()=>{
 async function tool(call:{id:string;query:string;source_count:number;elapsed_ms?:number;answer_offset?:number},finished:boolean){
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  listeners.get('query-progress')!({payload:{id:query.id,stage:finished?'SAG2 检索完成':'正在调用 SAG2 检索文件',delta:null,count:0,tool:{call,finished}}});await tick();
 }
 it('按文字、工具、后续文字的顺序显示，完成并重开会话后保持顺序',async()=>{
  const intro='我先查一下🙂。';
  await progress('正在生成回答',intro);
  const call={id:'ordered',query:'实际检索问题',source_count:0,answer_offset:intro.length};
  await tool(call,false);
  const current=()=>document.querySelectorAll('.answer')[1];
  const content=()=>[...current().querySelectorAll('.answer-text,.tool-call')].map(node=>node.matches('.tool-call')?'工具':node.textContent);
  await vi.waitFor(()=>expect(content()).toEqual([intro,'工具','']));
  await tool({...call,source_count:2},true);
  await progress('正在生成回答','这是检索后的回答。');
  await vi.waitFor(()=>expect(content()).toEqual([intro,'工具','这是检索后的回答。']));
  const answer=intro+'这是检索后的回答。';
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer,tool_calls:[{...call,source_count:2}],status:'complete'}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  document.querySelector<HTMLButtonElement>('[aria-label="返回会话列表"]')!.click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('[title="已有对话"]')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[title="已有对话"]')!.click();await tick();
  await vi.waitFor(()=>expect(content()).toEqual([intro,'工具','这是检索后的回答。']));
 });
 it('接收真实工具参数与完成事件，显示独立卡片和返回数量',async()=>{
  const call={id:'call-1',query:'实际检索问题',source_count:0};await tool(call,false);await progress('正在扩展相关文件');
  const card=document.querySelector<HTMLDetailsElement>('.tool-call')!;
  expect(card.open).toBe(false);expect(card.textContent).toContain('实际检索问题');expect(card.querySelector('.tool-state')!.textContent).toBe('正在扩展相关文件');
  expect(card.closest('.chat-messages')).not.toBeNull();expect(document.querySelector('.composer-wrap')!.contains(card)).toBe(false);
  card.querySelector('summary')!.click();await tick();
  await tool({...call,source_count:4},true);expect(card.open).toBe(true);expect(card.textContent).toContain('已完成');expect(card.textContent).toContain('返回 4 个原文片段');
 });
 it('折叠卡片显示当前检索和排序阶段，完成后不再随阶段更新',async()=>{
  const call={id:'search-stages',query:'实际检索问题',source_count:0};await tool(call,false);
  const card=document.querySelector<HTMLDetailsElement>('.tool-call')!;
  expect(card.open).toBe(false);
  await progress('正在检索原文');
  expect(card.querySelector('summary .tool-state')!.textContent).toBe('正在查找相关文件');
  await progress('Jev 正在判断相关原文');
  expect(card.querySelector('summary .tool-state')!.textContent).toBe('正在按相关性排序');
  expect(card.open).toBe(false);
  await tool({...call,source_count:4},true);await progress('正在生成回答');
  expect(card.querySelector('summary .tool-state')!.textContent).toBe('已完成');
 });
 it('多次调用分别保留状态，停止时只标记当前调用',async()=>{
  await tool({id:'first',query:'第一次查询',source_count:2},true);await tool({id:'second',query:'第二次查询',source_count:0},false);
  document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();await tick();await tick();
  const cards=document.querySelectorAll('.tool-call');expect(cards).toHaveLength(2);expect(cards[0].textContent).toContain('已完成');expect(cards[1].textContent).toContain('已停止');
 });
 it('调用中持续计时，进度更新不重置，完成后显示实际耗时',async()=>{
  vi.useFakeTimers({toFake:['setInterval','clearInterval','performance']});
  const call={id:'timed-call',query:'实际检索问题',source_count:0};
  await tool(call,false);
  const duration=()=>document.querySelector('.tool-duration')?.textContent;
  expect(duration()).toBe('0 ms');
  await vi.advanceTimersByTimeAsync(1200);await tick();
  expect(duration()).toBe('1.2s');
  await progress('正在扩展相关文件');await tool({...call},false);
  await vi.advanceTimersByTimeAsync(800);await tick();
  expect(duration()).toBe('2s');
  await tool({...call,source_count:4,elapsed_ms:1875},true);
  expect(duration()).toBe('1.9s');
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(1000);await tick();
  expect(duration()).toBe('1.9s');
 });
 it('停止调用后计时停止，新调用单独计时',async()=>{
  vi.useFakeTimers({toFake:['setInterval','clearInterval','performance']});
  await tool({id:'finished',query:'第一次查询',source_count:2,elapsed_ms:1250},true);
  await tool({id:'running',query:'第二次查询',source_count:0},false);
  expect([...document.querySelectorAll('.tool-duration')].map(node=>node.textContent)).toEqual(['1.3s','0 ms']);
  await vi.advanceTimersByTimeAsync(500);await tick();
  document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();await tick();await tick();
  expect([...document.querySelectorAll('.tool-duration')].map(node=>node.textContent)).toEqual(['1.3s','500 ms']);
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(1000);await tick();
  expect(document.querySelectorAll('.tool-duration')[1].textContent).toBe('500 ms');
 });
});

it('后台历史刷新不会重复显示正在回答的问题',async()=>{
 savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer:'',status:'running'}]};
 listeners.get('history-changed')!({payload:null});await tick();await tick();await tick();
 expect(document.querySelectorAll('.user-message')).toHaveLength(2);
 expect([...document.querySelectorAll('.user-message')].filter(node=>node.textContent==='这次问题')).toHaveLength(1);
 savedConversation={...savedConversation,messages:[previous,{...previous,question:'这次问题',answer:'完成回答',status:'complete'}]};
 listeners.get('history-changed')!({payload:null});await tick();await tick();await tick();
 expect(document.querySelectorAll('.user-message')).toHaveLength(2);
});

it('复核时替换当前草稿，后续流式内容不重复，历史保持不变',async()=>{
 await progress('正在生成回答','待修正的草稿 [99]');
 expect(document.querySelectorAll('.answer-text')[1].textContent).toContain('待修正的草稿');
 await progress('正在复核证据与引用','',undefined,true);
 expect(document.querySelectorAll('.answer-text')[1].textContent).not.toContain('待修正的草稿');
 await progress('正在生成回答','修正后的回答');
 expect(document.querySelectorAll('.answer-text')[1].textContent).toBe('修正后的回答');
 expect(document.querySelectorAll('.answer-text')[0].textContent).toBe('上次回答');
});

describe('流式 Markdown',()=>{
 const currentAnswer=()=>document.querySelectorAll('.answer-text')[1];
 it('追加文本时保留已显示的标题和段落节点，避免整段重绘',async()=>{
  await progress('正在生成回答','说明。\n\n### 标题\n\n已经完成的段落。\n\n正在输出');
  await vi.waitFor(()=>expect(currentAnswer().innerHTML).toContain('<h3>标题</h3>'));
  const heading=currentAnswer().querySelector('h3');
  const paragraph=currentAnswer().querySelector('p');
  let tail='正在输出';
  for(const delta of ['的','后续','内容。']){
   tail+=delta;
   await progress('正在生成回答',delta);
   await vi.waitFor(()=>expect(currentAnswer().querySelector('p:last-child')?.textContent).toBe(tail));
   expect(currentAnswer().querySelector('h3')).toBe(heading);
   expect(currentAnswer().querySelector('p')).toBe(paragraph);
  }
 });
 it('流式追加期间不切回未格式化文本',async()=>{
  await progress('正在生成回答','说明。\n\n### 标题\n\n已经完成的段落。\n\n正在输出');
  await vi.waitFor(()=>expect(currentAnswer().innerHTML).toContain('<h3>标题</h3>'));
  const formats:boolean[]=[];
  const observer=new MutationObserver(()=>formats.push(!!currentAnswer().querySelector('h3')));
  observer.observe(currentAnswer(),{childList:true,subtree:true,characterData:true});
  await progress('正在生成回答','更多内容。');
  await vi.waitFor(()=>expect(currentAnswer().querySelector('p:last-child')?.textContent).toBe('正在输出更多内容。'));
  observer.disconnect();
  expect(formats.length).toBeGreaterThan(0);
  expect(formats.every(Boolean)).toBe(true);
 });
 it('生成过程中立即渲染标题、加粗、行内代码和列表',async()=>{
  await progress('正在生成回答','格式说明。\n\n### 1. 整体结构\n\n这是 **ZIP** 文件，包含 `manifest.json`。\n\n- 第一项\n- 第二项\n');
  await vi.waitFor(()=>expect(currentAnswer().querySelector('h3')?.textContent).toBe('1. 整体结构'));
  expect(currentAnswer().querySelector('strong')?.textContent).toBe('ZIP');
  expect(currentAnswer().querySelector('code')?.textContent).toBe('manifest.json');
  expect(currentAnswer().querySelectorAll('li')).toHaveLength(2);
  expect(document.querySelector('.answer-progress')).not.toBeNull();
 });
 it('未闭合代码块可继续追加，停止后保留已有格式',async()=>{
  await progress('正在生成回答','代码示例：\n\n```ts\nconst first = 1;');
  await vi.waitFor(()=>expect(currentAnswer().querySelector('pre code')?.textContent).toContain('const first = 1;'));
  await progress('正在生成回答','\nconst second = 2;\n```\n\n**代码说明**');
  await vi.waitFor(()=>expect(currentAnswer().querySelector('strong')?.textContent).toBe('代码说明'));
  expect(currentAnswer().querySelector('pre code')?.textContent).toContain('const second = 2;');
  document.querySelector<HTMLButtonElement>('[aria-label="停止生成"]')!.click();await tick();await tick();
  expect(currentAnswer().querySelector('strong')?.textContent).toBe('代码说明');
 });
 it('完成后保持 Markdown 格式，并将引用连接到对应原文',async()=>{
  const answer='结构说明。\n\n### 文件内容\n\n这是 **ZIP** 文件 [1]。';
  await progress('正在生成回答',answer);
  await vi.waitFor(()=>expect(currentAnswer().querySelector('h3')?.textContent).toBe('文件内容'));
  const evidence:Candidate={id:1,document_id:1,path:'/fixtures/format.md',title:'format.md',heading:'格式',content:'这是引用对应的原文。',start_line:1,end_line:1,source_ranges:[[0,12]],bm25:1,score:1,order:0,hash:'test'};
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',answer,evidence:[evidence]}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});
  await vi.waitFor(()=>expect(currentAnswer().querySelector<HTMLButtonElement>('[data-citation="0"]')).not.toBeNull());
  expect(currentAnswer().querySelector('h3')?.textContent).toBe('文件内容');
  const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((command,args)=>command==='read_document'?{path:evidence.path,name:evidence.title,body:evidence.content,hash:evidence.hash}:base(command,args));
  currentAnswer().querySelector<HTMLButtonElement>('[data-citation="0"]')!.click();await tick();
  await vi.waitFor(()=>expect(document.querySelector('.source-panel .search-hit.active')?.textContent).toBe(evidence.content));
 });
});

it.each([321,0,null])('回答统计显示模型输出用量 %s，不将缺失值显示成零',async(outputTokens)=>{
 savedConversation={...conversation,messages:[{...previous,context:{limit:65536,reserved_output:2048,estimated_input:600,omitted_sources:0,omitted_history:0,input_tokens:523,output_tokens:outputTokens}}]};
 resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});
 await vi.waitFor(()=>expect(document.querySelector('.answer-context')?.textContent).toContain('523 输入 tokens'));
 expect(document.querySelector('.answer-context')?.textContent).toContain(outputTokens===null?'输出 tokens 未返回':`${outputTokens} 输出 tokens`);
 expect(document.querySelector('.answer-context')?.textContent).not.toContain('总上限');
});

it('统计使用 K 简写，同时显示实际耗时和回答阶段的平均输出速度',async()=>{
 savedConversation={...conversation,messages:[{...previous,context:{limit:65536,reserved_output:2048,estimated_input:12000,omitted_sources:0,omitted_history:0,input_tokens:12002,output_tokens:1187,elapsed_ms:18000,generation_ms:5000}}]};
 resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});
 await vi.waitFor(()=>expect(document.querySelector('.answer-context')?.textContent).toContain('12K 输入 tokens'));
 const text=document.querySelector('.answer-context')!.textContent;
 expect(text).toContain('1.2K 输出 tokens');
 expect(text).toContain('耗时 18s');
 expect(text).toContain('237.4 tokens/s');
 expect(text).not.toContain('总上限');
});

describe('MCP 工具调用详情',()=>{
 const record={id:'mcp-1',query:'{\n  "repoName": "sample/repo"\n}',source_count:0,tool_name:'read_wiki_structure',server_name:'DeepWiki'};
 async function emitTool(call:import('../src/lib/types').ToolCallRecord,finished:boolean){
  const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
  listeners.get('query-progress')!({payload:{id:query.id,stage:finished?'MCP 调用完成':'正在调用 MCP',delta:null,count:0,tool:{call,finished}}});await tick();
 }
 it('展开后显示参数、完整结果和原始 JSON，回答更新不收起卡片',async()=>{
  await emitTool(record,false);
  expect(document.querySelector<HTMLDetailsElement>('.mcp-call')!.open).toBe(false);
  const result={content:[{type:'text',text:'主题一\n主题二\n<script>untrusted()</script>'}],isError:false};
  await emitTool({...record,result},true);
  const card=document.querySelector<HTMLDetailsElement>('.mcp-call')!;expect(card.open).toBe(false);
  card.querySelector('summary')!.click();await tick();
  expect(card.textContent).toContain('调用参数');expect(card.textContent).toContain('sample/repo');
  expect(card.querySelector('.tool-output')!.textContent).toContain('主题一\n主题二');expect(card.querySelector('script')).toBeNull();
  card.querySelector<HTMLDetailsElement>('.tool-raw')!.querySelector('summary')!.click();await tick();
  expect(card.querySelector('.tool-raw pre')!.textContent).toContain('"isError": false');
  await progress('正在生成回答','回答内容');expect(card.open).toBe(true);
  savedConversation={...conversation,messages:[previous,{...previous,question:'这次问题',tool_calls:[{...record,result}],status:'complete'}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  expect(document.querySelector('.mcp-call .tool-output')!.textContent).toContain('主题二');
 });
 it('MCP 返回错误时保持默认收起，展开后显示具体原因和原始返回值',async()=>{
  const error='Repository sample/repo was not found (404)';
  await emitTool({...record,is_error:true,error,result:{isError:true,content:[{type:'text',text:error}]}},true);
  const card=document.querySelector<HTMLDetailsElement>('.mcp-call')!;expect(card.open).toBe(false);
  card.querySelector('summary')!.click();await tick();expect(card.open).toBe(true);
  expect(card.querySelector('.tool-error')!.textContent).toContain(error);expect(card.querySelector('.tool-output')!.textContent).toContain(error);
 });
 it('停止后的历史记录保留停止原因，之前的失败不会改成已停止',async()=>{
  savedConversation={...conversation,messages:[{...previous,status:'stopped',tool_calls:[{...record,id:'failed',is_error:true,error:'Server rejected request'},{...record,id:'stopped',is_error:false,error:'已停止'}]}]};
  resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  const cards=document.querySelectorAll('.mcp-call');expect(cards[0].textContent).toContain('调用失败');expect(cards[1].textContent).toContain('停止原因');
  expect(cards[1].querySelector('.tool-state')!.textContent).toBe('已停止');
 });
 it('连接失败保留完整原因，无历史结果时明确提示',async()=>{
  await emitTool({...record,is_error:true,error:'MCP 工具调用超时：remote request timed out'},true);
  expect(document.querySelector('.tool-error')!.textContent).toContain('remote request timed out');
  savedConversation={...conversation,messages:[{...previous,tool_calls:[record]}]};resolveQuery({terms:[],results:[],trace:[],conversation:savedConversation});await tick();await tick();
  expect(document.querySelector('.tool-history-note')!.textContent).toContain('未保存返回结果');
 });
});
