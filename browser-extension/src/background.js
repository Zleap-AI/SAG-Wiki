import {browserAPI as api, translator, errorMessage} from './i18n.js';
import {defaultFolder, validateFolder, makeClipping} from './clipping.js';
import {isSafari} from './platform.js';
import {getDirectory} from './directory.js';
async function menus() {
 const t = await translator();
 await api.contextMenus.removeAll();
 api.contextMenus.create({id:'save-page',title:t('save'),contexts:['page','selection'],documentUrlPatterns:['http://*/*','https://*/*'],...(isSafari?{icons:{16:'icon-16.png',32:'icon-32.png'}}:{})});
}
api.runtime.onInstalled.addListener(async () => {
 await api.storage.local.remove(['token','lastResult','lastError','toolbarEnabled']);
 await menus();
});
api.runtime.onStartup.addListener(menus);
api.storage.onChanged.addListener((changes, area) => {
 if(area!=='local') return;
 if(changes.language) void menus();
});
async function report(result) {
 await api.storage.local.set({lastResult:result});
}
async function finishDownload(item) {
 if(item.state !== 'complete' && item.state !== 'interrupted') return;
 const key = `download:${item.id}`;
 const saved = await api.storage.local.get(key);
 if(!saved[key]) return;
 await report({...saved[key],state:item.state,path:item.filename,error:item.error});
 await api.storage.local.remove(key);
}
if(!isSafari) api.downloads.onChanged.addListener(async delta => {
 if(!delta.state) return;
 const [item] = await api.downloads.search({id:delta.id});
 if(item) await finishDownload(item);
});
async function preview(tabId) {
 const tab=await api.tabs.get(tabId);
 const t = await translator();
 if (!tab?.id || !/^https?:\/\//.test(tab.url)) throw new Error(t('invalidPage'));
 const results = await api.scripting.executeScript({target:{tabId:tab.id},files:['extract.js']});
 const clip = results[0]?.result;
 if (!clip?.content?.trim()) throw new Error(t('empty'));
 return clip;
}
async function save(clip) {
 const t = await translator();
 if(typeof clip?.title!=='string' || typeof clip.url!=='string' || !/^https?:\/\//.test(clip.url)) throw new Error(t('invalidPage'));
 if(typeof clip.content!=='string' || !clip.content.trim()) throw new Error(t('empty'));
 const settings = await api.storage.local.get({folder:defaultFolder,saveAs:false,destination:'default'});
 let folder;
 try {folder=validateFolder(settings.folder);} catch {throw new Error(t('invalidFolder'));}
 const {filename,content} = makeClipping(clip);
 await api.storage.local.remove('lastError');
 if(isSafari) {
  const result = await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'save',folder,filename,content,destination:settings.destination,useDownloads:settings.destination==='downloads'});
  if(result.error) throw new Error(errorMessage(result,t));
  const saved = {state:'complete',url:clip.url,path:result.path};
  await report(saved);
  return saved;
 }
 if(settings.destination!=='downloads') {
  const handle=await getDirectory(settings.destination);
  if(!handle || await handle.queryPermission({mode:'readwrite'})!=='granted') throw new Error(t('folderRequired'));
  try {await handle.getFileHandle(filename);throw new Error(t('fileExists'));}
  catch(error) {if(error.name!=='NotFoundError')throw error;}
  const file=await handle.getFileHandle(filename,{create:true});
  const writer=await file.createWritable();
  await writer.write(content);await writer.close();
  const saved={state:'complete',url:clip.url,path:`${handle.name}/${filename}`};
  await report(saved);return saved;
 }
 const path = folder ? `${folder}/${filename}` : filename;
 const id = await api.downloads.download({url:`data:text/markdown;charset=utf-8,${encodeURIComponent(content)}`,filename:path,conflictAction:'uniquify',saveAs:settings.saveAs});
 const pending = {state:'in_progress',url:clip.url,path};
 await api.storage.local.set({[`download:${id}`]:pending});
 await report(pending);
 // The download may have completed before its record was saved.
 const [item] = await api.downloads.search({id});
 if(item) await finishDownload(item);
 return (await api.storage.local.get('lastResult')).lastResult;
}
async function fail(error) {
 await api.storage.local.set({lastError:String(error.message)});
}
async function openPreview(tab) {
 try {
  const source=await api.windows.get(tab.windowId);
  const width=520,height=660;
  await api.windows.create({url:api.runtime.getURL(`/popup.html?tabId=${tab.id}`),type:'popup',width,height,left:Math.round(source.left+(source.width-width)/2),top:Math.round(source.top+(source.height-height)/2)});
 }
 catch(error) {await fail(error);}
}
api.contextMenus.onClicked.addListener(async (info,tab) => {
 if(info.menuItemId === 'save-page') await openPreview(tab);
});
api.runtime.onMessage.addListener((message, sender, respond) => {
 if(!['preview','save'].includes(message.type) || !sender.url?.startsWith(api.runtime.getURL('/'))) return false;
 (async () => {
  try {respond(message.type==='preview'?{ok:true,clip:await preview(message.tabId)}:{ok:true,...await save(message.clip)});}
  catch(error) {await fail(error);respond({ok:false,error:String(error.message)});}
 })();
 return true;
});
