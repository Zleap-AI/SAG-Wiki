// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import {get} from 'svelte/store';
import AppUpdates from '../src/components/AppUpdates.svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import UpdateNotification from '../src/components/UpdateNotification.svelte';
import {checkUpdates,defaultUpdateSettings,updateState,updateDownload} from '../src/lib/updates';
import {setLanguage} from '../src/lib/i18n';
import type {Settings,UpdateInfo} from '../src/lib/types';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke,Channel:class{onmessage:((value:unknown)=>void)|undefined}}));
const latest:UpdateInfo={current_version:'0.1.5',latest_version:'0.1.6',available:true,notes:'修复与改进',download_url:'https://example.com/SAG-Wiki-0.1.6-macOS-arm64.dmg'};
const settings:Settings={answer:{url:'https://example.com/v1',model:'test',models:['test'],has_key:false},jev:{url:'https://example.com/v1',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:['.git'],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048};
const apps:ReturnType<typeof mount>[]=[];
const click=(text:string)=>{
 const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.trim()===text);
 expect(button, text).toBeDefined();button!.click();
};
beforeEach(()=>{setLanguage('zh-CN');updateDownload.set({key:'',version:'',stage:'idle',downloaded:0,total:null,error:''});updateState.set({key:'',checking:false,info:null,error:''});invoke.mockReset();invoke.mockResolvedValue(latest)});
afterEach(async()=>{for(const app of apps.splice(0))await unmount(app);document.body.innerHTML=''});

it('checks at startup, shows a new-version prompt and opens About on click',async()=>{
 const onOpen=vi.fn();
 apps.push(mount(UpdateNotification,{target:document.body,props:{config:{...defaultUpdateSettings},onOpen}}));
 await vi.waitFor(()=>expect(document.querySelector('.update-notification')?.textContent).toContain('0.1.6'));
 expect(invoke).toHaveBeenCalledExactlyOnceWith('check_app_update',{config:defaultUpdateSettings});
 document.querySelector<HTMLButtonElement>('.update-open')!.click();await tick();
 expect(onOpen).toHaveBeenCalledOnce();
 expect(document.querySelector('.update-notification')).toBeNull();
});

it('downloads in app, keeps progress across settings navigation, and only installs on request',async()=>{
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}}));
 expect(invoke).not.toHaveBeenCalled();
 click('检查更新');
 await vi.waitFor(()=>expect(document.body.textContent).toContain('发现新版本 0.1.6'));
 expect(document.querySelectorAll('.app-updates button')).toHaveLength(1);
 expect(document.querySelector('.app-updates .setting-row button')?.textContent).toBe('下载更新');
 expect(document.body.textContent).toContain('修复与改进');
 let finish!:()=>void;
 invoke.mockImplementationOnce((_command,args)=>new Promise<void>(resolve=>{
  finish=resolve;
  args.onProgress.onmessage({stage:'downloading',downloaded:50,total:100});
 }));
 click('下载更新');
 await vi.waitFor(()=>expect(document.body.textContent).toContain('50%'));
 expect(invoke).toHaveBeenLastCalledWith('download_app_update',{config:defaultUpdateSettings,version:'0.1.6',onProgress:expect.anything()});
 expect(document.querySelector<HTMLButtonElement>('.setting-row button')!.disabled).toBe(true);
 await unmount(apps.pop()!);
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}}));
 await tick();
 expect(document.body.textContent).toContain('50%');
 finish();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('重启并更新'));
 expect(invoke).not.toHaveBeenCalledWith('install_app_update',expect.anything());
 click('重启并更新');
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('install_app_update',{version:'0.1.6'}));
 expect(invoke.mock.calls.map(call=>call[0])).not.toContain('open_update_download');
});

