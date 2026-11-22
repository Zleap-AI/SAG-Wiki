// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import {get} from 'svelte/store';
import App from '../src/App.svelte';
import {version} from '../package.json';
import SettingsPanel from '../src/components/Settings.svelte';
import RichEditor from '../src/components/RichEditor.svelte';
import ToolCall from '../src/components/ToolCall.svelte';
import {language,setLanguage,translate,type Language} from '../src/lib/i18n';
import type {ChatImportSchedule, ChatSource, Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn(async (_command:string,_args?:Record<string,unknown>):Promise<any>=>({directory:'/Documents/sag-wiki/share',clipboard:false}))}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const settings:Settings={language:'zh-CN',answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:['.git'],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const click=(label:string,root:ParentNode=document)=>{const element=[...root.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()===label||node.getAttribute('aria-label')===label);expect(element,label).toBeDefined();element!.click()};
async function chooseLanguage(locale:Language){
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="界面语言"]')).not.toBeNull());
 const control=document.querySelector<HTMLButtonElement>('[aria-label="界面语言"]')!;
 control.focus();control.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${locale}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${locale}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
}

it('shows project and company links in About and opens them outside the app',async()=>{
 setLanguage('zh-CN');
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,initial:'about',onSave:vi.fn(),onClose:vi.fn()}});
 await tick();
 expect(document.querySelector('.settings-about-note')).toBeNull();
 expect(document.querySelector('.settings-about-version')?.textContent).toBe(`v${version}`);
 expect(document.querySelector('.settings-about-mark .brand-mark')).not.toBeNull();
 const links=[...document.querySelectorAll<HTMLAnchorElement>('.settings-about-links a')];
 expect(links.map(link=>link.href)).toEqual(['https://github.com/Zleap-AI/SAG-Wiki','https://zleap.com/']);
 for(const [index,target] of ['project','company'].entries()){
  expect(links[index].dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}))).toBe(false);
  await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_about_link',{target}));
 }
});

it('defaults to Edit and saves View as the document opening preference',async()=>{
 setLanguage('zh-CN');
 const onSave=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,onSave,onClose:vi.fn()}});
 await tick();
 const control=document.querySelector<HTMLButtonElement>('[aria-label="文档默认打开方式"]')!;
 expect(control.textContent?.trim()).toBe('编辑');
 control.focus();control.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="view"]')).not.toBeNull());
 document.querySelector('[role=option][data-value="view"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
 click('保存设置');
 await vi.waitFor(()=>expect(onSave.mock.calls[0][0].editor_open_mode).toBe('view'));
 expect(settings.editor_open_mode).toBeUndefined();
});

it.each(['en','ja','zh-TW','ko','es'] as const)('previews %s immediately, persists it on save, and restores Chinese on cancel',async(locale)=>{
 const heading=translate('通用',locale),saveLabel=translate('保存设置',locale);
 expect(heading).not.toBe('通用');
 expect(saveLabel).not.toBe('保存设置');
 const onSave=vi.fn(),onClose=vi.fn();
 const original={...settings,answer:{...settings.answer,models:['another-model','test']}};
 app=mount(SettingsPanel,{target:document.body,props:{value:original,onSave,onClose}});
 await chooseLanguage(locale);
 expect(document.documentElement.lang).toBe(locale);
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe(heading);
 expect(document.querySelector('.settings-footer')?.textContent).toContain(saveLabel);
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].language).toBe(locale);
 expect(onSave.mock.calls[0][0].answer.model).toBe('test');
 await unmount(app);document.body.innerHTML='';setLanguage('zh-CN');
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,onSave,onClose}});
 await chooseLanguage(locale);await unmount(app);document.body.innerHTML='';
 expect(get(language)).toBe('zh-CN');
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,onSave,onClose}});
});

