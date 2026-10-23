<script lang="ts">
 import {displayPath} from '../lib/domain';
 import {t} from '../lib/i18n';
 import{Button,Checkbox}from 'bits-ui';
 import {icon} from '../lib/icons';import {inside} from '../lib/domain';import type{Entry,Root}from '../lib/types';
 export let nodes:(Entry|Root)[]=[];export let depth=0;export let expanded:Set<string>=new Set();export let cache:Record<string,Entry[]>={};export let selected='';export let mode:'browse'|'scope'|'folder'='browse';export let checked:string[]=[];
 export let onbrowse:(path:string)=>void=()=>{};export let ontoggle:(path:string)=>void=()=>{};export let oncheck:(path:string)=>void=()=>{};export let onmenu:(event:MouseEvent,path:string,isRoot:boolean,file:boolean)=>void=()=>{};
</script>
{#each nodes as node(node.path)}
 {@const directory='directory'in node?node.directory:true}{@const root='status'in node}{@const open=expanded.has(node.path)}
 {#if mode==='browse'||directory}
 <div class="tree-row" class:selected={selected===node.path} class:folder-choice={mode==='folder'} style:padding-left={`min(${depth*14}px, max(0px, calc(100% - 128px)))`} on:contextmenu={(e)=>{e.preventDefault();onmenu(e,node.path,root,!directory)}} role="presentation">
  {#if directory&&!('symlink'in node&&node.symlink)}<Button.Root type="button" class="tree-toggle" aria-expanded={open} aria-label={`${open?$t("折叠"):$t("展开")}${node.name}`} onclick={()=>ontoggle(node.path)}>{@html icon(open?'down':'chevron')}</Button.Root>{:else}<span class="tree-toggle"></span>{/if}
  {#if mode==='scope'}<label class="scope-tree-label"><Checkbox.Root class="ui-checkbox" checked={checked.some(p=>inside(node.path,p))} onCheckedChange={()=>oncheck(node.path)} aria-label={node.name}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root>{@html icon('folder')}<span>{node.name}</span></label>
  {:else if mode==='folder'}<Button.Root type="button" class="folder-choice-name" aria-pressed={selected===node.path} title={node.name} onclick={()=>onbrowse(node.path)}>{@html icon('folder')}<span>{node.name}</span>{#if selected===node.path}{@html icon('check')}{/if}</Button.Root>
  {:else}<Button.Root class="tree-name" aria-current={selected===node.path?'page':undefined} title={displayPath(node.path)} onclick={()=>onbrowse(node.path)}>{@html icon(directory?'folder':'file')}<span>{node.name}</span></Button.Root><Button.Root class="tree-more" title={$t(directory?"文件夹选项":"文件选项")} aria-haspopup="menu" aria-label={$t(directory?"{value0} 的文件夹选项":"{value0} 的文件选项",{value0:node.name})} onclick={(e:MouseEvent)=>onmenu(e,node.path,root,!directory)}>{@html icon('more')}</Button.Root>{/if}
 </div>
 {#if open&&directory}<svelte:self nodes={cache[node.path]||[]} depth={depth+1} {expanded} {cache} {selected} {mode} {checked} {onbrowse} {ontoggle} {oncheck} {onmenu}/>{/if}
 {/if}
{/each}

<style>
 .folder-choice{border-radius:7px;margin:2px 0}
 .folder-choice.selected{background:var(--color-selected)}
 .folder-choice:hover{background:var(--color-hover)}
 .folder-choice :global(.tree-toggle){width:28px;min-width:28px;height:32px;align-self:center;color:var(--color-secondary)}
 .folder-choice :global(.tree-toggle>.icon){width:13px;height:13px}
 .folder-choice :global(.folder-choice-name){display:flex;align-items:center;gap:9px;flex:1;min-width:0;padding:8px 10px 8px 2px;text-align:left;background:transparent;font-size:var(--text-ui);line-height:1.4}
 .folder-choice :global(.folder-choice-name>.icon){width:17px;height:17px;flex-shrink:0;color:var(--color-secondary)}
 .folder-choice :global(.folder-choice-name>span){flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
</style>
