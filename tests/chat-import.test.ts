// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import ChatImport from '../src/components/ChatImport.svelte';
import '../src/lib/markdown';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>|undefined;
const session={id:'c1',title:'发布流程讨论',project:'/demo/project',updated:'2026-11-15T01:00:00Z',paths:['/transcripts/one.jsonl']};
const onImported=vi.fn(async()=>{});
const button=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===label||b.querySelector('strong')?.textContent===label)!;
beforeEach(()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_chat_import_folder')return '/Documents/sag-wiki/agent-history';
  if(command==='get_chat_import_schedule')return {schedule:{enabled:false,time:'09:00',source_root:null,include_tools:false},last_run_at:null,last_run:null};
  if(command==='choose_chat_import_folder')return '/Archive/Chats';
  if(command==='list_chat_sessions')return{source_root:'/transcripts',sessions:[session],issues:[],total:1,page:0,page_size:50,projects:['/demo/project']};
  if(command==='preview_chat_import')return{title:'发布流程讨论',body:'# 发布流程讨论\n\n用户问题\n\n助手回答'};
  if(command==='import_chat_sessions')return[{path:'/library/chat.md',status:'imported',error:null}];
 });
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN')});
async function start(source:'claude'|'codex'='claude'){
 app=mount(ChatImport,{target:document.body,props:{source,onClose:vi.fn(),onImported}});
 await vi.waitFor(()=>expect(button(session.title)).toBeDefined());
 await vi.waitFor(()=>expect(document.querySelector('.chat-preview-document')).not.toBeNull());
}
it('默认关闭工具过程，选择会话后预览并导入固定目录',async()=>{
 await start();
 expect(document.body.textContent).toContain('/Documents/sag-wiki/agent-history/claude code');
 expect(document.querySelector('.chat-sources')).toBeNull();
 expect(document.querySelector('.chat-import-heading h2')?.textContent).toBe('导入 Claude Code 聊天记录');
 const toggle=document.querySelector('[role="switch"]')!;expect(toggle.getAttribute('aria-checked')).toBe('false');
 expect(document.querySelector('[role="checkbox"][aria-label="选择 发布流程讨论"]')?.getAttribute('aria-checked')).toBe('false');
 expect(button('导入').disabled).toBe(true);
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('preview_chat_import',expect.objectContaining({includeTools:false,paths:session.paths})));
 await vi.waitFor(()=>expect(document.querySelector('.chat-preview-document')).not.toBeNull());expect(document.querySelector('.chat-preview-document p')?.textContent).toBe('用户问题');
 document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="选择 ${session.title}"]`)!.click();await tick();button('导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_chat_sessions',{source:'claude',sourceRoot:'/transcripts',sessions:[session],includeTools:false}));
 await vi.waitFor(()=>expect(onImported).toHaveBeenCalledOnce());expect(document.body.textContent).toContain('已导入');
});
it('Codex 入口直接加载对应记录，开启工具过程后导入',async()=>{
 await start('codex');expect(document.body.textContent).toContain('/Documents/sag-wiki/agent-history/codex');button(session.title).click();await vi.waitFor(()=>expect(invoke.mock.calls.some(([name])=>name==='preview_chat_import')).toBe(true));
 document.querySelector<HTMLElement>('[role="switch"]')!.click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('preview_chat_import',expect.objectContaining({includeTools:true})));
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('list_chat_sessions',{source:'codex',sourceRoot:null,query:{page:0,search:'',project:''}}));
 await vi.waitFor(()=>expect(button(session.title)).toBeDefined());
 await vi.waitFor(()=>expect(document.querySelector('.chat-preview-document')).not.toBeNull());document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="选择 ${session.title}"]`)!.click();await tick();button('导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_chat_sessions',expect.objectContaining({source:'codex',includeTools:true})));
});
it('英文界面显示默认目录且没有整理笔记选项',async()=>{
 setLanguage('en');await start();
 expect(document.body.textContent).toContain('/Documents/sag-wiki/agent-history/claude code');
 expect(button('Change')).toBeDefined();
 document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="Select ${session.title}"]`)!.click();await tick();button('Import').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_chat_sessions',{source:'claude',sourceRoot:'/transcripts',sessions:[session],includeTools:false}));
 expect(document.querySelector('[role="switch"]')!.getAttribute('aria-label')).toBe('Show tool calls and results');
 expect(document.body.textContent).not.toMatch(/整理|摘要/);
});
it('失败信息保留在窗口，不显示已成功导入',async()=>{
 await start();invoke.mockResolvedValueOnce([{path:session.title,status:'failed',error:'source damaged'}]);
 document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="选择 ${session.title}"]`)!.click();await tick();button('导入').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('source damaged'));expect(onImported).not.toHaveBeenCalled();expect(document.querySelector('.chat-import')).not.toBeNull();
});

