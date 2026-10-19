<script lang="ts">
 import {tick} from 'svelte';
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Timestamp from './Timestamp.svelte';
 import Input from './ui/Input.svelte';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {Conversation} from '../lib/types';
 export let conversations:Conversation[];
 export let onClose:()=>void;
 export let onSelect:(conversation:Conversation)=>Promise<void>;
 let query='',selected=0,list:HTMLDivElement;
 $:indexed=conversations.map(conversation=>({conversation,content:conversation.messages.flatMap(message=>[message.question,message.answer]).join('\n')}));
 $:terms=query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
 $:matches=indexed.filter(({conversation,content})=>terms.every(term=>(conversation.title+'\n'+content).toLocaleLowerCase().includes(term)));
 function snippet(content:string){const index=terms.length?content.toLocaleLowerCase().indexOf(terms[0]):0;const start=Math.max(0,index-32);return(start?'…':'')+content.slice(start,start+140).replace(/\s+/g,' ')+(content.length>start+140?'…':'')}
 async function changed(){selected=0;await tick();if(list)list.scrollTop=0}
 async function key(event:KeyboardEvent){
  if(event.isComposing||event.keyCode===229)return;
  if(event.key==='Enter'){event.preventDefault();if(matches[selected])await onSelect(matches[selected].conversation)}
  else if(event.key==='ArrowDown'||event.key==='ArrowUp'){
   event.preventDefault();selected=Math.max(0,Math.min(matches.length-1,selected+(event.key==='ArrowDown'?1:-1)));await tick();list.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({block:'nearest'});
  }
 }
</script>
<Modal label={$t('会话列表')} class="history-search-dialog" {onClose}>
 <div class="dialog-head"><h2>{$t('会话列表')}</h2><Button.Root class="icon-button" aria-label={$t('关闭会话列表')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="history-search-input">{@html icon('search')}<Input data-autofocus aria-label={$t('搜索会话')} placeholder={$t('搜索会话标题和内容…')} role="combobox" aria-expanded={true} aria-controls="history-search-results" aria-autocomplete="list" aria-activedescendant={matches[selected]?`history-result-${matches[selected].conversation.id}`:undefined} bind:value={query} oninput={changed} onkeydown={key}/></div>
 <div class="history-search-results" id="history-search-results" role="listbox" aria-label={$t('会话列表')} bind:this={list}>
  {#each matches as item,i(item.conversation.id)}<Button.Root id={`history-result-${item.conversation.id}`} class="history-search-result" role="option" tabindex={-1} aria-selected={i===selected} onclick={()=>onSelect(item.conversation)}>{@html icon('chat')}<span><strong>{item.conversation.title}</strong><small>{snippet(item.content)}</small></span><Timestamp value={item.conversation.updated}/></Button.Root>
  {:else}<p class="history-search-empty">{$t('没有找到相关会话')}</p>{/each}
 </div>
</Modal>
<style>
 :global(.history-search-dialog){width:640px;max-width:calc(100vw - 48px);display:flex;flex-direction:column;overflow:hidden}
 :global(.history-search-dialog .dialog-head){display:flex;align-items:center;justify-content:space-between;padding:18px 20px 12px;flex-shrink:0}
 .history-search-input{display:flex;align-items:center;gap:9px;margin:0 20px 12px;padding:10px 12px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-panel)}
 .history-search-input :global(.icon){width:17px;height:17px;color:var(--color-muted);flex-shrink:0}
 .history-search-input :global(input){flex:1;padding:0;border:0;background:transparent;outline:none}
 .history-search-input:focus-within{border-color:var(--color-focus)}
 .history-search-results{min-height:120px;max-height:55vh;overflow:auto;padding:0 10px 10px;overscroll-behavior:contain}
 .history-search-results :global(.history-search-result){display:flex;align-items:flex-start;gap:10px;width:100%;padding:12px 10px;text-align:left;border-radius:8px}
 .history-search-results :global(.history-search-result[aria-selected=true]),.history-search-results :global(.history-search-result:hover){background:var(--color-hover)}
 .history-search-results :global(.history-search-result>.icon){width:17px;height:17px;flex-shrink:0;color:var(--color-secondary);margin-top:2px}
 .history-search-results :global(.history-search-result>span){display:flex;flex:1;min-width:0;flex-direction:column;gap:4px}
 .history-search-results :global(strong){font-weight:550;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .history-search-results :global(small){font-size:var(--text-caption);color:var(--color-secondary);line-height:1.5;display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;overflow-wrap:anywhere}
 .history-search-empty{padding:26px 12px;text-align:center;color:var(--color-muted);font-size:var(--text-ui)}
</style>
