<script lang="ts">
 import {t} from '../lib/i18n';
 import{Button,Checkbox}from 'bits-ui';
 import {icon} from '../lib/icons';import {displayPath,inside} from '../lib/domain';import type{Entry,Root}from '../lib/types';
 export let nodes:(Entry|Root)[]=[];export let depth=0;export let expanded:Set<string>=new Set();export let cache:Record<string,Entry[]>={};export let selected='';export let mode:'browse'|'scope'='browse';export let checked:string[]=[];
 export let onbrowse:(path:string)=>void=()=>{};export let ontoggle:(path:string)=>void=()=>{};export let oncheck:(path:string)=>void=()=>{};export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void=()=>{};
</script>
{#each nodes as node(node.path)}
 {@const directory='directory'in node?node.directory:true}{@const root='status'in node}{@const open=expanded.has(node.path)}
 {#if mode==='browse'||directory}
 <div class="tree-row" class:selected={selected===node.path} style:padding-left={`min(${depth*14}px, max(0px, calc(100% - 128px)))`} on:contextmenu={(e)=>{e.preventDefault();onmenu(e,node.path,root,!directory)}} role="presentation">
  {#if directory&&!('symlink'in node&&node.symlink)}<Button.Root class="tree-toggle" aria-expanded={open} aria-label={`${open?$t("折叠"):$t("展开")}${node.name}`} onclick={()=>ontoggle(node.path)}>{@html icon(open?'down':'chevron')}</Button.Root>{:else}<span class="tree-toggle"></span>{/if}
  {#if mode==='scope'}<label class="scope-tree-label"><Checkbox.Root class="ui-checkbox" checked={checked.some(p=>inside(node.path,p))} onCheckedChange={()=>oncheck(node.path)} aria-label={node.name}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root>{@html icon('folder')}<span>{node.name}</span></label>
  {:else}<Button.Root class="tree-name" aria-current={selected===node.path?'page':undefined} title={displayPath(node.path)} onclick={()=>onbrowse(node.path)}>{@html icon(directory?'folder':'file')}<span>{node.name}</span></Button.Root><Button.Root class="tree-more" title={$t(directory?"文件夹选项":"文件选项")} aria-haspopup="menu" aria-label={$t(directory?"{value0} 的文件夹选项":"{value0} 的文件选项",{value0:node.name})} onclick={(e:MouseEvent)=>onmenu(e,node.path,root,!directory)}>{@html icon('more')}</Button.Root>{/if}
 </div>
 {#if open&&directory}<svelte:self nodes={cache[node.path]||[]} depth={depth+1} {expanded} {cache} {selected} {mode} {checked} {onbrowse} {ontoggle} {oncheck} {onmenu}/>{/if}
 {/if}
{/each}