it('更改保存位置后显示新目录，取消选择保留原目录',async()=>{
 await start();button('更改').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('/Archive/Chats/claude code'));
 invoke.mockResolvedValueOnce(null);button('更改').click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([name])=>name==='choose_chat_import_folder')).toHaveLength(2));
 expect(document.body.textContent).toContain('/Archive/Chats/claude code');
});

it('按页读取且跨页保留勾选，搜索从全部记录筛选',async()=>{
 const second={...session,id:'c2',title:'另一条聊天'};
 const normal=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='list_chat_sessions')return {source_root:'/transcripts',sessions:[args.query.page?second:session],issues:[],total:51,page:args.query.page,page_size:50,projects:['/demo/project']};
  return normal(command,args);
 });
 await start('codex');
 document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="选择 ${session.title}"]`)!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click();
 await vi.waitFor(()=>expect(button(second.title)).toBeDefined());
 expect(invoke).toHaveBeenCalledWith('list_chat_sessions',{source:'codex',sourceRoot:'/transcripts',query:{page:1,search:'',project:''}});
 button('导入').click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_chat_sessions',expect.objectContaining({sessions:[session]})));
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索聊天"]')!;input.value='查找';input.dispatchEvent(new Event('input',{bubbles:true}));
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('list_chat_sessions',{source:'codex',sourceRoot:'/transcripts',query:{page:0,search:'查找',project:''}}));
});

for(const source of ['claude','codex'] as const){
 it(`${source} 展示开关应用于切换后的聊天及导入`,async()=>{
  const second={...session,id:'c2',title:'第二条聊天',paths:['/transcripts/two.jsonl']};
  const normal=invoke.getMockImplementation()!;
  invoke.mockImplementation(async(command,args)=>{
   if(command==='list_chat_sessions')return{source_root:'/transcripts',sessions:[session,second],issues:[],total:2,page:0,page_size:50,projects:['/demo/project']};
   if(command==='preview_chat_import')return{title:session.title,body:`# 对话\n\n正常回答${args.includeTools?'\n\n### Tool: Read\n\n工具调用内容':''}`};
   return normal(command,args);
  });
  await start(source);
  const toggle=document.querySelector<HTMLElement>('[role="switch"][aria-label="展示工具调用过程"]')!;
  expect(document.querySelector('.chat-preview-document')?.textContent).not.toContain('工具调用内容');
  toggle.click();
  await vi.waitFor(()=>expect(document.querySelector('.chat-preview-document')?.textContent).toContain('工具调用内容'));
  button(second.title).click();
  await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('preview_chat_import',expect.objectContaining({source,paths:second.paths,includeTools:true})));
  await vi.waitFor(()=>expect(document.querySelector('.chat-preview-document')?.textContent).toContain('工具调用内容'));
  document.querySelector<HTMLElement>(`[role="checkbox"][aria-label="选择 ${second.title}"]`)!.click();await tick();button('导入').click();
  await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('import_chat_sessions',expect.objectContaining({source,sessions:[second],includeTools:true})));
  await vi.waitFor(()=>expect(button('导入').disabled).toBe(false));
  toggle.click();await tick();
  await vi.waitFor(()=>{
   const content=document.querySelector('.chat-preview-document')?.textContent;
   expect(content).toContain('正常回答');
   expect(content).not.toContain('工具调用内容');
  });
 });
}
