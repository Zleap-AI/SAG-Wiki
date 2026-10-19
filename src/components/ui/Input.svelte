<!-- Adapted from shadcn-svelte Input (MIT); styles use the app's existing tokens. -->
<script lang="ts">
 import type { HTMLInputAttributes } from 'svelte/elements';
 let {ref=$bindable(),value=$bindable(),type='text',class:className='',...restProps}:HTMLInputAttributes & {ref?:HTMLInputElement|null}=$props();
</script>
<input bind:this={ref} data-slot="input" class={`ui-input ${className}`} {type} bind:value {...restProps}/>
