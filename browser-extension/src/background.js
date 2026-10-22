import {browserAPI as api, translator, errorMessage} from './i18n.js';
import {defaultFolder, validateFolder, makeClipping} from './clipping.js';
import {isSafari} from './platform.js';
import {getDirectory} from './directory.js';
async function menus() {
 const t = await translator();
 if(!isSafari) await api.action.setBadgeBackgroundColor({color:'#000000'});
 await api.contextMenus.removeAll();
 api.contextMenus.create({id:'save-page',title:t('save'),contexts:['page','selection'],documentUrlPatterns:['http://*/*','https://*/*']});
}
api.runtime.onInstalled.addListener(async () => {
 await api.storage.local.remove(['token','lastResult','lastError']);
 await menus();
});
api.runtime.onStartup.addListener(menus);
api.storage.onChanged.addListener((changes, area) => {if(area === 'local' && changes.language) void menus();});
async function report(result) {
 const t = await translator();
 await api.storage.local.set({lastResult:result});
 await api.action.setBadgeText({text:result.state==='complete'?'✓':result.state==='interrupted'?'!':'…'});
 await api.action.setTitle({title:result.state==='complete'?`${t('savedTo')} ${result.path}`:result.state==='interrupted'?t('downloadFailed')+(result.error?' '+errorMessage(result.error,t):''):t('saving')});
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
async function save(tab) {
 const t = await translator();
 if (!tab?.id || !/^https?:\/\//.test(tab.url)) throw new Error(t('invalidPage'));
 await api.action.setBadgeText({text:'…'});
 const results = await api.scripting.executeScript({target:{tabId:tab.id},files:['extract.js']});
 const clip = results[0]?.result;
 if (!clip?.content?.trim()) throw new Error(t('empty'));
 const settings = await api.storage.local.get({folder:defaultFolder,saveAs:false,destination:'default'});
 let folder;
 try {folder=validateFolder(settings.folder);} catch {throw new Error(t('invalidFolder'));}
 const {filename,content} = makeClipping(clip);
 await api.storage.local.remove('lastError');
 if(isSafari) {
  const result = await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'save',folder,filename,content,destination:settings.destination,useDownloads:settings.destination==='downloads'});
  if(result.error) throw new Error(errorMessage(result,t));
  const saved = {state:'complete',url:tab.url,path:result.path};
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
  const saved={state:'complete',url:tab.url,path:`${handle.name}/${filename}`};
  await report(saved);return saved;
 }
 const path = folder ? `${folder}/${filename}` : filename;
 const id = await api.downloads.download({url:`data:text/markdown;charset=utf-8,${encodeURIComponent(content)}`,filename:path,conflictAction:'uniquify',saveAs:settings.saveAs});
 const pending = {state:'in_progress',url:tab.url,path};
 await api.storage.local.set({[`download:${id}`]:pending});
 await report(pending);
 // The download may have completed before its record was saved.
 const [item] = await api.downloads.search({id});
 if(item) await finishDownload(item);
 return (await api.storage.local.get('lastResult')).lastResult;
}
async function fail(error) {
 await api.action.setBadgeText({text:'!'});
 await api.action.setTitle({title:String(error.message)});
 await api.storage.local.set({lastError:String(error.message)});
}
api.contextMenus.onClicked.addListener(async (info,tab) => {
 if(info.menuItemId !== 'save-page') return;
 try { await save(tab); }
 catch(error) {await fail(error);await api.runtime.openOptionsPage();}
});
api.runtime.onMessage.addListener((message, sender, respond) => {
 if(message.type !== 'save' || !sender.url?.startsWith(api.runtime.getURL('/'))) return false;
 (async () => {
  try {respond({ok:true,...await save(await api.tabs.get(message.tabId))});}
  catch(error) {await fail(error);respond({ok:false,error:String(error.message)});}
 })();
 return true;
});