it('restores saved English across all settings pages and preserves original file and chat names',async()=>{
 const saved={...settings,language:'en' as const};
 invoke.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return {directory:'/Documents/sag-wiki/share',clipboard:false};
  if(command==='get_settings')return saved;
  if(command==='get_quick_search')return {preferences:{enabled:true,shortcut:'CommandOrControl+Shift+Space'},error:null};
  if(command==='list_roots')return [{path:'/资料',name:'资料',status:'ready',count:1,error:null}];
  if(command==='list_conversations')return [{id:'1',title:'用户原来的问题',messages:[],updated:0,deleted:false}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome h1')?.textContent).toBe('Ask your files'));
 expect(document.body.textContent).toContain('用户原来的问题');expect(document.body.textContent).toContain('资料');
 expect(document.querySelector('.workspace-switch')?.textContent).toContain('Chat');
 click('Settings');await tick();
 expect([...document.querySelectorAll('.settings-nav-group')].map(group=>[
  group.getAttribute('aria-label'),
  [...group.querySelectorAll('button')].map(button=>button.textContent?.trim()),
 ])).toEqual([
  ['Application',['General','Appearance','Shortcuts']],
  ['Knowledge base',['Data sources','File settings','Web Clipper']],
  ['Search & Q&A',['Model settings','Chat settings','Search settings']],
  ['Connections & services',['CLI settings','Remote services','Network proxy']],
  ['Other',['Data and privacy','About']],
 ]);
 for(const page of ['General','Appearance','Data sources','File settings','Shortcuts','Model settings','Chat settings','Search settings','Data and privacy','About']){
  click(page,document.querySelector('.settings-nav')!);await tick();
  expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe(page);
  if(page==='Chat settings')expect(document.querySelector('.settings-content')?.textContent).toContain('MCP servers');
  expect(document.querySelector('.settings-content')?.textContent,page).not.toMatch(/[\u3400-\u9fff]/);
 }
 click('Close settings');await tick();
 click('Search');await vi.waitFor(()=>expect(document.querySelector('.search-welcome h1')?.textContent).toBe('Search your files'));
});

it.each(['','原文保持不变'])('updates editor labels without changing document content: %s',async(body)=>{
 const onChange=vi.fn();
 app=mount(RichEditor,{target:document.body,props:{body,path:'/原文.md',onChange}});
 await tick();setLanguage('en');await tick();
 expect(document.querySelector('[aria-label="Bold"]')).not.toBeNull();
 expect(document.querySelector('[aria-label="Undo"]')).not.toBeNull();
 expect(document.querySelector('[aria-label="Document content"]')?.textContent).toBe(body);
 if(!body)expect(document.querySelector('[data-placeholder]')?.getAttribute('data-placeholder')).toBe('Start writing, or paste Markdown…');
 expect(onChange).not.toHaveBeenCalled();
});

it('translates tool progress and failure details while preserving arguments and result content',async()=>{
 setLanguage('en');
 const call={id:'tool',query:'用户提供的参数',source_count:0,tool_name:'read',server_name:'Example',is_error:true,error:'MCP 工具调用超时',result:{content:[{type:'text',text:'工具返回的原文'}]}};
 app=mount(ToolCall,{target:document.body,props:{call,state:'failed'}});await tick();
 expect(document.querySelector('summary')?.textContent).toContain('Failed');
 expect(document.body.textContent).toContain('MCP tool call timed out');
 expect(document.body.textContent).toContain('用户提供的参数');
 expect(document.body.textContent).toContain('工具返回的原文');
});

it('数据源打开聊天窗口，关闭后保留设置，并保存新的导入位置',async()=>{
 setLanguage('zh-CN');
 invoke.mockImplementation(async command=>{
  if(command==='get_chat_import_folder')return '/Documents/AgentHistory';
  if(command==='choose_chat_import_folder')return '/Archive/Chats';
  if(command==='list_chat_sessions')return {source_root:'/transcripts',sessions:[],issues:[],total:0,page:0,page_size:50,projects:[]};
  if(command==='get_chat_import_schedule')return {schedule:{enabled:false,time:'09:00',source_root:null,include_tools:false},last_run_at:null,last_run:null};
  return [];
 });
 const onSave=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,initial:'imports',onSave,onClose:vi.fn()}});
 await tick();
 expect(document.querySelector('.settings-content')?.textContent).toContain('Claude Code');
 expect(document.querySelector('.settings-content')?.textContent).toContain('Codex');
 expect(document.querySelector('.settings-footer')?.textContent).not.toContain('保存设置');
 click('导入 Codex 聊天记录');
 await vi.waitFor(()=>expect(document.querySelector('.chat-import')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('list_chat_sessions',{source:'codex',sourceRoot:null,query:{page:0,search:'',project:''}});
 expect(document.querySelector('.chat-import-heading h2')?.textContent).toBe('导入 Codex 聊天记录');
 click('更改');await vi.waitFor(()=>expect(document.querySelector('.chat-import')?.textContent).toContain('/Archive/Chats'));
 click('关闭');await vi.waitFor(()=>expect(document.querySelector('.chat-import')).toBeNull());
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('数据源');
 click('通用');await tick();click('保存设置');
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalled());
 expect(onSave.mock.calls[0][0].chat_import_folder).toBe('/Archive/Chats');
});

