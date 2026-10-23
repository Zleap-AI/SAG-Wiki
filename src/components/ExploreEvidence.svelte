<script lang="ts">
 import {Button} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import {highlightedText} from '../lib/search-highlights';
 import {displayPath,sourceLocation} from '../lib/domain';
 import type {ExploreEvidence} from '../lib/explore';
 export let evidence:ExploreEvidence|null=null;
 export let error='';
 export let loading=false;
 export let onClose:()=>void;
 export let onOpen:(path:string,terms:string[])=>void;
</script>
<Modal label={$t('关联依据')} drawer onClose={onClose} class="explore-evidence">
 <div class="source-head"><strong>{$t('关联依据')}</strong><Button.Root class="icon-button" aria-label={$t('关闭')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
 <div class="explore-evidence-body">
  {#if loading}<p role="status">{$t('正在读取原文…')}</p>{:else if error}<p role="alert">{$t(error)}</p>{:else if evidence}
   <p class="explore-caption">{$t('根据共同词语关联，请结合原文判断。')}</p>
   {#each evidence.pairs as pair,index}
    <section class="evidence-pair"><h3>{$t('原文对照 {value0}',{value0:index+1})}</h3><div class="explore-terms">{#each pair.terms as term}<span>{term}</span>{/each}</div>
     {#each [pair.left,pair.right] as source}
      <article><strong>{source.title}</strong><p class="source-path">{displayPath(source.path)}</p><small>{source.heading} · {sourceLocation(source)}</small><blockquote>{@html highlightedText(source.content,pair.terms)}</blockquote><Button.Root class="outline" onclick={()=>onOpen(source.path,pair.terms)}>{$t('打开文档')}</Button.Root></article>
     {/each}
    </section>
   {/each}
  {/if}
 </div>
</Modal>
