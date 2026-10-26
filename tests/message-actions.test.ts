// @vitest-environment happy-dom
import{afterEach,beforeEach,expect,it,vi}from 'vitest';
import{mount,tick,unmount}from 'svelte';
import App from '../src/App.svelte';
import type{Conversation}from '../src/lib/types';
const{invoke,listeners,writeText}=vi.hoisted(()=>({invoke:vi.fn(),listeners:new Map<string,(event:{payload:unknown})=>void>(),writeText:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name,callback)=>{listeners.set(name,callback);return()=>listeners.delete(name)})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const original:Conversation={id:'chat',title:'测试对话',updated:0,deleted:false,messages:[
 {question:'原来的问题',answer:'原来的 **Markdown** 回答',scope:['/notes'],references:['/notes/a.md'],model:'test',evidence:[],status:'complete',error:null,retrieval_question:'原来的问题'},
 {question:'后续问题',answer:'后续回答',scope:[],references:[],model:'test',evidence:[],status:'complete',error:null,retrieval_question:'后续问题'}
]};
let app:ReturnType<typeof mount>,saved:Conversation,resolveQuery:(value:unknown)=>void,rejectQuery:(error:Error)=>void;
const buttons=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>(`[aria-label="${label}"]`)];
const click=async(label:string,index=0)=>{buttons(label)[index].click();await tick()};
beforeEach(async()=>{
 vi.clearAllMocks();saved=structuredClone(original);
 Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText}});
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return[saved];
  if(command==='replace_messages'){saved={...saved,messages:structuredClone(args.messages)};return saved;}
  if(command==='run_query')return new Promise((resolve,reject)=>{resolveQuery=resolve;rejectQuery=reject});
  return[];
 });
 app=mount(App,{target:document.body});await vi.waitFor(()=>expect(listeners.has('query-progress')).toBe(true));
 document.querySelector<HTMLButtonElement>('[title="测试对话"]')!.click();await tick();
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';listeners.clear()});
it('复制问题与回答保留原始 Markdown',async()=>{
 await click('复制问题');expect(writeText).toHaveBeenLastCalledWith('原来的问题');
 await click('复制回答');expect(writeText).toHaveBeenLastCalledWith('原来的 **Markdown** 回答');
});
it('删除一轮问答并可以撤销，后续消息保留',async()=>{
 await click('删除此轮问答');
 await vi.waitFor(()=>expect(document.querySelectorAll('.user-message')).toHaveLength(1));
 expect(saved.messages.map(m=>m.question)).toEqual(['后续问题']);
 await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('已删除此轮问答'));
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(document.querySelectorAll('.user-message')).toHaveLength(2));
 expect(saved.messages).toEqual(original.messages);
});
it('编辑时可取消，保存后从该问题重新生成并沿用原范围',async()=>{
 await click('编辑问题');
 let input=document.querySelector<HTMLTextAreaElement>('[aria-label="编辑问题"]')!;
 expect(input.value).toBe('原来的问题');
 [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent==='取消')!.click();await tick();
 expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(false);
 await click('编辑问题');input=document.querySelector<HTMLTextAreaElement>('[aria-label="编辑问题"]')!;
 input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));
 input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',ctrlKey:true,isComposing:true,bubbles:true}));await tick();
 expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(false);
 input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
 input.dispatchEvent(new KeyboardEvent('keyup',{key:'Enter',bubbles:true}));
 input.value='修改后的问题';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
 expect(invoke.mock.calls.find(([name])=>name==='run_query')![1].query).toMatchObject({question:'修改后的问题',replace_from:0,conversation_id:'chat',scope:['/notes'],references:['/notes/a.md']});
 expect(document.querySelectorAll('.user-message')).toHaveLength(1);
 expect(document.querySelector('.user-message')!.textContent).toBe('修改后的问题');
});
it('重试较早回答先确认，完成后可以恢复原来的后续问答',async()=>{
 await click('重新生成回答');
 expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(false);
 expect(document.querySelector('[role="dialog"]')!.textContent).toContain('后续 1 轮问答');
 [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(b=>b.textContent==='重新生成')!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='run_query')).toBe(true));
 expect(invoke.mock.calls.find(([name])=>name==='run_query')![1].query).toMatchObject({question:'原来的问题',replace_from:0});
 saved={...saved,messages:[{...saved.messages[0],answer:'重新生成的回答'}]};
 resolveQuery({terms:[],results:[],trace:[],conversation:saved});
 await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
 await vi.waitFor(()=>expect(document.querySelector('#notice')!.textContent).toContain('已重新生成回答'));
 [...document.querySelectorAll<HTMLButtonElement>('#notice button')].find(b=>b.textContent==='恢复')!.click();
 await vi.waitFor(()=>expect(saved.messages).toEqual(original.messages));
 await vi.waitFor(()=>expect(document.querySelectorAll('.user-message')).toHaveLength(2));
});
it('新对话失败后重试同一条消息，不创建重复对话',async()=>{
 await click('返回会话列表');
 await vi.waitFor(()=>expect(document.querySelectorAll('.user-message')).toHaveLength(0));
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
 input.value='失败后重试';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='run_query')).toHaveLength(1));
 const query=invoke.mock.calls.find(([name])=>name==='run_query')![1].query;
 saved={...original,id:query.id,title:'失败后重试',messages:[{...original.messages[0],question:'失败后重试',answer:'部分回答',status:'failed',error:'连接中断'}]};
 rejectQuery(new Error('连接中断'));
 await vi.waitFor(()=>expect(document.querySelector('.answer-progress')).toBeNull());
 await click('重新生成回答');
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='run_query')).toHaveLength(2));
 expect(invoke.mock.calls.filter(([name])=>name==='run_query')[1][1].query).toMatchObject({conversation_id:query.id,replace_from:0,question:'失败后重试'});
});
