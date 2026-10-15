// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {Settings} from '../src/lib/types';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>;
const value:Settings={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:100,seed_k:10,expansion_term_limit:20,max_hops:2,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
let installed=new Set(['cli']);
beforeEach(()=>{installed=new Set(['cli']);setLanguage('zh-CN');invoke.mockImplementation(async(command,args)=>{
 if(command==='list_roots')return [{path:'/project',name:'Project',status:'ready',count:5,error:null}];
 if(command==='get_cli_settings')return {access:{enabled:false,roots:[]},installations:['cli','claude-code','codex','opencode'].map(name=>({name,path:`/home/skills/${name}`,installed:installed.has(name),version:installed.has(name)?'0.1.0':null}))};
 if(command==='install_cli_integration'){args.uninstall?installed.delete(args.target):installed.add(args.target);return 'installed';}
});});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks();});
function click(text:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()===text);expect(button,text).toBeDefined();button!.click();}
async function ready(){await vi.waitFor(()=>expect(document.querySelector('[aria-label="允许搜索 Project"]')).not.toBeNull());}

it('saves CLI permission changes only with the settings form, across tab switches and installation refreshes',async()=>{
 const onSave=vi.fn(),onClose=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave,onClose}});
 await ready();
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('CLI');
 expect(document.body.textContent).not.toContain('Factory Droid');
 expect(document.body.textContent).not.toContain('应用 CLI 设置');
 document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();
 document.querySelector<HTMLButtonElement>('[aria-label="允许搜索 Project"]')!.click();await tick();
 expect(invoke.mock.calls.some(([command])=>command==='save_cli_settings')).toBe(false);
 expect([...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='测试连接')?.disabled).toBe(true);
 expect(document.body.textContent).toContain('请先保存访问权限，再测试连接。');
 click('安装');await vi.waitFor(()=>expect(document.querySelector('[role=status]')?.textContent).toContain('已安装'));
 expect(document.querySelector('[aria-label="允许搜索 Project"]')?.getAttribute('data-state')).toBe('checked');
 click('通用');await tick();click('CLI');await ready();
 expect(document.querySelector('[aria-label="启用 CLI"]')?.getAttribute('data-state')).toBe('checked');
 click('保存设置');await vi.waitFor(()=>expect(onClose).toHaveBeenCalled());
 expect(invoke).toHaveBeenCalledWith('save_cli_settings',{access:{enabled:true,roots:['/project']}});
 expect(onSave).toHaveBeenCalledOnce();
});

it('cancel discards CLI edits without sending an authorization change',async()=>{
 const onClose=vi.fn();app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose}});
 await ready();document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();click('取消');
 expect(onClose).toHaveBeenCalled();expect(invoke.mock.calls.some(([command])=>command==='save_cli_settings')).toBe(false);
});

it('shows installation errors and clears the busy state so the user can retry',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 invoke.mockImplementationOnce(async()=>{throw Error('发现同名文件，未修改');});click('安装');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('发现同名文件'));
 expect([...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='安装')?.disabled).toBe(false);
});


it('reports partial failure without announcing success or closing, and lets the user retry CLI permissions',async()=>{
 const onSaved=vi.fn(),onSave=vi.fn(),onClose=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave,onSaved,onClose}});await ready();
 document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();
 const normalInvoke=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>{if(command==='save_cli_settings')throw Error('权限写入失败');return normalInvoke(command,...args);});
 click('保存设置');await vi.waitFor(()=>expect(document.querySelector('.settings-feedback')?.textContent).toContain('其他设置已保存，但 CLI 权限未保存：'));
 expect(onSave).toHaveBeenCalledOnce();expect(onSaved).not.toHaveBeenCalled();expect(onClose).not.toHaveBeenCalled();
 invoke.mockImplementation(normalInvoke);click('保存设置');await vi.waitFor(()=>expect(onSaved).toHaveBeenCalledOnce());expect(onClose).toHaveBeenCalledOnce();
});


it.each([['cli','SAG CLI'],['claude-code','Claude Code'],['codex','Codex'],['opencode','OpenCode']])('%s has one button that switches between install and uninstall',async(target,name)=>{
 installed.clear();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 expect(document.querySelectorAll('.cli-install-row')).toHaveLength(4);
 for(const row of document.querySelectorAll('.cli-install-row'))expect(row.querySelectorAll('button')).toHaveLength(1);
 const action=()=>document.querySelector<HTMLButtonElement>(`[aria-label="${installed.has(target)?'卸载':'安装'} ${name}"]`)!;
 expect(action()?.textContent?.trim()).toBe('安装');action().click();
 await vi.waitFor(()=>expect(action()?.textContent?.trim()).toBe('卸载'));
 expect(invoke).toHaveBeenCalledWith('install_cli_integration',{target,uninstall:false});
 action().click();await vi.waitFor(()=>expect(action()?.textContent?.trim()).toBe('安装'));
 expect(invoke).toHaveBeenCalledWith('install_cli_integration',{target,uninstall:true});
 expect(document.querySelector('.cli-settings')?.textContent).not.toContain('更新');
});
