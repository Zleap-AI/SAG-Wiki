import {get,writable} from 'svelte/store';
import {Channel} from '@tauri-apps/api/core';
import {api} from './api';
import type {UpdateInfo,UpdateProgress,UpdateSettings} from './types';

export const defaultUpdateSettings:UpdateSettings={source:'website',github_url:'https://github.com/Zleap-AI/SAG-Wiki',website_url:'https://zleap.com/update/latest-v2.json',follow_language:true};
export function updateSettingsForLanguage(config:UpdateSettings=defaultUpdateSettings,language='zh-CN'):UpdateSettings{
 return config.follow_language&&config.source==='website'&&config.website_url===defaultUpdateSettings.website_url
  ?{...config,source:language==='zh-CN'?'website':'github'}:config;
}
export const updateKey=(config:UpdateSettings)=>JSON.stringify(config);
export const updateState=writable<{key:string;checking:boolean;info:UpdateInfo|null;error:string}>({key:'',checking:false,info:null,error:''});
export const updateDownload=writable<{version:string;stage:'idle'|'downloading'|'verifying'|'ready'|'installing';downloaded:number;total:number|null;error:string}>({version:'',stage:'idle',downloaded:0,total:null,error:''});
let requestId=0;

export async function checkUpdates(config:UpdateSettings,{automatic=false}={}){
 const key=updateKey(config);
 const state=get(updateState);
 if(get(updateDownload).stage!=='idle')return;
 if(state.key===key&&state.checking)return;
 if(!automatic)updateDownload.update(value=>({...value,error:''}));
 const request=++requestId;
 updateState.set({key,checking:true,info:null,error:''});
 try{
  const info=await api<UpdateInfo>('check_app_update',{config:structuredClone(config)});
  if(request===requestId)updateState.set({key,checking:false,info,error:''});
 }catch(error){
  if(request===requestId)updateState.set({key,checking:false,info:null,error:String(error)});
 }
}

export async function downloadUpdate(config:UpdateSettings,info:UpdateInfo){
 if(get(updateDownload).stage!=='idle')return;
 updateDownload.set({version:info.latest_version,stage:'downloading',downloaded:0,total:null,error:''});
 const onProgress=new Channel<UpdateProgress>();
 onProgress.onmessage=progress=>updateDownload.update(state=>({...state,...progress}));
 try{
  await api('download_app_update',{config:structuredClone(config),version:info.latest_version,onProgress});
  updateDownload.update(state=>({...state,stage:'ready'}));
 }catch(error){updateDownload.update(state=>({...state,stage:'idle',error:String(error)}))}
}

export async function installUpdate(){
 const state=get(updateDownload);
 if(state.stage!=='ready')return;
 updateDownload.update(value=>({...value,stage:'installing',error:''}));
 try{await api('install_app_update',{version:state.version})}
 catch(error){updateDownload.update(value=>({...value,stage:'ready',error:String(error)}))}
}
