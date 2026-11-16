<script lang="ts">
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t,language} from '../lib/i18n';
 import {icon} from '../lib/icons';
 export let onError:(message:string)=>void;
 async function openHelp(){
  const locale=$language==='en'?'en':'zh';
  try{await api('open_external_link',{url:`http://sag-wiki.zleap.com/${locale}/docs/file-sharing`})}
  catch(error){onError(String(error))}
 }
</script>
<Button.Root type="button" class="share-help" aria-label={$t('分享码使用说明')} title={$t('分享码使用说明')} onclick={openHelp}>{@html icon('help')}</Button.Root>
<style>
 :global(.share-help){display:inline-flex;align-items:center;justify-content:center;width:24px;height:24px;padding:4px;flex-shrink:0;color:var(--color-muted);border-radius:4px;vertical-align:middle}
 :global(.share-help:hover){color:var(--color-ink);background:var(--color-hover)}
 :global(.share-help:focus-visible){outline:2px solid var(--color-focus);outline-offset:2px}
 :global(.share-help .icon){width:15px;height:15px;stroke-width:1.5}
</style>
