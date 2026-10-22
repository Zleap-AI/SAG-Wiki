import {browserAPI as api, localize, errorMessage} from './i18n.js';
import {defaultFolder} from './clipping.js';
import {getDirectory} from './directory.js';
import {isSafari} from './platform.js';
const t = await localize();
const message = document.querySelector('#message');
const save = document.querySelector('#save');
const [tab] = await api.tabs.query({active:true,currentWindow:true});
document.querySelector('#page-title').textContent = tab?.title || '';
document.querySelector('#settings').onclick = () => api.runtime.openOptionsPage();
const settings = await api.storage.local.get({folder:defaultFolder,saveAs:false,lastResult:null,destination:'default',defaultPath:'Documents/sag-wiki/clippings'});
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
function show(result) {
 if(result?.url!==tab?.url) return;
 message.dataset.error=String(result.state==='interrupted');
 if(result.state==='complete') {message.textContent=t('savedTo')+' '+result.path;save.textContent=t('saved');save.disabled=false;}
 else if(result.state==='interrupted') {message.textContent=t('downloadFailed')+(result.error?' '+errorMessage(result.error,t):'');save.textContent=t('save');save.disabled=false;}
 else {message.textContent=t('saving');save.textContent=t('saving');save.disabled=true;}
}
show(settings.lastResult);
api.storage.onChanged.addListener((changes,area)=>{if(area==='local'&&changes.lastResult)show(changes.lastResult.newValue);});
save.onclick = async () => {
 save.disabled=true;save.textContent=t('saving');message.textContent='';message.dataset.error='false';
 try {
  const result = await api.runtime.sendMessage({type:'save',tabId:tab?.id});
  if(!result.ok) throw new Error(result.error);
  show(result);
 } catch(error) {message.textContent=errorMessage(error,t);message.dataset.error='true';save.textContent=t('save');save.disabled=false;}
};
