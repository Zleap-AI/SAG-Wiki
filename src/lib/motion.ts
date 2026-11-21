import type {TransitionConfig} from 'svelte/transition';

const welcomeMotions={
 chat:['bubble','typing','squash','echo'],
 search:['scan','focus','tilt','glint'],
} as const;
export type WelcomeGlyph=keyof typeof welcomeMotions;
export type WelcomeMotion=(typeof welcomeMotions)[WelcomeGlyph][number];

export function createWelcomeMotionPicker(){
 const remaining:Record<WelcomeGlyph,WelcomeMotion[]>={chat:[],search:[]};
 const last:Partial<Record<WelcomeGlyph,WelcomeMotion>>={};
 return (glyph:WelcomeGlyph):WelcomeMotion=>{
  const queue=remaining[glyph];
  if(queue.length===0){
   queue.push(...welcomeMotions[glyph]);
   for(let i=queue.length-1;i>0;i--){
    const j=Math.floor(Math.random()*(i+1));
    [queue[i],queue[j]]=[queue[j],queue[i]];
   }
   if(queue[0]===last[glyph]){
    const j=1+Math.floor(Math.random()*(queue.length-1));
    [queue[0],queue[j]]=[queue[j],queue[0]];
   }
  }
  const next=queue.shift()!;
  last[glyph]=next;
  return next;
 };
}

// Keyboard commands stay instant; pointer interactions use short compositor-only transitions.
export function surface(_node:Element,{kind='popover',keyboard=false}:{kind?:'popover'|'dialog'|'drawer'|'overlay';keyboard?:boolean}={}):TransitionConfig {
 const still=keyboard||window.matchMedia('(prefers-reduced-motion: reduce)').matches;
 if(still)return {duration:0};
 return {duration:kind==='popover'?160:200,easing:t=>1-Math.pow(1-t,3),css:t=>{
  const transform=kind==='drawer'?`translateX(${(1-t)*24}px)`:kind==='overlay'?'none':`translateY(${(1-t)*4}px) scale(${.98+.02*t})`;
  return `opacity:${t};transform:${transform}`;
 }};
}
