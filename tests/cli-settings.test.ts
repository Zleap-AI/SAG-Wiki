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
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.useRealTimers();vi.clearAllMocks();vi.restoreAllMocks();});
function click(text:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()===text);expect(button,text).toBeDefined();button!.click();}
async function ready(){await vi.waitFor(()=>expect(document.querySelector('[aria-label="启用 CLI"]')).not.toBeNull());}
it('Windows Skill 安装位置与下载提示显示普通路径',async()=>{
 const path=String.raw`\\?\C:\Users\PC\skills\sag`;
 invoke.mockImplementation(async command=>{
  if(command==='get_cli_settings')return{access:{enabled:false,roots:[]},installations:[{name:'codex',path,installed:true,version:'0.1.0'}]};
  if(command==='download_cli_skill')return path;
 });
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 document.querySelector<HTMLButtonElement>('.cli-location button')!.click();await tick();
 expect(document.querySelector('.cli-skill-path code')?.textContent).toBe(String.raw`C:\Users\PC\skills\sag`);
 document.querySelector<HTMLButtonElement>('[aria-label="下载 SAG Skill"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.cli-feedback[role=status]')?.textContent).toContain(String.raw`C:\Users\PC\skills\sag`));
 expect(document.querySelector('.cli-feedback[role=status]')?.textContent).not.toContain('\\\\?\\');
});

it('saves CLI enabled changes only with the settings form, across tab switches and installation refreshes',async()=>{
 const onSave=vi.fn(),onClose=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave,onClose}});
 await ready();
 expect(document.querySelector('.settings-page-heading h2')?.textContent).toBe('CLI设置');
 expect(document.querySelector('#cli-tools-heading')?.textContent).toBe('接入Agent');
 expect(document.querySelector('.settings-content')?.textContent).not.toContain('编程工具');
 expect(document.body.textContent).not.toContain('Factory Droid');
 expect(document.body.textContent).not.toContain('应用 CLI 设置');
 document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();
 await tick();
 expect(invoke.mock.calls.some(([command])=>command==='save_cli_settings')).toBe(false);
 expect([...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='测试连接')?.disabled).toBe(true);
 click('安装');await vi.waitFor(()=>expect(document.querySelector('[role=status]')?.textContent).toContain('已安装'));
 click('通用');await tick();click('CLI设置');await ready();
 expect(document.querySelector('[aria-label="启用 CLI"]')?.getAttribute('data-state')).toBe('checked');
 click('保存设置');await vi.waitFor(()=>expect(onClose).toHaveBeenCalled());
 expect(invoke).toHaveBeenCalledWith('save_cli_settings',{access:{enabled:true}});
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


it.each([['claude-code','Claude Code'],['codex','Codex'],['opencode','OpenCode']])('%s has one button that switches between install and uninstall',async(target,name)=>{
 installed.clear();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 expect(document.querySelectorAll('.cli-install-row')).toHaveLength(4);
 for(const row of document.querySelectorAll('.cli-install-row')){
  expect(row.querySelectorAll('.cli-action')).toHaveLength(1);
  expect(row.textContent).not.toMatch(/已安装|未安装/);
 }
 const action=()=>document.querySelector<HTMLButtonElement>(`[aria-label="${installed.has(target)?'卸载':'安装'} ${name}"]`)!;
 expect(action()?.textContent?.trim()).toBe('安装');action().click();
 await vi.waitFor(()=>expect(action()?.textContent?.trim()).toBe('卸载'));
 expect(action().classList.contains('cli-uninstall')).toBe(true);
 expect(invoke).toHaveBeenCalledWith('install_cli_integration',{target,uninstall:false});
 action().click();await vi.waitFor(()=>expect(action()?.textContent?.trim()).toBe('安装'));
 expect(invoke).toHaveBeenCalledWith('install_cli_integration',{target,uninstall:true});
 expect(document.querySelector('.cli-settings')?.textContent).not.toContain('更新');
});


it('copies each tool installation command without installing it',async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);
 vi.spyOn(navigator.clipboard,'writeText').mockImplementation(writeText);
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 for(const [target,name] of [['claude-code','Claude Code'],['codex','Codex'],['opencode','OpenCode']]){
  const copy=document.querySelector<HTMLButtonElement>(`[aria-label="复制 ${name} 安装命令"]`);
  expect(copy).not.toBeNull();copy!.click();
  await vi.waitFor(()=>expect(document.querySelector('[role=status]')?.textContent).toContain(`已复制 ${name} 的安装命令`));
  expect(writeText).toHaveBeenLastCalledWith(`sag install ${target}`);
 }
 expect(invoke.mock.calls.some(([command])=>command==='install_cli_integration')).toBe(false);
});

