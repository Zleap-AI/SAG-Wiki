import { invoke } from '@tauri-apps/api/core';
import {web,webRequest} from './web-connection';

export function api<T>(command:string,args?:Record<string,unknown>):Promise<T>{
  if(!web)return invoke<T>(command,args);
  return callWeb<T>(command,args);
}
async function callWeb<T>(command:string,args?:Record<string,unknown>):Promise<T>{
  if(command==='open_external_link'||command==='open_about_link'){
    const about:Record<string,string>={
      project:'https://github.com/Zleap-AI/SAG-Wiki',company:'https://zleap.com',
      docling:'https://docling.ai/', 'zleap-console':'https://platform.zleap.com/console',
      '302-api-keys':'https://302.ai/api-keys/list'
    };
    const url=new URL(String(command==='open_external_link'?args?.url:about[String(args?.target)]));
    if(!['http:','https:','mailto:'].includes(url.protocol))throw new Error('不支持此链接地址');
    window.open(url.href,'_blank','noopener,noreferrer');return undefined as T;
  }
  if(command==='save_settings'){
    const {theme,language,start_page,editor_size,model_profiles}=args!.settings as import('./types').Settings;
    return webRequest<T>({theme,language,start_page,editor_size,model_profiles});
  }
  const operation=command==='list_removed_directories'?'removed_directories':command;
  const named=Object.fromEntries(Object.entries(args??{}).map(([key,value])=>[key.replace(/[A-Z]/g,c=>'_'+c.toLowerCase()),value]));
  return webRequest<T>({operation,...(args?{args:named}:{})});
}
