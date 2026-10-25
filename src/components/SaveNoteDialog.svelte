<script lang="ts">
 import {Button,Checkbox} from 'bits-ui';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 import Tree from './Tree.svelte';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import {t} from '../lib/i18n';
 import {basename,displayPath,fileName} from '../lib/domain';
 import type {Entry,Message,NoteDetail,Root} from '../lib/types';

 export let message:Message;
 export let conversationId:string;
 export let messageIndex:number;
 export let roots:Root[];
 export let defaultFolder:string|null|undefined=undefined;
 export let cache:Record<string,Entry[]>={};
 export let onClose:()=>void;
 export let onSaved:(note:NoteDetail,context:{conversationId:string;messageIndex:number})=>void;
 export let onAddFolder:(()=>void)|undefined=undefined;

 const suggested=message.question.replace(/[\u0000-\u001f/\\:?*<>|"]/g,' ').replace(/\s+/g,' ').trim().slice(0,60).replace(/[. ?!,;:？！。，、；：]+$/,'').trim()||'note';
 let name=suggested+'.md';
 let folder=defaultFolder&&roots.some(root=>folder_inside(defaultFolder!,root.path))?defaultFolder:'';
 let remember=!!defaultFolder;
 let expanded=new Set<string>(),error='',saving=false;
 function folder_inside(path:string,root:string){return path===root||path.startsWith(root.replace(/[\\/]$/,'')+(root.includes('\\')?'\\':'/'));}
 $:citedFiles=new Set(cited().map(c=>c.path)).size;
 function cited(){const labels=new Set<number>();for(const m of message.answer.replace(/```[\s\S]*?```|`[^`\n]*`/g,'').matchAll(/\[(\d+(?:[ ,，]+\d+)*)\]/g)){for(const n of m[1].match(/\d+/g)??[]){const i=Number(n);if(i>0&&i<=message.evidence.length)labels.add(i);}}return[...labels].map(i=>message.evidence[i-1]);}
 $:citedCount=cited().length;
 async function toggle(path:string){error='';if(expanded.has(path)){expanded=new Set([...expanded].filter(p=>p!==path));return;}try{cache={...cache,[path]:await api<Entry[]>('list_entries',{path})};expanded=new Set([...expanded,path]);}catch(e){error=$t(String(e));}}
 async function save(event:SubmitEvent){
  event.preventDefault();
  if(saving)return;
  let file:string;
  try{file=fileName(name);}catch(e){error=$t(String(e));return;}
  saving=true;error='';
  try{
   const note=await api<NoteDetail>('save_note',{conversationId,messageIndex,target:folder||null,fileName:file,expectedHash:null,remember:remember&&!!folder});
   onSaved(note,{conversationId,messageIndex});
  }catch(e){error=$t(String(e));}
  finally{saving=false;}
 }
</script>

<Modal label={$t('保存为笔记')} class="new-document-dialog save-note-dialog" {onClose}>
 <form on:submit={save}>
  <div class="dialog-head"><h2>{$t('保存为笔记')}</h2><Button.Root type="button" class="icon-button" aria-label={$t('关闭')} onclick={onClose}>{@html icon('close')}</Button.Root></div>
  <div class="dialog-body">
   <label class="save-document-name"><span>{$t('文件名')}</span><Input data-autofocus aria-label={$t('文件名')} bind:value={name} required onfocus={event=>event.currentTarget.select()} oninput={()=>error=''}/></label>
   <p class="save-note-summary">{$t('将写入回答正文')}{#if citedCount} · {$t('{value0} 条引用，来自 {value1} 个文件',{value0:citedCount,value1:citedFiles})}{:else} · {$t('此回答没有引用，保存后不会校验')}{/if}</p>
   {#if roots.length}
    <p class="save-note-location"><span>{$t('保存位置')}</span><strong>{folder?basename(folder):$t('默认笔记文件夹')}</strong>{#if folder}<small title={folder}>{displayPath(folder)}</small>{:else}<small>{$t('文稿 / sag-wiki / notes，不存在时自动创建并加入知识库')}</small>{/if}</p>
    <div class="save-folder-tree" role="group" aria-label={$t('保存位置')}><Tree nodes={roots} {expanded} {cache} mode="folder" selected={folder} onbrowse={path=>folder=folder===path?'':path} ontoggle={toggle}/></div>
    <label class="save-note-remember"><Checkbox.Root class="ui-checkbox" bind:checked={remember} disabled={!folder} aria-label={$t('记住此位置，下次直接保存')}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root><span>{$t('记住此位置，下次直接保存')}</span></label>
   {:else}
    <p>{$t('没有文件夹时，笔记会保存到默认笔记文件夹并自动加入知识库。')}</p>
    {#if onAddFolder}<Button.Root type="button" class="outline" onclick={onAddFolder}>{@html icon('plus')}{$t('添加文件夹')}</Button.Root>{/if}
   {/if}
   {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  </div>
  <div class="dialog-footer"><Button.Root type="button" onclick={onClose} disabled={saving}>{$t('取消')}</Button.Root><Button.Root type="submit" class="primary" disabled={saving||!name.trim()}>{$t(saving?'正在保存…':'保存')}</Button.Root></div>
 </form>
</Modal>

<style>
 .save-note-summary{font-size:var(--text-caption);color:var(--color-secondary);margin:4px 0 12px}
 .save-note-location{display:flex;flex-wrap:wrap;align-items:baseline;gap:6px;font-size:var(--text-ui);margin-bottom:8px}
 .save-note-location span{color:var(--color-secondary)}
 .save-note-location small{flex-basis:100%;color:var(--color-muted);font-size:var(--text-caption);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .save-note-remember{display:flex;align-items:center;gap:8px;margin-top:12px;font-size:var(--text-ui)}
</style>
