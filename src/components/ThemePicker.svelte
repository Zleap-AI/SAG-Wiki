<script lang="ts">
 import {t} from '../lib/i18n';
 export let value:string;
 $: choices=[{value:'system',label:$t('跟随系统')},{value:'light',label:$t('浅色')},{value:'dark',label:$t('深色')}];
</script>

<fieldset class="theme-picker">
 <legend class="theme-legend">{$t('主题')}</legend>
 {#each choices as choice}
  <label class="theme-choice" title={choice.label}>
   <input type="radio" name="appearance-theme" value={choice.value} bind:group={value}/>
   <span class="theme-thumbnail" aria-hidden="true">
    <span class="theme-window" class:dark={choice.value==='dark'}>
     <span class="mini-sidebar"><span class="mini-logo"></span><i></i><i></i><i></i></span>
     <span class="mini-content"><i class="mini-heading"></i><i></i><i></i><i class="mini-message"></i><span class="mini-composer"><b></b></span></span>
    </span>
    {#if choice.value==='system'}
     <span class="theme-system-half"><span class="theme-window dark">
      <span class="mini-sidebar"><span class="mini-logo"></span><i></i><i></i><i></i></span>
      <span class="mini-content"><i class="mini-heading"></i><i></i><i></i><i class="mini-message"></i><span class="mini-composer"><b></b></span></span>
     </span></span>
    {/if}
   </span>
   <span class="theme-label">{choice.label}</span>
  </label>
 {/each}
</fieldset>

<style>
 .theme-picker{display:flex;gap:14px;flex-shrink:0;border:0;padding:0;margin:0;min-width:0}
 .theme-legend,.theme-choice input{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;border:0}
 .theme-choice{position:relative;display:grid;gap:7px;justify-items:center;cursor:pointer;margin:0;font-weight:400}
 .theme-thumbnail{--preview-accent:#ff636d;position:relative;display:block;width:80px;height:60px;padding:5px;border:2px solid transparent;border-radius:12px;background:var(--color-hover);transition:border-color .15s,box-shadow .15s;overflow:hidden}
 .theme-choice:hover .theme-thumbnail{border-color:var(--color-muted)}
 .theme-choice input:checked+.theme-thumbnail{border-color:var(--color-accent);box-shadow:0 0 0 2px var(--color-paper)}
 .theme-choice input:focus-visible+.theme-thumbnail{outline:2px solid var(--color-focus);outline-offset:4px}
 .theme-label{font-size:11px;line-height:1.4;color:var(--color-secondary);white-space:nowrap}
 .theme-choice input:checked~.theme-label{color:var(--color-ink);font-weight:600}
 .theme-window{--preview-paper:#fff;--preview-sidebar:#f1f1f1;--preview-line:#e7e7e7;--preview-text:#c8c8c8;display:flex;width:66px;height:46px;border:1px solid var(--preview-line);border-radius:6px;background:var(--preview-paper);overflow:hidden;box-shadow:0 1px 3px #0000000c}
 .theme-window.dark{--preview-paper:#171717;--preview-sidebar:#303030;--preview-line:#404040;--preview-text:#666}
 .mini-sidebar{display:flex;flex-direction:column;align-items:center;gap:3px;width:13px;flex-shrink:0;padding-top:5px;background:var(--preview-sidebar);border-right:1px solid var(--preview-line)}
 .mini-logo{width:5px;height:5px;border:1px solid var(--preview-text);border-radius:50%;margin-bottom:3px}
 .mini-sidebar i{width:5px;height:1px;background:var(--preview-text);border-radius:1px}
 .mini-content{display:flex;flex-direction:column;align-items:flex-start;gap:3px;flex:1;padding:5px 5px 3px}
 .mini-content>i{display:block;width:80%;height:1px;border-radius:1px;background:var(--preview-line)}
 .mini-content>.mini-heading{width:70%;background:var(--preview-accent);margin-bottom:1px}
 .mini-content>.mini-message{align-self:flex-end;width:60%;margin-top:2px;background:var(--preview-accent)}
 .mini-composer{display:flex;align-items:center;justify-content:flex-end;width:100%;height:9px;margin-top:auto;border:1px solid var(--preview-line);border-radius:3px;padding:2px}
 .mini-composer b{width:3px;height:3px;background:var(--preview-accent);border-radius:50%}
 .theme-system-half{position:absolute;inset:5px 5px 5px 50%;overflow:hidden;background:#252525;border-radius:0 6px 6px 0}
 .theme-system-half .theme-window{position:absolute;right:0;top:0}
 @media(prefers-reduced-motion:reduce){.theme-thumbnail{transition:none}}
 @media(max-width:600px){.theme-picker{gap:10px}}
</style>
