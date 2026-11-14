import {listen as nativeListen,type EventCallback} from '@tauri-apps/api/event';
import {web,webListen} from './web-connection';

export const listen=<T>(name:string,handler:EventCallback<T>)=>
  web ? Promise.resolve(webListen<T>(name,event=>handler({event:name,id:0,...event}))) : nativeListen<T>(name,handler);
