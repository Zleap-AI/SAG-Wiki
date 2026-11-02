<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {listen} from '@tauri-apps/api/event';
 import {api} from './lib/api';
 import {t,setLanguage} from './lib/i18n';
 import {applyTheme,watchSystemTheme} from './lib/theme';
 import {documentResults,sourceName,sourceFolder} from './lib/domain';
 import type {Candidate,QueryOutput,Root,Settings} from './lib/types';
 import {icon,logo} from './lib/icons';
 import {plainText,searchSnippet} from './lib/search-highlights';
 import CitationDocument from './components/CitationDocument.svelte';
 let input:HTMLInputElement,query='',results:Candidate[]=[],terms:string[]=[],roots:Root[]=[],settings:Settings|null=null;
 let selected=0,busy=false,error='',searched=false,deep=false,job='',revision=0,active=true,composing=false,compositionEnded=-Infinity;
 let timer:ReturnType<typeof setTimeout>;
 $:groups=documentResults(results).slice(0,10);
 $:deepReady=!!settings?.jev.url&&!!settings.jev.model&&(settings.jev.auth_mode==='none'||settings.jev.has_key);
 async function refresh(){
  [settings,roots]=await Promise.all([api<Settings>('get_settings'),api<Root[]>('list_roots')]);
  setLanguage(settings.language??'zh-CN');applyTheme(settings.theme);
 }
 async function cancel(){const previous=job;job='';busy=false;if(previous)await api('cancel_query',{id:previous});}
 function changed(){
  revision++;clearTimeout(timer);results=[];selected=0;searched=false;deep=false;error='';
  void cancel().catch(e=>error=String(e));
  if(!composing&&query.trim())timer=setTimeout(()=>void search('search'),250);
 }
 async function search(kind:'search'|'deep_search'){
  clearTimeout(timer);const question=query.trim();if(!question||!settings||!active)return;
  const generation=++revision;
  try{
   await cancel();if(generation!==revision||!active)return;
   const id=crypto.randomUUID();job=id;busy=true;error='';searched=true;selected=0;
   const output=await api<QueryOutput>('run_query',{query:{id,kind,question,scope:[],references:[],model:settings.answer.model,conversation_id:null,replace_from:null,page:kind==='search'?1:null}});
   if(generation===revision&&active){results=output.results;terms=output.terms;deep=kind==='deep_search';}
  }catch(e){if(generation===revision&&active)error=String(e);}
  finally{if(generation===revision){job='';busy=false;}}
 }
 async function open(candidate:Candidate|null){
  try{await api('open_quick_search_result',{request:{query:query.trim(),candidate,deep}})}catch(e){error=String(e)}
 }
 async function hide(){try{await api('hide_quick_search')}catch(e){error=String(e)}}
 function hidden(){active=false;revision++;clearTimeout(timer);void cancel().catch(e=>error=String(e));}
 async function shown(){
  active=true;error='';try{await refresh();await tick();input.focus();input.select();}catch(e){error=String(e)}
 }
 function keys(event:KeyboardEvent){
  if(composing||event.isComposing||event.keyCode===229||performance.now()-compositionEnded<80)return;
  if(event.key==='Escape'){event.preventDefault();void hide();return;}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){
   event.preventDefault();if(groups.length){selected=(selected+(event.key==='ArrowDown'?1:-1)+groups.length)%groups.length;document.getElementById(`quick-result-${selected}`)?.scrollIntoView({block:'nearest'});}return;
  }
  if(event.key==='Enter'&&(event.target===input||(event.target as HTMLElement).getAttribute('role')==='option')){
   event.preventDefault();
   if(event.metaKey||event.ctrlKey)void open(null);
   else if(groups[selected])void open(groups[selected].first);
   else if(!busy)void search('search');
  }
 }
 onMount(()=>{
  const stops:(()=>void)[]=[watchSystemTheme()];let disposed=false;
  void(async()=>{
   for(const [name,handler] of [['quick-search-shown',()=>void shown()],['quick-search-hidden',hidden]] as const){const stop=await listen(name,handler);if(disposed)stop();else stops.push(stop);}
   if(!disposed)await shown();
  })().catch(e=>error=String(e));
  return()=>{disposed=true;hidden();stops.forEach(stop=>stop());};
 });
