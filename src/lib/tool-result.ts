import {displayPath} from './domain';

// Display MCP text as text; the complete response remains available as JSON.
export function toolResultText(value:unknown,translate:(text:string,parameters?:Record<string,unknown>)=>string=(text,parameters={})=>text.replace(/\{(value\d+)\}/g,(_,key)=>String(parameters[key])),sagTool?:string):string {
 if(typeof value==='string')return value;
 if(!value||typeof value!=='object'||Array.isArray(value))return JSON.stringify(value,null,2)??'';
 const result=value as Record<string,unknown>;
 if(sagTool&&result.isError!==true&&result.structuredContent&&typeof result.structuredContent==='object'&&!Array.isArray(result.structuredContent)){
  const data={...result.structuredContent as Record<string,unknown>};
  delete data.note;
  if(sagTool==='search'&&Array.isArray(data.sources)){
   const sources=data.sources as {source_id:number;title:string;path:string;content:string}[];
   const lines=sources.length?[translate('返回 {value0} 个原文片段',{value0:sources.length}),...sources.map(source=>`[${source.source_id}] ${source.title}\n${displayPath(source.path)}\n${source.content}`)]:[];
   if(!lines.length)lines.push(translate(data.omitted_sources?'本轮没有新增原文片段。':'未找到相关内容。请尝试调整关键词，或检查文件是否已完成索引。'));
   return lines.join('\n\n');
  }
  return JSON.stringify(data,null,2);
 }
 const parts=Array.isArray(result.content)?result.content.map((item:unknown)=>{
  if(item&&typeof item==='object'){
   const block=item as Record<string,unknown>;
   if(block.type==='text'&&typeof block.text==='string')return block.text;
   if(block.type==='image'||block.type==='audio')return translate('[{value0}{value1}，可展开原始 JSON 查看]',{value0:translate(block.type==='image'?'图片':'音频'),value1:typeof block.mimeType==='string'?` · ${block.mimeType}`:''});
  }
  return JSON.stringify(item,null,2);
 }):[];
 if(result.structuredContent!==undefined&&!parts.includes(JSON.stringify(result.structuredContent)))parts.push(JSON.stringify(result.structuredContent,null,2));
 return parts.length?parts.join('\n\n'):JSON.stringify(result,null,2);
}
