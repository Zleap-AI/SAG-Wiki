<script lang="ts">
 import LibraryWorkspace from '../../src/components/LibraryWorkspace.svelte';
 import type {Entry} from '../../src/lib/types';
 import type {LibrarySession} from '../../src/lib/explore';
 export let scope:string;
 export let session:LibrarySession;
 export let entries:Entry[];
 export let reading:boolean;
 export let revision:number;
 export let onbrowse:(path:string)=>void;
 export let onopen:(path:string,terms:string[])=>void;
 export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void;
</script>
{#if session.mode==='explore'}<button data-files-mode on:click={()=>session.mode='files'}>返回文件树模式</button>{/if}
<LibraryWorkspace {scope} bind:session {entries} {reading} {revision} {onbrowse} {onopen} {onmenu}/>