</script>

<svelte:window on:keydown={keys}/>
<main class="quick-search" aria-label={$t('快捷搜索')}>
 <div class="quick-input-row"><span class="quick-logo">{@html logo}</span><input bind:this={input} bind:value={query} on:input={changed} on:compositionstart={()=>{composing=true;revision++;clearTimeout(timer);}} on:compositionend={()=>{composing=false;compositionEnded=performance.now();changed();}} role="combobox" aria-label={$t('快捷搜索关键词')} aria-controls="quick-results" aria-expanded={groups.length>0} aria-autocomplete="list" aria-activedescendant={groups[selected]?`quick-result-${selected}`:undefined} placeholder={$t('搜索文件、内容或描述问题…')} autocomplete="off" spellcheck="false"/><button class="quick-close" type="button" aria-label={$t('关闭快捷搜索')} on:click={hide}>{@html icon('close')}</button></div>
 <div class="quick-content" class:has-results={groups.length>0&&!busy}>
 <div class="quick-body" aria-busy={busy}>
  {#if error}<p class="quick-error" role="alert">{$t(error)}</p>{/if}
  {#if busy}<div class="quick-message" role="status"><span class="quick-progress"></span>{$t('正在搜索…')}</div>
  {:else if groups.length}<div id="quick-results" role="listbox" aria-label={$t('搜索结果')}>
   {#each groups as group,index (group.first.path)}<button type="button" role="option" aria-selected={selected===index} id={`quick-result-${index}`} tabindex="-1" class="quick-result" class:chosen={selected===index} on:click={()=>selected=index} on:dblclick={()=>open(group.first)}>
    <span class="quick-file-icon">{@html icon('file')}</span><span class="quick-result-copy"><strong>{sourceName(group.first)}</strong><span class="quick-snippet">{@html searchSnippet(plainText(group.first.path,group.first.content),terms)}</span><span class="quick-path">{sourceFolder(group.first.path,roots)}</span></span><span class="quick-enter" aria-hidden="true">↵</span>
   </button>{/each}
  </div>
  {:else}<div class="quick-empty"><span>{@html icon('search')}</span><strong>{$t(searched?'没有找到相关文件':'随时查找你的文件')}</strong><p>{$t(!roots.length?'请先在主窗口添加文件夹。':searched?'尝试其他关键词，或使用深入检索。':'输入关键词，快速查看匹配的原文。')}</p>{#if !roots.length}<button type="button" on:click={()=>open(null)}>{$t('打开主窗口')}</button>{/if}</div>{/if}
 </div>
 {#if groups[selected]&&!busy}<section class="quick-preview" aria-label={$t('原文预览')}>
  <div class="quick-preview-heading"><strong>{sourceName(groups[selected].first)}</strong><button type="button" on:click={()=>open(groups[selected].first)}>{$t('打开原文')} ↗</button></div>
  {#key groups[selected].first.id}<CitationDocument compact source={groups[selected].first} onOpen={()=>open(groups[selected].first)}/>{/key}
 </section>{/if}
 </div>
 <footer class="quick-footer"><span class="quick-hints"><span><kbd>↑</kbd><kbd>↓</kbd> {$t('选择')}</span><span><kbd>↵</kbd> {$t('打开')}</span><span><kbd>Esc</kbd> {$t('关闭')}</span></span><div class="quick-actions"><button type="button" disabled={!query.trim()||!deepReady||busy} title={$t(deepReady?'使用 Jev 查找相关证据':'请先在模型设置中配置 Jev')} on:click={()=>search('deep_search')}>{$t('深入检索')}</button><button type="button" on:click={()=>open(null)}>{$t('完整搜索')} ↗</button></div></footer>
</main>

<style>
 :global(body:has(.quick-search)){margin:0;min-width:0;background:var(--color-paper);overflow:hidden}
 .quick-search{height:100dvh;display:flex;flex-direction:column;background:var(--color-paper);color:var(--color-ink);font-family:var(--font-body);border:1px solid var(--color-rule);box-sizing:border-box}
 .quick-close{margin-left:auto;padding:5px;display:flex;background:none;border:0;color:var(--color-muted);cursor:pointer}.quick-close :global(svg){width:16px;height:16px}.quick-logo,.quick-file-icon{display:flex;align-items:center;justify-content:center;width:32px;height:32px;flex:0 0 32px}.quick-logo :global(img){width:40px;height:40px;max-width:none;object-fit:contain}
 .quick-input-row{display:flex;align-items:center;gap:12px;padding:16px 16px;border-bottom:1px solid var(--color-rule)}.quick-input-row :global(svg){width:21px;height:21px;color:var(--color-muted);flex-shrink:0}.quick-input-row input{width:100%;padding:0;border:0;outline:none;font:inherit;font-size:20px;background:transparent;color:var(--color-ink);box-shadow:none}.quick-input-row input::placeholder{color:var(--color-muted);font-size:18px}
 .quick-content{flex:1;min-height:0;display:flex}.quick-content.has-results{display:grid;grid-template-columns:300px minmax(0,1fr)}.quick-body{flex:1;min-width:0;min-height:0;overflow:auto;padding:7px}.quick-result{width:100%;display:flex;text-align:left;align-items:center;gap:12px;border:0;background:transparent;color:inherit;border-radius:8px;padding:10px 9px;cursor:pointer}.quick-result:hover{background:var(--color-hover)}.quick-result.chosen{background:var(--color-selected)}.quick-result-copy{flex:1;min-width:0;display:flex;gap:3px;flex-direction:column}.quick-result strong{font-size:13px;line-height:18px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.quick-snippet,.quick-path{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:12px;line-height:16px;color:var(--color-secondary)}.quick-path{font-size:11px;line-height:14px;color:var(--color-muted)}.quick-file-icon :global(svg){width:24px;height:24px;color:var(--color-secondary)}.quick-enter{display:none}
 .quick-preview{min-height:0;min-width:0;display:flex;flex-direction:column;border-left:1px solid var(--color-rule)}.quick-preview-heading{height:45px;flex-shrink:0;display:flex;align-items:center;gap:12px;padding:0 18px;border-bottom:1px solid var(--color-rule)}.quick-preview-heading strong{font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0}.quick-preview-heading button{border:0;background:transparent;color:var(--color-secondary);font:inherit;font-size:11px;white-space:nowrap;cursor:pointer}.quick-preview :global(.context-body){flex:1;min-height:0;display:flex;flex-direction:column;padding:0;overflow:hidden}.quick-preview :global(.citation-document){flex:1;min-height:0}.quick-preview :global(.search-preview-body){flex:1;min-height:0;padding:20px 24px;font-size:14px;line-height:1.8;overflow:auto}.quick-preview :global(.search-active-paragraph){background:var(--color-selected);border-radius:5px}.quick-preview :global(.markdown-window){min-height:100%}
 .quick-empty{height:100%;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:10px;color:var(--color-secondary)}.quick-empty>span :global(svg){height:27px;width:27px;opacity:.5}.quick-empty strong{font-size:14px;font-weight:500}.quick-empty p{font-size:12px;margin:0}.quick-message{padding:24px;display:flex;align-items:center;justify-content:center;gap:10px;font-size:13px;color:var(--color-secondary)}.quick-progress{width:7px;height:7px;border-radius:50%;background:var(--color-secondary)}.quick-error{font-size:12px;color:var(--color-danger);padding:8px 14px;white-space:pre-wrap}
 .quick-footer{height:45px;flex-shrink:0;border-top:1px solid var(--color-rule);display:flex;align-items:center;justify-content:space-between;padding:0 15px;gap:8px;font-size:11px}.quick-hints,.quick-hints>span,.quick-actions{display:flex;align-items:center;gap:5px}.quick-hints{gap:12px;color:var(--color-muted)}kbd{font:inherit;border:1px solid var(--color-rule);border-radius:4px;min-width:17px;text-align:center;padding:1px 3px}.quick-actions{gap:9px}.quick-actions button,.quick-empty button{border:1px solid var(--color-rule);border-radius:6px;padding:5px 8px;background:var(--color-panel);color:var(--color-ink);font:inherit;cursor:pointer}.quick-actions button:disabled{opacity:.4;cursor:default}button:focus-visible{outline:2px solid var(--color-focus);outline-offset:2px}
</style>
