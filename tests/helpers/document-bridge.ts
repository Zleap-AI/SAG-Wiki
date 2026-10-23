import type {Window} from 'happy-dom';
import script from '../../src/lib/document-bridge.js?raw';
import {runInNewContext} from 'node:vm';
(window as unknown as Window).happyDOM.settings.handleDisabledFileLoadingAsSuccess=true;
export function loadDocumentBridge(frame:HTMLIFrameElement,document:Document){
 const child=frame.contentWindow!;
 let receive:(event:unknown)=>void;
 const parent={postMessage:(data:unknown)=>window.dispatchEvent(new MessageEvent('message',{source:child,data}))};
 runInNewContext(script,{document,window:{parent,addEventListener:(type:string,callback:(event:unknown)=>void)=>{if(type==='message')receive=callback;}}});
 Object.defineProperty(child,'postMessage',{value:(data:unknown)=>receive({source:parent,data})});
 frame.dispatchEvent(new Event('load'));
}
