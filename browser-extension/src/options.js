import {browserAPI as api, localize, errorMessage} from './i18n.js';
import {defaultFolder,validateFolder} from './clipping.js';
import {isSafari} from './platform.js';
import {getDirectory,setDirectory} from './directory.js';
let t = await localize();
const message = document.querySelector('#message');
const folder = document.querySelector('#directory');
const language = document.querySelector('#language');
const saveAs = document.querySelector('#save-as');
const destination = document.querySelector('#destination');
const choose = document.querySelector('#choose-folder');
const notice = document.querySelector('#folder-notice');
const save = document.querySelector('#apply');
const saved = await api.storage.local.get({folder:defaultFolder,saveAs:false,language:'',lastError:'',destination:'default',defaultPath:'Documents/sag-wiki/clippings'});
const paths = {default:saved.defaultPath,folder:''};
const handles = {};
const pending = {};
const changed = {};
destination.value=saved.destination;folder.value=saved.folder;language.value=saved.language;saveAs.checked=saved.saveAs;
document.querySelector('#save-as-field').hidden=isSafari;
function render() {
 destination.querySelector('[value="default"]').textContent=`${t('defaultDestination')} (${paths.default})`;
 destination.querySelector('[value="folder"]').textContent=paths.folder?`${t('selectedFolder')} (${paths.folder})`:t('otherFolder');
 choose.hidden=destination.value!=='folder';
 choose.textContent=t(paths.folder?'changeFolder':'chooseFolder');
 document.querySelector('#download-options').hidden=destination.value!=='downloads';
 document.querySelector('#folder-help').textContent=t(isSafari?'safariFolderHelp':'folderHelp');
}
render();
destination.onchange=()=>{notice.hidden=true;message.textContent='';render();};
async function pickFolder(kind) {
 if(isSafari) {
  const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'choose',destination:kind,language:document.documentElement.lang,persist:false});
  if(result.error) throw new Error(errorMessage(result,t));
  if(result.cancelled) return false;
  changed[kind]=result.path!==paths[kind];pending[kind]=result.bookmark;paths[kind]=result.path;
 } else {
  const selected=await window.showDirectoryPicker({id:`sag-clippings-${kind}`,mode:'readwrite',startIn:kind==='folder' && handles.folder?handles.folder:'documents'});
  changed[kind]=kind==='default' || !handles.folder || !await handles.folder.isSameEntry(selected);
  pending[kind]=selected;
  paths[kind]=kind==='default'?`${selected.name}/sag-wiki/clippings`:selected.name;
 }
 render();return true;
}
choose.onclick=async()=>{
 message.textContent='';message.dataset.error='false';notice.hidden=true;
 try{await pickFolder(destination.value);}
 catch(error){if(error.name!=='AbortError'){message.textContent=errorMessage(error,t);message.dataset.error='true';}}
};
language.onchange=async()=>{t=await localize(language.value);render();message.textContent='';};
save.onclick=async()=>{
 message.textContent='';message.dataset.error='false';save.disabled=true;
 const kind=destination.value;
 try {
  const directory=validateFolder(folder.value);
  if(kind!=='downloads' && !(isSafari && kind==='default')) {
   const missing=isSafari?!paths[kind] || (kind==='default' && !handles.default):!handles[kind] || await handles[kind].queryPermission({mode:'readwrite'})!=='granted';
   if(!pending[kind] && missing && !await pickFolder(kind)) return;
   if(pending[kind]) {
    if(isSafari) {
     const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'commit',destination:kind,bookmark:pending[kind]});
     if(result.error) throw new Error(errorMessage(result,t));
     paths[kind]=result.path;handles[kind]=true;
    } else {
     const handle=kind==='default'?await(await pending[kind].getDirectoryHandle('sag-wiki',{create:true})).getDirectoryHandle('clippings',{create:true}):pending[kind];
     await setDirectory(handle,kind);handles[kind]=handle;
    }
    delete pending[kind];
   }
  }
  const wasChanged=kind==='folder'?(saved.destination!==kind || changed.folder):kind==='downloads'&&(saved.destination!==kind || directory!==saved.folder);
  const updates={destination:kind,language:language.value,folder:directory,saveAs:isSafari?false:saveAs.checked,defaultPath:kind==='default'?paths.default:saved.defaultPath};
  await api.storage.local.set(updates);Object.assign(saved,updates);changed[kind]=false;
  folder.value=directory;notice.hidden=!wasChanged;message.textContent=t('folderSaved');render();
 } catch(error) {
  if(error.name!=='AbortError'){message.textContent=error.message==='invalidFolder'?t('invalidFolder'):errorMessage(error,t);message.dataset.error='true';}
 } finally {save.disabled=false;}
};
try {
 for(const kind of ['default','folder']) {
  if(isSafari) {
   const result=await api.runtime.sendNativeMessage('com.sag.webclipper',{action:'status',destination:kind});
   if(result.error) throw new Error(errorMessage(result,t));
   if(result.path){paths[kind]=result.path;handles[kind]=result.ready!==false;}
  } else {
   handles[kind]=await getDirectory(kind);
   if(kind==='folder') paths.folder=handles.folder?.name || '';
  }
 }
 render();
} catch(error){message.textContent=errorMessage(error,t);message.dataset.error='true';}
if(saved.lastError){message.textContent=saved.lastError;message.dataset.error='true';await api.storage.local.remove('lastError');}
