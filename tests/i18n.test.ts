import {describe,expect,it} from 'vitest';
import {get} from 'svelte/store';
import {language,setLanguage,t,translate} from '../src/lib/i18n';
import english from '../src/lib/locales/en.json';
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
 it('translates dynamic backend errors and preserves path values',()=>{
  expect(translate('路径不可用：/资料/会议.md','en')).toBe('Path unavailable: /资料/会议.md');
  expect(translate('无法读取原文件：/资料/会议.md','en')).toBe('Could not read the source file: /资料/会议.md');
  expect(translate('文件已修改，请刷新索引：/资料/会议.md','en')).toBe('The source file has changed. Refresh the index: /资料/会议.md');
  expect(translate('Error: MCP 工具调用超时','en')).toBe('Error: MCP tool call timed out');
  expect(translate('Returned by provider: example error','en')).toBe('Returned by provider: example error');
 });
 it('updates the active translator and formats original source locations',()=>{
  setLanguage('en');
  expect(get(language)).toBe('en');expect(get(t)('设置')).toBe('Settings');
  const source:Candidate={id:1,document_id:1,path:'/中文.md',title:'中文.md',heading:'',content:'原文',start_line:2,end_line:3,source_ranges:[],source_spans:[{start_byte:0,end_byte:4,start_line:2,end_line:3,start_column:1,end_column:2},{start_byte:5,end_byte:8,start_line:4,end_line:5,start_column:3,end_column:4}],bm25:0,score:0,order:0,hash:''};
  expect(sourceLocation(source)).toBe('Line 2, column 1 → line 3, column 2; Line 4, column 3 → line 5, column 4');
  setLanguage('zh-CN');expect(get(t)('设置')).toBe('设置');
 });
});
