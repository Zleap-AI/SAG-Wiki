// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import {get} from 'svelte/store';
import App from '../src/App.svelte';
import {version} from '../package.json';
import SettingsPanel from '../src/components/Settings.svelte';
import RichEditor from '../src/components/RichEditor.svelte';
import ToolCall from '../src/components/ToolCall.svelte';
import {language,setLanguage} from '../src/lib/i18n';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const settings:Settings={language:'zh-CN',answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:['.git'],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
const click=(label:string)=>{const element=[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()===label||node.getAttribute('aria-label')===label);expect(element,label).toBeDefined();element!.click()};
async function chooseEnglish(){
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="界面语言"]')).not.toBeNull());
 const control=document.querySelector<HTMLButtonElement>('[aria-label="界面语言"]')!;
 control.focus();control.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="en"]')).not.toBeNull());
 document.querySelector('[role=option][data-value="en"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));await tick();
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

it('previews English immediately, persists it on save, and restores Chinese on cancel',async()=>{
 const onSave=vi.fn(),onClose=vi.fn();
 const original={...settings,answer:{...settings.answer,models:['another-model','test']}};
 app=mount(SettingsPanel,{target:document.body,props:{value:original,onSave,onClose}});
 await chooseEnglish();
 expect(document.documentElement.lang).toBe('en');
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('General');
 expect(document.querySelector('.settings-footer')?.textContent).toContain('Save settings');
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();
 expect(onSave.mock.calls[0][0].language).toBe('en');
 expect(onSave.mock.calls[0][0].answer.model).toBe('test');
 await unmount(app);document.body.innerHTML='';setLanguage('zh-CN');
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,onSave,onClose}});
 await chooseEnglish();await unmount(app);document.body.innerHTML='';
 expect(get(language)).toBe('zh-CN');
 app=mount(SettingsPanel,{target:document.body,props:{value:settings,onSave,onClose}});
});

it('restores saved English across all settings pages and preserves original file and chat names',async()=>{
 const saved={...settings,language:'en' as const};
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return saved;
  if(command==='list_roots')return [{path:'/资料',name:'资料',status:'ready',count:1,error:null}];
  if(command==='list_conversations')return [{id:'1',title:'用户原来的问题',messages:[],updated:0,deleted:false}];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome h1')?.textContent).toBe('Ask your files'));
 expect(document.body.textContent).toContain('用户原来的问题');expect(document.body.textContent).toContain('资料');
 expect(document.querySelector('.workspace-switch')?.textContent).toContain('Chat');
 click('Settings');await tick();
 for(const page of ['General','Appearance','File settings','Shortcuts','Model settings','Chat settings','Search settings','Data and privacy','About']){
  click(page);await tick();
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
