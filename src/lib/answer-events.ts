import type {ReasoningBlock,ToolCallRecord} from './types';

export function answerEvents(tools:ToolCallRecord[]=[],reasoning:ReasoningBlock[]=[]){
 const markers=[
  ...tools.map((call,index)=>({kind:'tool' as const,index,offset:call.answer_offset??0,order:index+0.5})),
  ...reasoning.map((block,index)=>({kind:'reasoning' as const,index,offset:block.answer_offset,order:block.tool_count})),
 ].sort((a,b)=>a.offset-b.offset||a.order-b.order);
 let offset=0;
 return markers.map(marker=>{const event={...marker,from:offset};offset=marker.offset;return event});
}