it('定时导入共用聊天文件夹和工具开关，并按来源自动保存',async()=>{
 setLanguage('zh-CN');
 const schedules:Record<ChatSource,ChatImportSchedule>={
  codex:{enabled:false,time:'09:00',source_root:null,include_tools:false},
  claude:{enabled:true,time:'09:00',source_root:'/claude-chats',include_tools:true},
  claude_desktop:{enabled:false,time:'09:00',source_root:null,include_tools:false},
  workbuddy:{enabled:false,time:'09:00',source_root:null,include_tools:false},
 };
 invoke.mockImplementation(async(command,args)=>{
  const source=args?.source as ChatSource;
  if(command==='get_chat_import_schedule')return {
   schedule:schedules[source],last_run_at:null,last_run:null,
  };
  if(command==='save_chat_import_schedule')schedules[source]=args?.schedule as ChatImportSchedule;
  if(command==='list_chat_sessions')return {source_root:args?.sourceRoot??'/transcripts',sessions:[],issues:[],total:0,page:0,page_size:50,projects:[]};
  return null;
 });
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,initial:'imports',onSave:vi.fn(),onClose:vi.fn()}});
 await tick();
 expect(document.querySelector('[aria-label="Codex 定时导入"]')).toBeNull();
 click('导入 Codex 聊天记录');
 const selector='.chat-import .chat-import-schedule';
 const syncSwitch=()=>document.querySelector<HTMLButtonElement>(`${selector} [role="switch"]`)!;
 await vi.waitFor(()=>expect(syncSwitch()?.disabled).toBe(false));
 expect(invoke).toHaveBeenCalledWith('get_chat_import_schedule',{source:'codex'});
 expect(syncSwitch().getAttribute('aria-checked')).toBe('false');
 expect(document.querySelector(`${selector} select`)).toBeNull();
 expect(document.querySelector(`${selector} input[type="time"]`)).toBeNull();
 click('定时同步');
 await vi.waitFor(()=>expect(schedules.codex.enabled).toBe(true));
 await tick();
 const chooseFrequency=(value:string)=>{
  const select=document.querySelector<HTMLSelectElement>(`${selector} select`)!;
  select.value=value;
  select.dispatchEvent(new Event('change',{bubbles:true}));
 };
 const frequency=document.querySelector<HTMLSelectElement>(`${selector} select`)!;
 expect(frequency.value).toBe('daily');
 expect(Array.from(frequency.options,option=>option.textContent)).toEqual(['每 30 分钟','每小时','每 6 小时','每 12 小时','每天']);
 chooseFrequency('30');
 await vi.waitFor(()=>expect(schedules.codex).toEqual({enabled:true,time:'09:00',interval_minutes:30,source_root:'/transcripts',include_tools:false}));
 await tick();
 expect(document.querySelector(`${selector} input[type="time"]`)).toBeNull();
 chooseFrequency('daily');
 await vi.waitFor(()=>expect(schedules.codex.interval_minutes).toBeNull());
 await tick();
 const time=document.querySelector<HTMLInputElement>(`${selector} input[type="time"]`)!;
 time.value='20:45';time.dispatchEvent(new Event('change',{bubbles:true}));
 await vi.waitFor(()=>expect(schedules.codex.time).toBe('20:45'));
 await tick();
 const folder=document.querySelector<HTMLInputElement>('.chat-source-path input')!;
 folder.value='/archive/chats';folder.dispatchEvent(new Event('input',{bubbles:true}));
 expect(schedules.codex.source_root).toBe('/transcripts');
 document.querySelector('.chat-source-path')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(schedules.codex.source_root).toBe('/archive/chats'));
 await tick();
 click('展示工具调用过程');
 await vi.waitFor(()=>expect(schedules.codex.include_tools).toBe(true));
 await tick();
 expect(document.querySelectorAll(`${selector} button`)).toHaveLength(1);
 click('关闭');
 await vi.waitFor(()=>expect(document.querySelector(selector)).toBeNull());
 click('导入 Codex 聊天记录');
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>(`${selector} input[type="time"]`)?.value).toBe('20:45'));
 expect(document.querySelector<HTMLInputElement>('.chat-source-path input')?.value).toBe('/archive/chats');
 expect(document.querySelector('[aria-label="展示工具调用过程"]')?.getAttribute('aria-checked')).toBe('true');
 await vi.waitFor(()=>expect(syncSwitch().disabled).toBe(false));
 chooseFrequency('360');
 await vi.waitFor(()=>expect(schedules.codex.interval_minutes).toBe(360));
 await tick();
 click('定时同步');
 await vi.waitFor(()=>expect(schedules.codex.enabled).toBe(false));
 await vi.waitFor(()=>expect(document.querySelector(`${selector} select`)).toBeNull());
 expect(schedules.codex.interval_minutes).toBe(360);
 await tick();
 click('关闭');
 await vi.waitFor(()=>expect(document.querySelector(selector)).toBeNull());
 click('导入 Claude Code 聊天记录');
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>(`${selector} input[type="time"]`)?.value).toBe('09:00'));
 expect(invoke).toHaveBeenCalledWith('get_chat_import_schedule',{source:'claude'});
 expect(invoke).toHaveBeenCalledWith('list_chat_sessions',{source:'claude',sourceRoot:'/claude-chats',query:{page:0,search:'',project:''}});
});
