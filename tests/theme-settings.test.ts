// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const initial:Settings={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512},theme:'light',start_page:'ask',editor_size:15,show_hidden:false,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let app:ReturnType<typeof mount>,saved:Settings,failCliSave=false;
beforeEach(()=>{
 saved=structuredClone(initial);failCliSave=false;
 invoke.mockImplementation(async(command:string,args?:{settings?:Settings})=>{
  if(command==='get_settings')return structuredClone(saved);
  if(command==='save_settings'&&args?.settings){saved=structuredClone(args.settings);return;}
  if(command==='get_index_status')return {paused:false,tasks:[]};
  if(command==='get_cli_settings')return {access:{enabled:false},installations:[]};
  if(command==='save_cli_settings'&&failCliSave)throw new Error('权限写入失败');
  return [];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';delete document.documentElement.dataset.theme;vi.clearAllMocks()});
function click(label:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(node=>node.textContent?.trim()===label||node.getAttribute('aria-label')===label);expect(button,label).toBeDefined();button!.click()}
async function openSettings(){click('设置');await tick();expect(document.querySelector('.settings-window')).not.toBeNull()}
async function chooseTheme(theme:string){click('外观');await tick();document.querySelector<HTMLInputElement>(`input[name="appearance-theme"][value="${theme}"]`)!.click();await tick();expect(document.documentElement.dataset.theme).toBe(theme)}
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('.welcome h1')).not.toBeNull());await openSettings()}
async function cancel(){click('取消');await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull())}
it('取消主题预览时恢复已保存的主题',async()=>{
 await start();await chooseTheme('dark');await cancel();
 expect(saved.theme).toBe('light');expect(document.documentElement.dataset.theme).toBe('light');
});
it('保存主题后保持新主题，重新打开仍然选中',async()=>{
 await start();await chooseTheme('dark');click('保存设置');
 await vi.waitFor(()=>expect(document.querySelector('.settings-window')).toBeNull());
 expect(saved.theme).toBe('dark');expect(document.documentElement.dataset.theme).toBe('dark');
 await openSettings();click('外观');await tick();
 expect(document.querySelector<HTMLInputElement>('input[value="dark"]')!.checked).toBe(true);
 await cancel();expect(document.documentElement.dataset.theme).toBe('dark');
});
it.each(['light','dark'])('CLI 保存失败后再次预览并取消，恢复最近保存的 %s 主题',async(theme)=>{
 failCliSave=true;await start();await chooseTheme(theme);click('CLI设置');
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="启用 CLI"]')).not.toBeNull());
 click('保存设置');
 await vi.waitFor(()=>expect(document.querySelector('.settings-feedback')?.textContent).toContain('其他设置已保存，但 CLI 权限未保存：'));
 expect(saved.theme).toBe(theme);
 await chooseTheme(theme==='light'?'dark':'light');await cancel();
 expect(saved.theme).toBe(theme);expect(document.documentElement.dataset.theme).toBe(theme);
});
