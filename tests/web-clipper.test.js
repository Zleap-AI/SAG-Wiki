import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {expect,it,vi} from 'vitest';

it('extracts Markdown from the displayed DOM and keeps source links without altering the page',()=>{
 const html=`<!doctype html><html><head><title>Local clipping test</title></head><body><nav>Site navigation</nav><article><h1>Local clipping test</h1><p>This article explains how a browser saves knowledge as a local Markdown document. This sentence provides enough article content for extraction and preserves the original document.</p><h2>A linked section</h2><p>Read the <a href="/reference">reference document</a> for implementation details. The reference is connected to the original page using a regular hyperlink.</p><pre><code class="language-js">const value = 42;</code></pre></article><script>window.secret = 'page-script';</script></body></html>`;
 const dom=new JSDOM(html,{url:'https://example.org/article',runScripts:'outside-only'});
 const before=dom.window.document.documentElement.outerHTML;
 const clip=dom.window.eval(readFileSync('browser-extension/build/chrome/extract.js','utf8'));
 expect(clip.title).toBe('Local clipping test');
 expect(clip.url).toBe('https://example.org/article');
 expect(clip.content).toContain('## A linked section');
 expect(clip.content).toContain('[reference document](https://example.org/reference)');
 expect(clip.content).toContain('const value = 42;');
 expect(clip.content).not.toContain('window.secret');
 expect(dom.window.document.documentElement.outerHTML).toBe(before);
 dom.window.close();
});


function extension(browser, options={}) {
 let receiver=()=>{}, downloadChanged=()=>{}, contextClicked=()=>{}, storageChanged=()=>{};
 const data={folder:'clippings',saveAs:false,destination:'downloads',language:options.language || 'en',toolbarEnabled:options.toolbarEnabled || false};
 const listeners={addListener:vi.fn()};
 let state=options.state || 'in_progress';
 const storage={
  get:async key=>typeof key==='string'?{[key]:data[key]}:Array.isArray(key)?Object.fromEntries(key.map(k=>[k,data[k]])):{...key,...data},
  set:vi.fn(async value=>Object.assign(data,value)),
  remove:vi.fn(async keys=>{for(const key of Array.isArray(keys)?keys:[keys])delete data[key];})
 };
 const api={
  i18n:{getUILanguage:()=> 'en'},
  storage:{local:storage,onChanged:{addListener:fn=>storageChanged=fn}},
  runtime:{getURL:path=>'chrome-extension://test'+path,onInstalled:listeners,onStartup:listeners,onMessage:{addListener:fn=>receiver=fn},openOptionsPage:vi.fn(),sendNativeMessage:vi.fn(async()=>options.nativeResult || {path:'/Users/test/Downloads/网页剪辑/article.md'})},
  contextMenus:{onClicked:{addListener:fn=>contextClicked=fn},removeAll:vi.fn(),create:vi.fn()},
  windows:{get:vi.fn(async()=>options.window || {left:100,top:50,width:1800,height:1180}),create:vi.fn()},
  action:{enable:vi.fn(),disable:vi.fn(),setTitle:vi.fn(),setBadgeText:vi.fn(),onClicked:{addListener:vi.fn()}},
  tabs:{get:async()=>({id:7,url:'https://example.org/article'})},
  scripting:{executeScript:vi.fn(async()=>[{result:{title:'Article',url:'https://example.org/article',content:'# Article'}}])}
 };
 if(browser==='chrome')api.downloads={download:vi.fn(async()=>42),search:vi.fn(async()=>[{id:42,state,filename:'/Downloads/网页剪辑/article.md',error:state==='interrupted'?'USER_CANCELED':undefined}]),onChanged:{addListener:fn=>downloadChanged=fn}};
 const fetch=vi.fn(()=>{throw new Error('No server should be contacted');});
 new Function('globalThis','fetch',readFileSync(`browser-extension/build/${browser}/background.js`,'utf8'))({chrome:api},fetch);
 const message=request=>new Promise(resolve=>receiver(request,{url:'chrome-extension://test/popup.html'},resolve));
 return {api,data,fetch,message,send:async()=>{const result=await message({type:'preview',tabId:7});return message({type:'save',clip:result.clip});},open:()=>contextClicked({menuItemId:'save-page'},{id:7,windowId:3}),change:storageChanged,complete:async()=>{state='complete';await downloadChanged({id:42,state:{current:'complete'}});}};
}

for(const browser of ['chrome','safari'])it(`${browser} opens a preview from the context menu without saving`,async()=>{
 const ext=extension(browser);
 await ext.open();
 expect(ext.api.windows.create).toHaveBeenCalledWith({url:'chrome-extension://test/popup.html?tabId=7',type:'popup',width:520,height:660,left:740,top:310});
 expect(ext.api.windows.get).toHaveBeenCalledWith(3);
 const result=await ext.message({type:'preview',tabId:7});
 expect(result).toMatchObject({ok:true,clip:{title:'Article',content:'# Article'}});
 expect(ext.api.runtime.sendNativeMessage).not.toHaveBeenCalled();
 if(browser==='chrome')expect(ext.api.downloads.download).not.toHaveBeenCalled();
 expect(ext.data.lastResult).toBeUndefined();
});

