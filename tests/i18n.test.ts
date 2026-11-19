// @vitest-environment happy-dom
import {describe,expect,it} from 'vitest';
import {get} from 'svelte/store';
import {language,resolveBrowserLanguage,setLanguage,t,translate} from '../src/lib/i18n';
import english from '../src/lib/locales/en.json';
import japanese from '../src/lib/locales/ja.json';
import traditionalChinese from '../src/lib/locales/zh-TW.json';
import korean from '../src/lib/locales/ko.json';
import spanish from '../src/lib/locales/es.json';
import {sourceLocation} from '../src/lib/domain';
import type {Candidate} from '../src/lib/types';

describe('interface translations',()=>{
 it('keeps every interpolation in English and has no unfinished translations',()=>{
  for(const [source,target] of Object.entries(english)){
   expect(target.trim(),source).not.toBe('');
   expect([...target.matchAll(/\{value\d+\}/g)].map(m=>m[0]).sort(),source)
    .toEqual([...source.matchAll(/\{value\d+\}/g)].map(m=>m[0]).sort());
   if(source!=='简体中文')expect(target,source).not.toMatch(/[\u3400-\u9fff]/);
  }
 });
 it.each([['Japanese',japanese],['Traditional Chinese',traditionalChinese],['Korean',korean],['Spanish',spanish]] as const)
 ('covers the English catalog in %s with matching placeholders',(_language,catalog)=>{
  expect(Object.keys(catalog)).toEqual(Object.keys(english));
  for(const key of Object.keys(english) as (keyof typeof english)[]){
   const placeholders=(text:string)=>[...text.matchAll(/\{value\d+\}/g)].map(match=>match[0]).sort();
   expect(catalog[key].trim(),key).not.toBe('');
   expect(placeholders(catalog[key]),key).toEqual(placeholders(english[key]));
  }
 });
 it('translates dynamic backend errors and preserves path values',()=>{
  expect(translate('路径不可用：/资料/会议.md','en')).toBe('Path unavailable: /资料/会议.md');
  expect(translate('无法读取原文件：/资料/会议.md','en')).toBe('Could not read the source file: /资料/会议.md');
  expect(translate('文件已修改，请刷新索引：/资料/会议.md','en')).toBe('The source file has changed. Refresh the index: /资料/会议.md');
  expect(translate('Error: MCP 工具调用超时','en')).toBe('Error: MCP tool call timed out');
  expect(translate('不支持的转换格式：unknown','en')).toBe('Unsupported conversion format: unknown');
  expect(translate('Returned by provider: example error','en')).toBe('Returned by provider: example error');
 });
 it('resolves Japanese messages and keeps dynamic file names unchanged',()=>{
  expect(translate('settings.interfaceLanguage','ja')).toBe('表示言語');
  expect(translate('路径不可用：/資料/会議.md','ja')).not.toContain('路径不可用');
  expect(translate('路径不可用：/資料/会議.md','ja')).toContain('/資料/会議.md');
  expect(translate('Provider error: retry later','ja')).toBe('Provider error: retry later');
 });
 it.each([
  ['ja','署名付きの更新を取得できませんでした。更新マニフェストと署名が公開されていることを確認してください。'],
  ['en','Could not retrieve a signed update. Make sure the update manifest and signatures are published.'],
 ] as const)('translates the current signed update error in %s and keeps its diagnostic', (locale, message)=>{
  const source='无法获取已签名的更新，请确认更新清单和签名已发布';
  expect(translate(source,locale)).toBe(message);
  expect(translate(source+': HTTP 503 from https://example.com/latest-v2.json',locale))
   .toBe(message+' HTTP 503 from https://example.com/latest-v2.json');
 });
 it.each([
  ['zh-TW','zh-TW'], ['zh-Hant-HK','zh-TW'], ['zh-MO','zh-TW'],
  ['zh-Hans-TW','zh-CN'], ['zh-CN','zh-CN'], ['ko-KR','ko'],
  ['es-MX','es'], ['ja-JP','ja'], ['en-GB','en'], ['fr-FR','en'], ['','en'],
 ])('resolves browser language %s as %s', (browserLocale, expected)=>{
  expect(resolveBrowserLanguage(browserLocale)).toBe(expected);
 });
 it.each(['zh-TW','ko','es'])('keeps saved %s and translates dynamic errors without changing paths',locale=>{
  setLanguage(locale);
  expect(get(language)).toBe(locale);
  expect(document.documentElement.lang).toBe(locale);
  expect(get(t)('路径不可用：/資料/회의.md')).toContain('/資料/회의.md');
  expect(get(t)('路径不可用：/資料/회의.md')).not.toContain('路径不可用');
  setLanguage('zh-CN');
 });
 it('applies the saved Japanese language to the document and translator',()=>{
  setLanguage('ja');
  expect(get(language)).toBe('ja');
  expect(document.documentElement.lang).toBe('ja');
  expect(get(t)('设置')).toBe('設定');
  setLanguage('zh-CN');
 });
 it('updates the active translator and formats original source locations',()=>{
  setLanguage('en');
  expect(get(language)).toBe('en');expect(get(t)('设置')).toBe('Settings');
  const source:Candidate={id:1,document_id:1,path:'/中文.md',title:'中文.md',heading:'',content:'原文',start_line:2,end_line:3,source_ranges:[],source_spans:[{start_byte:0,end_byte:4,start_line:2,end_line:3,start_column:1,end_column:2},{start_byte:5,end_byte:8,start_line:4,end_line:5,start_column:3,end_column:4}],bm25:0,score:0,order:0,hash:''};
  expect(sourceLocation(source)).toBe('Line 2, column 1 → line 3, column 2; Line 4, column 3 → line 5, column 4');
  setLanguage('zh-CN');expect(get(t)('设置')).toBe('设置');
 });
});
