// The host sets this flag on its own HTML. The documentation fixture still uses Tauri mocks.
export const web = typeof document !== 'undefined' && document.documentElement.hasAttribute('data-sag-web');

export function requestId():string {
  if(crypto.randomUUID)return crypto.randomUUID();
  // getRandomValues is available on HTTP LAN addresses too.
  const bytes=crypto.getRandomValues(new Uint8Array(16));
  bytes[6]=(bytes[6]&15)|64;bytes[8]=(bytes[8]&63)|128;
  const value=Array.from(bytes,byte=>byte.toString(16).padStart(2,'0')).join('');
  return `${value.slice(0,8)}-${value.slice(8,12)}-${value.slice(12,16)}-${value.slice(16,20)}-${value.slice(20)}`;
}

let code='',tab='',stream:AbortController|undefined;
const events=new EventTarget();
const codeKey='sag.web.code';
export const savedWebCode=()=>localStorage.getItem(codeKey)??sessionStorage.getItem(codeKey)??'';

function forgetWebCode(accessCode:string){
  if(localStorage.getItem(codeKey)===accessCode)localStorage.removeItem(codeKey);
  if(sessionStorage.getItem(codeKey)===accessCode)sessionStorage.removeItem(codeKey);
}

export async function connectWeb(accessCode:string){
  const result=await fetch('/web/session',{headers:{Authorization:`Bearer ${accessCode}`},cache:'no-store'});
  if(result.status===401)forgetWebCode(accessCode);
  if(!result.ok)throw new Error(result.status===401?'访问码不正确':'无法连接网页服务');
  code=accessCode;tab=requestId();localStorage.setItem(codeKey,code);sessionStorage.removeItem(codeKey);
}

export function disconnectWeb(){
  forgetWebCode(code);
  stream?.abort();stream=undefined;code='';tab='';
}

function headers(){return {Authorization:`Bearer ${code}`,'X-Sag-Tab':tab};}

export async function webRequest<T>(payload:unknown):Promise<T>{
  const result=await fetch('/web/operation',{method:'POST',headers:{...headers(),'Content-Type':'application/json'},body:JSON.stringify(payload)});
  if(result.status===401||result.status===404){disconnectWeb();events.dispatchEvent(new Event('signed-out'));throw new Error('请重新连接网页服务');}
  const value=await result.json();
  if(!result.ok)throw new Error(value.error.message);
  return value as T;
}

export function onWebSignOut(listener:()=>void){
  events.addEventListener('signed-out',listener);
  return ()=>events.removeEventListener('signed-out',listener);
}

export function webListen<T>(name:string,handler:(event:{payload:T})=>void){
  const listener=(event:Event)=>handler({payload:(event as CustomEvent<T>).detail});
  events.addEventListener(name,listener);
  if(!stream){stream=new AbortController();void readEvents(stream);}
  return ()=>events.removeEventListener(name,listener);
}

async function readEvents(controller:AbortController){
  const signal=controller.signal;
  while(!signal.aborted){
    try{
      const response=await fetch('/web/events',{headers:headers(),signal});
      if(response.status===401||response.status===404){disconnectWeb();events.dispatchEvent(new Event('signed-out'));return;}
      if(!response.ok)throw new Error(`HTTP ${response.status}`);
      const reader=response.body!.getReader(),decoder=new TextDecoder();
      let buffer='';
      while(true){
        const {done,value}=await reader.read();
        if(done)break;
        buffer+=decoder.decode(value,{stream:true});
        let boundary:number;
        while((boundary=buffer.indexOf('\n\n'))>=0){
          const frame=buffer.slice(0,boundary);buffer=buffer.slice(boundary+2);
          if(!frame.startsWith('data: '))continue;
          const {event,payload}=JSON.parse(frame.slice(6));
          events.dispatchEvent(new CustomEvent(event,{detail:payload}));
        }
      }
    }catch(error){
      if(signal.aborted)return;
      console.warn('SAG web event stream will reconnect',error);
    }
    if(signal.aborted)return;
    await new Promise(resolve=>setTimeout(resolve,1500));
  }
}

export async function copyText(text:string){
  if(!web||navigator.clipboard?.writeText){await navigator.clipboard.writeText(text);return;}
  const field=document.createElement('textarea');
  field.value=text;field.style.position='fixed';field.style.opacity='0';field.readOnly=true;
  const previous=document.activeElement;
  document.body.append(field);field.select();
  try{if(!document.execCommand('copy'))throw new Error('浏览器不允许复制');}
  finally{field.remove();if(previous instanceof HTMLElement)previous.focus();}
}
