import {describe,expect,it} from 'vitest';
import {cooperativeTextMatches,packedTextMatches,textMatches} from '../src/lib/text-matches';

describe('context-sensitive casing after compatibility normalization',()=>{
 const cases=[
  {text:'ΟΣ ﬁ',terms:['ος'],expected:[{from:0,to:2}]},
  {text:'ΟΣ Ａ',terms:['ος'],expected:[{from:0,to:2}]},
  {text:'ﬃ ΟΣ',terms:['ος'],expected:[{from:2,to:4}]},
  {text:'e\u0301 ΟΣ',terms:['ος'],expected:[{from:3,to:5}]},
  {text:'ﬃ ΟΣ ΟΣΑ',terms:['ος','οσα'],expected:[{from:2,to:4},{from:5,to:8}]},
 ];
 it.each(cases)('retains exact source offsets in $text',async({text,terms,expected})=>{
  expect(textMatches(text,terms)).toEqual(expected);
  const packed=expected.flatMap(({from,to})=>[from,to]);
  expect(Array.from(packedTextMatches(text,terms))).toEqual(packed);
  expect(Array.from((await cooperativeTextMatches(text,terms,()=>true))!)).toEqual(packed);
 });
 it('keeps casing context when starting after a normalized prefix',()=>{
  expect(textMatches('ﬃ ΟΣ ΟΣ',['ος'],4)).toEqual([{from:5,to:7}]);
  expect(Array.from(packedTextMatches('ﬃ ΟΣ ΟΣ',['ος'],4))).toEqual([5,7]);
 });
 it.each([4092,4094])('matches near a search-window boundary after %i spaces and an expanded grapheme',async(spaces)=>{
  const prefix='ﬃ'+' '.repeat(spaces),from=prefix.length,to=from+2;
  expect(textMatches(prefix+'ΟΣ',['ος'])).toEqual([{from,to}]);
  expect(Array.from(packedTextMatches(prefix+'ΟΣ',['ος']))).toEqual([from,to]);
  expect(Array.from((await cooperativeTextMatches(prefix+'ΟΣ',['ος'],()=>true))!)).toEqual([from,to]);
 });
});