it('reports up-to-date versions and surfaces check and download failures',async()=>{
 invoke.mockResolvedValueOnce({...latest,available:false,download_url:null});
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}}));
 click('检查更新');
 await vi.waitFor(()=>expect(document.body.textContent).toContain('当前已是最新版本（0.1.5）'));
 expect([...document.querySelectorAll('button')].some(button=>button.textContent?.trim()==='下载更新')).toBe(false);
 invoke.mockRejectedValueOnce('更新检查失败（HTTP 403）');click('检查更新');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('HTTP 403'));
 invoke.mockResolvedValueOnce(latest);click('检查更新');
 await vi.waitFor(()=>expect(document.body.textContent).toContain('发现新版本 0.1.6'));
 invoke.mockRejectedValueOnce('Signature verification failed');click('下载更新');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Signature verification failed'));
});

it('prevents duplicate checks while a request is pending',async()=>{
 let resolve!:(info:UpdateInfo)=>void;
 invoke.mockImplementationOnce(()=>new Promise<UpdateInfo>(done=>resolve=done));
 const first=checkUpdates(defaultUpdateSettings);
 await checkUpdates(defaultUpdateSettings);
 expect(invoke).toHaveBeenCalledOnce();expect(get(updateState).checking).toBe(true);
 resolve(latest);await first;
 expect(get(updateState).checking).toBe(false);
});

it('keeps update addresses hidden and preserves configured sources when saving settings',async()=>{
 const onSave=vi.fn();
 const configured={...settings,updates:{...defaultUpdateSettings,github_url:'https://github.com/customer/wiki'}};
 apps.push(mount(SettingsPanel,{target:document.body,props:{value:configured,initial:'about',onSave,onClose:vi.fn()}}));
 await tick();
 expect(document.querySelector('[aria-label="GitHub 项目地址"]')).toBeNull();
 expect(document.querySelector('[aria-label="更新地址"]')).toBeNull();
 expect(document.body.textContent).not.toContain('修改地址后');
 expect(document.body.textContent).not.toContain('官网');
 expect(document.querySelector('[aria-label="更新来源"]')).toBeNull();
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalled());
 expect(onSave.mock.calls[0][0].updates).toEqual(configured.updates);
 expect(settings.updates).toBeUndefined();
});

it('does not let a draft repository check trigger another startup check',async()=>{
 apps.push(mount(UpdateNotification,{target:document.body,props:{config:{...defaultUpdateSettings},onOpen:vi.fn()}}));
 await vi.waitFor(()=>expect(get(updateState).checking).toBe(false));
 const draft={...defaultUpdateSettings,github_url:'https://github.com/customer/wiki'};
 await checkUpdates(draft);await tick();
 expect(invoke).toHaveBeenCalledTimes(2);
 expect(get(updateState).key).toContain('customer/wiki');
});

it('renders update controls in English',async()=>{
 setLanguage('en');
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}}));
 expect(document.body.textContent).toContain('Check for updates');
 expect(document.body.textContent).not.toMatch(/[\u3400-\u9fff]/);
});


it('requires saved-document approval and retains a verified update after installation failure',async()=>{
 await checkUpdates(defaultUpdateSettings);
 await import('../src/lib/updates').then(module=>module.downloadUpdate(defaultUpdateSettings,latest));
 const onBeforeInstall=vi.fn().mockResolvedValueOnce(false).mockResolvedValue(true);
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings},onBeforeInstall}}));
 await tick();click('重启并更新');await tick();
 expect(invoke).not.toHaveBeenCalledWith('install_app_update',expect.anything());
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('.setting-row button')!.disabled).toBe(false));
 invoke.mockRejectedValueOnce('Permission denied');click('重启并更新');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Permission denied'));
 expect(get(updateDownload).stage).toBe('ready');
 expect(document.querySelector('.setting-row button')?.textContent).toBe('重启并更新');
});

it('does not offer installation after signature verification fails and allows a download retry',async()=>{
 await checkUpdates(defaultUpdateSettings);
 invoke.mockRejectedValueOnce('Invalid signature');
 apps.push(mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}}));
 await tick();click('下载更新');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Invalid signature'));
 expect(get(updateDownload).stage).toBe('idle');
 expect(document.querySelector('.setting-row button')?.textContent).toBe('下载更新');
 invoke.mockResolvedValueOnce(undefined);click('下载更新');
 await vi.waitFor(()=>expect(get(updateDownload).stage).toBe('ready'));
 expect(document.querySelector('[role=alert]')).toBeNull();
});
