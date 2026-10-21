<script lang="ts">
 import OtherFormats from '../../src/components/OtherFormats.svelte';
 import DoclingDownload from '../../src/components/DoclingDownload.svelte';
 export let globalWorkers=false;
 export let enabled=false;
 export let formats:string[]|null|undefined=undefined;
 export let second=false;
 export let removable=false;
 let visible=true;
 let otherEnabled=false;
 let otherFormats:string[]|null|undefined;
</script>
{#if removable}<button aria-label="关闭发起设置" onclick={()=>visible=false}>关闭发起设置</button>{/if}
{#if visible}<OtherFormats workers={globalWorkers?undefined:4} bind:enabled bind:formats/>{/if}
<output aria-label="已应用转换设置">{JSON.stringify({enabled,formats})}</output>
{#if second}<OtherFormats workers={4} bind:enabled={otherEnabled} bind:formats={otherFormats}/><output aria-label="另一文件夹转换设置">{JSON.stringify({enabled:otherEnabled,formats:otherFormats})}</output>{/if}
<DoclingDownload/>
