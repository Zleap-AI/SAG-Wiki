/** Follow the bottom until the reader scrolls up; resume when they return. */
export function followChat(node:HTMLElement,[resetKey]:readonly [number,...unknown[]]){
 const bottom=()=>Math.max(0,node.scrollHeight-node.clientHeight);
 let frame=0,following=true,lastTop=node.scrollTop,lastBottom=bottom();
 const scroll=()=>{
  if(!following||frame)return;
  frame=requestAnimationFrame(()=>{
   frame=0;
   if(!following)return;
   lastBottom=bottom();
   node.scrollTop=lastBottom;
   // The browser can clamp or round the requested offset to a fractional value.
   lastTop=node.scrollTop;
  });
 };
 const onScroll=()=>{
  const top=node.scrollTop,nextBottom=bottom();
  // A layout change can clamp scrollTop before the next streamed update grows it again.
  // Its deferred scroll event can run before the queued follow frame, even at the old height.
  if(top<lastTop){
   if(!frame&&nextBottom===lastBottom)following=false;
  }else if(nextBottom-top<=4)following=true;
  lastTop=top;lastBottom=nextBottom;
 };
 const onWheel=(event:WheelEvent)=>{
  if(event.deltaY<0&&node.scrollHeight>node.clientHeight)following=false;
 };
 node.addEventListener('scroll',onScroll,{passive:true});
 node.addEventListener('wheel',onWheel,{passive:true});
 const resize=new ResizeObserver(scroll);
 resize.observe(node);
 resize.observe(node.firstElementChild!);
 scroll();
 return {
  update([nextKey]:readonly [number,...unknown[]]){
   if(nextKey!==resetKey){resetKey=nextKey;following=true;lastTop=node.scrollTop;lastBottom=bottom()}
   scroll();
  },
  destroy(){
   cancelAnimationFrame(frame);resize.disconnect();
   node.removeEventListener('scroll',onScroll);node.removeEventListener('wheel',onWheel);
  }
 };
}
