import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {afterEach,expect,it,vi} from 'vitest';

let dom;
afterEach(()=>{vi.unstubAllGlobals();vi.resetModules();dom?.window.close();});

async function popup({query='?tabId=7',error,safari=false,configured=true,cancelled=false,destination='default'}={}) {
 dom=new JSDOM(readFileSync('browser-extension/static/popup.html','utf8'),{url:`https://extension.test/popup.html${query}`});
 const clip={title:'Saved article',url:'https://example.org/article',content:'# Article\n\nThe previewed content.'};
 const data={destination};
 const directories={default:configured?{name:'clippings',queryPermission:async()=> 'granted'}:undefined};
 const native=vi.fn(async(_app,request)=>{
  if(request.action==='status') return {path:request.destination==='default' || configured?'/Users/Test/Documents/sag-wiki/clippings':'',ready:request.destination==='default' || configured};
  if(request.action==='choose') return cancelled?{cancelled:true}:{path:'/Users/Test/Documents/sag-wiki/clippings',bookmark:'selected-folder'};
  if(request.action==='commit') {configured=true;return {path:'/Users/Test/Documents/sag-wiki/clippings'};}
 });
 const api={
  i18n:{getUILanguage:()=> 'en'},
  storage:{local:{get:async defaults=>typeof defaults==='string'?{language:'en'}:{...defaults,...data},set:vi.fn(async values=>Object.assign(data,values))},onChanged:{addListener:vi.fn()}},
  tabs:{query:vi.fn(async()=>[{id:12}])},
  runtime:{openOptionsPage:vi.fn(),sendNativeMessage:native,sendMessage:vi.fn(async request=>request.type==='preview'?(error?{ok:false,error}:{ok:true,clip}):{ok:true,state:'complete',url:clip.url,path:'clippings/article.md'})},
 };
 vi.resetModules();
 vi.doMock('../browser-extension/src/platform.js',()=>({isSafari:safari}));
 vi.doMock('../browser-extension/src/directory.js',()=>({getDirectory:vi.fn(async kind=>directories[kind]),setDirectory:vi.fn(async(handle,kind)=>directories[kind]=handle)}));
 vi.stubGlobal('window',dom.window);vi.stubGlobal('document',dom.window.document);vi.stubGlobal('chrome',api);
 await import('../browser-extension/src/popup.js');
 return {api,clip,data,directories,element:id=>dom.window.document.querySelector(id)};
}

it('previews the original context-menu tab and saves only after clicking Save',async()=>{
 const ui=await popup();
 expect(ui.api.tabs.query).not.toHaveBeenCalled();
 expect(ui.api.runtime.sendMessage).toHaveBeenCalledExactlyOnceWith({type:'preview',tabId:7});
 expect(ui.element('#page-title').textContent).toBe(ui.clip.title);
 expect(ui.element('#preview').textContent).toBe(ui.clip.content);
 expect(ui.element('#directory').textContent).toContain('Documents/sag-wiki/clippings');
 expect(ui.element('#save').disabled).toBe(false);
 await ui.element('#save').onclick();
 expect(ui.api.runtime.sendMessage).toHaveBeenLastCalledWith({type:'save',clip:ui.clip});
 expect(ui.element('#message').textContent).toBe('Saved to clippings/article.md');
 expect(ui.element('#save').disabled).toBe(true);
});

it('does not save when the preview is cancelled or cannot be extracted',async()=>{
 const ui=await popup({error:'No readable content'});
 expect(ui.element('#save').disabled).toBe(true);
 expect(ui.element('#message').textContent).toBe('No readable content');
 const close=vi.spyOn(dom.window,'close');
 ui.element('#cancel').onclick();
 expect(close).toHaveBeenCalledOnce();
 expect(ui.api.runtime.sendMessage).toHaveBeenCalledTimes(1);
});

it('saves to the Safari default folder on first use without choosing a folder',async()=>{
 const ui=await popup({safari:true,configured:false});
 expect(ui.api.runtime.sendNativeMessage).toHaveBeenCalledWith('com.sag.webclipper',{action:'status',destination:'default'});
 expect(ui.element('#folder-setup').hidden).toBe(true);
 await ui.element('#save').onclick();
 expect(ui.api.runtime.sendNativeMessage.mock.calls.every(([,request])=>request.action==='status')).toBe(true);
 expect(ui.element('#directory').textContent).toContain('/Users/Test/Documents/sag-wiki/clippings');
 expect(ui.element('#folder-setup').hidden).toBe(true);
 expect(ui.api.runtime.sendMessage).toHaveBeenLastCalledWith({type:'save',clip:ui.clip});
 expect(ui.api.runtime.openOptionsPage).not.toHaveBeenCalled();
});

it('keeps the preview and writes nothing when a custom-folder picker is cancelled',async()=>{
 const ui=await popup({safari:true,configured:false,cancelled:true,destination:'folder'});
 await ui.element('#save').onclick();
 expect(ui.api.runtime.sendMessage).toHaveBeenCalledExactlyOnceWith({type:'preview',tabId:7});
 expect(ui.element('#save').disabled).toBe(false);
 expect(ui.element('#message').textContent).toBe('');
 expect(ui.element('#preview').textContent).toBe(ui.clip.content);
});
