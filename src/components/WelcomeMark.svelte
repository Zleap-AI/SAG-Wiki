<script lang="ts">
 import {icon} from '../lib/icons';
 import type {WelcomeGlyph,WelcomeMotion} from '../lib/motion';

 let {glyph,enabled,chooseMotion}:{
  glyph:WelcomeGlyph;
  enabled:boolean;
  chooseMotion:(glyph:WelcomeGlyph)=>WelcomeMotion;
 }=$props();
 let mark:HTMLDivElement;
 let motion=$state<WelcomeMotion>();

 $effect(()=>{
  if(!enabled)return;
  const node=mark;
  const kind=glyph;
  const reduced=matchMedia('(prefers-reduced-motion: reduce)');
  let visible=false,timer=0;

  function canPlay(){
   return visible&&!document.hidden&&document.hasFocus()&&!reduced.matches;
  }
  function schedule(delay=12000+Math.random()*10000){
   window.clearTimeout(timer);
   if(!canPlay())return;
   timer=window.setTimeout(()=>{
    if(canPlay())motion=chooseMotion(kind);
    schedule();
   },delay);
  }
  function stopAndWait(){
   motion=undefined;
   schedule();
  }
  function hover(event:PointerEvent){
   if(event.pointerType==='touch'||!canPlay())return;
   if(!motion)motion=chooseMotion(kind);
   schedule();
  }
  const observer=new IntersectionObserver(([entry])=>{
   visible=entry.isIntersecting;
   if(!visible)motion=undefined;
   schedule(2800);
  });
  observer.observe(node);
  node.addEventListener('pointerenter',hover);
  document.addEventListener('visibilitychange',stopAndWait);
  reduced.addEventListener('change',stopAndWait);
  window.addEventListener('focus',stopAndWait);
  window.addEventListener('blur',stopAndWait);
  window.addEventListener('input',stopAndWait);
  return ()=>{
   observer.disconnect();
   window.clearTimeout(timer);
   node.removeEventListener('pointerenter',hover);
   document.removeEventListener('visibilitychange',stopAndWait);
   reduced.removeEventListener('change',stopAndWait);
   window.removeEventListener('focus',stopAndWait);
   window.removeEventListener('blur',stopAndWait);
   window.removeEventListener('input',stopAndWait);
   motion=undefined;
  };
 });
</script>

<div class="welcome-mark" bind:this={mark} aria-hidden="true">
 <span class="welcome-glyph" data-motion={motion} onanimationend={event=>{
  if(event.target===event.currentTarget&&!event.pseudoElement)motion=undefined;
 }}>{@html icon(glyph)}</span>
</div>

<style>
 .welcome-glyph{position:relative;display:grid;place-items:center;transform-origin:50% 55%;--ease-out:cubic-bezier(.23,1,.32,1)}
 .welcome-glyph :global(.icon){width:26px;height:26px}

 @media(prefers-reduced-motion:no-preference){
  [data-motion=bubble]{animation:welcome-bubble 800ms var(--ease-out)}
  [data-motion=typing]{animation:welcome-typing 710ms ease-out}
  [data-motion=typing] :global(path:not(:first-child)){
   animation:welcome-line 360ms ease-out both;transform-box:fill-box;transform-origin:left center
  }
  [data-motion=typing] :global(path:nth-child(4)){animation-delay:40ms}
  [data-motion=typing] :global(path:nth-child(2)){animation-delay:140ms}
  [data-motion=typing] :global(path:nth-child(3)){animation-delay:240ms}
  [data-motion=squash]{animation:welcome-squash 690ms var(--ease-out)}
  [data-motion=echo]{animation:welcome-echo 760ms ease-in-out}
  [data-motion=echo]::after{
   position:absolute;inset:1px 0 0;border:1px solid currentColor;border-radius:7px;
   content:'';animation:welcome-ripple 590ms 100ms ease-out both
  }
  [data-motion=scan]{animation:welcome-scan 900ms ease-in-out}
  [data-motion=focus]{animation:welcome-focus 740ms var(--ease-out)}
  [data-motion=focus] :global(circle){animation:welcome-outline 540ms 80ms ease-out}
  [data-motion=tilt]{animation:welcome-tilt 780ms ease-in-out}
  [data-motion=glint]{animation:welcome-glint 830ms ease-in-out}
  [data-motion=glint]::after{
   position:absolute;inset:5px auto auto 5px;width:8px;height:5px;border-top:1.5px solid currentColor;
   border-radius:50%;content:'';animation:welcome-lens 550ms 120ms ease-in-out both
  }
 }

 @keyframes welcome-bubble{
  0%,100%{transform:translateY(0) rotate(0)}
  22%{transform:translateY(-5px) rotate(-6deg)}
  47%{transform:translateY(-2px) rotate(3deg)}
  67%{transform:translateY(0) rotate(-1deg)}
 }
 @keyframes welcome-typing{
  0%,100%{transform:translateY(0)}
  34%,67%{transform:translateY(-1.5px)}
 }
 @keyframes welcome-line{
  0%,100%{opacity:1;stroke-width:2;transform:scaleX(1)}
  22%{opacity:.38;stroke-width:1.6;transform:scaleX(.4)}
  58%{opacity:1;stroke-width:2.6;transform:scaleX(1.06)}
 }
 @keyframes welcome-squash{
  0%,100%{transform:translateY(0) scale(1)}
  20%{transform:translateY(2px) scale(1.10,.86)}
  44%{transform:translateY(-3px) scale(.94,1.10)}
  72%{transform:translateY(0) scale(1.03,.97)}
 }
 @keyframes welcome-echo{
  0%,100%{transform:scale(1)}
  29%{transform:scale(1.09)}
  58%{transform:scale(.98)}
 }
 @keyframes welcome-ripple{
  0%{opacity:0;transform:scale(.91)}
  25%{opacity:.44}
  100%{opacity:0;transform:scale(1.52)}
 }
 @keyframes welcome-scan{
  0%,100%{transform:translate(0,0) rotate(0) scale(1)}
  25%{transform:translate(5px,-3px) rotate(7deg) scale(1.09)}
  59%{transform:translate(-4px,-1px) rotate(-7deg) scale(1.04)}
  79%{transform:translate(1px,0) rotate(2deg) scale(1)}
 }
 @keyframes welcome-focus{
  0%,100%{transform:translate(0,0) scale(1)}
  27%{transform:translate(1.5px,-1.5px) scale(1.19)}
  59%{transform:translate(1.5px,-1.5px) scale(1.13)}
  76%{transform:translate(0,0) scale(.97)}
 }
 @keyframes welcome-outline{
  0%,100%{stroke-width:2}
  39%{stroke-width:2.8}
 }
 @keyframes welcome-tilt{
  0%,100%{transform:rotate(0)}
  26%{transform:rotate(-19deg)}
  56%{transform:rotate(10deg)}
  79%{transform:rotate(-3deg)}
 }
 @keyframes welcome-glint{
  0%,100%{transform:translate(0,0) rotate(0)}
  39%{transform:translate(2px,-2px) rotate(-8deg)}
  70%{transform:translate(-1px,0) rotate(3deg)}
 }
 @keyframes welcome-lens{
  0%{opacity:0;transform:translate(-2px,-1px) rotate(-30deg)}
  38%{opacity:.8}
  100%{opacity:0;transform:translate(5px,4px) rotate(-30deg)}
 }
</style>
