<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {Button} from 'bits-ui';
 import Input from './ui/Input.svelte';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import type {Conversation} from '../lib/types';

 export let conversations:Conversation[];
 export let activeId='';
 export let generating=false;
 export let onSelect:(conversation:Conversation)=>void|Promise<void>;
 export let onMenu:(event:MouseEvent,conversation:Conversation)=>void;
 export let onStop:()=>void|Promise<void>;

 let query='',list:HTMLDivElement,searchInput:HTMLInputElement|null=null,now=Date.now();
 // Search reads raw and Markdown-stripped text; the snippet shows prose and skips a question that repeats the title.
 $:indexed=conversations.map(conversation=>{const raw=conversation.messages.flatMap(message=>[message.question,message.answer]).join('\n');return{conversation,search:(conversation.title+'\n'+raw+'\n'+plain(raw)).toLocaleLowerCase(),content:plain(conversation.messages.flatMap(message=>[message.question===conversation.title?'':message.question,message.answer]).filter(Boolean).join('\n'))}});
 $:terms=query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
 $:matches=indexed.filter(({search})=>terms.every(term=>search.includes(term)));
 $:rows=matches.map((item,index)=>({...item,index,label:group(item.conversation.updated,now)}));
 $:sections=!rows.length?[]:terms.length?[{label:'',rows}]:rows.reduce((all,row)=>{const last=all.at(-1);if(last?.label===row.label)last.rows.push(row);else all.push({label:row.label,rows:[row]});return all},[] as {label:string;rows:typeof rows}[]);

 // Snippets and search read Markdown answers as prose, without emphasis or link syntax.
 function plain(text:string){return text.replace(/!?\[([^\]]*)\]\([^)]*\)/g,'$1').replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+\.)\s+/gm,'').replace(/(\*\*|__|~~|`+)/g,'')}
 function group(updated:number,at:number){const date=new Date(at),day=(offset:number)=>new Date(date.getFullYear(),date.getMonth(),date.getDate()-offset).getTime();return updated>=day(0)?'今天':updated>=day(1)?'昨天':updated>=day(6)?'最近 7 天':updated>=day(29)?'最近 30 天':'更早'}
 // The group heading already names the day, so rows only add what it leaves out.
 function stamp(updated:number,label:string){const date=new Date(updated),pad=(value:number)=>String(value).padStart(2,'0');return label==='今天'||label==='昨天'?`${pad(date.getHours())}:${pad(date.getMinutes())}`:label==='更早'?`${date.getFullYear()}-${pad(date.getMonth()+1)}-${pad(date.getDate())}`:`${pad(date.getMonth()+1)}-${pad(date.getDate())}`}
 onMount(()=>{const refresh=()=>now=Date.now();const timer=setInterval(refresh,60000);window.addEventListener('focus',refresh);return()=>{clearInterval(timer);window.removeEventListener('focus',refresh)}});
 async function clear(){query='';await changed();searchInput?.focus()}
 function snippet(content:string){const index=terms.length?content.toLocaleLowerCase().indexOf(terms[0]):0;const start=Math.max(0,index-24);return(start?'…':'')+content.slice(start,start+100).replace(/\s+/g,' ')+(content.length>start+100?'…':'')}
 function blocked(conversation:Conversation){return generating&&conversation.id!==activeId}
 async function select(conversation:Conversation){if(!blocked(conversation))await onSelect(conversation)}
 async function changed(){await tick();if(list)list.scrollTop=0}
 async function key(event:KeyboardEvent,index?:number){
  if(event.isComposing||event.keyCode===229)return;
  if(event.key==='Escape'&&index===undefined&&query){event.preventDefault();event.stopPropagation();await clear();return}
  const available=matches.map((item,i)=>({item,index:i})).filter(({item})=>!blocked(item.conversation));
  if(event.key==='Enter'&&index===undefined){event.preventDefault();if(available[0])await select(available[0].item.conversation);return}
  if(event.key!=='ArrowDown'&&event.key!=='ArrowUp')return;
  event.preventDefault();
  if(!available.length)return;
  const current=available.findIndex(item=>item.index===index);
  const next=index===undefined?(event.key==='ArrowDown'?0:available.length-1):Math.max(0,Math.min(available.length-1,current+(event.key==='ArrowDown'?1:-1)));
  const button=list.querySelector<HTMLButtonElement>(`[data-conversation-index="${available[next].index}"]`);
  button?.focus();button?.scrollIntoView({block:'nearest'});
 }
</script>

<!-- Lives inside the app sidebar; new-chat and open/close belong to the sidebar itself. -->
<section id="conversation-sidebar" class="conversation-sidebar" aria-label={$t('会话列表')}>
 <div class="conversation-sidebar-search">{@html icon('search')}<Input type="search" aria-label={$t('搜索会话')} placeholder={$t('搜索会话…')} bind:ref={searchInput} bind:value={query} oninput={changed} onkeydown={event=>key(event)}/>{#if query}<Button.Root type="button" class="conversation-sidebar-clear" title={$t('清除搜索')} aria-label={$t('清除搜索')} onclick={clear}>{@html icon('close')}</Button.Root>{/if}</div>
 {#if generating}<div class="conversation-sidebar-progress" role="status"><span class="conversation-sidebar-pulse" aria-hidden="true"></span><span class="conversation-sidebar-progress-copy"><strong>{$t('正在生成回答')}</strong><small>{$t('停止后可切换会话')}</small></span><Button.Root type="button" class="conversation-sidebar-stop" title={$t('停止生成')} onclick={()=>onStop()}>{@html icon('stop')}{$t('停止')}</Button.Root></div>{/if}
 <div class="conversation-sidebar-list" bind:this={list}>
  {#each sections as section(section.label)}
   {#if section.label}<div class="conversation-sidebar-group" aria-hidden="true">{$t(section.label)}</div>{/if}
   <ul aria-label={section.label?$t(section.label):$t('会话列表')}>
    {#each section.rows as item(item.conversation.id)}
     {@const current=item.conversation.id===activeId}
     <li class="conversation-sidebar-row" class:current class:searching={terms.length>0}>
      <Button.Root type="button" class="conversation-sidebar-item" data-conversation-index={item.index} title={item.conversation.title} aria-current={current?'page':undefined} disabled={generating&&item.conversation.id!==activeId} onclick={()=>select(item.conversation)} onkeydown={event=>key(event,item.index)}>
       <span class="conversation-sidebar-line"><strong>{item.conversation.title}</strong><span class="conversation-sidebar-meta">{#if current&&generating}<span class="conversation-sidebar-pulse" aria-hidden="true"></span>{/if}<time datetime={new Date(item.conversation.updated).toISOString()} title={new Date(item.conversation.updated).toLocaleString()}>{stamp(item.conversation.updated,item.label)}</time></span></span>
       {#if terms.length&&item.content}<span class="conversation-sidebar-snippet">{snippet(item.content)}</span>{/if}
      </Button.Root>
      <Button.Root type="button" class="conversation-sidebar-more" aria-label={$t('{value0} 的选项',{value0:item.conversation.title})} title={$t('{value0} 的选项',{value0:item.conversation.title})} disabled={generating&&current} onclick={event=>onMenu(event,item.conversation)}>{@html icon('more')}</Button.Root>
     </li>
    {/each}
   </ul>
  {:else}
   <div class="conversation-sidebar-empty">{@html icon(terms.length?'search':'chat')}<strong>{$t(terms.length?'没有找到相关会话':'还没有问答记录')}</strong><p>{$t(terms.length?'试试其他关键词。':'发送第一个问题后，会话会自动保存在这里。')}</p></div>
  {/each}
 </div>
</section>

<style>
 .conversation-sidebar{display:flex;flex-direction:column;gap:8px;height:100%;min-height:0;min-width:0;color:var(--color-ink)}
 .conversation-sidebar-search{display:flex;align-items:center;gap:8px;min-width:0;height:34px;margin:0 2px;padding:0 10px;background:var(--color-paper);border:1px solid var(--color-rule);border-radius:9px;transition:border-color .15s,box-shadow .15s}
 .conversation-sidebar-search :global(.icon){width:14px;height:14px;flex-shrink:0;color:var(--color-muted)}
 .conversation-sidebar-search :global(input){min-width:0;width:100%;padding:0;border:0;background:transparent;font-size:var(--text-caption);outline:none;box-shadow:none}
 .conversation-sidebar-search :global(input::-webkit-search-cancel-button){-webkit-appearance:none;appearance:none}
 .conversation-sidebar-search:focus-within{border-color:var(--color-focus);box-shadow:0 0 0 3px var(--color-hover)}
 .conversation-sidebar-search :global(.conversation-sidebar-clear){display:flex;align-items:center;justify-content:center;flex-shrink:0;width:24px;height:24px;min-height:24px;margin:0 -6px 0 0;padding:6px;border-radius:6px;color:var(--color-muted)}
 .conversation-sidebar-search :global(.conversation-sidebar-clear:hover){background:var(--color-hover);color:var(--color-ink)}
 .conversation-sidebar-search :global(.conversation-sidebar-clear .icon){width:12px;height:12px}
 .conversation-sidebar-progress{display:flex;align-items:center;gap:9px;margin:0 2px;padding:8px 8px 8px 11px;border:1px solid var(--color-rule);border-radius:9px;background:var(--color-paper);box-shadow:0 1px 2px var(--color-shadow)}
 .conversation-sidebar-progress-copy{display:flex;flex-direction:column;flex:1;min-width:0;line-height:1.35}
 .conversation-sidebar-progress-copy strong{font-size:var(--text-caption);font-weight:550}
 .conversation-sidebar-progress-copy small{font-size:11.5px;color:var(--color-muted)}
 .conversation-sidebar-pulse{width:6px;height:6px;flex-shrink:0;border-radius:50%;background:currentColor;animation:conversation-pulse 1.4s ease-in-out infinite}
 @keyframes conversation-pulse{50%{opacity:.3}}
 @media(prefers-reduced-motion:reduce){.conversation-sidebar-pulse{animation:none}}
 :global(.conversation-sidebar-stop){display:inline-flex;align-items:center;gap:5px;flex-shrink:0;height:26px;min-height:26px;padding:0 9px;border:1px solid var(--color-rule);border-radius:7px;background:var(--color-paper);font-size:12px}
 :global(.conversation-sidebar-stop .icon){width:11px;height:11px}
 .conversation-sidebar-list{flex:1;min-height:0;margin:0 -4px;padding:0 4px 8px;overflow:auto;overscroll-behavior:contain}
 .conversation-sidebar-list ul{list-style:none;margin:0;padding:0}
 .conversation-sidebar-group{margin:0;padding:14px 10px 5px;font-size:11.5px;font-weight:550;letter-spacing:.2px;line-height:1.4;color:var(--color-muted)}
 .conversation-sidebar-group:first-child{padding-top:6px}
 .conversation-sidebar-row{position:relative;display:flex;align-items:center;min-width:0;margin-bottom:1px;border-radius:8px;transition:background-color .12s}
 .conversation-sidebar-row:hover{background:var(--color-hover)}
 .conversation-sidebar-row.current,.conversation-sidebar-row.current:hover{background:var(--color-selected)}
 :global(.conversation-sidebar-item){display:flex;flex:1;flex-direction:column;align-items:stretch;gap:3px;min-width:0;min-height:36px;padding:8px 10px;text-align:left;border-radius:8px;background:transparent;white-space:normal}
 :global(.conversation-sidebar-item:hover){background:transparent}
 .conversation-sidebar-line{display:flex;align-items:center;gap:10px;min-width:0}
 .conversation-sidebar-line strong{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:var(--text-ui);font-weight:400;line-height:1.45}
 .conversation-sidebar-row.current .conversation-sidebar-line strong{font-weight:550}
 .conversation-sidebar-meta{display:flex;align-items:center;gap:6px;flex-shrink:0;transition:opacity .12s}
 .conversation-sidebar-meta time{font-size:11.5px;color:var(--color-muted);font-variant-numeric:tabular-nums;white-space:nowrap}
 .conversation-sidebar-snippet{font-size:12px;line-height:1.55;color:var(--color-secondary);display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;overflow-wrap:anywhere}
 /* The menu takes the timestamp's place on hover or focus, so rows stay one line. */
 :global(.conversation-sidebar-more){position:absolute;top:5px;right:4px;display:flex;align-items:center;justify-content:center;width:26px;height:26px;min-height:26px;padding:5px;border-radius:6px;color:var(--color-secondary);opacity:0;transition:opacity .12s}
 :global(.conversation-sidebar-more .icon){width:15px;height:15px}
 .conversation-sidebar-row:is(:hover,:focus-within) :global(.conversation-sidebar-more:not(:disabled)){opacity:1}
 .conversation-sidebar-row:is(:hover,:focus-within):has(:global(.conversation-sidebar-more:not(:disabled))) .conversation-sidebar-meta{opacity:0}
 @media(hover:none){:global(.conversation-sidebar-more:not(:disabled)){opacity:1}.conversation-sidebar-meta{display:none}}
 :global(.conversation-sidebar-item:disabled){opacity:.45;cursor:not-allowed}
 :global(.conversation-sidebar-more:disabled){opacity:0;pointer-events:none}
 :global(.conversation-sidebar-more:not(:disabled):hover),:global(.conversation-sidebar-stop:hover){background:var(--color-selected)}
 :global(.conversation-sidebar button:focus-visible){outline:2px solid var(--color-focus);outline-offset:-2px}
 .conversation-sidebar-empty{display:flex;flex-direction:column;align-items:center;gap:6px;padding:36px 16px;text-align:center;color:var(--color-secondary)}
 .conversation-sidebar-empty :global(.icon){width:22px;height:22px;margin-bottom:4px;color:var(--color-muted)}
 .conversation-sidebar-empty strong{font-size:var(--text-caption);font-weight:550;line-height:1.5;color:var(--color-ink)}
 .conversation-sidebar-empty p{margin:0;font-size:12px;line-height:1.6}
</style>
