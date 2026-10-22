import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {afterEach,expect,it,vi} from 'vitest';

let dom;
afterEach(()=>{vi.unstubAllGlobals();vi.resetModules();dom?.window.close();});

async function popup({query='?tabId=7',error}={}) {
 dom=new JSDOM(readFileSync('browser-extension/static/popup.html','utf8'),{url:`https://extension.test/popup.html${query}`});
 const clip={title:'Saved article',url:'https://example.org/article',content:'# Article\n\nThe previewed content.'};
 const api={
  i18n:{getUILanguage:()=> 'en'},
  storage:{local:{get:async defaults=>typeof defaults==='string'?{language:'en'}:{...defaults}},onChanged:{addListener:vi.fn()}},
  tabs:{query:vi.fn(async()=>[{id:12}])},
  runtime:{openOptionsPage:vi.fn(),sendMessage:vi.fn(async request=>request.type==='preview'?(error?{ok:false,error}:{ok:true,clip}):{ok:true,state:'complete',url:clip.url,path:'clippings/article.md'})},
 };
 vi.resetModules();
 vi.doMock('../browser-extension/src/platform.js',()=>({isSafari:false}));
 vi.doMock('../browser-extension/src/directory.js',()=>({getDirectory:vi.fn()}));
 vi.stubGlobal('window',dom.window);vi.stubGlobal('document',dom.window.document);vi.stubGlobal('chrome',api);
 await import('../browser-extension/src/popup.js');
 return {api,clip,element:id=>dom.window.document.querySelector(id)};
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

it('uses the active webpage when opened from the toolbar',async()=>{
 const ui=await popup({query:''});
 expect(ui.api.tabs.query).toHaveBeenCalledWith({active:true,currentWindow:true});
 expect(ui.api.runtime.sendMessage).toHaveBeenCalledExactlyOnceWith({type:'preview',tabId:12});
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
