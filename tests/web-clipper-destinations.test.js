import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {afterEach,expect,it,vi} from 'vitest';

let dom;
afterEach(()=>{vi.unstubAllGlobals();vi.resetModules();dom?.window.close();});

async function settings({saved={},directories={},safari=false}={}) {
 dom=new JSDOM(readFileSync('browser-extension/static/options.html','utf8'),{url:'https://extension.test'});
 const data={language:'en',...saved};
 const getDirectory=vi.fn(async kind=>directories[kind]);
 const setDirectory=vi.fn(async(handle,kind)=>{directories[kind]=handle;});
 const api={i18n:{getUILanguage:()=> 'en'},storage:{local:{get:async defaults=>typeof defaults==='string'?{[defaults]:data[defaults]}:{...defaults,...data},set:async values=>Object.assign(data,values),remove:async key=>delete data[key]}},runtime:{sendNativeMessage:vi.fn(async(_app,message)=>({path:directories[message.destination] || ''}))}};
 vi.resetModules();
 vi.doMock('../browser-extension/src/platform.js',()=>({isSafari:safari}));
 vi.doMock('../browser-extension/src/directory.js',()=>({getDirectory,setDirectory}));
 vi.stubGlobal('window',dom.window);vi.stubGlobal('document',dom.window.document);vi.stubGlobal('chrome',api);
 await import('../browser-extension/src/options.js');
 return {data,api,getDirectory,setDirectory,element:id=>dom.window.document.querySelector(id)};
}

it('starts with a default location and no repeated path or indexing notice',async()=>{
 const ui=await settings();
 expect(ui.element('#destination').value).toBe('default');
 expect(ui.element('#destination option:checked').textContent).toBe('Default folder (Documents/sag-wiki/clippings)');
 expect(ui.element('#selected-folder')).toBeNull();
 expect(ui.element('[data-i18n="defaultLibrary"]')).toBeNull();
 expect(ui.element('#folder-notice').hidden).toBe(true);
});

it('shows an authorized location inside the selection and only warns when the folder changes',async()=>{
 const current={name:'Agent Workplace',queryPermission:async()=> 'granted',isSameEntry:async other=>other===current};
 const ui=await settings({saved:{destination:'folder'},directories:{folder:current,default:{name:'clippings',queryPermission:async()=> 'granted'}}});
 expect(ui.element('#destination option:checked').textContent).toBe('Custom folder (Agent Workplace)');
 expect(ui.element('#folder-notice').hidden).toBe(true);
 dom.window.showDirectoryPicker=vi.fn(async()=>current);
 await ui.element('#choose-folder').onclick();
 expect(ui.setDirectory).not.toHaveBeenCalled();
 await ui.element('#apply').onclick();
 expect(ui.element('#folder-notice').hidden).toBe(true);
 dom.window.showDirectoryPicker=vi.fn(async()=>({name:'Other notes'}));
 await ui.element('#choose-folder').onclick();
 expect(ui.element('#destination option:checked').textContent).toBe('Custom folder (Other notes)');
 expect(ui.element('#folder-notice').hidden).toBe(true);
 await ui.element('#apply').onclick();
 expect(ui.element('#folder-notice').hidden).toBe(false);
 ui.element('#language').value='zh-CN';await ui.element('#language').onchange();
 expect(ui.element('#folder-notice').textContent).toBe('选择其他文件夹后，请在 SAG Wiki 的文件夹管理中添加，否则无法检索其中的内容。');
 expect(ui.data.language).toBe('en');
 ui.element('#destination').value='default';await ui.element('#destination').onchange();
 expect(ui.data.destination).toBe('folder');
 await ui.element('#apply').onclick();
 expect(ui.data.destination).toBe('default');
 expect(ui.element('#folder-notice').hidden).toBe(true);
});

