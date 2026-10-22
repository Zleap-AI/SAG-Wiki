import {browserAPI as api, localize, errorMessage} from './i18n.js';
import {defaultFolder} from './clipping.js';
import {getDirectory} from './directory.js';
import {isSafari} from './platform.js';
const t = await localize();
const message = document.querySelector('#message');
const save = document.querySelector('#save');
const preview = document.querySelector('#preview');
const sourceTabId=new URLSearchParams(window.location.search).get('tabId');
if(sourceTabId) document.body.classList.add('dialog');
document.querySelector('#cancel').onclick=()=>window.close();
document.querySelector('#settings').onclick=()=>api.runtime.openOptionsPage();
let clip;
let saving=false;
async function showLocation() {
 const settings=await api.storage.local.get({folder:defaultFolder,saveAs:false,destination:'default',defaultPath:'Documents/sag-wiki/clippings'});
 let location;
 if(settings.destination==='default') location=`${t('defaultDestination')} (${settings.defaultPath})`;
 else if(settings.destination==='folder') {
  let path;
  if(isSafari) {
   const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'status',destination:'folder'});
   if(result.error) throw new Error(errorMessage(result,t));
   path=result.path;
  } else path=(await getDirectory('folder'))?.name;
  location=path?`${t('selectedFolder')} (${path})`:t('noFolder');
 } else location=settings.saveAs?t('chooseEachTime'):`${t('downloads')}${settings.folder?'/'+settings.folder:''}`;
 document.querySelector('#directory').textContent=location;
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
 save.disabled=true;save.textContent=t('saving');saving=true;message.textContent='';message.dataset.error='false';
 try {
  const result=await api.runtime.sendMessage({type:'save',clip});
  if(!result.ok) throw new Error(result.error);
  show(result);
 } catch(error) {showError(error);save.textContent=t('confirmSave');save.disabled=false;saving=false;}
};
preview.textContent=t('loading');
try {
 const tabId=sourceTabId?Number(sourceTabId):(await api.tabs.query({active:true,currentWindow:true}))[0]?.id;
 await showLocation();
 const result=await api.runtime.sendMessage({type:'preview',tabId});
 if(!result.ok) throw new Error(result.error);
 clip=result.clip;
 document.querySelector('#page-title').textContent=clip.title;
 document.querySelector('#page-source').textContent=clip.url;
 preview.textContent=clip.content;save.disabled=false;
} catch(error) {preview.textContent='';showError(error);}
finally {preview.setAttribute('aria-busy','false');}
