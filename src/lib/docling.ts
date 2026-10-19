import {writable} from 'svelte/store';
import {api} from './api';
import type {DoclingStatus} from './types';

export const doclingStatus=writable<DoclingStatus|null>(null);
export const doclingDownloadOpen=writable(false);
let request:Promise<DoclingStatus>|null=null;
export function refreshDoclingStatus(){
 request??=api<DoclingStatus>('get_docling_status').then(status=>{doclingStatus.set(status);return status;}).finally(()=>request=null);
 return request;
}
