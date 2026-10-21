<script lang="ts">
 import OtherFormats from './OtherFormats.svelte';
 import {Checkbox} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import type {FileTypes,FolderIndexOptions} from '../lib/types';
 export let value:FolderIndexOptions;
 export let disabled=false;
 const types:[keyof FileTypes,string,string][]=[['markdown','Markdown','.md · .markdown'],['text','文本与 RTF','.txt · .rst · .rtf'],['code','代码','Python · JavaScript · Rust · HTML · CSS'],['config','配置文件','JSON · YAML · TOML · XML · INI']];
</script>
<fieldset {disabled} class="folder-index-types">
 {#if value.extensions}<p>{value.extensions.map(extension=>`.${extension}`).join(' · ')}</p>{/if}
 {#each types as [kind,label,description]}
  <div class="setting-row"><label for={`folder-type-${kind}`}><strong>{$t(label)}</strong><p>{description}</p></label><Checkbox.Root id={`folder-type-${kind}`} class="ui-checkbox" aria-label={$t('索引 {value0}',{value0:$t(label)})} checked={value.file_types[kind]} {disabled} onCheckedChange={checked=>value={...value,extensions:undefined,file_types:{...value.file_types,[kind]:checked}}}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root></div>
 {/each}
 <OtherFormats bind:enabled={value.convert_other_formats} bind:formats={value.conversion_formats} {disabled}/>
</fieldset>
<style>
 .folder-index-types{border:0;padding:0;margin:0;min-width:0}
 .folder-index-types :global(.setting-row){padding:12px 0;min-height:64px;gap:20px;align-items:center}
 .setting-row label{cursor:pointer;margin:0;min-width:0}
 .folder-index-types :global(.setting-row strong){font-size:13px;font-weight:500;line-height:1.5}
 .folder-index-types :global(.setting-row p){margin:4px 0 0;font-size:12px;line-height:1.6;color:var(--color-muted);overflow-wrap:anywhere}
 .folder-index-types :global(.other-formats){border-bottom:0}
 .folder-index-types :global(.other-formats .title){gap:7px}
 .folder-index-types :global(.other-formats .title label){margin:0;line-height:1.5}
</style>