it('uses automatic scope without requesting a folder authorization list',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 expect(document.querySelectorAll('[role=checkbox]')).toHaveLength(0);
 expect(document.body.textContent).toContain('无需选择授权范围');
 expect(invoke.mock.calls.some(([command])=>command==='list_roots')).toBe(false);
});


it('omits the command-line tool panel while preserving CLI and skill settings',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 expect(document.body.textContent).not.toContain('命令行工具');
 expect(document.querySelector('#cli-access-heading')?.parentElement?.textContent).toContain('测试连接');
 const testButton=[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()==='测试连接')!;
 testButton.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('test_cli_connection',undefined));
 expect(document.querySelector('#cli-command-heading')).toBeNull();
 expect(document.querySelectorAll('.cli-section')).toHaveLength(2);
 expect(document.querySelector('[aria-label="启用 CLI"]')).not.toBeNull();
 expect(document.querySelectorAll('.cli-install-row')).toHaveLength(4);
});

it.each(['SAG 服务已连接\n{"pid":1}','SAG 服务已连接\n{"pid":2}'])('retests with one fresh result and a fresh success deadline: %s',async result=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 vi.useFakeTimers({toFake:['setTimeout','clearTimeout','Date']});
 invoke.mockResolvedValueOnce('SAG 服务已连接\n{"pid":1}');click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功'));
 document.querySelector<HTMLButtonElement>('.cli-connection-trigger')!.click();await tick();
 expect(document.querySelector<HTMLElement>('.cli-connection-content')!.hidden).toBe(false);
 await vi.advanceTimersByTimeAsync(2500);
 let finish!:(result:string)=>void;
 invoke.mockImplementationOnce(()=>new Promise<string>(resolve=>{finish=resolve}));click('测试连接');await tick();
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 expect(document.querySelector<HTMLButtonElement>('.cli-service-actions button')!.disabled).toBe(true);
 await vi.advanceTimersByTimeAsync(1000);finish(result);
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-content pre')?.textContent).toBe(result));
 expect(document.querySelectorAll('.cli-connection-result')).toHaveLength(1);
 const trigger=document.querySelector<HTMLButtonElement>('.cli-connection-trigger')!;
 const content=document.querySelector<HTMLElement>('.cli-connection-content')!;
 expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(content.hidden).toBe(true);
 expect(trigger.getAttribute('aria-controls')).toBe(content.id);
 trigger.click();await tick();expect(content.hidden).toBe(false);
 trigger.click();await tick();expect(content.hidden).toBe(true);
 await vi.advanceTimersByTimeAsync(2800);await tick();
 expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功');
 await vi.advanceTimersByTimeAsync(200);await tick();
 expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('');
 expect(document.querySelectorAll('.cli-connection-result')).toHaveLength(1);
 expect(document.querySelector('.cli-connection-content pre')?.textContent).toBe(result);
 expect(invoke.mock.calls.filter(([command])=>command==='test_cli_connection')).toHaveLength(2);
});

it('replaces immediately resolved results without duplicate panels or duplicate requests',async()=>{
 const result='SAG 服务已连接\n{"pid":9}';
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 vi.useFakeTimers({toFake:['setTimeout','clearTimeout','Date']});
 invoke.mockResolvedValueOnce(result);click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功'));
 for(let attempt=0;attempt<3;attempt++){
  document.querySelector<HTMLButtonElement>('.cli-connection-trigger')!.click();await tick();
  const previous=document.querySelector('.cli-connection-result');
  await vi.advanceTimersByTimeAsync(2500);
  invoke.mockResolvedValueOnce(result);
  const test=document.querySelector<HTMLButtonElement>('.cli-service-actions button')!;
  test.click();test.click();
  await vi.waitFor(()=>{
   expect(document.querySelector('.cli-connection-result')).not.toBe(previous);
   expect(document.querySelector('.cli-connection-content pre')?.textContent).toBe(result);
  });
  expect(document.querySelectorAll('.cli-connection-result')).toHaveLength(1);
  expect(document.querySelector<HTMLElement>('.cli-connection-content')!.hidden).toBe(true);
  await vi.advanceTimersByTimeAsync(600);await tick();
  expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功');
 }
 expect(invoke.mock.calls.filter(([command])=>command==='test_cli_connection')).toHaveLength(4);
});

