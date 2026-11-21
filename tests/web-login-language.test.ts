// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import WebApp from '../src/WebApp.svelte';
import {setLanguage} from '../src/lib/i18n';
import {changeWebAccessCode,disconnectWeb,webRequest} from '../src/lib/web-connection';

vi.mock('../src/App.svelte',()=>({default:()=>({})}));

let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{
 if(app)await unmount(app);
 app=undefined;
 disconnectWeb();
 document.body.replaceChildren();
 vi.restoreAllMocks();
 vi.unstubAllGlobals();
});

function submitAccessCode(code:string){
 const input=document.querySelector<HTMLInputElement>('#web-code')!;
 input.value=code;
 input.dispatchEvent(new Event('input',{bubbles:true}));
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
}

it.each([
 ['ja-JP','ja','Web アクセスコード','アクセスコードが正しくありません'],
 ['en-US','en','Web access code','Incorrect access code'],
 ['zh-Hant-HK','zh-TW','網頁存取碼','存取碼不正確'],
 ['ko-KR','ko','웹 접속 코드','접속 코드가 올바르지 않습니다'],
 ['es-MX','es','Código de acceso web','Código de acceso incorrecto'],
 ['zh-CN','zh-CN','网页访问码','访问码不正确'],
 ['fr-FR','en','Web access code','Incorrect access code'],
])('uses %s for the web entry and invalid access code error',async(browserLocale,language,label,error)=>{
 vi.spyOn(navigator,'language','get').mockReturnValue(browserLocale);
 vi.stubGlobal('fetch',vi.fn(async()=>new Response(null,{status:401})));
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('label')?.textContent).toBe(label));
 expect(document.documentElement.lang).toBe(language);
 submitAccessCode('wrong-code');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toBe(error));
});

it.each([
 ['ja-JP','Web サービスに接続できませんでした'],
 ['zh-CN','无法连接网页服务'],
])('localizes network failures for a %s browser',async(browserLocale,error)=>{
 vi.spyOn(navigator,'language','get').mockReturnValue(browserLocale);
 vi.stubGlobal('fetch',vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('#web-code')).not.toBeNull());
 submitAccessCode('access-code');
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toBe(error));
});

it('returns to a localized sign-in form after the workspace session expires',async()=>{
 vi.spyOn(navigator,'language','get').mockReturnValue('en-US');
 vi.stubGlobal('fetch',vi.fn(async(path:string,options:RequestInit)=>path==='/web/session'&&new Headers(options.headers).has('Authorization')?Response.json({access_code_required:true}):new Response(null,{status:401})));
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('label')?.textContent).toBe('Web access code'));
 submitAccessCode('saved-code');
 await vi.waitFor(()=>expect(document.querySelector('form')).toBeNull());

 setLanguage('ja');
 await expect(webRequest({})).rejects.toThrow('请重新连接网页服务');
 await vi.waitFor(()=>expect(document.querySelector('label')?.textContent).toBe('Web アクセスコード'));
 expect(document.querySelector('[role=alert]')?.textContent).toBe('Web アクセスコードを再入力してください。');
});

it('opens a passwordless workspace without displaying the access-code form',async()=>{
 const fetch=vi.fn(async(_path:string,_options:RequestInit)=>Response.json({access_code_required:false}));
 vi.stubGlobal('fetch',fetch);
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.web-login')).toBeNull());
 expect(fetch).toHaveBeenCalledWith('/web/session',{headers:{},cache:'no-store'});
 expect(localStorage.getItem('sag.web.code')).toBeNull();
 await webRequest({operation:'list_roots'});
 expect(new Headers(fetch.mock.calls.at(-1)?.[1]?.headers).has('Authorization')).toBe(false);
});

it('keeps the initiating browser signed in if an old request expires during rotation',async()=>{
 let expireRequest!:(response:Response)=>void;
 let finishRotation!:(response:Response)=>void;
 const staleResponse=new Promise<Response>(resolve=>expireRequest=resolve);
 const rotationResponse=new Promise<Response>(resolve=>finishRotation=resolve);
 const fetch=vi.fn(async(path:string,options:RequestInit)=>{
  if(path==='/web/session')return Response.json({access_code_required:true});
  const {operation}=JSON.parse(String(options.body));
  if(operation==='list_roots')return staleResponse;
  if(operation==='api')return rotationResponse;
  return Response.json({});
 });
 vi.stubGlobal('fetch',fetch);
 localStorage.setItem('sag.web.code','previous-code');
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.web-login')).toBeNull());
 const stale=expect(webRequest({operation:'list_roots'})).rejects.toThrow('请重新连接网页服务');
 const rotation=changeWebAccessCode({action:'web_rotate_code'});
 expireRequest(new Response(null,{status:401}));
 await Promise.resolve();
 finishRotation(Response.json({code:'rotated-code'}));
 await Promise.all([rotation,stale]);
 expect(document.querySelector('.web-login')).toBeNull();
 expect(localStorage.getItem('sag.web.code')).toBe('rotated-code');
 await webRequest({operation:'get_settings'});
 expect(new Headers(fetch.mock.calls.at(-1)?.[1]?.headers).get('Authorization')).toBe('Bearer rotated-code');
});
