import {get,writable} from 'svelte/store';
import {api} from './api';
import type {DoclingStatus} from './types';

export const doclingStatus=writable<DoclingStatus|null>(null);
export const doclingDownloadOpen=writable(false);
/// A pending "install, then apply these formats" request from the format picker.
export type DoclingInstallRequest={formats:string[];apply?:(valid:()=>boolean)=>Promise<void>;valid:boolean;pending:boolean;started:boolean};
export const doclingInstallRequest=writable<DoclingInstallRequest|null>(null);
export function openDoclingProgress(){
 const current=get(doclingInstallRequest);
 const unaccepted=current?.valid&&!current.started&&!get(doclingStatus)?.running;
 if(!current?.pending&&!unaccepted&&!current?.started){
  if(current){current.valid=false;current.apply=undefined;}
  doclingInstallRequest.set({formats:[],valid:false,pending:false,started:true});
 }
 doclingDownloadOpen.set(true);
}
export function openDoclingDownload(formats:string[],apply:(valid:()=>boolean)=>Promise<void>){
 if(get(doclingStatus)?.running||get(doclingInstallRequest)?.pending){openDoclingProgress();return()=>{};}
 const previous=get(doclingInstallRequest);if(previous){previous.valid=false;previous.apply=undefined;}
 const context:DoclingInstallRequest={formats:[...formats],apply,valid:true,pending:false,started:false};
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
