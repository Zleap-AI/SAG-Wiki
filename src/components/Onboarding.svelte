<script lang="ts">
 import {Button,Popover,Tooltip,IsUsingKeyboard} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon,logo} from '../lib/icons';
 import {surface} from '../lib/motion';
 import Modal from './ui/Modal.svelte';
 import './onboarding.css';

 export let onClose:()=>void;
 let page=0,track:HTMLDivElement,targetPage:number|null=null;
 let dragging=false,dragX=0,dragPage=0;
 let benchmarkOpen=false,indexBenchmarkOpen=false;
 const keyboard=new IsUsingKeyboard();
 $:if(page!==1)benchmarkOpen=false;
 $:if(page!==0)indexBenchmarkOpen=false;
 const slides=[
  {label:'飞牛文件，授权接入',title:'你的文件，就是知识库。',description:'通过飞牛选择并授权 NAS 文件夹，文档、笔记和代码进入索引。原文件保留在原处。'},
  {label:'SAG2 · 深入检索',title:'顺着线索，找到更多。',description:'跨文件关联检索，减少重要信息的遗漏。'},
  {label:'有依据的回答',title:'答案清楚，出处明确。',description:'用自然语言提问，或输入 @ 指定文件。点击答案旁的引用，直接查看对应的原文位置。'},
  {label:'准备就绪',title:'最轻。最快。最准。',description:'接入文件，连接模型，开始搜索和问答。你的知识，就在手边。'},
 ];
 function go(next:number,instant=false){
  page=Math.max(0,Math.min(slides.length-1,next));
  targetPage=page;
  track.scrollTo({left:page*track.clientWidth,behavior:instant||matchMedia('(prefers-reduced-motion: reduce)').matches?'instant':'smooth'});
 }
 function scroll(){
  if(dragging||!track.clientWidth)return;
  if(targetPage!==null){if(Math.abs(track.scrollLeft-targetPage*track.clientWidth)<1)targetPage=null;return;}
  page=Math.round(track.scrollLeft/track.clientWidth);
 }
 function dragStart(event:PointerEvent){
  if(event.pointerType!=='mouse'||event.button!==0||(event.target as Element).closest('button'))return;
  targetPage=null;dragging=true;dragX=event.clientX;dragPage=page;track.setPointerCapture(event.pointerId);
 }
 function dragMove(event:PointerEvent){if(dragging)track.scrollLeft=dragPage*track.clientWidth+dragX-event.clientX}
 function dragEnd(event:PointerEvent){
  if(!dragging)return;
  const delta=dragX-event.clientX;
  dragging=false;go(dragPage+(Math.abs(delta)>50?Math.sign(delta):0));
 }
 function key(event:KeyboardEvent){
  if(event.key==='ArrowRight'||event.key==='ArrowLeft'){
   event.preventDefault();event.stopPropagation();go(page+(event.key==='ArrowRight'?1:-1),true);
  }
 }
</script>

