import {derived,writable} from 'svelte/store';
import registry from '../../protocol/src/interface-locales.json';
import english from './locales/en.json';
import chinese from './locales/zh-CN.json';
import japanese from './locales/ja.json';
import traditionalChinese from './locales/zh-TW.json';
import korean from './locales/ko.json';
import spanish from './locales/es.json';

export const interfaceLocales=registry.languages;
export type Language=keyof typeof interfaceLocales;
export const defaultLanguage=registry.defaultLanguage as Language;
export const languageOptions=(Object.keys(interfaceLocales) as Language[])
 .map(value=>({value,label:interfaceLocales[value].label}));
const catalogs:Record<Language,Record<string,string>>={'zh-CN':chinese,'zh-TW':traditionalChinese,en:english,ja:japanese,ko:korean,es:spanish};

type Parameters=Record<string,unknown>;
export const language=writable<Language>(defaultLanguage);
const placeholder=/\{(value\d+)\}/g;
const escape=(value:string)=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
const patterns=Object.keys(english).filter(key=>/\{value\d+\}/.test(key)).map(key=>{
 const names=[...key.matchAll(placeholder)].map(match=>match[1]);
 const parts=key.split(placeholder).map((part,index)=>index%2?'([\\s\\S]*?)':escape(part));
 return {expression:new RegExp('^'+parts.join('')+'$'),key,names};
}).sort((a,b)=>b.key.length-a.key.length);

export function resolveLanguage(value?:string|null):Language{
 return value&&Object.hasOwn(interfaceLocales,value)?value as Language:defaultLanguage;
}

export function resolveBrowserLanguage(value:string):Language{
 try{
  const locale=new Intl.Locale(value);
  if(locale.language==='zh')return locale.maximize().script==='Hant'?'zh-TW':'zh-CN';
  if(Object.hasOwn(interfaceLocales,locale.baseName))return locale.baseName as Language;
  if(Object.hasOwn(interfaceLocales,locale.language))return locale.language as Language;
 }catch{
  return 'en';
 }
 return 'en';
}

function format(text:string,parameters:Parameters){
 return text.replace(placeholder,(match,name)=>name in parameters?String(parameters[name]):match);
}

function lookup(source:string,locale:Language):string|undefined{
 const catalog=catalogs[locale];
 if(Object.hasOwn(catalog,source))return catalog[source];
 if(locale!=='en'&&locale!==defaultLanguage&&Object.hasOwn(english,source))return english[source as keyof typeof english];
}

export function translate(source:string,locale:Language,parameters:Parameters={}):string{
 const message=lookup(source,locale);
 if(message!==undefined||locale===defaultLanguage)return format(message??source,parameters);
 if(!/[\u3400-\u9fff]/.test(source))return format(source,parameters);
 // Some service messages still arrive with their values already interpolated.
 for(const pattern of patterns){
  const match=source.match(pattern.expression);
  if(match)return format(lookup(pattern.key,locale)??pattern.key,Object.fromEntries(pattern.names.map((name,index)=>[name,match[index+1]])));
 }
 if(source.startsWith('Error: '))return 'Error: '+translate(source.slice(7),locale);
 return source;
}

export const t=derived(language,locale=>(text:string,parameters?:Parameters)=>translate(text,locale,parameters));
export function setLanguage(value?:string|null){
 const locale=resolveLanguage(value);
 language.set(locale);
 if(typeof document!=='undefined')document.documentElement.lang=locale;
}
