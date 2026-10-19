import {expect,it} from 'vitest';
import {toolResultText} from '../src/lib/tool-result';
it('MCP 文本结果保留换行与全部内容',()=>{
 expect(toolResultText({content:[{type:'text',text:'first\nsecond'},{type:'text',text:'third'}]})).toBe('first\nsecond\n\nthird');
});
it('结构化结果和文本都可查看',()=>{
 const result=toolResultText({content:[{type:'text',text:'summary'}],structuredContent:{count:2}});
 expect(result).toContain('summary');expect(result).toContain('"count": 2');
});
it('图片和音频展示类型，原始数据留在原始 JSON 中',()=>{
 const raw={content:[{type:'image',mimeType:'image/png',data:'base64-test'}]};
 expect(toolResultText(raw)).toContain('图片 · image/png');expect(toolResultText(raw)).not.toContain('base64-test');
 expect(raw.content[0].data).toBe('base64-test');
});
it('未识别的返回结构仍能查看',()=>{
 expect(toolResultText({answer:'plain'})).toContain('"answer": "plain"');expect(toolResultText('plain')).toBe('plain');
});
it('相同的 MCP 文本与结构化内容只显示一次',()=>{
 const data={path:'/notes/a.md',sources:[{source_id:1,content:'原文'}]};
 expect(toolResultText({content:[{type:'text',text:JSON.stringify(data)}],structuredContent:data})).toBe(JSON.stringify(data));
});