it('clears a previous success when retesting fails and allows a fresh retry',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 invoke.mockResolvedValueOnce('SAG 服务已连接\n{"pid":1}');click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-result')).not.toBeNull());
 invoke.mockRejectedValueOnce(Error('连接中断'));click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-error')?.textContent).toContain('连接中断'));
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 invoke.mockResolvedValueOnce('SAG 服务已连接\n{"pid":2}');click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-content pre')?.textContent).toContain('"pid":2'));
 expect(document.querySelector('.cli-error')).toBeNull();
 expect(document.querySelectorAll('.cli-connection-result')).toHaveLength(1);
});

it.each([500,3500])('preserves the last result and disclosure across settings pages without replaying success after %i ms',async away=>{
 const result='SAG 服务已连接\n{"pid":42}';
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 vi.useFakeTimers({toFake:['setTimeout','clearTimeout','Date']});
 invoke.mockResolvedValueOnce(result);click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功'));
 document.querySelector<HTMLButtonElement>('.cli-connection-trigger')!.click();await tick();
 await vi.advanceTimersByTimeAsync(500);click('通用');await tick();
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 await vi.advanceTimersByTimeAsync(away);click('CLI设置');await ready();
 expect(document.querySelector('.cli-connection-content pre')?.textContent).toBe(result);
 expect(document.querySelector('.cli-connection-trigger')?.getAttribute('aria-expanded')).toBe('true');
 expect(document.querySelector<HTMLElement>('.cli-connection-content')!.hidden).toBe(false);
 expect(document.querySelector('.cli-connection-summary')?.textContent).toBe(away<3000?'连接成功':'');
 await vi.advanceTimersByTimeAsync(2100);await tick();
 expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('');
 expect(document.querySelector('.cli-connection-content pre')?.textContent).toBe(result);
 expect(invoke.mock.calls.filter(([command])=>command==='test_cli_connection')).toHaveLength(1);
});

it('clears the last connection when CLI is disabled, including the cached result across pages',async()=>{
 const originalInvoke=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,...args)=>{
  const response=await originalInvoke(command,...args);
  return command==='get_cli_settings'?{...response,access:{enabled:true}}:response;
 });
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 vi.useFakeTimers({toFake:['setTimeout','clearTimeout','Date']});
 invoke.mockResolvedValueOnce('SAG 服务已连接\n{"pid":1}');click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功'));
 document.querySelector<HTMLButtonElement>('.cli-connection-trigger')!.click();await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();await tick();
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 click('通用');await tick();click('CLI设置');await ready();
 expect(document.querySelector('[aria-label="启用 CLI"]')?.getAttribute('data-state')).toBe('unchecked');
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="启用 CLI"]')!.click();await tick();
 expect(document.querySelector('.cli-connection-result')).toBeNull();
 await vi.advanceTimersByTimeAsync(2500);
 invoke.mockResolvedValueOnce('SAG 服务已连接\n{"pid":2}');click('测试连接');
 await vi.waitFor(()=>expect(document.querySelector('.cli-connection-content pre')?.textContent).toContain('"pid":2'));
 await vi.advanceTimersByTimeAsync(600);await tick();
 expect(document.querySelector('.cli-connection-summary')?.textContent).toBe('连接成功');
 expect(document.querySelector('.cli-connection-trigger')?.getAttribute('aria-expanded')).toBe('false');
});

