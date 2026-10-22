<script lang="ts">
 import {t,setLanguage} from './lib/i18n';
 import {applyTheme,watchSystemTheme} from './lib/theme';
 import DesktopTitlebar from './components/DesktopTitlebar.svelte';
 import {indexStages} from './lib/index-stages';
 import LibraryWorkspace from './components/LibraryWorkspace.svelte';
 import IndexDetails from './components/IndexDetails.svelte';
 import DoclingDownload from './components/DoclingDownload.svelte';
 import type {IndexStatus} from './lib/types';
 import SearchSummary from './components/SearchSummary.svelte';
 import {createLibrarySession} from './lib/explore';
 import type {LibrarySession} from './lib/explore';
 import Input from "./components/ui/Input.svelte";
 import Textarea from "./components/ui/Textarea.svelte";
 import Select from "./components/ui/Select.svelte";
 import{Button,Pagination,Popover}from 'bits-ui';
 import type {SearchPagination} from './lib/types';
 import{onMount,tick}from 'svelte';import{listen}from '@tauri-apps/api/event';
 import Modal from './components/ui/Modal.svelte';
 import HistorySearch from './components/HistorySearch.svelte';
 import Timestamp from './components/Timestamp.svelte';
 import FolderPicker from './components/FolderPicker.svelte';
 import FolderIndexSettings from './components/FolderIndexSettings.svelte';
 import type {FolderIndexOptions} from './lib/types';
 let folderIndexPath='';
 import {rememberScroll} from './lib/remember-scroll';
 import Onboarding from './components/Onboarding.svelte';
 import ActionMenu from './components/ui/ActionMenu.svelte';import type{MenuState}from './components/ui/ActionMenu.svelte';
 import ToolCall from './components/ToolCall.svelte';
 import CitationDocument from './components/CitationDocument.svelte';
 import Reasoning from './components/Reasoning.svelte';
 import ReasoningControl from './components/ReasoningControl.svelte';
 import ContextUsage from './components/ContextUsage.svelte';
 import type {ReasoningEffort} from './lib/reasoning';
 import {followChat} from './lib/chat-scroll';
 import {answerHtml} from './lib/answer-html';
 import {answerEvents} from './lib/answer-events';
 import {subscribeDataChanges} from './lib/data-changes';
 import {stableRows} from './lib/stable-snapshot';
 import HighlightedDocument from './components/HighlightedDocument.svelte';import HtmlDocument from './components/HtmlDocument.svelte';import{highlightedText,searchSnippet,searchText,documentMatchIndex}from './lib/search-highlights';
 import Tree from './components/Tree.svelte';import SettingsPanel from './components/Settings.svelte';import UpdateNotification from './components/UpdateNotification.svelte';import{api}from './lib/api';import{icon,logo}from './lib/icons';import{basename,dirname,displayPath,normalizeScope,documentResults,escaped,fileName,inside,isMarkdown,isRtf,isHtml,readonlyHtml,compactNumber,durationLabel}from './lib/domain';import type{Root,Entry,FileInfo,Document,Draft,Settings,Conversation,Candidate,Progress,QueryOutput,Message,McpCredentials,ToolApproval}from './lib/types';
 let librarySessions:Record<string,LibrarySession>={'':createLibrarySession()};let libraryRevision=0;let exploreReturn='';let documentFromSearch=false,documentFromConversation=false,documentSearchScroll=0,documentConversationScroll=0,documentInitialMatch=0;let chatLog:HTMLDivElement;
 $:if(selected&&!librarySessions[selected])librarySessions={...librarySessions,[selected]:createLibrarySession()};
 let roots:Root[]=[],files:FileInfo[]=[],conversations:Conversation[]=[],settings:Settings|null=null,cache:Record<string,Entry[]>={},expanded=new Set<string>(JSON.parse(localStorage.getItem('sag.sidebar.expanded')??'[]'));
 let readOnlyDoc:Document|null=null;let searchTerms:string[]=[],documentSearchTerms:string[]=[];
 let previewDocument:Document|null=null,previewInitialMatch=0,previewLoading=false,previewError='',previewRequest=0;
 let view='ask',selected='',readingFolder='',collapsed=false,conversation:Conversation|null=null,ask='',search='',searched=false,results:Candidate[]=[],focus:Candidate|null=null,preview='',source:Candidate|null=null;
 let sidebarMotion=false,sidebarAnimations:Animation[]=[];
 let composing=false,compositionEndedAt=-Infinity;
 let references:string[]=[],mention=false,mentionTerm='',mentionIndex=0,mentionStart=0,mentionEnd=0,askInput:HTMLTextAreaElement;
 let composer:HTMLFormElement,workspaceContent:HTMLDivElement,mentionSide:'top'|'bottom'='top';
 let searchMode:'search'|'deep_search'='deep_search';
 let searchHistory:string[]=[],searchInput:HTMLInputElement;
 let searchPagination:SearchPagination|null=null,searchPage=1,searchQuestion='',paging=false,searchList:HTMLDivElement;
 let summaryJob='',summaryText='',summaryError='',summaryBusy=false,summaryEvidence:Candidate[]=[];
 let searchStopped=false,searchRanked=false,searchElapsed:number|null=null,searchJobKind:'search'|'deep_search'='deep_search',searchError='';
 let pendingHistoryLength=0,chatScrollKey=0;
 let askJob='',searchJob='',askStage='',searchStage='',askBusy=false,searchBusy=false;
 let activeToolId='',pending:Message|null=null,toast='',undo:(()=>Promise<void>)|null=null;
 $:busy=view==='search'?searchBusy:askBusy;
 $:stage=view==='search'?searchStage:askStage;
 let noticeTimer:ReturnType<typeof setTimeout>;
 let draft:Draft|null=null,dirty=false,EditorComponent:any=null,draftTimer:ReturnType<typeof setTimeout>;
 let quitPending=false;
 let indexDetailsOpen=false,indexPaused=false;
 let indexStatusHidden=false;
 let indexProgress:{path:string;count:number;total:number;stage?:string}|null=null;
 $:indexPercent=indexProgress&&indexProgress.total>0?Math.floor(indexProgress.count*100/indexProgress.total):null;
 $:indexing=roots.some(r=>r.status!=='ready'&&r.status!=='error'&&r.status!=='partial');
 $:indexFailed=roots.some(r=>r.status==='error'||r.status==='partial');
 $:indexStage=indexStages.find(item=>item.stage===indexProgress?.stage);
 $:indexStatusText=indexPaused?$t("SAG 索引已暂停"):indexing?indexStage?$t("SAG {value0}{value1}{value2}",{value0:$t(indexStage.progress),value1:indexPercent===null?'':`（${indexPercent}%）`,value2:''}):(indexPercent===null?$t("SAG 正在索引…"):$t("SAG 正在索引（{value0}%）",{value0:indexPercent})):$t("SAG · 索引已结束");
 $:indexStatusCount=indexProgress?$t("已处理 {value0} / {value1} 个文件",{value0:indexProgress.count,value1:indexProgress.total}):$t("已索引 {value0} 个文件",{value0:roots.reduce((count,root)=>count+root.count,0)});
 $:indexFailureText=roots.length>0&&roots.every(root=>root.status==='error')?$t('读取失败'):$t('部分失败');
 let onboardingOpen=false;
 function closeOnboarding(){localStorage.setItem('sag.onboarding.seen','1');onboardingOpen=false;}
 let historySearchOpen=false,folderPickerOpen=false;
 let settingsOpen=false,settingsPage='general',scopeOpen=false,scope:string[]=[],scopeDraft:string[]=[],scopeExpanded=new Set<string>();
 let menu:MenuState|null=null;let toolApproval:ToolApproval|null=null;
 let removeFolderPath='',removingFolder=false,removeFolderError='';
 let renderedAnswers:Record<string,ReturnType<typeof import('./lib/markdown').renderAnswer>>={};
 let answerRenderer:typeof import('./lib/markdown').renderAnswer|undefined;
 let answerRendererLoading:Promise<void>|undefined;
 let renameId='',renameTitle='',leave:((value:boolean)=>void)|null=null,loading=true,error='';
 let editingMessage:number|null=null,editingText='',retryIndex:number|null=null,messageInput:HTMLTextAreaElement;
 const mac=/Mac/i.test(navigator.userAgent);
 $:scopeLabel=scope.length===0?$t("全部文件"):scope.length===1?basename(scope[0]):$t("{value0} 个文件夹",{value0:scope.length});
 $:currentEntries=cache[selected]||[];
 $:directoryEntries=selected?currentEntries:roots.map(root=>({path:root.path,name:root.name,directory:true,symlink:false}));
 $:indexedPaths=new Set(files.map(file=>file.path));
 $:indexedCache=Object.fromEntries(Object.entries(cache).map(([path,entries])=>[path,entries.filter(entry=>entry.directory?!entry.name.startsWith('.'):indexedPaths.has(entry.path))]));
 $:fileView=view==='files'||view==='document'||view==='editor';
 $:libraryParent=roots.some(root=>root.path===selected)?'':dirname(selected);
 $:shownMessages=pending?conversation?.messages.slice(0,pendingHistoryLength)||[]:conversation?.messages||[];
 $:displayedMessages=[...shownMessages,...(pending?[pending]:[])];
 $:answerHasError=view==='ask'&&displayedMessages.some(message=>message.error&&$t(message.error)===$t(error));
 $:if(displayedMessages.length)void renderAnswers(displayedMessages);
 function answerKey(message:Message){return JSON.stringify([message.question,message.answer,message.tool_calls?.map(call=>[call.id,call.answer_offset]),message.reasoning_blocks?.map(block=>[block.answer_offset,block.tool_count]),message.evidence.map(({id,path,hash})=>[id,path,hash])]);}
 function renderAnswers(messages:Message[]){if(!answerRenderer){answerRendererLoading??=import('./lib/markdown').then(module=>{answerRenderer=module.renderAnswer;renderAnswers(displayedMessages)});return;}const next:typeof renderedAnswers={};for(const message of messages){const key=answerKey(message);next[key]=renderedAnswers[key]??answerRenderer(message.answer,message.evidence,message.tool_calls,message.reasoning_blocks);}renderedAnswers=next;}
 $:rankedGroups=documentResults(results);
 $:groups=searchRanked?rankedGroups.slice((searchPage-1)*20,searchPage*20):rankedGroups;
 $:searchStageText=stage==='SAG2 正在分析'?$t("正在准备搜索"):stage==='正在检索原文'?$t("正在查找相关文件"):stage==='正在扩展检索'?$t("正在扩展搜索范围"):stage==='Jev 正在判断相关原文'?$t("正在按相关性排序"):$t(stage);
 $:mentions=files.filter(f=>!references.includes(f.path)&&(f.name.toLowerCase().includes(mentionTerm.toLowerCase())||f.path.toLowerCase().includes(mentionTerm.toLowerCase()))).slice(0,20);
 $:if(settings){applyTheme(settings.theme);document.documentElement.style.setProperty('--editor-font-size',`${settings.editor_size}px`)}
 onMount(watchSystemTheme);
 async function answerApproval(allow:boolean){if(!toolApproval)return;const id=toolApproval.id;toolApproval=null;try{await api('resolve_tool_approval',{id,allow})}catch(e){notify($t(String(e)))}}
 function clearConversationState(){closeNotice();conversations=[];conversation=null;pending=null;renderedAnswers={};ask='';references=[];source=null;historySearchOpen=false;editingMessage=null;retryIndex=null;askJob='';askBusy=false;askStage='';activeToolId='';error='';toolApproval=null;localStorage.removeItem('sag.history.scroll');}
 function closeNotice(){clearTimeout(noticeTimer);toast='';undo=null;}
 function clearFoldersState(){refreshGeneration++;refreshPending=false;roots=[];files=[];scope=[];references=[];expanded=new Set();scopeExpanded=new Set();cache={};selected='';indexProgress=null;if(view==='files')view='ask';}
 function scheduleNotice(){clearTimeout(noticeTimer);if(!toast||document.hidden||document.querySelector('#notice:hover, #notice:focus-within'))return;noticeTimer=setTimeout(closeNotice,undo?8000:3000);}
 function notify(message:string,action:(()=>Promise<void>)|null=null){toast=message;undo=action;scheduleNotice();}
 let refreshGeneration=0,refreshPending=false,refreshRunning:Promise<void>|null=null,appDisposed=false;
 function refresh():Promise<void>{
  if(appDisposed)return Promise.resolve();
  refreshGeneration++;refreshPending=true;
  if(!refreshRunning)refreshRunning=refreshSnapshots().finally(()=>{refreshRunning=null;if(refreshPending&&!appDisposed)return refresh();});
  return refreshRunning;
 }
 function indexPauseChanged(paused:boolean){if(indexPaused===paused)return;indexPaused=paused;void refresh().catch(e=>notify($t(String(e))));}
 async function refreshSnapshots(){
  while(refreshPending&&!appDisposed){
   refreshPending=false;const generation=refreshGeneration;
   try{
    const paths=[...new Set([...expanded,...scopeExpanded,selected].filter(Boolean))];
    const [nextRoots,nextStatus,nextFiles,removed]=await Promise.all([
     api<Root[]>('list_roots'),api<IndexStatus>('get_index_status'),api<FileInfo[]>('list_files'),api<string[]>('list_removed_directories')
    ]);
    if(appDisposed||generation!==refreshGeneration)continue;
    const included=(path:string)=>!removed.some(directory=>inside(path,directory));
    const entries=await Promise.all(paths.filter(path=>included(path)&&nextRoots.some(root=>inside(path,root.path))).map(async path=>[path,await api<Entry[]>('list_entries',{path})] as const));
    if(appDisposed||generation!==refreshGeneration)continue;
    const stableRoots=stableRows(roots,nextRoots,root=>root.path),stableFiles=stableRows(files,nextFiles,file=>file.id);
    if(stableRoots!==roots)roots=stableRoots;
    if(stableFiles!==files)files=stableFiles;
    if(indexPaused!==nextStatus.paused)indexPaused=nextStatus.paused;
    if(indexProgress&&!nextRoots.some(root=>root.path===indexProgress?.path&&root.status==='indexing'))indexProgress=null;
    expanded=new Set([...expanded].filter(path=>included(path)&&nextRoots.some(root=>inside(path,root.path))));scopeExpanded=new Set([...scopeExpanded].filter(included));
    scope=scope.filter(included);references=references.filter(included);
    const nextCache=Object.fromEntries(Object.entries(cache).filter(([path])=>included(path)));
    let cacheChanged=Object.keys(nextCache).length!==Object.keys(cache).length;
    for(const [path,rows] of entries){const stable=stableRows(cache[path]??[],rows,entry=>entry.path);if(stable!==cache[path]){nextCache[path]=stable;cacheChanged=true;}}
    if(cacheChanged)cache=nextCache;
    if(selected&&!included(selected)&&!draft)selected=nextRoots.find(root=>inside(selected,root.path))?.path||'';
    libraryRevision++;
   }catch(error){if(appDisposed)return;if(!refreshPending)throw error;}
  }
 }

 async function history(){conversations=await api<Conversation[]>('list_conversations');if(conversation){conversation=conversations.find(c=>c.id===conversation!.id)||null}}
 async function protect():Promise<boolean>{if(!draft||!dirty)return true;return new Promise(resolve=>leave=resolve)}
 async function requestQuit(){if(quitPending||leave)return;quitPending=true;try{if(draft&&dirty)await api('reveal_main_window');if(await protect())await api('quit_application')}catch(e){notify($t(String(e)))}finally{quitPending=false}}
 async function leaveChoice(choice:'save'|'discard'|'cancel'){if(choice==='save'&&!await save())return;if(choice==='discard'&&draft){clearTimeout(draftTimer);await api('delete_draft',{path:draft.path});dirty=false;draft=null}const done=leave;leave=null;done?.(choice!=='cancel')}
 async function navigate(next:string){if(!await protect())return;view=next;menu=null;if(next!=='editor')draft=null;await tick();if(next==='ask')askInput?.focus({preventScroll:true});}
 async function toggleSidebar(event:MouseEvent){
  const animate=event.detail>0&&!matchMedia('(prefers-reduced-motion: reduce)').matches;
  const controls=[...(event.currentTarget as HTMLElement).closest('aside')!.querySelectorAll<HTMLElement>('.brand-mark,.sidebar-titlebar>.icon-button,.nav-item,.sidebar-tools>button')];
  const before=animate?controls.map(node=>node.getBoundingClientRect()):[];
  sidebarAnimations.forEach(animation=>animation.cancel());sidebarAnimations=[];
  sidebarMotion=animate;collapsed=!collapsed;
  await tick();
  if(!animate)return;
  controls.forEach((node,index)=>{
   const after=node.getBoundingClientRect();
   sidebarAnimations.push(node.animate([
    {transform:`translate(${before[index].left-after.left}px,${before[index].top-after.top}px)`},
    {transform:'translate(0,0)'}
   ],{duration:220,easing:'cubic-bezier(.32,.72,0,1)'}));
  });
 }
 async function browse(path:string){if(!await protect())return;exploreReturn='';const entry=files.find(f=>f.path===path)||Object.values(cache).flat().find(e=>e.path===path&&!e.directory);if(entry||isMarkdown(path)){view='files';await edit(path);return}selected=path;view='files';draft=null;menu=null;readingFolder=path;try{cache={...cache,[path]:await api<Entry[]>('list_entries',{path})}}finally{if(readingFolder===path)readingFolder=''}}
 async function toggle(path:string,isScope=false){let set=new Set(isScope?scopeExpanded:expanded);if(set.has(path))set.delete(path);else{set.add(path);cache={...cache,[path]:await api<Entry[]>('list_entries',{path})}}if(isScope)scopeExpanded=set;else{expanded=set;localStorage.setItem('sag.sidebar.expanded',JSON.stringify([...set]));}}
 function add(){folderPickerOpen=true}
 async function addFolders(paths:string[],options?:FolderIndexOptions){
  const failures:{path:string;error:string}[]=[];
  for(const path of paths){try{await api<string>(options?'add_root_with_options':'add_root',options?{path,options}:{path})}catch(e){failures.push({path,error:$t(String(e))})}}
  if(failures.length<paths.length){await refresh();notify($t('已添加 {value0} 个文件夹，正在建立本地索引',{value0:paths.length-failures.length}))}
  return failures;
 }
 async function edit(path:string,terms:string[]=[],origin='',citation?:Candidate){
  if(!await protect())return;
  const fromSearch=view==='search',fromConversation=view==='ask',target=citation??(fromSearch&&focus?.path===path?focus:null),scroll=fromSearch?(searchList?.scrollTop??0):0,chatScroll=fromConversation?(chatLog?.scrollTop??0):0;
  try{
   const doc=await api<Document>('read_document',{path});
   documentInitialMatch=target?await documentMatchIndex(doc.body,path,terms,target):0;
   documentFromSearch=fromSearch;documentFromConversation=fromConversation;documentSearchScroll=scroll;documentConversationScroll=chatScroll;exploreReturn=origin;documentSearchTerms=terms;
   if(!isMarkdown(path)){readOnlyDoc=doc;draft=null;dirty=false;selected=dirname(path);view='document';source=null;return;}
   draft={...doc,is_new:false};dirty=false;selected=dirname(path);EditorComponent=(await import('./components/RichEditor.svelte')).default;view='editor';source=null;
  }catch(e){notify($t(String(e)))}
 }
 async function returnToFiles(){
  if(documentFromConversation){
   if(!await protect())return;documentFromConversation=false;view='ask';draft=null;readOnlyDoc=null;
   await tick();await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
   chatLog.scrollTop=documentConversationScroll;chatLog.dispatchEvent(new Event('scroll'));
  }else if(documentFromSearch){
   if(!await protect())return;documentFromSearch=false;view='search';draft=null;readOnlyDoc=null;
   await tick();if(searchList)searchList.scrollTop=documentSearchScroll;
  }else if(exploreReturn){if(!await protect())return;selected=exploreReturn;exploreReturn='';view='files';draft=null;readOnlyDoc=null;}else await browse(selected)
 }
 async function libraryBrowse(path:string){if(!path){if(await protect()){selected='';view='files';draft=null;readOnlyDoc=null;exploreReturn='';documentFromSearch=false;documentFromConversation=false;}return;}const entry=currentEntries.find(e=>e.path===path);if(entry?.symlink){try{await api('reveal_path',{path})}catch(e){notify($t(String(e)))}}else try{await browse(path)}catch(e){notify($t(String(e)))}}
 async function create(){if(!selected||!await protect())return;documentFromConversation=false;documentFromSearch=false;documentSearchTerms=[];draft={path:selected+'/.draft-'+crypto.randomUUID(),name:$t("未命名.md"),body:'',hash:'',is_new:true};dirty=false;EditorComponent=(await import('./components/RichEditor.svelte')).default;view='editor';}
 function changed(body:string){if(!draft)return;draft={...draft,body};dirty=true;clearTimeout(draftTimer);draftTimer=setTimeout(()=>{if(draft)api('put_draft',{draft}).catch(e=>notify($t('草稿保存失败：{value0}',{value0:$t(String(e))})))},500)}
 async function save(){if(!draft)return false;try{const old=draft.path;const name=fileName(draft.name);const path=draft.is_new?dirname(draft.path)+'/'+name:draft.path;const doc=await api<Document>('save_document',{path,body:draft.body,expectedHash:draft.is_new?null:draft.hash});clearTimeout(draftTimer);await api('delete_draft',{path:old});draft={path:doc.path,name:doc.name,body:doc.body,hash:doc.hash,is_new:false};dirty=false;await refresh();notify($t("文件已保存，索引正在更新"));return true}catch(e){notify($t(String(e)));return false}}
 function scopeShow(){scopeDraft=[...scope];scopeExpanded=new Set(expanded);scopeOpen=true;menu=null}
 function scopeCheck(path:string){scopeDraft=scopeDraft.some(p=>inside(path,p))?scopeDraft.filter(p=>!inside(path,p)&&!inside(p,path)):normalizeScope([...scopeDraft,path]);}
 async function applyScope(){await stop();scope=normalizeScope(scopeDraft);scopeOpen=false;searched=false;results=[];summaryText='';summaryError='';summaryEvidence=[];}
 function showMenu(e:MouseEvent,path:string,isRoot:boolean,file:boolean){
  const items:{label:string;glyph:string;run:()=>void}[]=[
   {label:$t("问答"),glyph:'chat',run:()=>folderQuery(path,'ask',file)},
   {label:$t("搜索"),glyph:'search',run:()=>folderQuery(path,'search',file)}
  ];
  if(!file)items.push({label:$t("索引设置"),glyph:'settings',run:()=>folderIndexPath=path});
  if(!file||indexedPaths.has(path))items.push({label:$t(isRoot?"移除":"从知识库移除"),glyph:'close',run:()=>{if(file)void remove(path,true).catch(e=>notify($t(String(e))));else{removeFolderError='';removeFolderPath=path;}}});
  items.push({label:mac?$t("在 Finder 中打开"):$t("在文件资源管理器中打开"),glyph:'folder',run:()=>api('reveal_path',{path}).catch(e=>notify($t(String(e))))});
  const trigger=(e.target as Element).closest<HTMLElement>('button')||(e.currentTarget as HTMLElement).querySelector<HTMLElement>('button')!;const rect=trigger.getBoundingClientRect();menu={x:Math.min(e.clientX||rect.left,innerWidth-240),y:Math.min((e.clientY||rect.bottom)+8,innerHeight-items.length*38-12),trigger,items};
 }
 async function folderQuery(path:string,next:string,file=false){if(!await protect())return;await stop(next==='ask'?'ask':'search');if(next==='ask'){editingMessage=null;conversation=null;pending=null;ask='';references=file?[path]:[];draft=null;error='';}scope=[path];searched=false;view=next;menu=null;await tick();if(next==='ask')askInput?.focus();else searchInput?.focus();}
 async function remove(path:string,file=false){if(!await protect())return;await stop();await api('remove_root',{path});scope=scope.filter(p=>!inside(p,path));references=references.filter(p=>!inside(p,path));expanded=new Set([...expanded].filter(p=>!inside(p,path)));cache=Object.fromEntries(Object.entries(cache).filter(([p])=>!inside(p,path)));if(inside(selected,path)){selected='';view='files';draft=null}if(file&&(draft?.path===path||readOnlyDoc?.path===path)){draft=null;readOnlyDoc=null;view='files'}await refresh();notify($t(file?"已从知识库移除文件，原文件保留":"已移除文件夹，原文件保留"),async()=>{await api('add_root',{path});await refresh();notify($t(file?"已恢复文件":"已恢复文件夹"))});}
 async function confirmRemoveFolder(){removingFolder=true;removeFolderError='';try{await remove(removeFolderPath);removeFolderPath='';}catch(e){removeFolderError=$t(String(e));}finally{removingFolder=false;}}
 function chatMenu(e:MouseEvent,c:Conversation){const trigger=e.currentTarget as HTMLElement;const rect=trigger.getBoundingClientRect();menu={x:Math.min(e.clientX||rect.left,innerWidth-240),y:Math.min((e.clientY||rect.bottom)+8,innerHeight-92),trigger,items:[{label:$t("重命名"),glyph:'edit',run:()=>{renameId=c.id;renameTitle=c.title}},{label:$t("删除"),glyph:'close',run:async()=>{await api('edit_conversation',{id:c.id,title:null,deleted:true});if(conversation?.id===c.id)conversation=null;await history();notify($t("已删除问答记录"),async()=>{await api('edit_conversation',{id:c.id,title:null,deleted:false});await history();notify($t("已恢复问答记录"))});}}]};}
 async function rename(){await api('edit_conversation',{id:renameId,title:renameTitle,deleted:null});renameId='';await history()}
 async function copyMessage(text:string){try{await navigator.clipboard.writeText(text);notify($t("已复制"))}catch(e){notify($t('复制失败：{value0}',{value0:$t(String(e))}))}}
 async function replaceMessages(id:string,messages:Message[]){const updated=await api<Conversation>('replace_messages',{id,messages});if(conversation?.id===id){conversation=updated;pending=null}await history()}
 async function deleteMessage(index:number){try{const before=structuredClone(conversation!);await replaceMessages(before.id,before.messages.filter((_,i)=>i!==index));editingMessage=null;notify($t("已删除此轮问答"),async()=>{await replaceMessages(before.id,before.messages);notify($t("已恢复此轮问答"))})}catch(e){notify($t(String(e)))}}
 async function beginEdit(index:number){editingMessage=index;editingText=displayedMessages[index].question;await tick();messageInput.focus();messageInput.setSelectionRange(editingText.length,editingText.length)}
 async function regenerate(index:number,question:string){const before=conversation?structuredClone(conversation):null;const message=displayedMessages[index];editingMessage=null;retryIndex=null;scope=[...message.scope];references=[...message.references];ask=question;await run('ask',before&&index<before.messages.length?index:undefined);if(before)notify(error?$t("重新生成未完成"):$t("已重新生成回答"),async()=>{await replaceMessages(before.id,before.messages);notify($t("已恢复原问答"))})}
 function retryMessage(index:number){if(index<displayedMessages.length-1)retryIndex=index;else void regenerate(index,displayedMessages[index].question)}
 async function newChat(){if(!await protect())return;editingMessage=null;await stop('ask');conversation=null;pending=null;ask='';references=[];view='ask';draft=null;await tick();askInput?.focus({preventScroll:true})}
 async function stopSummary(){const id=summaryJob;summaryJob='';summaryBusy=false;if(id)await api('cancel_query',{id});}
 async function summarizeSearch(question:string,candidates:Candidate[]){
  if(settings?.search_summary_enabled===false||!candidates.length)return;
  const id=crypto.randomUUID();summaryJob=id;summaryBusy=true;summaryError='';summaryText='';summaryEvidence=documentResults(candidates).slice(0,settings?.search_summary_sources??10).map(group=>group.first);
  try{const output=await api<{text:string;evidence:Candidate[]}>('summarize_search',{id,question,results:summaryEvidence});if(summaryJob===id){summaryText=output.text;summaryEvidence=output.evidence}}
  catch(e){if(summaryJob===id)summaryError=$t(String(e))}
  finally{if(summaryJob===id){summaryBusy=false;summaryJob=''}}
 }
 async function stop(kind:'ask'|'search'=view==='search'?'search':'ask'){
  if(kind==='ask'){toolApproval=null;if(askJob)await api('cancel_query',{id:askJob});askJob='';askBusy=false;askStage='';}
  else{const summaryCancellation=stopSummary();if(searchBusy)searchStopped=true;const id=searchJob;searchJob='';searchBusy=false;paging=false;searchPage=searchPagination?.page??1;searchStage='';await Promise.all([summaryCancellation,...(id?[api('cancel_query',{id})]:[])]);}
 }
 async function resetSearch(focusInput=true){
  await stop('search');summaryText='';summaryError='';summaryEvidence=[];search='';searched=false;results=[];focus=null;previewDocument=null;preview='';previewError='';previewLoading=false;previewRequest++;searchTerms=[];searchQuestion='';searchPagination=null;searchPage=1;searchElapsed=null;searchError='';
  await tick();if(focusInput)searchInput.focus();
 }
 async function clearAllDataState(){
  clearTimeout(draftTimer);draft=null;dirty=false;readOnlyDoc=null;
  await resetSearch(false);searchHistory=[];clearConversationState();clearFoldersState();
  librarySessions={};exploreReturn='';documentFromSearch=false;documentFromConversation=false;view='ask';
  for(const key of Object.keys(localStorage))if(key.startsWith('sag.'))localStorage.removeItem(key);
  settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');
  notify($t('所有应用数据已清除，设置已恢复默认，本地原文件已保留。'));
 }
 function clearSearchHistory(){
  try{localStorage.removeItem('sag.search.history');searchHistory=[]}catch(e){notify($t(String(e)))}
 }
 function mentionPlacement(node:HTMLFormElement){
  function update(){
   const anchor=node.getBoundingClientRect(),boundary=workspaceContent.getBoundingClientRect();
   const above=anchor.top-Math.max(0,boundary.top)-14,below=Math.min(window.innerHeight,boundary.bottom)-anchor.bottom-14;
   // Compare against the full menu height, even after a smaller window has shrunk it.
   mentionSide=above>=300||above>=below?'top':'bottom';
  }
  const observer=new ResizeObserver(update);observer.observe(node);observer.observe(workspaceContent);
  window.addEventListener('resize',update);workspaceContent.addEventListener('scroll',update,true);update();
  return {destroy(){observer.disconnect();window.removeEventListener('resize',update);workspaceContent.removeEventListener('scroll',update,true)}};
 }
 function mentionUpdate(){const value=askInput.value.slice(0,askInput.selectionStart);const match=value.match(/(?:^|\s)@([^@\n]*)$/);mention=!!match;if(match){mentionStart=value.lastIndexOf('@');mentionEnd=askInput.selectionStart;mentionTerm=match[1];mentionIndex=0;}}
 function reference(path:string){references=[...references,path];ask=ask.slice(0,mentionStart)+ask.slice(mentionEnd);mention=false;tick().then(()=>{askInput.focus();askInput.setSelectionRange(mentionStart,mentionStart)})}
 function imeKey(e:KeyboardEvent){return composing||e.isComposing||e.keyCode===229||(e.key==='Enter'&&performance.now()-compositionEndedAt<100)}
 function compositionStart(){composing=true}
 function compositionEnd(){composing=false;compositionEndedAt=performance.now()}
 function compositionKeyUp(e:KeyboardEvent){if(e.key==='Enter')compositionEndedAt=-Infinity}
 function changeSearchMode(value:string){searchMode=value as 'search'|'deep_search';if(searched&&search.trim())void run(searchMode);}
 async function changeSearchPage(page:number){
  if(searchBusy||page===searchPagination?.page)return;
  if(!searchRanked){await run('search',undefined,page);return;}
  searchPage=page;searchPagination={...searchPagination!,page};
  void focusResult(rankedGroups[(page-1)*20].first);
  await tick();if(searchList)searchList.scrollTop=0;
 }
 function searchKey(e:KeyboardEvent){if(e.key==='Enter'&&imeKey(e)&&!composing&&!e.isComposing)e.preventDefault()}
 function askKey(e:KeyboardEvent){if(imeKey(e)){if(e.key==='Enter'&&!composing&&!e.isComposing)e.preventDefault();return;}if(mention){if(e.key==='Escape'){mention=false;e.preventDefault()}else if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();mentionIndex=(mentionIndex+(e.key==='ArrowDown'?1:-1)+mentions.length)%Math.max(1,mentions.length);tick().then(()=>{document.querySelector<HTMLElement>('.mention-options [aria-selected="true"]')?.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'})})}else if(e.key==='Enter'){e.preventDefault();if(mentions[mentionIndex])reference(mentions[mentionIndex].path)}return}if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();void run('ask')}}
 async function run(kind:'ask'|'search'|'deep_search',replaceFrom?:number,page?:number){
  if(composing||performance.now()-compositionEndedAt<100)return;
  const question=(kind==='ask'?ask:page===undefined?search:searchQuestion).trim();
  if(!question||(kind==='ask'?askBusy:searchBusy)||!settings)return;
  closeNotice();menu=null;
  const id=crypto.randomUUID(),conversationId=conversation?.id||null;let summaryCancellation:Promise<void>|undefined;
  if(kind==='ask'){
   error='';askJob=id;askBusy=true;askStage='思考中';activeToolId='';
   if(!conversation)conversation={id,title:question.slice(0,30),messages:[],updated:Date.now(),deleted:false};
   chatScrollKey++;pendingHistoryLength=replaceFrom??conversation.messages.length;
   pending={created_at:Date.now(),question,answer:'',evidence:[],model:settings.answer.model,scope:[...scope],references:[...references],status:'running',error:null,retrieval_question:question};ask='';mention=false;
  }else{
   if(page===undefined){summaryCancellation=stopSummary();summaryText='';summaryError='';summaryEvidence=[]}
   searchError='';searchJob=id;searchBusy=true;searchJobKind=kind;searchStage=kind==='search'?'正在搜索本地文件':'SAG2 正在分析';
   paging=page!==undefined;searched=true;searchStopped=false;searchElapsed=null;searchRanked=kind==='deep_search';
   if(!paging){searchQuestion=question;searchPagination=null;searchPage=1;results=[];searchTerms=[];focus=null;previewDocument=null;preview='';previewError='';previewLoading=false;previewRequest++}
  }
  try{
   if(summaryCancellation)await summaryCancellation;
   if(kind!=='ask'&&page===undefined){const recent=[question,...searchHistory.filter(item=>item!==question)].slice(0,8);localStorage.setItem('sag.search.history',JSON.stringify(recent));searchHistory=recent;}
   if(kind==='ask'&&!answerRenderer)renderAnswers(displayedMessages);
   const output=await api<QueryOutput>('run_query',{query:{id,kind,question,scope,references:kind==='ask'?references:[],model:settings.answer.model,conversation_id:kind==='ask'?conversationId:null,replace_from:replaceFrom??null,page:kind==='search'?(page??1):null}});
   if((kind==='ask'?askJob:searchJob)!==id)return;
   if(kind==='ask'){conversation=output.conversation;pending=null;references=[];await history()}
   else{searchTerms=output.terms||[];results=output.results;searchRanked=kind==='deep_search';searchElapsed=output.elapsed_ms??null;searchPagination=kind==='deep_search'?{total:documentResults(output.results).length,page:1,page_size:20}:output.pagination??null;searchPage=searchPagination?.page??1;await tick();if(searchList)searchList.scrollTop=0;if(results.length)void focusResult(results.find(c=>c.path===focus?.path)||results[0]);if(page===undefined)void summarizeSearch(question,output.results);}
  }catch(e){
   if((kind==='ask'?askJob:searchJob)===id){
    if(kind==='ask'){error=$t(String(e));if(pending)pending={...pending,status:'failed',error};await history()}
    else searchError=$t(String(e));
   }
  }finally{
   if(kind==='ask'){if(askJob===id){askBusy=false;askStage='';askJob='';toolApproval=null}}
   else if(searchJob===id){searchBusy=false;paging=false;searchPage=searchPagination?.page??1;searchStage='';searchJob=''}
  }
 }
 async function focusResult(c:Candidate){
  focus=c;previewDocument=null;preview='';previewError='';previewLoading=true;const request=++previewRequest;
  try{
   const doc=await api<Document>('read_document',{path:c.path});
   if(request!==previewRequest||focus?.id!==c.id)return;
   const initial=await documentMatchIndex(doc.body,c.path,searchTerms,c);
   const md=isMarkdown(c.path)?await import('./lib/markdown'):null;
   if(request!==previewRequest||focus?.id!==c.id)return;
   previewDocument=doc;previewInitialMatch=initial;preview=md?md.renderMarkdown(doc.body):isHtml(c.path)?'':readonlyHtml(c.path,doc.body);
   if(doc.hash!==c.hash)previewError=$t('原文件已更新，以下显示当前内容。');
  }catch(e){if(request===previewRequest)previewError=$t(String(e))}
  finally{if(request===previewRequest)previewLoading=false}
 }
 async function chooseHistory(c:Conversation){if(!await protect())return;editingMessage=null;await stop('ask');chatScrollKey++;view='ask';draft=null;conversation=c;pending=null;references=[];error='';}
 function setReasoning(value:ReasoningEffort|null){if(!settings)return;const model=settings.answer.model;const profile=settings.model_profiles?.[model]??{system_prompt:'',sag2_enabled:true,mcp_servers:[],mcp_tools:{}};settings={...settings,model_profiles:{...settings.model_profiles,[model]:{...profile,reasoning_effort:value}}};}
 function openAgentSetup(){settingsPage='cli';settingsOpen=true;}
 async function saveReasoning(){if(!settings)return;try{await api('save_settings',{settings,answerKey:'',jevKey:'',mcpCredentials:{}})}catch(e){notify($t('思考强度保存失败：{value0}',{value0:$t(String(e))}))}}
 async function saveSettings(value:Settings,a:string,j:string,credentials:Record<string,McpCredentials>={}){await api('save_settings',{settings:value,answerKey:a,jevKey:j,mcpCredentials:credentials});settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.search_summary_enabled===false)await stopSummary();cache={};await refresh()}
 async function keys(e:KeyboardEvent){if(onboardingOpen)return;if(e.key==='Escape'){if(leave||renameId||settingsOpen||historySearchOpen||folderPickerOpen||menu||source||scopeOpen||toolApproval)return;mention=false;return;}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='s'&&view==='editor'){e.preventDefault();await save()}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='k'){e.preventDefault();await navigate('search')}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='n'){e.preventDefault();await newChat()}}
 onMount(()=>{const cleanup:(()=>void)[]=[];let active=true;const addCleanup=(stop:()=>void)=>{if(active)cleanup.push(stop);else stop();};void(async()=>{try{addCleanup(await listen('quit-requested',requestQuit));addCleanup(await listen<{path:string;count:number;total:number;stage?:string}>('index-progress',e=>{if(e.payload.count===0)indexStatusHidden=false;indexProgress=e.payload;roots=roots.map(r=>r.path===e.payload.path?{...r,status:'indexing'}:r)}));addCleanup(await subscribeDataChanges(()=>{void refresh().catch(e=>notify($t(String(e))))}));settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.credential_error)notify(settings.credential_error);searchHistory=JSON.parse(localStorage.getItem('sag.search.history')??'[]');view=settings.start_page;await Promise.all([refresh(),history()]);const saved=await api<Draft[]>('list_drafts');if(saved.length){notify($t("有 {value0} 份未保存草稿",{value0:saved.length}),async()=>{draft=saved[0];dirty=true;selected=dirname(draft.path);EditorComponent=(await import('./components/RichEditor.svelte')).default;view='editor';closeNotice()})}addCleanup(await listen('history-changed',()=>history().catch(e=>notify($t(String(e))))));addCleanup(await listen('settings-changed',async()=>{try{settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.search_summary_enabled===false)await stopSummary()}catch(e){notify($t(String(e)))}}));addCleanup(await listen<string>('app-error',e=>notify(e.payload)));addCleanup(await listen<ToolApproval>('tool-approval',e=>{if(e.payload.query_id===askJob)toolApproval=e.payload}));addCleanup(await listen<Progress>('query-progress',e=>{if(e.payload.id===summaryJob){if(e.payload.delta!==null)summaryText=e.payload.replace?e.payload.delta:summaryText+e.payload.delta;return;}if(e.payload.id===searchJob){searchStage=e.payload.stage;return;}if(e.payload.id!==askJob)return;askStage=e.payload.stage;if(pending&&e.payload.tool){const {call,finished}=e.payload.tool;activeToolId=finished?'':call.id;pending={...pending,tool_calls:[...(pending.tool_calls||[]).filter(t=>t.id!==call.id),call]};}if(pending&&e.payload.reasoning_delta)pending={...pending,reasoning:(pending.reasoning||'')+e.payload.reasoning_delta};if(pending&&e.payload.reasoning_block){const block=e.payload.reasoning_block;const blocks=[...(pending.reasoning_blocks||[])];const last=blocks.at(-1);if(last&&last.answer_offset===block.answer_offset&&last.tool_count===block.tool_count)blocks[blocks.length-1]={...last,content:last.content+block.content};else blocks.push(block);pending={...pending,reasoning_blocks:blocks};}if(pending&&e.payload.delta!==null)pending={...pending,answer:e.payload.replace?e.payload.delta:pending.answer+e.payload.delta,reasoning_blocks:e.payload.replace?pending.reasoning_blocks?.map(block=>({...block,answer_offset:Math.min(block.answer_offset,e.payload.delta!.length)})):pending.reasoning_blocks};}));onboardingOpen=localStorage.getItem('sag.onboarding.seen')!=='1';}catch(e){error=$t(String(e))}finally{if(active)loading=false}})();return()=>{active=false;appDisposed=true;refreshGeneration++;clearTimeout(draftTimer);clearTimeout(noticeTimer);cleanup.forEach(fn=>fn())}});
