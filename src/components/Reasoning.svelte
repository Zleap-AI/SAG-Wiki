<script lang="ts">
 import {t} from '../lib/i18n';
 import {Collapsible} from 'bits-ui';
 import {icon} from '../lib/icons';
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
  <div class="reasoning-text">{content}</div>
 </Collapsible.Content>
</Collapsible.Root>
