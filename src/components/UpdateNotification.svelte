<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {checkUpdates,defaultUpdateSettings,updateKey,updateState} from '../lib/updates';
 import type {UpdateSettings} from '../lib/types';
 export let config:UpdateSettings=defaultUpdateSettings;
 export let onOpen:()=>void;
 let mounted=false,dismissed='',checkedKey='';
 onMount(()=>{
  mounted=true;
  const timer=setInterval(()=>{void checkUpdates(config)},8*60*60*1000);
  return()=>clearInterval(timer);
 });
 $: key=updateKey(config);
 $: if(mounted&&checkedKey!==key){checkedKey=key;void checkUpdates(config)}
 $: info=$updateState.key===key?$updateState.info:null;
</script>

{#if info?.available&&dismissed!==`${key}:${info.latest_version}`}
 <aside class="update-notification" aria-label={$t('软件更新')}>
  <Button.Root class="update-open" onclick={()=>{dismissed=`${key}:${info!.latest_version}`;onOpen()}}>{$t('发现新版本 {value0}',{value0:info.latest_version})}<span>{$t('查看更新')}</span></Button.Root>
  <Button.Root class="icon-button" aria-label={$t('关闭更新提示')} onclick={()=>dismissed=`${key}:${info!.latest_version}`}>{@html icon('close')}</Button.Root>
 </aside>
{/if}

<style>
 .update-notification{position:fixed;top:50px;right:24px;z-index:40;display:flex;align-items:center;gap:8px;padding:8px 10px 8px 16px;border:1px solid var(--color-rule);border-radius:11px;background:var(--color-paper);box-shadow:0 4px 20px var(--color-shadow)}
 .update-notification :global(.update-open){display:flex;align-items:center;gap:16px;font-size:var(--text-ui);color:var(--color-ink)}
 .update-notification span{font-size:var(--text-caption);color:var(--color-secondary);text-decoration:underline;text-underline-offset:3px}
</style>