<svelte:window on:keydown={key}/>
<Modal label={$t('欢迎使用 SAG Wiki')} class="onboarding-dialog" overlayClass="onboarding-backdrop" {onClose}>
 <header class="onboarding-header"><span>{@html logo}<b>SAG Wiki</b><span>{$t('快速入门')}</span></span><Button.Root class="icon-button" aria-label={$t('关闭教程')} onclick={onClose}>{@html icon('close')}</Button.Root></header>
 <div role="region" aria-label={$t('快速入门')} class="onboarding-track" class:dragging bind:this={track} on:scroll={scroll} on:wheel={()=>targetPage=null} on:touchstart={()=>targetPage=null} on:pointerdown={dragStart} on:pointermove={dragMove} on:pointerup={dragEnd} on:pointercancel={()=>{dragging=false;go(dragPage)}}>
  {#each slides as slide,index}
   <section class="onboarding-slide" data-current={page===index} inert={page!==index} aria-label={$t('教程第 {value0} 页',{value0:index+1})}>
    <div class="onboarding-visual" aria-hidden="true">
     {#if index===0}
      <div class="onboarding-local">
       <div class="onboarding-file-stack">
        <div class="onboarding-mini-file onboarding-piece" style="--delay:60ms">{@html icon('file')}<span>{$t('项目笔记.md')}</span><span class="onboarding-file-type">MD</span></div>
        <div class="onboarding-mini-file onboarding-piece" style="--delay:160ms">{@html icon('file')}<span>{$t('研究资料.txt')}</span><span class="onboarding-file-type">TXT</span></div>
        <div class="onboarding-mini-file onboarding-piece" style="--delay:260ms">{@html icon('codeBlock')}<span>app.py</span><span class="onboarding-file-type">PY</span></div>
       </div>
       <div class="onboarding-connection"><i></i><i></i><i></i></div>
       <div class="onboarding-library onboarding-piece" style="--delay:350ms"><span class="onboarding-library-icon">{@html logo}</span><strong>{$t('我的知识库')}</strong><span class="onboarding-ready"><i></i>{$t('已建立本地索引')}</span><span class="onboarding-local-note onboarding-piece" style="--delay:500ms">{@html icon('refresh')}{$t('文件变化，自动同步')}</span></div>
      </div>
     {:else if index===1}
      <div class="onboarding-retrieval">
       <div class="onboarding-demo-query onboarding-piece">{@html icon('search')}<span>{$t('这个项目是如何实现的？')}</span><span class="onboarding-mode">{$t('深入')}</span></div>
       <div class="onboarding-search-links"><i></i><i></i><i></i></div>
       <div class="onboarding-evidence-grid">
        {#each [['file','文档'],['codeBlock','代码'],['file','笔记']] as [glyph,name],i}
         <div class="onboarding-evidence onboarding-piece" style={`--delay:${140+i*130}ms`}><span class="onboarding-evidence-icon">{@html icon(glyph)}</span><strong>{$t(name)}</strong></div>
        {/each}
       </div>
      </div>
     {:else if index===2}
      <div class="onboarding-answer-demo">
       <div class="onboarding-demo-question onboarding-piece">{$t('新项目的发布流程是什么？')}</div>
       <div class="onboarding-demo-answer onboarding-piece" style="--delay:140ms"><span>{@html logo}<b>SAG Wiki</b></span><p>{$t('先完成测试，再检查发布清单。')}<sup>1</sup></p><span class="onboarding-demo-citation">1 {@html icon('file')}{$t('发布流程.md')}</span></div>
       <div class="onboarding-source onboarding-piece" style="--delay:360ms"><div>{@html icon('file')}<strong>{$t('发布流程.md')}</strong><span>{$t('原文')}</span></div><p><span class="onboarding-source-line">12</span><mark>{$t('发布前，完成测试并逐项检查发布清单。')}</mark></p><span class="onboarding-source-rule"></span></div>
      </div>
     {:else}
      <div class="onboarding-start">
       <div class="onboarding-start-brand onboarding-piece">{@html logo}</div>
       {#each [['folder','授权 NAS 文件夹','点击侧边栏的 +，通过飞牛选择并授权资料目录。'],['settings','连接你的模型','在「设置 → 模型设置」中配置服务与密钥。'],['chat','开始搜索和问答','搜索原文，或让模型结合你的文件回答。']] as [glyph,title,detail],i}
        <div class="onboarding-start-row onboarding-piece" style={`--delay:${120+i*100}ms`}><span>{@html icon(glyph)}</span><div><strong>{$t(title)}</strong><p>{$t(detail)}</p></div><small>0{i+1}</small></div>
       {/each}
      </div>
     {/if}
    </div>
    <div class="onboarding-copy"><span class="onboarding-eyebrow">{$t(slide.label)}</span><h2>{$t(slide.title)}</h2><p>{$t(slide.description)}</p>
     {#if index===0}
      <Tooltip.Provider delayDuration={220}><Tooltip.Root bind:open={indexBenchmarkOpen} disabled={page!==0} disableCloseOnTriggerClick>
       <Tooltip.Trigger class="onboarding-benchmark onboarding-index-speed" aria-label={$t('查看建库速度依据')} onclick={()=>indexBenchmarkOpen=true}>{$t('建库约 200 毫秒 / 百万字')}{@html icon('info')}</Tooltip.Trigger>
       <Tooltip.Portal><Tooltip.Content side="top" sideOffset={10} collisionPadding={16} forceMount>
        {#snippet child({props,wrapperProps,open})}
         {#if open}<div {...wrapperProps}><div {...props} class="onboarding-benchmark-popover" aria-label={$t('建库速度依据')} transition:surface|global={{keyboard:keyboard.current}}><strong>{$t('SAG2 历史建库基准')}</strong><p>{$t('约 24.65 亿字符 · 511,962 份文档')}</p><p>{$t('完整流程 8 分 32 秒，包含文件清点、建库和完整性检查。全量平均每百万字符约 208 毫秒。')}</p><p>{$t('“字”按字符统计。历史本地基准采用 8 并发，并非当前客户端百万字单独计时；实际耗时随设备、文件数量和格式变化。')}</p></div></div>{/if}
        {/snippet}
       </Tooltip.Content></Tooltip.Portal>
      </Tooltip.Root></Tooltip.Provider>
     {:else if index===1}
      <Popover.Root bind:open={benchmarkOpen}>
       <Popover.Trigger class="onboarding-benchmark">{$t('SOTA 召回 · 论文基准')}{@html icon('info')}</Popover.Trigger>
       <Popover.Portal><Popover.Content side="top" sideOffset={10} collisionPadding={16} forceMount>
        {#snippet child({props,wrapperProps,open})}
         {#if open}<div {...wrapperProps}><div {...props} class="onboarding-benchmark-popover" aria-label={$t('SAG 论文基准')} transition:surface|global={{keyboard:keyboard.current}}><strong>{$t('SAG 论文基准')}</strong><p>{$t('论文在 HotpotQA、2WikiMultiHopQA、MuSiQue 上领先所列对比方法；实际效果取决于资料和模型。')}</p></div></div>{/if}
        {/snippet}
       </Popover.Content></Popover.Portal>
      </Popover.Root>
     {/if}
    </div>
   </section>
  {/each}
 </div>
 <footer class="onboarding-footer">
  <div class="onboarding-pagination" aria-label={$t('教程分页')}>{#each slides as _,index}<Button.Root aria-label={$t('教程第 {value0} 页',{value0:index+1})} aria-current={page===index?'step':undefined} onclick={(event:MouseEvent)=>go(index,event.detail===0)}><span></span></Button.Root>{/each}</div>
  <span class="onboarding-swipe-hint">{$t('左右滑动，了解更多')}</span>
  <div class="onboarding-actions">{#if page>0}<Button.Root class="onboarding-previous" onclick={(event:MouseEvent)=>go(page-1,event.detail===0)}>{$t('上一步')}</Button.Root>{/if}<Button.Root class="primary onboarding-next" onclick={(event:MouseEvent)=>page===slides.length-1?onClose():go(page+1,event.detail===0)}>{page===slides.length-1?$t('开始使用'):$t('下一步')}{@html icon(page===slides.length-1?'check':'chevron')}</Button.Root></div>
 </footer>
</Modal>