it('saves the default folder separately from an existing custom folder',async()=>{
 const current={name:'Agent Workplace'};
 const target={name:'clippings'};
 const root={name:'Documents',getDirectoryHandle:vi.fn(async()=>({getDirectoryHandle:vi.fn(async()=>target)}))};
 const ui=await settings({directories:{folder:current}});
 dom.window.showDirectoryPicker=vi.fn(async()=>root);
 expect(root.getDirectoryHandle).not.toHaveBeenCalled();
 expect(ui.setDirectory).not.toHaveBeenCalled();
 await ui.element('#apply').onclick();
 expect(root.getDirectoryHandle).toHaveBeenCalledWith('sag-wiki',{create:true});
 expect(ui.setDirectory).toHaveBeenCalledWith(target,'default');
 expect(ui.data.defaultPath).toBe('Documents/sag-wiki/clippings');
 expect(ui.element('#folder-notice').hidden).toBe(true);
 ui.element('#destination').value='folder';await ui.element('#destination').onchange();
 expect(ui.element('#destination option:checked').textContent).toBe('Custom folder (Agent Workplace)');
});

it('uses Safari native paths for both default and custom locations',async()=>{
 const ui=await settings({safari:true,directories:{default:'/Users/Test/Documents/sag-wiki/clippings',folder:'/Volumes/Notes/Agent Workplace'}});
 expect(ui.element('#destination option:checked').textContent).toContain('/Users/Test/Documents/sag-wiki/clippings');
 expect(ui.element('#destination option[value="folder"]').textContent).toBe('Custom folder (/Volumes/Notes/Agent Workplace)');
 expect(ui.api.runtime.sendNativeMessage).toHaveBeenCalledWith('com.sag.webclipper',{action:'status',destination:'default'});
 expect(ui.api.runtime.sendNativeMessage).toHaveBeenCalledWith('com.sag.webclipper',{action:'status',destination:'folder'});
});

it('supports setting up the default directory directly with Save settings',async()=>{
 const target={name:'clippings'};
 const root={name:'Documents',getDirectoryHandle:async()=>({getDirectoryHandle:async()=>target})};
 const ui=await settings();
 dom.window.showDirectoryPicker=vi.fn(async()=>root);
 expect(ui.element('#apply').closest('#download-options')).toBeNull();
 expect(ui.element('#choose-folder').hidden).toBe(true);
 await ui.element('#apply').onclick();
 expect(dom.window.showDirectoryPicker).toHaveBeenCalledTimes(1);
 expect(ui.setDirectory).toHaveBeenCalledWith(target,'default');
 expect(ui.data.destination).toBe('default');
});

it('keeps Safari folder choices pending until Save settings',async()=>{
 const ui=await settings({safari:true,saved:{destination:'folder'},directories:{folder:'/Volumes/Old notes'}});
 ui.api.runtime.sendNativeMessage.mockImplementation(async(_app,message)=>message.action==='choose'?{path:'/Volumes/New notes',bookmark:'test-bookmark'}:{path:'/Volumes/New notes'});
 await ui.element('#choose-folder').onclick();
 expect(ui.api.runtime.sendNativeMessage).toHaveBeenLastCalledWith('com.sag.webclipper',expect.objectContaining({action:'choose',persist:false}));
 expect(ui.element('#folder-notice').hidden).toBe(true);
 await ui.element('#apply').onclick();
 expect(ui.api.runtime.sendNativeMessage).toHaveBeenLastCalledWith('com.sag.webclipper',{action:'commit',destination:'folder',bookmark:'test-bookmark'});
 expect(ui.element('#folder-notice').hidden).toBe(false);
});

it('does not require Safari default-folder setup when saving settings',async()=>{
 const ui=await settings({safari:true});
 await ui.element('#apply').onclick();
 expect(ui.api.runtime.sendNativeMessage.mock.calls.every(([,request])=>request.action==='status')).toBe(true);
 expect(ui.data.destination).toBe('default');
});

it('has no toolbar setting and saves the remaining Safari preferences',async()=>{
 const ui=await settings({safari:true});
 expect(ui.element('#toolbar-enabled')).toBeNull();
 await ui.element('#apply').onclick();
 expect(ui.data.toolbarEnabled).toBeUndefined();
 expect(ui.data.destination).toBe('default');
});
