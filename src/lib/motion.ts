import type {TransitionConfig} from 'svelte/transition';

// Keyboard commands stay instant; pointer interactions use short compositor-only transitions.
export function surface(_node:Element,{kind='popover',keyboard=false}:{kind?:'popover'|'dialog'|'drawer'|'overlay';keyboard?:boolean}={}):TransitionConfig {
 const still=keyboard||window.matchMedia('(prefers-reduced-motion: reduce)').matches;
 if(still)return {duration:0};
 return {duration:kind==='popover'?160:200,easing:t=>1-Math.pow(1-t,3),css:t=>{
  const transform=kind==='drawer'?`translateX(${(1-t)*24}px)`:kind==='overlay'?'none':`translateY(${(1-t)*4}px) scale(${.98+.02*t})`;
  return `opacity:${t};transform:${transform}`;
 }};
}
