import morphdom from 'morphdom';

/** Update sanitized answer HTML without replacing unchanged paragraphs or code blocks. */
export function answerHtml(node:HTMLElement,html:string){
 let current:string|undefined;
 const update=(next:string)=>{
  if(next===current)return;
  const target=document.createElement('div');
  target.innerHTML=next;
  morphdom(node,target,{childrenOnly:true,onBeforeElUpdated:(from,to)=>!from.isEqualNode(to)});
  current=next;
 };
 update(html);
 return {update};
}
