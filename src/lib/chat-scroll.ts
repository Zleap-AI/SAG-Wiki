/** Follow the bottom until the reader scrolls up; resume when they return. */
export function followChat(node:HTMLElement,[resetKey]:readonly [number,...unknown[]]){
 let frame=0,following=true,lastTop=node.scrollTop;
 const scroll=()=>{
  if(!following)return;
  cancelAnimationFrame(frame);
  frame=requestAnimationFrame(()=>{
   if(following)lastTop=node.scrollTop=Math.max(0,node.scrollHeight-node.clientHeight);
  });
 };
 const onScroll=()=>{
  const top=node.scrollTop;
  if(top<lastTop)following=false;
  else if(node.scrollHeight-node.clientHeight-top<=4)following=true;
  lastTop=top;
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
   if(nextKey!==resetKey){resetKey=nextKey;following=true;lastTop=node.scrollTop}
   scroll();
  },
  destroy(){
   cancelAnimationFrame(frame);resize.disconnect();
   node.removeEventListener('scroll',onScroll);node.removeEventListener('wheel',onWheel);
  }
 };
}
