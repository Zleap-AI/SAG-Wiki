import {basename} from './domain';
import type {ToolCallRecord} from './types';

type Translate=(text:string,parameters?:Record<string,string|number>)=>string;

export function toolArgs(call:ToolCallRecord):Record<string,unknown>{try{const value=JSON.parse(call.query);return value&&typeof value==='object'?value:{}}catch{return {}}}
export function toolKind(call:ToolCallRecord){const name=call.server_name?call.tool_name??'':'search';return name==='read'?'read':name==='search'||name==='grep'?'search':name==='list'?'list':'other'}
// Readers care about what was looked at, not which tool ran: a file name or the search terms.
export function toolLabel(call:ToolCallRecord,t:Translate){
 const input=toolArgs(call),path=typeof input.path==='string'?input.path:'',query=typeof input.query==='string'?input.query:call.server_name?'':call.query;
 const kind=toolKind(call);
 return kind==='read'?t('阅读 {value0}',{value0:basename(path)||t('文件')}):kind==='search'?t('检索「{value0}」',{value0:query||t('相关内容')}):kind==='list'?t('浏览 {value0}',{value0:basename(path)||t('文件夹')}):t('调用 {value0}',{value0:[call.server_name,call.tool_name].filter(Boolean).join(' · ')});
}
