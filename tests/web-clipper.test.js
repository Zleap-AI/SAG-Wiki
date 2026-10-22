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
 let receiver=()=>{}, downloadChanged=()=>{};
 const data={folder:'clippings',saveAs:false,destination:'downloads',language:options.language || 'en'};
 const listeners={addListener:vi.fn()};
 let state=options.state || 'in_progress';
 const storage={
  get:async key=>typeof key==='string'?{[key]:data[key]}:Array.isArray(key)?Object.fromEntries(key.map(k=>[k,data[k]])):{...key,...data},
  set:vi.fn(async value=>Object.assign(data,value)),
  remove:vi.fn(async keys=>{for(const key of Array.isArray(keys)?keys:[keys])delete data[key];})
 };
 const api={
  i18n:{getUILanguage:()=> 'en'},
  storage:{local:storage,onChanged:listeners},
  runtime:{getURL:path=>'chrome-extension://test'+path,onInstalled:listeners,onStartup:listeners,onMessage:{addListener:fn=>receiver=fn},openOptionsPage:vi.fn(),sendNativeMessage:vi.fn(async()=>options.nativeResult || {path:'/Users/test/Downloads/网页剪辑/article.md'})},
  contextMenus:{onClicked:listeners,removeAll:vi.fn(),create:vi.fn()},
  action:{setBadgeText:vi.fn(),setBadgeBackgroundColor:vi.fn(),setTitle:vi.fn()},
  tabs:{get:async()=>({id:7,url:'https://example.org/article'})},
  scripting:{executeScript:vi.fn(async()=>[{result:{title:'Article',url:'https://example.org/article',content:'# Article'}}])}
 };
 if(browser==='chrome')api.downloads={download:vi.fn(async()=>42),search:vi.fn(async()=>[{id:42,state,filename:'/Downloads/网页剪辑/article.md',error:state==='interrupted'?'USER_CANCELED':undefined}]),onChanged:{addListener:fn=>downloadChanged=fn}};
 const fetch=vi.fn(()=>{throw new Error('No server should be contacted');});
 new Function('globalThis','fetch',readFileSync(`browser-extension/build/${browser}/background.js`,'utf8'))({chrome:api},fetch);
 return {api,data,fetch,send:()=>new Promise(resolve=>receiver({type:'save',tabId:7},{url:'chrome-extension://test/popup.html'},resolve)),complete:async()=>{state='complete';await downloadChanged({id:42,state:{current:'complete'}});}};
}

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
 expect(ext.api.action.setBadgeText).toHaveBeenLastCalledWith({text:'…'});
 await ext.complete();
 expect(ext.api.action.setBadgeText).toHaveBeenLastCalledWith({text:'✓'});
 expect(ext.data.lastResult).toMatchObject({state:'complete',path:'/Downloads/网页剪辑/article.md'});
 expect(ext.fetch).not.toHaveBeenCalled();
 expect(ext.api.runtime.sendNativeMessage).not.toHaveBeenCalled();
});

it('passes location choice to Chrome and reports cancellation without a success badge',async()=>{
 const ext=extension('chrome',{state:'interrupted'});
 ext.data.saveAs=true;
 await ext.send();
 expect(ext.api.downloads.download.mock.calls[0][0].saveAs).toBe(true);
 expect(ext.data.lastResult).toMatchObject({state:'interrupted',error:'USER_CANCELED'});
 expect(ext.api.action.setBadgeText).toHaveBeenLastCalledWith({text:'!'});
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
 expect(ext.api.action.setBadgeText).toHaveBeenLastCalledWith({text:'✓'});
});


it('translates native save errors into the chosen extension language',async()=>{
 const ext=extension('safari',{language:'zh-CN',nativeResult:{error:'File already exists.',code:'file-exists'}});
 expect(await ext.send()).toMatchObject({ok:false,error:'文件已存在，请重新保存以生成新文件名。'});
 expect(ext.api.action.setBadgeText).toHaveBeenLastCalledWith({text:'!'});
});

it('translates download errors in the toolbar status',async()=>{
 const ext=extension('chrome',{language:'zh-CN',state:'interrupted'});
 await ext.send();
 expect(ext.api.action.setTitle).toHaveBeenLastCalledWith({title:'下载失败。 保存已取消。'});
});
