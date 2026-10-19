import { invoke } from '@tauri-apps/api/core';
export const api=<T>(command:string,args?:Record<string,unknown>)=>invoke<T>(command,args);