</script>
<svelte:window on:keydown={keys} on:contextmenu|preventDefault/>
<div class="desktop-window real-client" class:windows-client={!mac} class:compact-sidebar={collapsed} data-sidebar-motion={sidebarMotion}>
 {#if !mac}<DesktopTitlebar/>{/if}
 <div class="shell">
 <aside class="sidebar"><div class="sidebar-titlebar" data-tauri-drag-region><Button.Root class="icon-button" aria-label={collapsed?$t("展开侧边栏"):$t("收起侧边栏")} title={collapsed?$t("展开侧边栏"):$t("收起侧边栏")} aria-expanded={!collapsed} onclick={toggleSidebar}>{@html icon('panel')}</Button.Root></div><div class="brand-row">{@html logo}<span class="brand">{'SAG Wiki'}</span></div>
 <div class="tree-heading"><Button.Root class="folder-management-link" onclick={()=>libraryBrowse('').catch(e=>notify($t(String(e))))}>{$t("文件夹管理")}</Button.Root><Button.Root class="icon-button" title={$t("添加文件夹")} aria-label={$t("添加文件夹")} onclick={add}>{@html icon('plus')}</Button.Root></div><div class="tree" use:rememberScroll={{key:'sag.sidebar.library.scroll',enabled:!loading&&!collapsed}}><Tree nodes={roots} {expanded} cache={indexedCache} selected={view==='files'?selected:view==='editor'?draft?.path||'':view==='document'?readOnlyDoc?.path||'':''} onbrowse={(path)=>browse(path).catch(e=>notify($t(String(e))))} ontoggle={(path)=>toggle(path).catch(e=>notify($t(String(e))))} onmenu={showMenu}/>{#if !loading&&!roots.length}<div class="folder-empty-state"><p>{$t("添加文件夹后，即可对其中的文件提问和搜索。")}</p><Button.Root class="outline" onclick={add}>{@html icon('plus')}<span>{$t("添加文件夹")}</span></Button.Root></div>{/if}</div>
 <div class="index-status">{#if !indexStatusHidden&&(indexFailed||indexing||indexPaused)}<div class="index-status-row"><Button.Root class="index-status-link" aria-label={$t("查看索引详情")} onclick={()=>indexDetailsOpen=true}><span class="index-status-primary">{indexStatusText}</span><span class="index-status-meta"><span>{indexStatusCount}</span>{#if indexFailed}<span class="index-status-failure">{indexFailureText}</span>{/if}</span></Button.Root><Button.Root class="index-status-dismiss" aria-label={$t("隐藏索引状态")} onclick={()=>indexStatusHidden=true}>{@html icon('close')}</Button.Root></div>{/if}</div><div class="sidebar-bottom"><div class="sidebar-tools"><Button.Root class="sidebar-settings" aria-label={$t("设置")} title={$t("设置")} onclick={()=>{settingsPage='general';settingsOpen=true}}>{@html icon('settings')}<span>{$t("设置")}</span></Button.Root><DoclingDownload/><Button.Root aria-label={$t("切换明暗主题")} onclick={()=>{if(settings)saveSettings({...settings,theme:settings.theme==='dark'?'light':'dark'},'','').then(()=>notify($t('设置已保存'))).catch(e=>notify($t(String(e))))}}>{@html icon(settings?.theme==='dark'?'sun':'moon')}</Button.Root></div></div></aside>
 <main class="workspace"><div class="workspace-header" data-tauri-drag-region>{#if view==='ask'&&(shownMessages.length||pending)}<Button.Root class="icon-button workspace-back" title={$t("返回会话列表")} aria-label={$t("返回会话列表")} onclick={newChat}>{@html icon('back')}</Button.Root>{:else if view==='search'&&searched}<Button.Root class="icon-button workspace-back" title={$t("返回搜索")} aria-label={$t("返回搜索")} onclick={()=>resetSearch().catch(e=>notify($t(String(e))))}>{@html icon('back')}</Button.Root>{/if}<nav class="workspace-switch" aria-label={$t("问答、搜索与文件")}><Button.Root class={`nav-item ${view==='ask'?'active':''}`} aria-current={view==='ask'?'page':undefined} title={$t("问答")} onclick={()=>navigate('ask')}>{@html icon('chat')}<span>{$t("问答")}</span></Button.Root><Button.Root class={`nav-item ${view==='search'?'active':''}`} aria-current={view==='search'?'page':undefined} title={$t("搜索（⌘ / Ctrl + K）")} onclick={()=>navigate('search')}>{@html icon('search')}<span>{$t("搜索")}</span></Button.Root><Button.Root class={`nav-item ${fileView?'active':''}`} aria-current={fileView?'page':undefined} title={$t("文件")} onclick={()=>navigate('files')}>{@html icon('folder')}<span>{$t("文件")}</span></Button.Root></nav><Button.Root type="button" class="outline agent-connect-entry" title={$t("接入 Agent")} aria-label={$t("接入 Agent")} onclick={openAgentSetup}>{@html icon('plug')}<span>{$t("接入 Agent")}</span></Button.Root></div>
 <div class="content" bind:this={workspaceContent}>
 {#if loading}<div class="empty">{$t("正在打开本地知识库…")}</div>
 {:else if view==='ask'}<section class="ask-content" class:ask-home={!shownMessages.length&&!pending} class:has-history={conversations.length>0} use:rememberScroll={{key:'sag.history.scroll',enabled:!loading&&!shownMessages.length&&!pending}}>{#if !shownMessages.length&&!pending}<div class="welcome"><div class="welcome-mark" aria-hidden="true">{@html icon('chat')}</div><h1>{roots.length?$t("向你的文件提问"):$t("你的文件，直接成为知识库。")}</h1><p>{roots.length?$t("跨文件查找信息，随时查看原文。"):$t("选一个文件夹，文档和代码自动加入。")}<br>{roots.length?'':$t("无需逐篇上传，无需模型预处理。")}</p></div>{:else}<div class="chat-log" bind:this={chatLog} use:followChat={[chatScrollKey,pending,renderedAnswers]}><div class="chat-messages">{#each displayedMessages as m,index}{@const rendered=renderedAnswers[answerKey(m)]}{@const events=answerEvents(m.tool_calls,m.reasoning_blocks)}<div class="user-turn">
 {#if editingMessage===index}<form class="message-editor" on:submit|preventDefault={()=>regenerate(index,editingText.trim())}>
 <Textarea aria-label={$t("编辑问题")} bind:ref={messageInput} bind:value={editingText} oncompositionstart={compositionStart} oncompositionend={compositionEnd} onkeyup={compositionKeyUp} onkeydown={(event:KeyboardEvent)=>{if(imeKey(event))return;if(event.key==='Escape'){editingMessage=null;event.preventDefault()}else if(event.key==='Enter'&&(event.metaKey||event.ctrlKey)){event.preventDefault();if(editingText.trim())void regenerate(index,editingText.trim())}}}/>
 {#if index<displayedMessages.length-1}<p class="message-edit-note">{$t("重新发送将替换此轮及后续 {value0} 轮问答，可在完成后撤销。",{value0:displayedMessages.length-index-1})}</p>{/if}
 <div class="message-editor-actions"><Button.Root type="button" class="outline" onclick={()=>editingMessage=null}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary" disabled={!editingText.trim()}>{$t("保存并发送")}</Button.Root></div>
 </form>{:else}<div class="user-message">{m.question}</div><div class="message-actions user-actions" aria-label={$t("问题操作")}>
 <Button.Root class="icon-button" title={$t("复制问题")} aria-label={$t("复制问题")} onclick={()=>copyMessage(m.question)}>{@html icon('copy')}</Button.Root>
 <Button.Root class="icon-button" title={$t("编辑问题")} aria-label={$t("编辑问题")} disabled={busy} onclick={()=>beginEdit(index)}>{@html icon('edit')}</Button.Root>
 <Button.Root class="icon-button" title={$t("删除此轮问答")} aria-label={$t("删除此轮问答")} disabled={busy||!conversation} onclick={()=>deleteMessage(index)}>{@html icon('delete')}</Button.Root>
 </div>{/if}</div><article class="answer"><div class="answer-head">{@html logo}<span>SAG Wiki</span></div>{#if m.reasoning&&!m.reasoning_blocks?.length}<Reasoning content={m.reasoning} running={m===pending&&busy&&stage==='思考中'&&!m.answer}/>{/if}{#each events as event,i(`${event.kind}:${event.index}`)}{@const html=rendered?.events[i]?.html??escaped(m.answer.slice(event.from,event.offset)).replace(/\n/g,'<br>')}{#if html}<div class="answer-text" role="presentation" on:click={(e)=>{const target=(e.target as HTMLElement).closest<HTMLElement>('[data-citation]');if(target)source=m.evidence[Number(target.dataset.citation)]}} use:answerHtml={html}></div>{/if}{#if event.kind==='reasoning'}{@const block=m.reasoning_blocks![event.index]}<Reasoning content={block.content} running={event.index===m.reasoning_blocks!.length-1&&m===pending&&busy&&stage==='思考中'&&block.answer_offset===m.answer.length&&block.tool_count===(m.tool_calls?.length??0)}/>{:else}{@const call=m.tool_calls![event.index]}<ToolCall {call} state={call.error&&!call.is_error&&m.status==='stopped'?'stopped':m===pending&&activeToolId===call.id?(m.error?'failed':busy?'running':'stopped'):'complete'} stage={m===pending&&activeToolId===call.id?searchStageText:''} scope={m.scope.length===0?$t("全部文件"):m.scope.map(basename).join('、')}/>{/if}{/each}{#if m===pending&&busy&&!activeToolId&&!(stage==='思考中'&&(m.reasoning_blocks?.length?m.reasoning_blocks.at(-1)!.answer_offset===m.answer.length&&m.reasoning_blocks.at(-1)!.tool_count===(m.tool_calls?.length??0):m.reasoning))}<div class="answer-progress" role="status"><span class="progress-indicator" aria-hidden="true"></span><span>{$t(stage)}</span></div>{/if}<div class="answer-text" role="presentation" on:click={(e)=>{const target=(e.target as HTMLElement).closest<HTMLElement>('[data-citation]');if(target)source=m.evidence[Number(target.dataset.citation)]}} use:answerHtml={rendered?.html??(answerRenderer?'':escaped(m.answer.slice(events.at(-1)?.offset??0)).replace(/\n/g,'<br>'))}></div>{#if m.error}<p class="form-error">{$t(m.error)}{#if m.error==="请在设置中填写问答模型 API 密钥"}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="models";settingsOpen=true}}>{$t("前往设置")}</Button.Root>{/if}</p>{/if}{#if rendered?.sources.length}<div class="source-list">{#each rendered.sources as group}<Button.Root class="source-chip" onclick={()=>source=group.first}><small>{group.number}</small>{@html icon('file')}<span>{group.first.title}</span></Button.Root>{/each}</div>{/if}{#if m.context||m.created_at!=null}<div class="answer-context-reveal"><p class="answer-context">{#if m.context}{$t("{value0} 输入 tokens{value1} · ",{value0:compactNumber(m.context.input_tokens??m.context.estimated_input),value1:m.context.input_tokens===null?$t("（估算）"):''})}{#if m.context.output_tokens===null}{$t("输出 tokens 未返回")}{:else}{$t("{value0} 输出 tokens",{value0:compactNumber(m.context.output_tokens)})}{/if}{#if m.context.elapsed_ms!=null}{$t(" · 耗时 {value0}",{value0:durationLabel(m.context.elapsed_ms)})}{/if}{#if m.context.output_tokens!==null&&m.context.generation_ms} · {compactNumber(m.context.output_tokens*1000/m.context.generation_ms)} tokens/s{/if}{#if m.context.omitted_sources}{$t(" · 已略去 {value0} 个片段以满足预算",{value0:compactNumber(m.context.omitted_sources)})}{/if}{/if}{#if m.created_at!=null}{#if m.context} · {/if}<Timestamp value={m.created_at}/>{/if}</p></div>{/if}{#if !m.tool_calls?.length&&m.retrieval_question!==m.question}<details class="trace"><summary>{$t("检索问题")}</summary>{m.retrieval_question}</details>{/if}
 {#if m.answer||!busy}<div class="message-actions answer-actions" aria-label={$t("回答操作")}>
 <Button.Root class="icon-button" title={$t("复制回答")} aria-label={$t("复制回答")} disabled={!m.answer} onclick={()=>copyMessage(m.answer)}>{@html icon('copy')}</Button.Root>
 <Button.Root class="icon-button" title={$t("重新生成回答")} aria-label={$t("重新生成回答")} disabled={busy} onclick={()=>retryMessage(index)}>{@html icon('refresh')}</Button.Root>
 <Button.Root class="icon-button" title={$t("删除此轮问答")} aria-label={$t("删除此轮问答")} disabled={busy||!conversation} onclick={()=>deleteMessage(index)}>{@html icon('delete')}</Button.Root>
 
 </div>{/if}</article>{/each}</div></div>{/if}
 <div class="composer-wrap"><Popover.Root bind:open={mention}><Popover.Portal to=".real-client"><Popover.Content class="mention-picker" customAnchor={composer} collisionBoundary={workspaceContent} side={mentionSide} sideOffset={6} collisionPadding={8} align="start" trapFocus={false} onOpenAutoFocus={event=>event.preventDefault()} onCloseAutoFocus={event=>event.preventDefault()}><div class="mention-heading">{$t("@ 引用文件")}</div><div class="mention-options" role="listbox" aria-label={$t("引用文件")}>{#each mentions as f,i}<Button.Root role="option" aria-selected={i===mentionIndex} onmousedown={(event:MouseEvent)=>{event.preventDefault();(()=>reference(f.path))();}}>{@html icon('file')}<span><strong>{f.name}</strong><small>{displayPath(f.path)}</small></span></Button.Root>{:else}<p>{$t("没有匹配文件")}</p>{/each}</div><div class="mention-help">{$t("↑ ↓ 选择 · Enter 引用 · Esc 关闭")}</div></Popover.Content></Popover.Portal></Popover.Root><form class="composer" bind:this={composer} use:mentionPlacement on:submit|preventDefault={()=>run('ask')}><div class="reference-chips">{#each references as path}<span class="reference-chip">{@html icon('file')}<span>{basename(path)}</span><Button.Root type="button" aria-label={$t("移除引用 {value0}",{value0:basename(path)})} onclick={()=>references=references.filter(p=>p!==path)}>{@html icon('close')}</Button.Root></span>{/each}</div><Textarea aria-label={$t("向你的文件提问")} bind:ref={askInput} bind:value={ask} oninput={mentionUpdate} onkeydown={askKey} onkeyup={compositionKeyUp} oncompositionstart={compositionStart} oncompositionend={compositionEnd} placeholder={$t("向你的文件提问，输入 @ 引用文件…")} disabled={busy}/><div class="composer-bottom"><div class="composer-options"><Button.Root class="scope-button" type="button" aria-haspopup="dialog" aria-expanded={scopeOpen} onclick={scopeShow}>{@html icon(scope.length?'folder':'globe')}<span>{scopeLabel}</span>{@html icon('down')}</Button.Root></div><div class="composer-model-controls">{#if settings}<ContextUsage context={busy?pending?.context:conversation?.messages.at(-1)?.context} limit={settings.answer_context_tokens??65536} loading={busy&&!pending?.context}/><ReasoningControl model={settings.answer.model} models={settings.answer.models} onModelChange={model=>{if(settings)settings={...settings,answer:{...settings.answer,model}}}} endpoint={settings.answer.url} value={settings.model_profiles?.[settings.answer.model]?.reasoning_effort} disabled={busy} onChange={setReasoning} onCommit={saveReasoning}/>{/if}{#if busy}<Button.Root type="button" class="send" aria-label={$t("停止生成")} onclick={()=>stop()}>{@html icon('stop')}</Button.Root>{:else}<Button.Root class="send" aria-label={$t("发送问题")} disabled={!ask.trim()}>{@html icon('arrow')}</Button.Root>{/if}</div></div></form></div>{#if !shownMessages.length&&!pending&&conversations.length}<section class="recent-conversations" aria-label={$t("会话列表")}><div class="recent-heading"><span>{$t("会话列表")}</span><Button.Root class="icon-button" title={$t("搜索会话")} aria-label={$t("搜索会话")} onclick={()=>historySearchOpen=true}>{@html icon('search')}</Button.Root></div><div class="recent-list">{#each conversations as c(c.id)}<div class="history-row" class:active={view==='ask'&&conversation?.id===c.id}><Button.Root class="conversation-item" title={c.title} onclick={()=>chooseHistory(c)}>{@html icon('chat')}<span>{c.title}</span></Button.Root><Timestamp value={c.updated}/><Button.Root class="history-more" aria-label={$t("{value0} 的选项",{value0:c.title})} onclick={(e:MouseEvent)=>chatMenu(e,c)}>{@html icon('more')}</Button.Root></div>{:else}<p class="history-empty">{$t("还没有问答记录")}</p>{/each}</div></section>{/if}</section>
 {:else if view==='search'}<section class="search-page" class:has-results={searched} aria-busy={busy}>{#if !searched}<div class="welcome search-welcome"><div class="welcome-mark" aria-hidden="true">{@html icon('search')}</div><h1>{$t("搜索你的文件")}</h1><p>{$t("查找文件内容，随时查看原文。")}</p></div>{/if}<form class="search-composer" on:submit|preventDefault={()=>run(searchMode)}><div class="search-entry"><Input aria-label={$t("搜索关键词")} bind:ref={searchInput} bind:value={search} onkeydown={searchKey} onkeyup={compositionKeyUp} oncompositionstart={compositionStart} oncompositionend={compositionEnd} placeholder={$t("输入关键词或问题，搜索你的文件…")}/></div><div class="search-composer-bottom"><Button.Root class="scope-button" type="button" aria-haspopup="dialog" aria-expanded={scopeOpen} onclick={scopeShow}>{@html icon(scope.length?'folder':'globe')}<span>{scopeLabel}</span>{@html icon('down')}</Button.Root><div class="search-composer-actions"><Select class="search-mode" label={$t("搜索模式")} value={searchMode} onValueChange={changeSearchMode} disabled={busy} items={[{value:"deep_search",label:$t("深入")},{value:"search",label:$t("快速")}]}/>{#if busy}<Button.Root type="button" class="send" aria-label={$t("停止搜索")} onclick={()=>stop()}>{@html icon('stop')}</Button.Root>{:else}<Button.Root class="send" aria-label={$t("搜索文件")} disabled={!search.trim()}>{@html icon('arrow')}</Button.Root>{/if}</div></div></form>{#if !searched&&searchHistory.length}<section class="search-history" aria-label={$t("最近搜索")}><div class="search-history-heading"><span>{$t("最近搜索")}</span><Button.Root class="icon-button" aria-label={$t("清空搜索历史")} title={$t("清空搜索历史")} onclick={clearSearchHistory}>{@html icon('delete')}</Button.Root></div><div class="search-history-items">{#each searchHistory as question}<Button.Root class="search-history-item" title={question} onclick={()=>{search=question;void run(searchMode)}}>{@html icon('search')}<span>{question}</span></Button.Root>{/each}</div></section>{/if}{#if searched&&settings?.search_summary_enabled!==false&&(summaryBusy||summaryText||summaryError)}<SearchSummary text={summaryText} running={summaryBusy} error={summaryError} evidence={summaryEvidence} onSource={candidate=>{source=candidate}} onCopy={copyMessage}/>{/if}{#if searched}<div class="search-summary"><span aria-live="polite">{searchStopped?$t("已停止搜索 · "):''}{busy&&!paging?$t(searchJobKind==='deep_search'?"深入搜索中":"快速搜索中"):$t("{value0} 个文件",{value0:searchPagination?.total??groups.length})}{#if searchElapsed!==null} · {searchElapsed<1000?`${searchElapsed} ms`:$t("{value0} 秒",{value0:(searchElapsed/1000).toFixed(1)})}{/if}</span>{#if !busy}<div class="search-summary-actions"><span>{searchRanked?$t("SAG2 深度结果"):$t("本地匹配")}</span></div>{/if}</div>{#if searchError}<p class="form-error" role="alert">{$t(searchError)}{#if searchError===$t("请在设置中填写 Jev API 密钥")}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="jev";settingsOpen=true}}>{$t("前往设置")}</Button.Root>{/if}</p>{/if}{#if busy&&!paging}<div class="search-loading" role="status" aria-live="polite" aria-atomic="true"><span class="search-loading-icon" aria-hidden="true">{@html icon('refresh')}</span><strong>{searchStageText}</strong></div>{:else if groups.length}<div class="search-layout" class:is-paging={paging} inert={paging}><div class="search-results" bind:this={searchList}>{#each groups as g}<Button.Root class={`search-result ${focus?.path===g.first.path?'selected':''}`} onclick={()=>focusResult(g.first)}><span class="search-result-title">{@html icon('file')}<strong>{@html highlightedText(g.first.title,searchTerms)}</strong>{@html icon('chevron')}</span><p>{@html searchSnippet(searchText(g.first.path,g.first.content),searchTerms)}</p><small>{basename(dirname(g.first.path))}{g.matches.length>1?$t(" · {value0} 个片段",{value0:g.matches.length}):''}</small></Button.Root>{/each}</div><article class="search-preview">{#if focus}<div class="search-preview-head"><div>{@html icon('file')}<strong>{focus.title}</strong></div><Button.Root class="outline" onclick={()=>edit(focus!.path,searchTerms)}>{$t("打开文档")}</Button.Root></div><div class="source-path">{displayPath(focus.path)}</div>{#if previewError}<p class="small muted">{previewError}</p>{/if}{#if previewLoading}<p class="small muted" role="status">{$t("正在读取文件…")}</p>{:else if previewDocument}{#if isHtml(focus.path)}<HtmlDocument body={previewDocument.body} terms={searchTerms} initialMatch={previewInitialMatch}/>{:else}<HighlightedDocument html={preview} terms={searchTerms} path={focus.path} initialMatch={previewInitialMatch}/>{/if}{/if}{/if}</article></div>
{#if searchPagination&&searchPagination.total>searchPagination.page_size}
 <div class="search-pagination-footer">
  <span class="search-page-range" aria-live="polite">{paging?$t("正在翻页…"):$t("第 {value0}–{value1} 个，共 {value2} 个文件",{value0:(searchPagination.page-1)*searchPagination.page_size+1,value1:Math.min(searchPagination.page*searchPagination.page_size,searchPagination.total),value2:searchPagination.total})}</span>
  <Pagination.Root class="search-pagination" role="navigation" aria-label={$t("搜索结果分页")} count={searchPagination.total} perPage={searchPagination.page_size} bind:page={searchPage} onPageChange={changeSearchPage} siblingCount={1}>
   {#snippet children({pages})}
    <Pagination.PrevButton class="page-button" disabled={busy} aria-label={$t("上一页")}>{@html icon('back')}</Pagination.PrevButton>
    {#each pages as page (page.key)}
     {#if page.type==='ellipsis'}<span class="page-ellipsis" aria-hidden="true">…</span>
     {:else}<Pagination.Page class="page-button" {page} disabled={busy}>
      {#snippet child({props})}<button {...props} disabled={busy} aria-label={$t("前往第 {value0} 页",{value0:page.value})} aria-current={searchPage===page.value?'page':undefined}>{page.value}</button>{/snippet}
     </Pagination.Page>{/if}
    {/each}
    <Pagination.NextButton class="page-button page-next" disabled={busy} aria-label={$t("下一页")}>{@html icon('back')}</Pagination.NextButton>
   {/snippet}
  </Pagination.Root>
 </div>
{/if}
{:else if !busy}<div class="search-empty">{@html icon('search')}<h2>{searchStopped?$t("已停止搜索"):searchError?$t("搜索未完成"):$t("没有找到相关文件")}</h2><p>{searchStopped?$t("可以重新搜索，或调整范围。"):searchError?$t("可以重试搜索，或切换搜索模式。"):$t("换一个关键词，或调整查询范围。")}</p><Button.Root class="outline" onclick={scopeShow}>{$t("选择范围")}</Button.Root></div>{/if}{/if}</section>
 {:else if view==='files'&&librarySessions[selected]}{#key selected}<LibraryWorkspace scope={selected} parent={libraryParent} session={librarySessions[selected]} onModeChange={()=>librarySessions={...librarySessions}} entries={directoryEntries} totalFiles={files.length} indexedFiles={files} {roots} onIndexDetails={()=>indexDetailsOpen=true} onPauseChange={indexPauseChanged} oncreate={create} onadd={add} reading={!!selected&&readingFolder===selected} revision={libraryRevision} onbrowse={libraryBrowse} onopen={(path,terms)=>edit(path,terms,selected)} onmenu={showMenu} onSettings={()=>{settingsPage='files';settingsOpen=true}}/>{/key}
 {:else if view==='document'&&readOnlyDoc}<section class="readonly-document"><div class="editor-head"><Button.Root class="icon-button" title={$t(documentFromConversation?"返回会话":documentFromSearch?"返回搜索":"返回文件夹")} onclick={returnToFiles}>{@html icon('back')}</Button.Root><span>{readOnlyDoc.name}</span><span class="document-status">{#if isRtf(readOnlyDoc.path)}{$t("RTF 正文")} · {/if}{$t("只读")}</span><Button.Root class="outline" onclick={()=>api('reveal_path',{path:readOnlyDoc!.path}).catch(e=>notify($t(String(e))))}>{mac?$t("在 Finder 中打开"):$t("在文件资源管理器中打开")}</Button.Root></div><div class="source-path">{displayPath(readOnlyDoc.path)}</div>{#if isHtml(readOnlyDoc.path)}<HtmlDocument body={readOnlyDoc.body} terms={documentSearchTerms} initialMatch={documentInitialMatch}/>{:else}<HighlightedDocument html={readonlyHtml(readOnlyDoc.path,readOnlyDoc.body)} terms={documentSearchTerms} initialMatch={documentInitialMatch}/>{/if}</section>
 {:else if view==='editor'&&draft}<section class="editor wysiwyg-editor"><div class="editor-head"><div class="editor-location"><Button.Root class="icon-button" title={$t(documentFromConversation?"返回会话":documentFromSearch?"返回搜索":"返回文件夹")} onclick={returnToFiles}>{@html icon('back')}</Button.Root><span class="scope-info" title={displayPath(selected)}>{@html icon('folder')}<span>{basename(selected)}</span></span></div><div class="document-name-wrap"><Input class="document-name" aria-label={$t("文件名")} title={draft.name} bind:value={draft.name} readonly={!draft.is_new} oninput={()=>dirty=true}/></div><div class="editor-save-actions"><span class="document-status">{dirty?$t("未保存"):draft.is_new?'':$t("已保存")}</span><Button.Root class="primary small" onclick={save}>{$t("保存")}</Button.Root></div></div>{#if EditorComponent}{#key draft.path}<svelte:component this={EditorComponent} body={draft.body} path={draft.path} searchTerms={documentSearchTerms} initialMatch={documentInitialMatch} onChange={changed}/>{/key}{/if}<div class="editor-footer">{draft.is_new?$t("保存后才创建文件并加入知识库。"):$t("修改后点击保存。")}</div></section>{/if}
 {#if error&&view!=='search'&&!answerHasError}<p class="form-error">{$t(error)}</p>{/if}
 </div></main></div></div>
{#if menu}<ActionMenu {menu} onClose={()=>menu=null}/>{/if}
{#if removeFolderPath}<Modal label={$t("移除文件夹")} onClose={()=>{if(!removingFolder)removeFolderPath=''}}><div class="dialog-head"><h2>{$t("移除文件夹")}</h2></div><div class="dialog-body"><p>{$t("确定移除「{value0}」及其索引吗？",{value0:basename(removeFolderPath)})}</p><p class="small muted" style="overflow-wrap:anywhere">{displayPath(removeFolderPath)}</p><p>{$t("本地原文件和会话记录会保留，之后可重新添加文件夹。")}</p>{#if removeFolderError}<p class="form-error" role="alert">{removeFolderError}</p>{/if}</div><div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消移除文件夹")} disabled={removingFolder} onclick={()=>removeFolderPath=''}>{$t("取消")}</Button.Root><Button.Root type="button" class="primary" aria-label={$t("确认移除文件夹")} disabled={removingFolder} onclick={confirmRemoveFolder}>{$t(removingFolder?"正在移除…":"确认移除")}</Button.Root></div></Modal>{/if}
{#if scopeOpen}<Modal label={$t("选择查询范围")} class="scope-dialog" onClose={()=>scopeOpen=false}><div class="dialog-head"><h2>{$t("选择查询范围")}</h2><Button.Root class="icon-button" aria-label={$t("关闭")} onclick={()=>scopeOpen=false}>{@html icon('close')}</Button.Root></div><div class="dialog-body"><p>{$t("选中的文件夹包含全部子文件夹。")}</p><Button.Root class="scope-all" aria-pressed={!scopeDraft.length} onclick={()=>scopeDraft=[]}>{@html icon('globe')}<span>{$t("全部文件")}</span>{#if !scopeDraft.length}{@html icon('check')}{/if}</Button.Root><div class="scope-tree"><Tree nodes={roots} expanded={scopeExpanded} {cache} mode="scope" checked={scopeDraft} ontoggle={(path)=>toggle(path,true).catch(e=>notify($t(String(e))))} oncheck={scopeCheck}/></div></div><div class="dialog-footer"><Button.Root onclick={()=>scopeOpen=false}>{$t("取消")}</Button.Root><Button.Root class="primary" onclick={applyScope}>{$t("应用")}</Button.Root></div></Modal>{/if}
{#if indexDetailsOpen}<IndexDetails {roots} onClose={()=>indexDetailsOpen=false} onPauseChange={indexPauseChanged}/>{/if}
{#if settingsOpen&&settings}<SettingsPanel onBeforeUpdate={protect} onIndexHistory={()=>indexDetailsOpen=true} onDataCleared={clearAllDataState} onFoldersRemoved={clearFoldersState} onConversationsCleared={clearConversationState} value={settings} initial={settingsPage} onClose={()=>settingsOpen=false} onSave={saveSettings} onSaved={()=>notify($t("设置已保存"))} onShowGuide={()=>{settingsOpen=false;onboardingOpen=true}}/>{/if}
{#if renameId}<Modal label={$t("重命名问答")} onClose={()=>renameId=''}><form on:submit|preventDefault={rename}><div class="dialog-head"><h2>{$t("重命名问答")}</h2></div><div class="dialog-body"><Input data-autofocus aria-label={$t("问答名称")} bind:value={renameTitle} required/></div><div class="dialog-footer"><Button.Root type="button" onclick={()=>renameId=''}>{$t("取消")}</Button.Root><Button.Root class="primary">{$t("保存")}</Button.Root></div></form></Modal>{/if}
{#if leave}<Modal label={$t("保存文档")} onClose={()=>{void leaveChoice('cancel')}}><div class="dialog-head"><h2>{$t("保存文档？")}</h2></div><div class="dialog-body"><p>{$t("「{value0}」还有未保存内容。",{value0:draft?.name})}</p></div><div class="dialog-footer"><Button.Root onclick={()=>leaveChoice('cancel')}>{$t("取消")}</Button.Root><Button.Root onclick={()=>leaveChoice('discard')}>{$t("不保存")}</Button.Root><Button.Root class="primary" onclick={()=>leaveChoice('save')}>{$t("保存")}</Button.Root></div></Modal>{/if}
{#if source}<Modal label={$t("引用原文")} drawer onClose={()=>source=null}><div class="context-top"><span>{source.title}</span><Button.Root class="icon-button" aria-label={$t("关闭引用")} onclick={()=>source=null}>{@html icon('close')}</Button.Root></div><CitationDocument {source} onOpen={terms=>edit(source!.path,terms,'',source!)}/></Modal>{/if}
<svelte:document on:visibilitychange={scheduleNotice}/>
{#if toast}<div id="notice" role="status" style:display="flex" on:mouseenter={()=>clearTimeout(noticeTimer)} on:mouseleave={scheduleNotice} on:focusin={()=>clearTimeout(noticeTimer)} on:focusout={()=>queueMicrotask(scheduleNotice)}><span>{$t(toast)}</span>{#if undo}<Button.Root onclick={()=>undo?.()}>{$t("恢复")}</Button.Root>{/if}<Button.Root aria-label={$t("关闭提示")} onclick={closeNotice}>{@html icon('close')}</Button.Root></div>{/if}

{#if retryIndex!==null}<Modal label={$t("重新生成回答")} onClose={()=>retryIndex=null}><form on:submit|preventDefault={()=>regenerate(retryIndex!,displayedMessages[retryIndex!].question)}><div class="dialog-head"><h2>{$t("重新生成回答")}</h2></div><div class="dialog-body"><p>{$t("此轮及后续 {value0} 轮问答将被替换。完成后可以撤销，恢复原问答。",{value0:displayedMessages.length-retryIndex-1})}</p></div><div class="dialog-actions"><Button.Root type="button" class="outline" onclick={()=>retryIndex=null}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary">{$t("重新生成")}</Button.Root></div></form></Modal>{/if}

{#if toolApproval}<Modal label={$t("确认 MCP 工具调用")} onClose={()=>answerApproval(false)}><div class="dialog-head"><h2>{$t("确认工具调用")}</h2></div><div class="dialog-body"><p><strong>{toolApproval.server_name}</strong> · {toolApproval.tool_name}</p><pre class="tool-approval-arguments">{JSON.stringify(toolApproval.arguments,null,2)}</pre><p class="small muted">{$t("只有允许后才会执行本次调用。")}</p></div><div class="dialog-footer"><Button.Root onclick={()=>answerApproval(false)}>{$t("拒绝")}</Button.Root><Button.Root class="primary" onclick={()=>answerApproval(true)}>{$t("允许本次")}</Button.Root></div></Modal>{/if}

{#if onboardingOpen}<Onboarding onClose={closeOnboarding}/>{/if}

{#if historySearchOpen}<HistorySearch {conversations} onClose={()=>historySearchOpen=false} onSelect={async c=>{await chooseHistory(c);if(conversation?.id===c.id)historySearchOpen=false}}/>{/if}

{#if folderPickerOpen}<FolderPicker defaults={{file_types:{markdown:true,text:true,code:false,config:false,...settings?.file_types},convert_other_formats:settings?.convert_other_formats??false,conversion_formats:settings?.conversion_formats}} roots={roots.map(root=>root.path)} onClose={()=>folderPickerOpen=false} onAdd={addFolders}/>{/if}

{#if folderIndexPath}<FolderIndexSettings path={folderIndexPath} onClose={()=>folderIndexPath=''} onSaved={rebuild=>{void refresh().catch(e=>notify($t(String(e))));notify($t(rebuild?'正在重新索引当前文件夹':'索引设置已保存，正在更新索引'))}}/>{/if}

{#if settings}<UpdateNotification config={settings.updates} onOpen={()=>{settingsPage='about';settingsOpen=true}}/>{/if}
