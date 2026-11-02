<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import Switch from './ui/Switch.svelte';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import {shortcutFromKey,shortcutLabel,type QuickSearchConfiguration,type QuickSearchPreferences} from '../lib/quick-search';
 let preferences:QuickSearchPreferences={enabled:true,shortcut:'CommandOrControl+Shift+Space'};
 let loaded=false,recording=false,busy=false,error='',status='';
 onMount(()=>{void api<QuickSearchConfiguration>('get_quick_search').then(value=>{preferences=value.preferences;error=value.error??'';loaded=true}).catch(e=>error=String(e))});
 function capture(event:KeyboardEvent){
  if(!recording)return;
  event.preventDefault();event.stopPropagation();
  if(event.key==='Escape'){recording=false;return;}
  const shortcut=shortcutFromKey(event);
  if(shortcut){preferences={...preferences,shortcut};recording=false;status='';error='';}
 }
 async function save(){busy=true;error='';status='';try{await api('save_quick_search',{preferences});status='快捷键已保存并生效'}catch(e){error=String(e)}finally{busy=false}}
</script>
<section class="settings-group quick-search-settings">
 <div class="setting-row"><div><strong>{$t('快捷搜索')}</strong><p>{$t('在其他应用中按全局快捷键搜索，关闭后返回原应用。')}</p></div><Switch label={$t('启用全局快捷键')} bind:checked={preferences.enabled} disabled={!loaded||busy}/></div>
 <div class="setting-row"><label for="quick-search-shortcut">{$t('全局快捷键')}</label><input id="quick-search-shortcut" aria-label={$t('全局快捷键')} readonly value={recording?$t('请按组合键，Esc 取消'):shortcutLabel(preferences.shortcut)} disabled={!loaded||busy} on:focus={()=>recording=true} on:blur={()=>recording=false} on:click={()=>recording=true} on:keydown={capture}/></div>
 <div class="quick-settings-footer">
  <p class="settings-help">{$t('点击快捷键输入框，按住 Ctrl、Alt 或 Command / Win 再按一个键。点击应用快捷键后立即生效。')}</p>
  <div class="quick-settings-actions"><Button.Root type="button" class="outline" disabled={!loaded||busy||recording} onclick={save}>{$t('应用快捷键')}</Button.Root><Button.Root type="button" class="outline" onclick={()=>api('show_quick_search').catch(e=>error=String(e))}>{$t('打开快捷搜索')}</Button.Root></div>
  {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}{#if status}<p class="settings-help" role="status">{$t(status)}</p>{/if}
 </div>
</section>
<style>
 #quick-search-shortcut{width:245px;max-width:100%;min-width:0;text-align:center;cursor:pointer;border:1px solid var(--color-rule);border-radius:8px;padding:9px;background:var(--color-paper);color:var(--color-ink)}
 .quick-settings-footer{display:flex;flex-direction:column;gap:14px;padding:15px 17px}
 .quick-settings-footer p{margin:0}
 .quick-settings-actions{display:flex;align-items:center;justify-content:flex-end;flex-wrap:wrap;gap:10px}
</style>
