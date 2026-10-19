import {derived,writable} from 'svelte/store';
import english from './locales/en.json';
import chinese from './locales/zh-CN.json';

export type Language='zh-CN'|'en';
type Parameters=Record<string,unknown>;
const messages:Record<string,string>=english;
const chineseMessages:Record<string,string>=chinese;
export const language=writable<Language>('zh-CN');
const placeholder=/\{(value\d+)\}/g;
const escape=(value:string)=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
const patterns=Object.entries(messages).filter(([key])=>/\{value\d+\}/.test(key)).map(([key,value])=>{
 const names=[...key.matchAll(placeholder)].map(match=>match[1]);
 const parts=key.split(placeholder).map((part,index)=>index%2?'([\\s\\S]*?)':escape(part));
 return {expression:new RegExp('^'+parts.join('')+'$'),names,value,length:key.length};
}).sort((a,b)=>b.length-a.length);

function format(text:string,parameters:Parameters){
 return text.replace(placeholder,(match,name)=>name in parameters?String(parameters[name]):match);
}
export function translate(text:string,locale:Language,parameters:Parameters={}):string{
 if(locale==='zh-CN')return format(Object.hasOwn(chineseMessages,text)?chineseMessages[text]:text,parameters);
 if(Object.hasOwn(messages,text))return format(messages[text],parameters);
 if(!/[\u3400-\u9fff]/.test(text))return format(text,parameters);
 // Backend errors and progress messages may already contain interpolated values.
 for(const pattern of patterns){
  const match=text.match(pattern.expression);
  if(match)return format(pattern.value,Object.fromEntries(pattern.names.map((name,index)=>[name,match[index+1]])));
 }
 if(text.startsWith('Error: '))return 'Error: '+translate(text.slice(7),locale);
 return text;
}
export const t=derived(language,locale=>(text:string,parameters?:Parameters)=>translate(text,locale,parameters));
export function setLanguage(locale:Language){
 language.set(locale);
 if(typeof document!=='undefined')document.documentElement.lang=locale;
}
