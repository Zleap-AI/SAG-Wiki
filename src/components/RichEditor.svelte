<script lang="ts">
 import {t} from '../lib/i18n';
 import Select from "./ui/Select.svelte";
 import{Button}from 'bits-ui';
 import './editor.css';
 import{icon}from '../lib/icons';
 import{TaskList,TaskItem}from '@tiptap/extension-list';
 import{onMount,onDestroy}from 'svelte';import{Editor,Extension}from '@tiptap/core';import{Plugin,PluginKey}from '@tiptap/pm/state';import{Decoration,DecorationSet}from '@tiptap/pm/view';import{editorMatches}from '../lib/search-highlights';import type{TextMatch}from '../lib/search-highlights';import MatchNavigation from './MatchNavigation.svelte';import StarterKit from '@tiptap/starter-kit';import{Markdown}from '@tiptap/markdown';import{TableKit}from '@tiptap/extension-table';import Placeholder from '@tiptap/extension-placeholder';import{LocalImage,splitFrontmatter,protectHtml,restoreHtml,hydrateImages}from '../lib/markdown';
 export let searchTerms:string[]=[];export let body='';export let path='';export let initialMatch=0;export let onChange:(body:string)=>void=()=>{};
 let host:HTMLDivElement;let editor:Editor;let style='paragraph';let active:string[]=[];let canUndo=false,canRedo=false;const original=splitFrontmatter(body);
 $:formatGroups=[
  [{name:'bold',label:$t("加粗")},{name:'italic',label:$t("斜体")},{name:'strike',label:$t("删除线")}],
  [{name:'bulletList',label:$t("无序列表")},{name:'orderedList',label:$t("有序列表")},{name:'blockquote',label:$t("引用")},{name:'codeBlock',label:$t("代码块")}]
 ];
 let matchCount=0,matchIndex=0;
 const matchKey=new PluginKey<{matches:TextMatch[];index:number}>('search-highlights');
 const highlights=Extension.create({name:'searchHighlights',addProseMirrorPlugins(){return [new Plugin({key:matchKey,
  state:{init:(_,state)=>({matches:editorMatches(state.doc,searchTerms),index:0}),apply(tr,value){const matches=tr.docChanged?editorMatches(tr.doc,searchTerms):value.matches;return {matches,index:tr.getMeta(matchKey)??Math.min(value.index,Math.max(0,matches.length-1))}}},
  props:{decorations(state){const {matches,index}=matchKey.getState(state)!;return DecorationSet.create(state.doc,matches.map((match,i)=>Decoration.inline(match.from,match.to,{class:'search-hit'+(i===index?' active':''),'data-search-match':String(i)})))}}
 })]}});
 function selectMatch(next:number){if(!matchCount)return;const index=(next+matchCount)%matchCount;editor.view.dispatch(editor.state.tr.setMeta(matchKey,index));host.querySelector<HTMLElement>(`[data-search-match="${index}"]`)!.scrollIntoView({block:'center'});}
 function update(){style=[1,2,3].find(level=>editor.isActive('heading',{level}))?.toString()||'paragraph';active=['bold','italic','strike','bulletList','orderedList','blockquote','codeBlock'].filter(name=>editor.isActive(name));canUndo=editor.can().undo();canRedo=editor.can().redo();}
 function format(name:string){const c=editor.chain().focus();switch(name){case'bold':c.toggleBold().run();break;case'italic':c.toggleItalic().run();break;case'strike':c.toggleStrike().run();break;case'bulletList':c.toggleBulletList().run();break;case'orderedList':c.toggleOrderedList().run();break;case'blockquote':c.toggleBlockquote().run();break;case'codeBlock':c.toggleCodeBlock().run();break;case'undo':c.undo().run();break;case'redo':c.redo().run();break;}update();}
 onMount(()=>{editor=new Editor({element:host,extensions:[highlights,StarterKit.configure({link:{openOnClick:false}}),Markdown,TableKit,LocalImage,TaskList,TaskItem.configure({nested:true}),Placeholder.configure({placeholder:()=>$t("开始写作，也可以直接粘贴 Markdown…")})],content:protectHtml(original.body),contentType:'markdown',editorProps:{attributes:{role:'textbox','aria-label':$t("文档正文"),'aria-multiline':'true'},handlePaste:(_view,event)=>{const text=event.clipboardData?.getData('text/plain');if(!text||event.clipboardData?.getData('text/html'))return false;event.preventDefault();editor.chain().focus().insertContent(text,{contentType:'markdown'}).run();return true;}},onUpdate:()=>{onChange(original.prefix+restoreHtml(editor.getMarkdown()));update();void hydrateImages(host,path)},onSelectionUpdate:update,onTransaction:({editor:current})=>{const value=matchKey.getState(current.state)!;matchCount=value.matches.length;matchIndex=value.index;}});
 // Complete editor normalization (including its trailing paragraph) without treating it as a user edit.
 editor.view.dispatch(editor.state.tr.setMeta('preventUpdate',true).setMeta('addToHistory',false));
 update();const value=matchKey.getState(editor.state)!;matchCount=value.matches.length;matchIndex=value.index;selectMatch(Math.min(initialMatch,Math.max(0,matchCount-1)));void hydrateImages(host,path);});
 $:if(editor){editor.setOptions({editorProps:{...editor.options.editorProps,attributes:{role:'textbox','aria-label':$t('文档正文'),'aria-multiline':'true'}}});editor.view.dispatch(editor.state.tr.setMeta('language',true));}
 onDestroy(()=>editor?.destroy());
</script>
<div class="editor-toolbar native-editor-toolbar" role="group" aria-label={$t("文档格式")}>
 <div class="format-controls">
  <div class="paragraph-control"><Select label={$t("段落样式")} bind:value={style} onValueChange={()=>{if(style==='paragraph')editor.chain().focus().setParagraph().run();else editor.chain().focus().toggleHeading({level:Number(style)as 1|2|3}).run();}} items={[{value:'paragraph',label:$t("正文")},{value:'1',label:$t("标题 1")},{value:'2',label:$t("标题 2")},{value:'3',label:$t("标题 3")}]}/></div>
  {#each formatGroups as group}
   <span class="toolbar-divider" aria-hidden="true"></span>
   <div class="native-format-group">
    {#each group as {name,label}}
     <Button.Root type="button" title={label} aria-label={label} aria-pressed={active.includes(name)} onmousedown={(e:MouseEvent)=>e.preventDefault()} onclick={()=>format(name)}>{@html icon(name)}</Button.Root>
    {/each}
   </div>
  {/each}
  <span class="toolbar-divider" aria-hidden="true"></span>
  <div class="native-format-group history-controls">
   <Button.Root type="button" title={$t("撤销")} aria-label={$t("撤销")} disabled={!canUndo} onmousedown={(e:MouseEvent)=>e.preventDefault()} onclick={()=>format('undo')}>{@html icon('undo')}</Button.Root>
   <Button.Root type="button" title={$t("重做")} aria-label={$t("重做")} disabled={!canRedo} onmousedown={(e:MouseEvent)=>e.preventDefault()} onclick={()=>format('redo')}>{@html icon('redo')}</Button.Root>
  </div>
 </div>
</div>
<MatchNavigation count={matchCount} index={matchIndex} onMove={step=>selectMatch(matchIndex+step)}/>
<div class="document-sheet rich-document-sheet"><div bind:this={host} id="rich-editor"></div></div>
