<script lang="ts">
 import {t} from '../lib/i18n';
 import {Collapsible} from 'bits-ui';
 import {icon} from '../lib/icons';
 import {answerHtml} from '../lib/answer-html';
 export let content='';
 export let running=false;
 let open=false;
 $: open=running;
</script>

<Collapsible.Root class="reasoning" bind:open>
 <Collapsible.Trigger class="reasoning-toggle">
  {#if running}<span class="progress-indicator" aria-hidden="true"></span>{/if}
  <span>{running?$t("思考中"):$t("思考过程")}</span>
  <span class="reasoning-chevron" aria-hidden="true">{@html icon('chevron')}</span>
 </Collapsible.Trigger>
 <Collapsible.Content class="reasoning-content">
  {#await import('../lib/markdown')}
   <div class="reasoning-text reasoning-plain">{content}</div>
  {:then markdown}
   <div class="reasoning-text" use:answerHtml={markdown.renderMarkdown(content)}></div>
  {/await}
 </Collapsible.Content>
</Collapsible.Root>
