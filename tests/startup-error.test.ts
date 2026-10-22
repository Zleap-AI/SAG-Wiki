// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import {setLanguage} from '../src/lib/i18n';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const settings={language:'zh-CN',theme:'light',start_page:'ask',editor_size:15,answer:{url:'',model:'test',models:['test']},jev:{url:'',model:'jev'},retrieval:{candidate_k:50,seed_k:10,expansion_term_k:5,max_hops:1,chunk_chars:600},index_workers:2,file_types:[],excluded:[],show_hidden:false,convert_other_formats:false,conversion_formats:[],search_summary_enabled:true,search_summary_sources:3,watch:true,model_profiles:{}};

let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN')});

async function settle(){for(let i=0;i<10;i++)await tick();await new Promise(r=>setTimeout(r,0));await tick();}

it('shows the connection error with a retry button instead of a blank workspace',async()=>{
 localStorage.setItem('sag.onboarding.seen','1');
 let attempts=0;
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings'){attempts++;if(attempts===1)throw new Error('SAG 服务协议版本 1 与客户端 2 不兼容');return settings;}
  return [];
 });
 app=mount(App,{target:document.body});
 await settle();
 const screen=document.querySelector('.startup-error');
 expect(screen?.textContent).toContain('无法连接本地 SAG 服务');
 expect(screen?.textContent).toContain('SAG 服务协议版本 1 与客户端 2 不兼容');
 const retry=[...document.querySelectorAll<HTMLButtonElement>('.startup-error button')].find(b=>b.textContent?.trim()==='重试');
 expect(retry).toBeTruthy();
 retry!.click();
 await settle();
 expect(document.querySelector('.startup-error')).toBeNull();
 expect(invoke).toHaveBeenCalledWith('get_settings',undefined);
 expect(attempts).toBe(2);
});

it('clears a transient startup error once the workspace loads',async()=>{
 localStorage.setItem('sag.onboarding.seen','1');
 let fire:((e:{payload:string})=>void)|null=null;
 const {listen}=await import('@tauri-apps/api/event');
 (listen as unknown as {mockImplementation:(f:(name:string,cb:(e:{payload:string})=>void)=>Promise<()=>void>)=>void}).mockImplementation(async(name,cb)=>{if(name==='app-error')fire=cb;return()=>{}});
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings'){fire?.({payload:'SAG 服务协议版本 1 与客户端 2 不兼容'});return settings;}
  return [];
 });
 app=mount(App,{target:document.body});
 await settle();
 expect(document.querySelector('.startup-error')).toBeNull();
});
