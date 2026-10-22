import {get,writable} from 'svelte/store';
import {api} from './api';
import type {DoclingProfile,DoclingStatus} from './types';

export const doclingStatus=writable<DoclingStatus|null>(null);
export const doclingDownloadOpen=writable(false);
export type DoclingInstallRequest={profiles:DoclingProfile[];formats:string[];apply?:(valid:()=>boolean)=>Promise<void>;valid:boolean;pending:boolean;started:boolean};
export const doclingInstallRequest=writable<DoclingInstallRequest|null>(null);
const legacyMedia=new Set(['wav','mp3','m4a','aac','ogg','flac','mp4','avi','mov','mkv','webm']);
export function profileForExtension(status:DoclingStatus,extension:string):DoclingProfile|undefined{
 if(status.profiles?.length)return status.profiles.find(profile=>(profile.id==='documents'||profile.id==='media')&&profile.extensions.includes(extension))?.id;
 return legacyMedia.has(extension)?'media':'documents';
}
export function profileInstalled(status:DoclingStatus,profile:DoclingProfile){
 return status.profiles?.length?status.profiles.some(item=>item.id===profile&&item.installed):profile==='documents'&&status.installed;
}
export function documentExtensions(status:DoclingStatus){
 return status.formats.flatMap(([,extensions])=>extensions).filter(extension=>profileForExtension(status,extension)==='documents');
}
export function openDoclingProgress(){
 const status=get(doclingStatus),current=get(doclingInstallRequest);
 const reported=status?.requested_profiles;
 const matches=!reported?.length||!!current&&reported.length===current.profiles.length&&reported.every(profile=>current.profiles.includes(profile));
 const unaccepted=current?.valid&&!current.started&&!status?.running;
 if(!current?.pending&&!unaccepted&&(!current?.started||!matches)){
  if(current){current.valid=false;current.apply=undefined;}
  doclingInstallRequest.set({profiles:reported?.length?[...reported]:['documents'],formats:[],valid:false,pending:false,started:true});
 }
 doclingDownloadOpen.set(true);
}
export function openDoclingDownload(profiles:DoclingProfile[],formats:string[],apply:(valid:()=>boolean)=>Promise<void>){
 if(get(doclingStatus)?.running||get(doclingInstallRequest)?.pending){openDoclingProgress();return()=>{};}
 const previous=get(doclingInstallRequest);if(previous){previous.valid=false;previous.apply=undefined;}
 const context:DoclingInstallRequest={profiles:[...profiles],formats:[...formats],apply,valid:true,pending:false,started:false};
 doclingInstallRequest.set(context);doclingDownloadOpen.set(true);
 return()=>{context.valid=false;context.apply=undefined;if(get(doclingInstallRequest)===context)doclingInstallRequest.set(context);};
}
let request:Promise<DoclingStatus>|null=null;
let generation=0;
export function updateDoclingStatus(status:DoclingStatus){generation++;request=null;doclingStatus.set(status);}
export function refreshDoclingStatus(fresh=false){
 if(fresh){generation++;request=null;}
 const current=generation;
 if(!request){const next=api<DoclingStatus>('get_docling_status').then(status=>{if(current===generation)doclingStatus.set(status);return status;}).finally(()=>{if(request===next)request=null;});request=next;}
 return request;
}