it('centers the preview on its source window on a secondary display',async()=>{
 const ext=extension('safari',{window:{left:-1920,top:100,width:1600,height:1000}});
 await ext.open();
 expect(ext.api.windows.create).toHaveBeenCalledWith(expect.objectContaining({left:-1380,top:270,width:520,height:660}));
});

it('adds the small brand icon to the Safari context menu without using unsupported Chrome properties',async()=>{
 for(const browser of ['chrome','safari']) {
  const ext=extension(browser);
  await ext.api.runtime.onInstalled.addListener.mock.calls[0][0]();
  const item=ext.api.contextMenus.create.mock.calls[0][0];
  expect(item.id).toBe('save-page');
  if(browser==='safari')expect(item.icons).toEqual({16:'icon-16.png',32:'icon-32.png'});
  else expect(item.icons).toBeUndefined();
 }
});

it('saves the preview snapshot even if the original page changes',async()=>{
 const ext=extension('chrome');
 const {clip}=await ext.message({type:'preview',tabId:7});
 ext.api.scripting.executeScript.mockResolvedValue([{result:{title:'Changed page',url:'https://example.org/changed',content:'Changed content'}}]);
 expect(await ext.message({type:'save',clip})).toMatchObject({ok:true,url:'https://example.org/article'});
 const text=decodeURIComponent(ext.api.downloads.download.mock.calls[0][0].url.split(',').slice(1).join(','));
 expect(text).toContain('# Article');
 expect(text).not.toContain('Changed content');
 expect(ext.api.scripting.executeScript).toHaveBeenCalledTimes(1);
});

it('saves through Chrome downloads without pairing and only reports success after completion',async()=>{
 const ext=extension('chrome');
 const pending=await ext.send();
 expect(pending).toMatchObject({ok:true,state:'in_progress'});
 const request=ext.api.downloads.download.mock.calls[0][0];
 expect(request.filename).toMatch(/^clippings\/.+\.md$/);
 expect(request).toMatchObject({conflictAction:'uniquify',saveAs:false});
 const text=decodeURIComponent(request.url.split(',').slice(1).join(','));
 expect(text).toContain('source: "https://example.org/article"');
 expect(text).toContain('# Article');
 await ext.complete();
 expect(ext.data.lastResult).toMatchObject({state:'complete',path:'/Downloads/网页剪辑/article.md'});
 expect(ext.fetch).not.toHaveBeenCalled();
 expect(ext.api.runtime.sendNativeMessage).not.toHaveBeenCalled();
});

it('passes location choice to Chrome and reports cancellation in the preview result',async()=>{
 const ext=extension('chrome',{state:'interrupted'});
 ext.data.saveAs=true;
 await ext.send();
 expect(ext.api.downloads.download.mock.calls[0][0].saveAs).toBe(true);
 expect(ext.data.lastResult).toMatchObject({state:'interrupted',error:'USER_CANCELED'});
});

it('rejects absolute or parent paths before starting a download',async()=>{
 const ext=extension('chrome');
 ext.data.folder='../outside';
 expect(await ext.send()).toMatchObject({ok:false});
 expect(ext.api.downloads.download).not.toHaveBeenCalled();
});

it('uses the bundled Safari extension helper without SAG Wiki or an HTTP connection',async()=>{
 const ext=extension('safari');
 expect(await ext.send()).toMatchObject({ok:true,state:'complete',path:'/Users/test/Downloads/网页剪辑/article.md'});
 expect(ext.api.runtime.sendNativeMessage).toHaveBeenCalledWith('com.sag.webclipper',expect.objectContaining({action:'save',folder:'clippings',content:expect.stringContaining('# Article')}));
 expect(ext.fetch).not.toHaveBeenCalled();
});


it('translates native save errors into the chosen extension language',async()=>{
 const ext=extension('safari',{language:'zh-CN',nativeResult:{error:'File already exists.',code:'file-exists'}});
 expect(await ext.send()).toMatchObject({ok:false,error:'文件已存在，请重新保存以生成新文件名。'});
 expect(ext.data.lastError).toBe('文件已存在，请重新保存以生成新文件名。');
});

it('removes the toolbar entry and clears its obsolete setting on upgrade',async()=>{
 for(const browser of ['chrome','safari']) {
  const manifest=JSON.parse(readFileSync(`browser-extension/build/${browser}/manifest.json`,'utf8'));
  expect(manifest.action).toBeUndefined();
  const ext=extension(browser);
  expect(ext.api.action.enable).not.toHaveBeenCalled();
  expect(ext.api.action.disable).not.toHaveBeenCalled();
  expect(ext.api.action.onClicked.addListener).not.toHaveBeenCalled();
  ext.data.toolbarEnabled=true;
  await ext.api.runtime.onInstalled.addListener.mock.calls[0][0]();
  expect(ext.data.toolbarEnabled).toBeUndefined();
  expect(ext.api.windows.create).not.toHaveBeenCalled();
  expect(ext.api.runtime.openOptionsPage).not.toHaveBeenCalled();
 }
});
