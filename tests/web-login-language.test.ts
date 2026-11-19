// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import WebApp from '../src/WebApp.svelte';
import {setLanguage} from '../src/lib/i18n';
import {disconnectWeb,webRequest} from '../src/lib/web-connection';

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

it('returns to a localized sign-in form after the workspace session expires',async()=>{
 vi.spyOn(navigator,'language','get').mockReturnValue('en-US');
 vi.stubGlobal('fetch',vi.fn(async(path:string)=>new Response(null,{status:path==='/web/session'?200:401})));
 app=mount(WebApp,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('label')?.textContent).toBe('Web access code'));
 submitAccessCode('saved-code');
 await vi.waitFor(()=>expect(document.querySelector('form')).toBeNull());

 setLanguage('ja');
 await expect(webRequest({})).rejects.toThrow('请重新连接网页服务');
 await vi.waitFor(()=>expect(document.querySelector('label')?.textContent).toBe('Web アクセスコード'));
 expect(document.querySelector('[role=alert]')?.textContent).toBe('Web アクセスコードを再入力してください。');
});
