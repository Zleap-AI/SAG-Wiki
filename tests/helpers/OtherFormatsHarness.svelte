<script lang="ts">
 import OtherFormats from '../../src/components/OtherFormats.svelte';
 import DoclingDownload from '../../src/components/DoclingDownload.svelte';
 export let globalWorkers=false;
</script>
<OtherFormats workers={globalWorkers?undefined:4}/>
<DoclingDownload/>
