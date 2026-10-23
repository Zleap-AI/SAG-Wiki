<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';

 let layout:HTMLDivElement,scroller:HTMLDivElement;
 let headings:{element:HTMLElement;text:string;level:number}[]=[];
 let active=0,wide=false,outlineWidth=232,manualExpanded:boolean|null=null;
 const navigationId='document-outline-'+crypto.randomUUID();
 $:expanded=manualExpanded??wide;
 $:baseLevel=headings.length?Math.min(...headings.map(heading=>heading.level)):1;

 function updateActive(){
  const top=scroller.getBoundingClientRect().top+32;
  let current=0;
  for(let i=0;i<headings.length;i++){
   if(headings[i].element.getBoundingClientRect().top>top)break;
   current=i;
  }
  if(scroller.scrollHeight>scroller.clientHeight&&scroller.scrollTop+scroller.clientHeight>=scroller.scrollHeight-2)current=headings.length-1;
  active=current;
 }
 function readHeadings(){
  headings=Array.from(scroller.querySelectorAll<HTMLElement>('h1,h2,h3,h4,h5,h6'))
   .map(element=>({element,text:element.textContent?.trim()??'',level:Number(element.tagName[1])}))
   .filter(heading=>heading.text);
  updateActive();
 }
 function navigate(index:number){
  const top=scroller.scrollTop+headings[index].element.getBoundingClientRect().top-scroller.getBoundingClientRect().top-22;
  scroller.scrollTo({top:Math.max(0,top)});
  active=index;
 }
 onMount(()=>{
  readHeadings();
  const mutations=new MutationObserver(readHeadings);
  mutations.observe(scroller,{childList:true,subtree:true,characterData:true});
  const resize=new ResizeObserver(([entry])=>{
   const {width}=entry.contentRect;
   wide=width>=1320;
   outlineWidth=wide?Math.min(360,(width-800)/2-24):232;
   void tick().then(updateActive);
  });
  resize.observe(layout);
  return()=>{mutations.disconnect();resize.disconnect();};
 });
</script>

<div class="document-reading-layout" class:compact-expanded={expanded&&!wide} bind:this={layout}>
 {#if headings.length}
  <aside class="document-outline" class:expanded style={`--outline-width:${outlineWidth}px`} aria-label={$t('文档目录')}>
   <div class="outline-header">
    <Button.Root type="button" class="icon-button outline-toggle" aria-label={$t(expanded?'收起文档目录':'展开文档目录')} title={$t(expanded?'收起文档目录':'展开文档目录')} aria-expanded={expanded} aria-controls={navigationId} onclick={()=>manualExpanded=!expanded}>{@html icon(expanded?'chevrons-left':'bulletList')}</Button.Root>
   </div>
   <nav id={navigationId} class="outline-list" aria-label={$t('文档标题')} hidden={!expanded}>
    {#each headings as heading,index}
     <Button.Root type="button" class={`outline-heading ${index===active?'active':''}`} aria-current={index===active?'location':undefined} title={heading.text} style={`--heading-depth:${heading.level-baseLevel}`} onclick={()=>navigate(index)}>{heading.text}</Button.Root>
    {/each}
   </nav>
  </aside>
 {/if}
 <div class="document-sheet rich-document-sheet" bind:this={scroller} on:scroll={updateActive}><slot/></div>
</div>

<style>
 .document-reading-layout{position:relative;display:flex;flex:1;min-height:0;min-width:0;overflow:hidden}
 .document-sheet{min-width:0}
 :global(.real-client) .document-sheet{padding:22px 32px 28px}
 :global(.real-client) .document-reading-layout:not(.compact-expanded) .document-outline + .document-sheet{padding-inline:48px}
 .document-outline{position:absolute;inset:0 auto 0 0;display:flex;flex-direction:column;flex-shrink:0;width:48px;min-height:0;padding:16px 8px;pointer-events:none}
 .document-outline.expanded{width:var(--outline-width);padding-inline:16px 8px}
 .compact-expanded .document-outline{position:static}
 .outline-header{display:flex;align-items:center;gap:8px;min-height:32px;flex-shrink:0;margin-bottom:8px;color:var(--color-secondary);font-size:var(--text-caption);pointer-events:auto}
 .outline-header :global(.outline-toggle){display:flex;align-items:center;justify-content:center;width:32px;height:32px;padding:7px;flex-shrink:0}
 .outline-header :global(.icon){width:16px;height:16px}
 .outline-list{min-height:0;overflow:auto;overscroll-behavior:contain;pointer-events:auto}
 .outline-list :global(.outline-heading){display:block;width:100%;padding:7px 8px 7px calc(8px + var(--heading-depth)*14px);border:0;border-radius:6px;background:transparent;color:var(--color-secondary);font-size:var(--text-ui);line-height:1.5;text-align:left;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
 .outline-list :global(.outline-heading:hover){background:var(--color-hover);color:var(--color-ink)}
 .outline-list :global(.outline-heading.active){color:var(--color-accent);background:var(--color-selected)}
 .outline-list :global(.outline-heading:focus-visible){outline:2px solid var(--color-focus);outline-offset:-2px}
</style>
