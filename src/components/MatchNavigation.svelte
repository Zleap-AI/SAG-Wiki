<script lang="ts">
 import {t} from '../lib/i18n';
 import{Button}from 'bits-ui';
 import{icon}from '../lib/icons';
 export let count=0;export let index=0;export let onMove:(step:number)=>void;
</script>
{#if count}<div class="match-navigation native-match-navigation" role="group" aria-label={$t("匹配位置")}><span aria-live="polite">{$t("{value0} / {value1} 处匹配",{value0:index+1,value1:count})}</span><div class="match-navigation-buttons"><Button.Root type="button" class="icon-button match-previous" aria-label={$t("上一处匹配")} title={$t("上一处匹配")} onclick={()=>onMove(-1)}>{@html icon('chevron')}</Button.Root><Button.Root type="button" class="icon-button match-next" aria-label={$t("下一处匹配")} title={$t("下一处匹配")} onclick={()=>onMove(1)}>{@html icon('chevron')}</Button.Root></div></div>{/if}

<style>
 .match-navigation.native-match-navigation{min-height:34px;gap:10px;font-size:12px;line-height:1.2}
 .native-match-navigation>span{margin:0;white-space:nowrap;color:var(--color-secondary);font-variant-numeric:tabular-nums}
 .match-navigation-buttons{display:flex;align-items:center;border:1px solid var(--color-rule);border-radius:7px;overflow:hidden;background:var(--color-paper)}
 .native-match-navigation :global(.icon-button){display:flex;align-items:center;justify-content:center;width:28px;height:28px;padding:0;border:0;border-radius:0;background:var(--color-transparent);color:var(--color-secondary)}
 .native-match-navigation :global(.icon-button)+:global(.icon-button){border-left:1px solid var(--color-rule)}
 .native-match-navigation :global(.icon-button):hover{background:var(--color-hover);color:var(--color-ink)}
 .native-match-navigation :global(.icon-button):active{background:var(--color-selected)}
 .native-match-navigation :global(.icon-button):focus-visible{outline:2px solid var(--color-focus);outline-offset:-2px}
 .native-match-navigation :global(.icon){width:16px;height:16px}
</style>
