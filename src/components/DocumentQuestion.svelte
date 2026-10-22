<script lang="ts">
 import {tick} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';

 export let name:string;
 export let onAsk:(question:string)=>Promise<void>;
 let open=false,question='',submitting=false,composing=false,compositionEndedAt=-Infinity;
 let input:HTMLInputElement;
 let toggle:HTMLButtonElement|null=null;

 async function expand(){open=true;await tick();input.focus({preventScroll:true});}
 async function close(){open=false;await tick();toggle!.focus({preventScroll:true});}
 async function submit(event:SubmitEvent){
  event.preventDefault();
  if(!question.trim()||submitting||composing||performance.now()-compositionEndedAt<100)return;
  submitting=true;
  try{await onAsk(question.trim());}finally{submitting=false;}
 }
 function keydown(event:KeyboardEvent){
  if(composing||event.isComposing||event.keyCode===229||performance.now()-compositionEndedAt<100)return;
  if(event.key==='Escape'){event.preventDefault();void close();}
  else if(event.key==='Enter'&&!event.shiftKey){event.preventDefault();input.form?.requestSubmit();}
 }
</script>

<div class="document-question" class:open>
 {#if open}
  <form class="document-question-form" onsubmit={submit} aria-label={`${$t('询问当前文档')}：${name}`}>
   <Input bind:ref={input} bind:value={question} aria-label={$t('对当前文档想问些什么？')} placeholder={$t('对当前文档想问些什么？')} disabled={submitting} onkeydown={keydown} onkeyup={event=>{if(event.key==='Enter')compositionEndedAt=-Infinity;}} oncompositionstart={()=>composing=true} oncompositionend={()=>{composing=false;compositionEndedAt=performance.now();}}/>
   <Button.Root type="button" class="icon-button" aria-label={$t('收起文档问答')} onclick={close} disabled={submitting}>{@html icon('close')}</Button.Root>
   <Button.Root type="submit" class="send" aria-label={$t('发送问题')} disabled={!question.trim()||submitting}>{@html icon('arrow')}</Button.Root>
  </form>
 {:else}
  <Button.Root bind:ref={toggle} type="button" class="document-question-toggle" aria-expanded={open} onclick={expand}>{@html icon('chat')}<span>{$t('问答')}</span></Button.Root>
 {/if}
</div>

<style>
 .document-question{position:absolute;right:24px;bottom:54px;z-index:2;}
 .document-question :global(.document-question-toggle){display:flex;align-items:center;gap:8px;min-height:42px;padding:10px 16px;border:1px solid var(--color-rule);border-radius:22px;background:var(--color-paper);color:var(--color-ink);box-shadow:0 4px 18px var(--color-shadow);font-size:var(--text-ui);}
 .document-question.open{position:relative;right:auto;bottom:auto;flex-shrink:0;width:100%;padding:0 24px 16px;}
 .document-question-form{display:flex;align-items:center;gap:8px;max-width:800px;height:52px;margin:0 auto;padding:8px 10px 8px 16px;border:1px solid var(--color-rule);border-radius:16px;background:var(--color-paper);box-shadow:0 4px 20px var(--color-shadow);}
 .document-question-form:focus-within{border-color:var(--color-focus);}
 .document-question-form :global(input){flex:1;min-width:0;height:34px;padding:0;border:0;outline:none;background:transparent;color:var(--color-ink);font-size:var(--text-reading);line-height:1.6;}
 .document-question-form :global(.icon-button){width:28px;height:28px;flex-shrink:0;}
 @media(max-width:1080px){.document-question{right:20px;}.document-question.open{padding-right:20px;padding-left:20px;}}
</style>
