/** RAF timestamps and performance.now() use the same time origin. Seed before the click. */
export function createFrameGapMonitor(started:number){
 let previous=started,worstFrameMs=0;
 return {
  get worstFrameMs(){return worstFrameMs;},
  frame(time:number){worstFrameMs=Math.max(worstFrameMs,time-previous);previous=time;}
 };
}

export type FeedbackKind='loading'|'content';
export function createFeedbackMonitor(started:number){
 let previous:FeedbackKind|null=null;
 let firstPaintMs:number|null=null,paintedKind:FeedbackKind|null=null;
 return {
  get feedback(){return {firstPaintMs,kind:paintedKind};},
  frame(time:number,kind:FeedbackKind|null){
   if(firstPaintMs!==null)return;
   if(previous&&kind){firstPaintMs=Math.max(0,time-started);paintedKind=previous;}
   previous=kind;
  }
 };
}
