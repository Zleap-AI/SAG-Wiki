<script lang="ts">
 import {onMount} from 'svelte';
 import {getCurrentWindow} from '@tauri-apps/api/window';
 import {t} from '../lib/i18n';
 let maximized=false;
 async function updateMaximized(){maximized=await getCurrentWindow().isMaximized()}
 async function toggleMaximized(){await getCurrentWindow().toggleMaximize();await updateMaximized()}
 onMount(()=>{
  if(!('isTauri' in window&&window.isTauri))return;
  let disposed=false;
  const update=()=>{void getCurrentWindow().isMaximized().then(value=>{if(!disposed)maximized=value})};
  update();window.addEventListener('resize',update);
  return()=>{disposed=true;window.removeEventListener('resize',update)};
 });
</script>

<div class="window-controls windows">
  <button type="button" aria-label={$t('最小化')} title={$t('最小化')} on:click={()=>getCurrentWindow().minimize()}><svg aria-hidden="true" viewBox="0 0 12 12"><path d="M1 6.5h10"/></svg></button>
  <button type="button" aria-label={$t(maximized?'还原窗口':'最大化')} title={$t(maximized?'还原窗口':'最大化')} on:click={toggleMaximized}><svg aria-hidden="true" viewBox="0 0 12 12">{#if maximized}<path d="M3.5 3.5v-2h7v7h-2m-7-5h7v7h-7z"/>{:else}<path d="M1.5 1.5h9v9h-9z"/>{/if}</svg></button>
  <button type="button" class="caption-close" aria-label={$t('关闭')} title={$t('关闭')} on:click={()=>getCurrentWindow().close()}><svg aria-hidden="true" viewBox="0 0 12 12"><path d="m1.5 1.5 9 9m0-9-9 9"/></svg></button>
</div>

<style>
 .window-controls{display:flex;height:32px}
 .window-controls button{display:flex;align-items:center;justify-content:center;width:24px;height:32px;padding:0;border:0;border-radius:0;flex-shrink:0;color:var(--color-ink);background:transparent}
 .window-controls button:hover{background:var(--color-hover)}
 .window-controls button:focus-visible{outline-offset:-3px}
 .window-controls .caption-close:hover{background:#c42b1c;color:#fff}
 svg{width:12px;height:12px;fill:none;stroke:currentColor;stroke-width:1}
</style>
