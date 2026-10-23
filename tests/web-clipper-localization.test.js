import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {expect,it,vi} from 'vitest';

it('provides both languages for extension UI and known save errors',async()=>{
 const keys=[...new Set(['options','popup'].flatMap(page=>[...readFileSync(`browser-extension/static/${page}.html`,'utf8').matchAll(/data-i18n="([^"]+)"/g)].map(match=>match[1])))];
 for(const language of ['en','zh-CN']) {
  const api={storage:{local:{get:async()=>({language})}},i18n:{getUILanguage:()=> 'en'}};
  const {translator,errorMessage}=new Function('globalThis',readFileSync('browser-extension/src/i18n.js','utf8').replaceAll('export ','')+';return {translator,errorMessage};')({chrome:api});
  const t=await translator();
  for(const key of keys)expect(t(key),`${language}: ${key}`).toBeTruthy();
  expect(errorMessage({code:'file-exists'},t)).toBe(t('fileExists'));
  expect(errorMessage({name:'NotAllowedError',code:0},t)).toBe(t('permissionDenied'));
  expect(errorMessage({name:'SecurityError',code:18},t)).toBe(t('permissionDenied'));
  expect(errorMessage('USER_CANCELED',t)).toBe(t('downloadCanceled'));
  expect(errorMessage(new Error('Detailed OS error'),t)).toBe('Detailed OS error');
 }
});

it('keeps Safari setup states bilingual when switching languages',()=>{
 const dom=new JSDOM(readFileSync('browser-extension/safari/Resources/Base.lproj/Main.html','utf8'),{url:'https://safari-setup.test',runScripts:'outside-only'});
 dom.window.eval(readFileSync('browser-extension/safari/Resources/Script.js','utf8'));
 const language=dom.window.document.querySelector('#language');
 for(const [locale,expected] of [['zh-CN',['正在检查','待启用','已启用']],['en',['Checking','not enabled','enabled']]]) {
  language.value=locale;language.dispatchEvent(new dom.window.Event('change'));
  for(const [index,state] of [undefined,false,true].entries()) {
   dom.window.show(state);
   expect(dom.window.document.querySelector('#state').textContent).toContain(expected[index]);
   expect(dom.window.document.querySelector('#state').textContent.toLowerCase()).toContain(locale==='en'?'installed':'已安装');
   expect(dom.window.document.documentElement.lang).toBe(locale);
  }
 }
 dom.window.close();
});

it('closes the installed Safari app with Done and opens settings only when not enabled',()=>{
 const dom=new JSDOM(readFileSync('browser-extension/safari/Resources/Base.lproj/Main.html','utf8'),{url:'https://safari-setup.test',runScripts:'outside-only'});
 const postMessage=vi.fn();
 dom.window.webkit={messageHandlers:{controller:{postMessage}}};
 dom.window.eval(readFileSync('browser-extension/safari/Resources/Script.js','utf8'));
 const button=dom.window.document.querySelector('button');
 expect(button.disabled).toBe(false);
 for(const [locale,label] of [['zh-CN','完成'],['en','Done']]) {
  const language=dom.window.document.querySelector('#language');language.value=locale;language.dispatchEvent(new dom.window.Event('change'));
  dom.window.show(true);
  expect(button.textContent).toBe(label);button.click();
  expect(postMessage).toHaveBeenLastCalledWith('close');
  dom.window.show(false);button.click();
  expect(postMessage).toHaveBeenLastCalledWith('open-preferences');
 }
 dom.window.close();
});
