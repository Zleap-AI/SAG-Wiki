// Capture before reflow. A DOM Range follows the same character through line wrapping;
// its viewport-relative top can be restored even when the piece itself never moved.
export type ViewportAnchor={target:Range|Element;context:Node;top:number};
type CaretDocument=Document&{
 caretPositionFromPoint?:(x:number,y:number)=>{offsetNode:Node;offset:number}|null;
 caretRangeFromPoint?:(x:number,y:number)=>Range|null;
};
export function captureViewportAnchor(host:HTMLElement):ViewportAnchor|null{
 const box=host.getBoundingClientRect();
 if(box.width<=0||box.height<=0)return null;
 const doc=host.ownerDocument as CaretDocument;
 let fallback:ViewportAnchor|null=null;
 for(const dy of [8,24,48]){
  const y=box.top+Math.min(dy,box.height/2);
  for(const fraction of [0.3,0.55]){
   const x=box.left+box.width*fraction;
   const position=doc.caretPositionFromPoint?.(x,y),caret=position?null:doc.caretRangeFromPoint?.(x,y);
   const node=position?.offsetNode??caret?.startContainer,offset=position?.offset??caret?.startOffset??0;
   if(node?.nodeType===Node.TEXT_NODE&&host.contains(node)&&node.parentElement?.closest('.reader-piece')&&node.textContent?.length){
    const range=doc.createRange(),start=Math.min(offset,node.textContent.length-1);
    range.setStart(node,start);range.setEnd(node,start+1);
    const rect=range.getBoundingClientRect?.();
    if(rect&&rect.height>0&&rect.bottom>=box.top&&rect.top<box.bottom)return {target:range,context:node,top:rect.top-box.top};
   }
   const block=doc.elementFromPoint?.(x,y)?.closest('p,li,pre,td,th,blockquote,h1,h2,h3,h4,h5,h6');
   if(!fallback&&block&&host.contains(block))fallback={target:block,context:block,top:block.getBoundingClientRect().top-box.top};
  }
 }
 return fallback;
}
export function restoreViewportAnchor(host:HTMLElement,anchor:ViewportAnchor|null){
 if(!anchor||!host.contains(anchor.context))return false;
 const rect=anchor.target.getBoundingClientRect();
 if(!rect.height)return false;
 const drift=rect.top-host.getBoundingClientRect().top-anchor.top;
 if(Math.abs(drift)>0.5)host.scrollTop+=drift;
 return true;
}