it('shows the usage guide last, collapsed by default, without submitting settings when expanded',async()=>{
 const onSave=vi.fn();
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave,onClose:vi.fn()}});await ready();
 const guide=document.querySelector<HTMLElement>('.cli-guide')!;
 const trigger=guide.querySelector<HTMLButtonElement>('button')!;
 const guides=document.querySelector('.cli-guides')!;
 expect(document.querySelector('.cli-settings')?.lastElementChild).toBe(guides);
 expect(guides.lastElementChild).toBe(guide);
 const skill=guides.querySelector('.cli-location')!;
 const skillTrigger=skill.querySelector<HTMLButtonElement>('button')!;
 const skillContent=skill.querySelector<HTMLElement>('.cli-location-content')!;
 expect(skillTrigger.textContent).toContain('Skill 安装指南');
 expect(skillTrigger.getAttribute('aria-expanded')).toBe('false');
 expect(skillContent.hidden).toBe(true);
 expect(document.querySelector('.cli-integrations .cli-skill-help')).toBeNull();
 skillTrigger.click();await tick();
 expect(skillContent.hidden).toBe(false);
 expect(skillContent.querySelector('p')?.textContent).toBe('点击上方对应 Agent 的“安装”按钮即可完成安装。');
 expect(skillContent.textContent).toContain('安装完成后，请在对应 Agent 中新开会话以加载 Skill。');
 expect(skillContent.textContent).toContain('/home/skills/codex');
 expect(trigger.textContent).toContain('CLI 使用指南');
 expect(trigger.getAttribute('aria-expanded')).toBe('false');
 expect(guide.querySelector<HTMLElement>('.cli-guide-content')!.hidden).toBe(true);
 const before=invoke.mock.calls.length;
 trigger.click();await tick();
 expect(trigger.getAttribute('aria-expanded')).toBe('true');
 expect(guide.querySelector<HTMLElement>('.cli-guide-content')!.hidden).toBe(false);
 expect(guide.textContent).toContain('sag add');
 expect(guide.textContent).toContain('sag add\nsag check');
 expect(guide.textContent).toContain('sag "这个项目如何处理登录与权限？" -m 5');
 expect(guide.textContent).not.toContain('sag -c');
 expect(guide.textContent).toContain('默认进行语义搜索，并返回原文片段、路径和行号');
 expect(guide.textContent).toContain('sag list');
 expect(guide.textContent).toContain('status 查看服务、CLI 接入和模型配置状态');
 expect(guide.textContent).toContain('sag remove ./docs');
 expect(guide.textContent).toContain('不会删除本地文件');
 expect(guide.textContent).toContain('sag --help');
 expect(onSave).not.toHaveBeenCalled();
 expect(invoke.mock.calls).toHaveLength(before);
 trigger.click();await tick();
 expect(trigger.getAttribute('aria-expanded')).toBe('false');
});

it('downloads the complete skill through the last Other row without installing an integration',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 const rows=document.querySelectorAll('.cli-install-row');
 expect(rows[rows.length-1].textContent).toContain('其他');
 expect(rows[rows.length-1].textContent).toContain('下载完整 Skill 文件夹');
 const download=document.querySelector<HTMLButtonElement>('[aria-label="下载 SAG Skill"]')!;
 expect(download.textContent).toBe('下载');
 invoke.mockImplementationOnce(async()=>'/Downloads/sag');download.click();
 await vi.waitFor(()=>expect(document.querySelector('.cli-feedback[role=status]')?.textContent).toContain('/Downloads/sag'));
 expect(invoke).toHaveBeenCalledWith('download_cli_skill',{title:'选择 Skill 保存位置'});
 expect(invoke.mock.calls.some(([command])=>command==='install_cli_integration')).toBe(false);
 expect(download.textContent).toBe('下载');
});

it('keeps cancellation silent and makes a failed download retryable',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 const download=document.querySelector<HTMLButtonElement>('[aria-label="下载 SAG Skill"]')!;
 invoke.mockResolvedValueOnce(null);download.click();await tick();
 await vi.waitFor(()=>expect(download.disabled).toBe(false));
 expect(document.querySelector('.cli-feedback')).toBeNull();
 invoke.mockRejectedValueOnce(Error('无法创建 Skill 文件夹'));download.click();
 await vi.waitFor(()=>expect(document.querySelector('.cli-error')?.textContent).toContain('无法创建 Skill 文件夹'));
 expect(download.disabled).toBe(false);
});

it('translates the download row and success message',async()=>{
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'cli',onSave:vi.fn(),onClose:vi.fn()}});await ready();
 setLanguage('en');await tick();
 const download=document.querySelector<HTMLButtonElement>('[aria-label="Download SAG Skill"]')!;
 expect(document.querySelectorAll('.cli-install-row')[3].textContent).toContain('Other');
 invoke.mockResolvedValueOnce('/Downloads/sag');download.click();
 await vi.waitFor(()=>expect(document.querySelector('.cli-feedback[role=status]')?.textContent).toContain('Skill downloaded to /Downloads/sag'));
 expect(invoke).toHaveBeenCalledWith('download_cli_skill',{title:'Choose where to save the Skill'});
});
