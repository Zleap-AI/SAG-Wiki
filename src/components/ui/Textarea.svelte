<!-- Adapted from shadcn-svelte Textarea (MIT); native events and selection are preserved. -->
<script lang="ts">
 import type { HTMLTextareaAttributes } from 'svelte/elements';
 let {ref=$bindable(),value=$bindable(),class:className='',...restProps}:HTMLTextareaAttributes & {ref?:HTMLTextAreaElement|null}=$props();
</script>
<textarea bind:this={ref} data-slot="textarea" class={`ui-textarea ${className}`} bind:value {...restProps}></textarea>
