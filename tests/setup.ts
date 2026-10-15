// Unit tests use reduced motion; native verification covers visible transitions.
if(typeof window!=='undefined'){
 const matchMedia=window.matchMedia.bind(window);
 window.matchMedia=(query:string)=>{
  const result=matchMedia(query);
  if(query==='(prefers-reduced-motion: reduce)')Object.defineProperty(result,'matches',{value:true});
  return result;
 };
}
import {beforeEach} from 'vitest';
import {setLanguage} from '../src/lib/i18n';
beforeEach(()=>{setLanguage('zh-CN');if(typeof window!=='undefined')localStorage.setItem('sag.onboarding.seen','1')});
