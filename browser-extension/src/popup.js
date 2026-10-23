import {browserAPI as api, localize, errorMessage} from './i18n.js';
import {defaultFolder} from './clipping.js';
import {getDirectory,setDirectory} from './directory.js';
import {isSafari} from './platform.js';
const t = await localize();
const message = document.querySelector('#message');
const save = document.querySelector('#save');
const preview = document.querySelector('#preview');
const sourceTabId=Number(new URLSearchParams(window.location.search).get('tabId'));
document.querySelector('#cancel').onclick=()=>window.close();
document.querySelector('#settings').onclick=()=>api.runtime.openOptionsPage();
let clip;
let saving=false;
let destination;
let directoryReady=false;
let directoryHandle;
async function showLocation() {
 const settings=await api.storage.local.get({folder:defaultFolder,saveAs:false,destination:'default',defaultPath:'Documents/sag-wiki/clippings'});
 destination=settings.destination;
 directoryReady=destination==='downloads';
 let location;
 if(destination!=='downloads') {
  let path;
  if(isSafari) {
   const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'status',destination});
   if(result.error) throw new Error(errorMessage(result,t));
   path=result.path;
   directoryReady=Boolean(path) && result.ready!==false;
  } else {
   directoryHandle=await getDirectory(destination);
   path=directoryHandle?.name;
   directoryReady=Boolean(directoryHandle) && await directoryHandle.queryPermission({mode:'readwrite'})==='granted';
  }
  location=destination==='default'?`${t('defaultDestination')} (${isSafari && path?path:settings.defaultPath})`:path?`${t('selectedFolder')} (${path})`:t('noFolder');
 } else location=settings.saveAs?t('chooseEachTime'):`${t('downloads')}${settings.folder?'/'+settings.folder:''}`;
 document.querySelector('#directory').textContent=location;
 document.querySelector('#folder-setup').hidden=directoryReady;
}
async function chooseSaveFolder() {
 let path;
 if(isSafari) {
  const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'choose',destination,language:document.documentElement.lang,persist:false});
  if(result.error) throw new Error(errorMessage(result,t));
  if(result.cancelled) return false;
  const committed=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'commit',destination,bookmark:result.bookmark});
  if(committed.error) throw new Error(errorMessage(committed,t));
  path=committed.path;
 } else {
  const selected=await window.showDirectoryPicker({id:`sag-clippings-${destination}`,mode:'readwrite',startIn:directoryHandle || 'documents'});
  const handle=destination==='default'?await(await selected.getDirectoryHandle('sag-wiki',{create:true})).getDirectoryHandle('clippings',{create:true}):selected;
  await setDirectory(handle,destination);
  path=`${selected.name}/sag-wiki/clippings`;
 }
 if(destination==='default') await api.storage.local.set({defaultPath:path});
 await showLocation();
 return true;
}
function showError(error) {
 message.textContent=errorMessage(error,t);message.dataset.error='true';
}
function show(result) {
 if(!saving || result?.url!==clip.url) return;
 message.dataset.error=String(result.state==='interrupted');
 if(result.state==='complete') {message.textContent=t('savedTo')+' '+result.path;save.textContent=t('saved');saving=false;}
 else if(result.state==='interrupted') {message.textContent=t('downloadFailed')+(result.error?' '+errorMessage(result.error,t):'');save.textContent=t('confirmSave');save.disabled=false;saving=false;}
 else {message.textContent=t('saving');save.textContent=t('saving');}
}
api.storage.onChanged.addListener((changes,area)=>{if(area==='local'&&changes.lastResult)show(changes.lastResult.newValue);});
window.addEventListener('focus',()=>{showLocation().catch(showError);});
save.onclick=async()=>{
 save.disabled=true;message.textContent='';message.dataset.error='false';
 try {
  if(!directoryReady) {
   save.textContent=t('chooseFolder');
   if(!await chooseSaveFolder()) {save.textContent=t('confirmSave');save.disabled=false;return;}
  }
  save.textContent=t('saving');saving=true;
  const result=await api.runtime.sendMessage({type:'save',clip});
  if(!result.ok) throw new Error(result.error);
  show(result);
 } catch(error) {if(error.name!=='AbortError') showError(error);save.textContent=t('confirmSave');save.disabled=false;saving=false;}
};
preview.textContent=t('loading');
try {
 await showLocation();
 const result=await api.runtime.sendMessage({type:'preview',tabId:sourceTabId});
 if(!result.ok) throw new Error(result.error);
 clip=result.clip;
 document.querySelector('#page-title').textContent=clip.title;
 document.querySelector('#page-source').textContent=clip.url;
 preview.textContent=clip.content;save.disabled=false;
} catch(error) {preview.textContent='';showError(error);}
finally {preview.setAttribute('aria-busy','false');}
