<script lang="ts">
 import OtherFormats from '../../src/components/OtherFormats.svelte';
 import DoclingDownload from '../../src/components/DoclingDownload.svelte';
 export let globalWorkers=false;
 export let enabled=false;
 export let formats:string[]|null|undefined=undefined;
</script>
<OtherFormats workers={globalWorkers?undefined:4} bind:enabled bind:formats/>
<output aria-label="已应用转换设置">{JSON.stringify({enabled,formats})}</output>
<DoclingDownload/>
