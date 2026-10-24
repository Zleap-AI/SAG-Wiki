/** Follow the bottom until the reader scrolls up; resume when they return. */
export function followChat(node:HTMLElement,[resetKey]:readonly [number,...unknown[]]){
 const bottom=()=>Math.max(0,node.scrollHeight-node.clientHeight);
 let frame=0,following=true,lastTop=node.scrollTop,lastBottom=bottom();
 const scroll=()=>{
  if(!following)return;
  cancelAnimationFrame(frame);
  frame=requestAnimationFrame(()=>{
   if(following)lastTop=node.scrollTop=lastBottom=bottom();
  });
 };
 const onScroll=()=>{
  const top=node.scrollTop,nextBottom=bottom();
  // Closing a process panel can clamp scrollTop to the new bottom without user input.
  const clamped=following&&nextBottom<lastBottom&&top>=nextBottom-1;
  if(top<lastTop&&!clamped)following=false;
  else if(nextBottom-top<=4)following=true;
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
