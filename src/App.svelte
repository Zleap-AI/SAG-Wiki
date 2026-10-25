<script lang="ts">
 import {displayPath,sourceName,sourceFolder,citedNumbers} from './lib/domain';
 import {t,setLanguage} from './lib/i18n';
 import {externalLinkClick} from './lib/external-links';
 import {applyTheme,watchSystemTheme} from './lib/theme';
 import DesktopTitlebar from './components/DesktopTitlebar.svelte';
 import {indexStages} from './lib/index-stages';
 import LibraryWorkspace from './components/LibraryWorkspace.svelte';
 import IndexDetails from './components/IndexDetails.svelte';
 import DoclingDownload from './components/DoclingDownload.svelte';
 import DocumentQuestion from './components/DocumentQuestion.svelte';
 import SaveNoteDialog from './components/SaveNoteDialog.svelte';
 import NoteStatus from './components/NoteStatus.svelte';
 import type {NoteDetail,NoteSourceStatus,NoteSummary} from './lib/types';
 import {isNote} from './lib/domain';
 let saveNoteFor:{index:number}|null=null;let noteDetail:NoteDetail|null=null,noteChecking=false,noteRequest=0;let noteSummaries:NoteSummary[]=[];
 import type {IndexStatus} from './lib/types';
 import SearchSummary from './components/SearchSummary.svelte';
 import SearchResultItem from './components/SearchResultItem.svelte';
 import SearchPassagePicker from './components/SearchPassagePicker.svelte';
 import {quietScroll} from './lib/quiet-scroll';
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
 import ConversationSidebar from './components/ConversationSidebar.svelte';
 import ToolGroup from './components/ToolGroup.svelte';
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
 import AnswerProcess from './components/AnswerProcess.svelte';
 import ReasoningControl from './components/ReasoningControl.svelte';
 import ContextUsage from './components/ContextUsage.svelte';
 import type {ReasoningEffort} from './lib/reasoning';
 import {followChat} from './lib/chat-scroll';
 import {answerHtml} from './lib/answer-html';
 import {answerEvents} from './lib/answer-events';
 import {subscribeDataChanges} from './lib/data-changes';
 import {stableRows} from './lib/stable-snapshot';
 import HighlightedDocument from './components/HighlightedDocument.svelte';import HtmlDocument from './components/HtmlDocument.svelte';import{highlightedText,searchSnippet,searchText,documentMatchIndex}from './lib/search-highlights';import{legacyPreview}from './lib/preview-mode';
 import Tree from './components/Tree.svelte';import SettingsPanel from './components/Settings.svelte';import UpdateNotification from './components/UpdateNotification.svelte';import{api}from './lib/api';import{icon,logo}from './lib/icons';import{basename,dirname,normalizeScope,documentResults,escaped,fileName,inside,indexedSourcePath,isMarkdown,isRtf,isHtml,readonlyHtml,compactNumber,durationLabel}from './lib/domain';import type{Root,Entry,FileInfo,Document,Draft,Settings,Conversation,Candidate,Progress,QueryOutput,Message,McpCredentials,ToolApproval,ToolCallRecord}from './lib/types';
 let librarySessions:Record<string,LibrarySession>={'':createLibrarySession()};let libraryRevision=0;let exploreReturn='';let documentFromSearch=false,documentFromConversation=false,documentSearchScroll=0,documentConversationScroll=0,documentInitialMatch=0;let chatLog:HTMLDivElement;
 $:if(selected&&!librarySessions[selected])librarySessions={...librarySessions,[selected]:createLibrarySession()};
 let roots:Root[]=[],files:FileInfo[]=[],conversations:Conversation[]=[],settings:Settings|null=null,cache:Record<string,Entry[]>={},expanded=new Set<string>(JSON.parse(localStorage.getItem('sag.sidebar.expanded')??'[]'));
 let readOnlyDoc:Document|null=null;let searchTerms:string[]=[],documentSearchTerms:string[]=[];
 let previewReader:{doc:Document;Reader:typeof import('./components/MarkdownReader.svelte').default;model:import('./lib/document-model').MarkdownModel;target:number}|null=null;
let previewDocument:Document|null=null,previewInitialMatch=0,previewLoading=false,previewError='',previewRequest=0;
 let view='ask',selected='',readingFolder='',collapsed=false,conversation:Conversation|null=null,ask='',search='',searched=false,results:Candidate[]=[],focus:Candidate|null=null,preview='',source:Candidate|null=null;
 let sidebarMotion=false,sidebarAnimations:Animation[]=[];
 let composing=false,compositionEndedAt=-Infinity;
 let references:string[]=[],mention=false,mentionTerm='',mentionIndex=0,mentionStart=0,mentionEnd=0,askInput:HTMLTextAreaElement;
 let composer:HTMLFormElement,workspaceContent:HTMLDivElement,mentionSide:'top'|'bottom'='top';
 let searchMode:'search'|'deep_search'=(localStorage.getItem('sag.search.mode') as 'search'|'deep_search'|null)??'deep_search';let searchModeChosen=localStorage.getItem('sag.search.mode')!==null;
 $:if(settings&&!searchModeChosen&&!jevReady)searchMode='search';
 let searchHistory:string[]=[],searchInput:HTMLInputElement;
 let searchPagination:SearchPagination|null=null,searchPage=1,searchQuestion='',paging=false,searchList:HTMLDivElement;
 let summaryJob='',summaryText='',summaryError='',summaryBusy=false,summaryEvidence:Candidate[]=[];
 let searchStopped=false,searchRanked=false,searchElapsed:number|null=null,searchJobKind:'search'|'deep_search'='deep_search',searchError='';
 let pendingHistoryLength=0,chatScrollKey=0;
 let askJob='',cancellingAsk='',searchJob='',askStage='',searchStage='',askBusy=false,searchBusy=false;
 let activeToolId='',pending:Message|null=null,toast='',undo:(()=>Promise<void>)|null=null;
 $:busy=view==='search'?searchBusy:askBusy||!!cancellingAsk;
 $:stage=view==='search'?searchStage:askStage;
 let noticeTimer:ReturnType<typeof setTimeout>;
 let draft:Draft|null=null,dirty=false,EditorComponent:any=null,draftTimer:ReturnType<typeof setTimeout>;
 let quitPending=false;
 let indexDetailsOpen=false,indexPaused=false;
 let indexStatusHidden=false;
 let dismissedIndexFailures=localStorage.getItem('sag.index.dismissed-failures');
 let indexProgress:{path:string;count:number;total:number;stage?:string}|null=null;
 $:indexPercent=indexProgress&&indexProgress.total>0?Math.floor(indexProgress.count*100/indexProgress.total):null;
 $:indexing=roots.some(r=>r.status!=='ready'&&r.status!=='error'&&r.status!=='partial');
 $:indexFailed=roots.some(r=>r.status==='error'||r.status==='partial');
 $:indexFailureSignature=JSON.stringify(roots.filter(r=>r.status==='error'||r.status==='partial').sort((a,b)=>a.path.localeCompare(b.path)).map(({path,status,error})=>[path,status,error]));
 $:indexStage=indexStages.find(item=>item.stage===indexProgress?.stage);
 $:indexStatusText=indexPaused?$t("SAG 索引已暂停"):indexing?indexStage?$t("SAG {value0}{value1}{value2}",{value0:$t(indexStage.progress),value1:indexPercent===null?'':`（${indexPercent}%）`,value2:''}):(indexPercent===null?$t("SAG 正在索引…"):$t("SAG 正在索引（{value0}%）",{value0:indexPercent})):$t("SAG · 索引已结束");
 $:indexStatusCount=indexProgress?$t("已处理 {value0} / {value1} 个文件",{value0:indexProgress.count,value1:indexProgress.total}):$t("已索引 {value0} 个文件",{value0:roots.reduce((count,root)=>count+root.count,0)});
 $:indexFailureText=roots.length>0&&roots.every(root=>root.status==='error')?$t('读取失败'):$t('部分失败');
 let onboardingOpen=false;
 let startupError='',retrying=false;
 // Treat a missing field as configured: older services omit `has_key`, and the backend still reports precise errors.
 $:answerReady=!settings||!settings.answer||(settings.answer.model??'').trim()!==''&&(settings.answer.auth_mode==='none'||settings.answer.has_key!==false);
 $:jevReady=!settings||!settings.jev||(settings.jev.model??'').trim()!==''&&(settings.jev.auth_mode==='none'||settings.jev.has_key!==false);
 async function loadWorkspace(){startupError='';settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.credential_error)notify(settings.credential_error);searchHistory=JSON.parse(localStorage.getItem('sag.search.history')??'[]');view=settings.start_page;await Promise.all([refresh(),history()]);startupError='';}
 async function retryStartup(){if(retrying)return;retrying=true;loading=true;try{await loadWorkspace();onboardingOpen=localStorage.getItem('sag.onboarding.seen')!=='1';}catch(e){startupError=String(e)}finally{retrying=false;loading=false}}
 function closeOnboarding(){localStorage.setItem('sag.onboarding.seen','1');onboardingOpen=false;}
 let historySearchOpen=false,sidebarTab:'conversations'|'folders'=localStorage.getItem('sag.sidebar.tab')==='folders'?'folders':'conversations',historyRequest=0,folderPickerOpen=false,newDocumentFolder='';
 let chooseSaveFolder:((folder:string|null)=>void)|null=null;
 let saveFolderExpanded=new Set<string>(),saveFolderError='',saveNameNeeded=false;
 let saving=false;
 $:if(chooseSaveFolder&&!newDocumentFolder&&roots.length)newDocumentFolder=roots[0].path;
 let settingsOpen=false,settingsPage='general',scopeOpen=false,scope:string[]=[],scopeDraft:string[]=[],scopeExpanded=new Set<string>();
 let menu:MenuState|null=null;let toolApproval:ToolApproval|null=null;
 let removeFolderPath='',removingFolder=false,removeFolderError='';
 let renderedAnswers:Record<string,ReturnType<typeof import('./lib/markdown').renderAnswer>>={};
 let answerRenderer:typeof import('./lib/markdown').renderAnswer|undefined;
 let answerRendererLoading:Promise<void>|undefined;
 let renameId='',renameTitle='',renameError='',renaming=false,leave:((value:boolean)=>void)|null=null,loading=true,error='';
 let editingMessage:number|null=null,editingText='',retryIndex:number|null=null,messageInput:HTMLTextAreaElement;
 const mac=/Mac/i.test(navigator.userAgent);
 $:scopeLabel=scope.length===0?$t("全部文件"):scope.length===1?basename(scope[0]):$t("{value0} 个文件夹",{value0:scope.length});
 $:currentEntries=cache[selected]||[];
 $:directoryEntries=selected?currentEntries:roots.map(root=>({path:root.path,name:root.name,directory:true,symlink:false}));
 $:indexedPaths=new Set(files.flatMap(file=>[file.path,indexedSourcePath(file.path)]));
 $:indexedCache=Object.fromEntries(Object.entries(cache).map(([path,entries])=>[path,entries.filter(entry=>entry.directory?!entry.name.startsWith('.'):indexedPaths.has(entry.path))]));
 $:fileView=view==='files'||view==='document'||view==='editor';
 $:libraryParent=roots.some(root=>root.path===selected)?'':dirname(selected);
 $:shownMessages=pending?conversation?.messages.slice(0,pendingHistoryLength)||[]:conversation?.messages||[];
 $:displayedMessages=[...shownMessages,...(pending?[pending]:[])];
 $:answerHasError=view==='ask'&&displayedMessages.some(message=>message.error&&$t(message.error)===$t(error));
 $:if(displayedMessages.length)void renderAnswers(displayedMessages);
 function answerKey(message:Message){return JSON.stringify([message.question,message.answer,message.tool_calls?.map(call=>[call.id,call.answer_offset]),message.reasoning_blocks?.map(block=>[block.answer_offset,block.tool_count]),message.evidence.map(({id,path,hash})=>[id,path,hash])]);}
 function loadAnswerRenderer(){return answerRendererLoading??=import('./lib/markdown').then(module=>{answerRenderer=module.renderAnswer;renderAnswers(displayedMessages)});}
 function renderAnswers(messages:Message[]){if(!answerRenderer){void loadAnswerRenderer();return;}const next:typeof renderedAnswers={};for(const message of messages){const key=answerKey(message);next[key]=renderedAnswers[key]??answerRenderer(message.answer,message.evidence,message.tool_calls,message.reasoning_blocks);}renderedAnswers=next;}
 $:rankedGroups=documentResults(results);
 $:groups=searchRanked?rankedGroups.slice((searchPage-1)*20,searchPage*20):rankedGroups;
 $:searchStageText=stage==='SAG2 正在分析'?$t("正在准备搜索"):stage==='正在检索原文'?$t("正在查找相关文件"):stage==='正在扩展检索'?$t("正在扩展搜索范围"):stage==='Jev 正在判断相关原文'?$t("正在按相关性排序"):$t(stage);
 $:mentions=files.filter(f=>!references.includes(f.path)&&(f.name.toLowerCase().includes(mentionTerm.toLowerCase())||f.path.toLowerCase().includes(mentionTerm.toLowerCase()))).slice(0,20);
 $:if(settings){applyTheme(settings.theme);document.documentElement.style.setProperty('--editor-font-size',`${settings.editor_size}px`)}
 onMount(watchSystemTheme);
 async function answerApproval(allow:boolean){if(!toolApproval)return;const id=toolApproval.id;toolApproval=null;try{await api('resolve_tool_approval',{id,allow})}catch(e){notify($t(String(e)))}}
 function clearConversationState(){historyRequest++;closeNotice();conversations=[];conversation=null;pending=null;renderedAnswers={};ask='';references=[];source=null;historySearchOpen=false;editingMessage=null;retryIndex=null;askJob='';cancellingAsk='';askBusy=false;askStage='';activeToolId='';error='';toolApproval=null;localStorage.removeItem('sag.history.scroll');}
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
    const [nextRoots,nextStatus,nextFiles,removed,nextNotes]=await Promise.all([
     api<Root[]>('list_roots'),api<IndexStatus>('get_index_status'),api<FileInfo[]>('list_files'),api<string[]>('list_removed_directories'),api<NoteSummary[]>('list_notes',{scope:null}).catch(()=>[] as NoteSummary[])
    ]);
    noteSummaries=nextNotes;if(draft&&!draft.is_new&&noteDetail&&noteDetail.summary.path===draft.path)void loadNote(draft.path);
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

 async function history(){
  const request=++historyRequest;
  const next=await api<Conversation[]>('list_conversations');
  if(appDisposed||request!==historyRequest)return;
  conversations=next;
  if(conversation)conversation=next.find(c=>c.id===conversation!.id)||(askBusy?conversation:null);
 }
 async function protect():Promise<boolean>{if(!draft||!dirty)return true;return new Promise(resolve=>leave=resolve)}
 async function requestQuit(){if(quitPending||leave)return;quitPending=true;try{if(draft&&dirty)await api('reveal_main_window');if(await protect())await api('quit_application')}catch(e){notify($t(String(e)))}finally{quitPending=false}}
 async function leaveChoice(choice:'save'|'discard'|'cancel'){if(choice==='save'&&!await save())return;if(choice==='discard'&&draft){clearTimeout(draftTimer);await api('delete_draft',{path:draft.path});dirty=false;draft=null}const done=leave;leave=null;done?.(choice!=='cancel')}
 async function navigate(next:string){if(!await protect())return;view=next;if(next==='files')showSidebarTab('folders');menu=null;if(next!=='editor')draft=null;await tick();if(next==='ask')askInput?.focus({preventScroll:true});}
 type AnswerEvent=ReturnType<typeof answerEvents>[number];
 // Consecutive tool calls with no answer text between them read as one research step.
 const joinsToolRun=(m:Message,events:AnswerEvent[],i:number)=>i>0&&events[i].kind==='tool'&&events[i-1].kind==='tool'&&!m.answer.slice(events[i].from,events[i].offset).trim();
 function toolRun(m:Message,events:AnswerEvent[],i:number){const run=[i];while(run.at(-1)!+1<events.length&&joinsToolRun(m,events,run.at(-1)!+1))run.push(run.at(-1)!+1);return run}
 const toolState=(m:Message,call:ToolCallRecord,current:Message|null,active:string,working:boolean)=>call.error&&!call.is_error&&m.status==='stopped'?'stopped':m===current&&active===call.id?(m.error?'failed':working?'running':'stopped'):'complete';
 // Converted sources are stored as <name>.md next to the original; show the original name.
 const citationName=(candidate:Candidate)=>candidate.path.includes('/.sag-converted/')||candidate.path.includes('\\.sag-converted\\')?candidate.title.replace(/\.md$/i,''):candidate.title;
 function showSidebarTab(tab:'conversations'|'folders'){sidebarTab=tab;localStorage.setItem('sag.sidebar.tab',tab)}
 async function toggleSidebar(event:MouseEvent){
  const animate=event.detail>0&&!matchMedia('(prefers-reduced-motion: reduce)').matches;
  const controls=[...(event.currentTarget as HTMLElement).closest('aside')!.querySelectorAll<HTMLElement>('.brand-mark,.sidebar-titlebar>.icon-button,.nav-item,.sidebar-new-chat,.sidebar-tab,.sidebar-tools>button')];
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
   documentInitialMatch=target?await documentMatchIndex(doc.body,doc.path,terms,target):0;
   documentFromSearch=fromSearch;documentFromConversation=fromConversation;documentSearchScroll=scroll;documentConversationScroll=chatScroll;exploreReturn=origin;documentSearchTerms=terms;
   if(!isMarkdown(doc.path)){readOnlyDoc=doc;draft=null;dirty=false;selected=dirname(path);view='document';source=null;return;}
   draft={...doc,is_new:false};dirty=false;selected=dirname(path);EditorComponent=(await import('./components/RichEditor.svelte')).default;view='editor';source=null;
   noteDetail=null;if(isNote(doc.body))void loadNote(doc.path);else noteRequest++;
  }catch(e){notify($t(String(e)))}
 }
 async function loadNote(path:string){
  const request=++noteRequest;
  try{const detail=await api<NoteDetail>('get_note',{path});if(request===noteRequest)noteDetail=detail;}
  catch{if(request===noteRequest)noteDetail=null;}
 }
 async function recheckNote(){
  if(!noteDetail||noteChecking)return;noteChecking=true;
  try{await api<NoteSummary[]>('verify_notes',{paths:[noteDetail.summary.path]});await loadNote(noteDetail.summary.path);}
  catch(e){notify($t(String(e)))}finally{noteChecking=false}
 }
 async function openNoteSource(status:NoteSourceStatus){
  const span=status.current??{start_byte:status.source.start_byte,end_byte:status.source.end_byte,start_line:status.source.start_line,start_column:1,end_line:status.source.end_line,end_column:1};
  const content=status.current_text??status.source.text;
  const candidate:Candidate={id:0,document_id:0,path:status.source.path,title:basename(status.source.path),heading:'',content,start_line:span.start_line,end_line:span.end_line,source_ranges:[[span.start_byte,span.end_byte]],source_spans:[span],bm25:0,score:0,order:0,hash:status.current_hash??status.source.hash};
  source=candidate;
 }
 async function reaskNote(){
  if(!noteDetail||!await protect())return;
  const detail=noteDetail;
  await stop('ask');editingMessage=null;conversation=null;pending=null;draft=null;readOnlyDoc=null;noteDetail=null;
  ask=detail.summary.question;scope=normalizeScope(detail.summary.scope??[]);references=[];view='ask';showSidebarTab('conversations');
  await tick();askInput?.focus({preventScroll:true});
 }
 async function openNote(path:string){await edit(path);}
 async function returnToFiles(){
  if(documentFromConversation){
   if(!await protect())return;documentFromConversation=false;view='ask';draft=null;readOnlyDoc=null;
   await tick();await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
   chatLog.scrollTop=documentConversationScroll;chatLog.dispatchEvent(new Event('scroll'));
  }else if(documentFromSearch){
   if(!await protect())return;documentFromSearch=false;view='search';draft=null;readOnlyDoc=null;
   await tick();if(searchList)searchList.scrollTop=documentSearchScroll;
  }else if(exploreReturn){if(!await protect())return;selected=exploreReturn;exploreReturn='';view='files';draft=null;readOnlyDoc=null;}else await libraryBrowse(selected)
 }
 async function libraryBrowse(path:string){if(!path){if(await protect()){selected='';view='files';draft=null;readOnlyDoc=null;exploreReturn='';documentFromSearch=false;documentFromConversation=false;}return;}const entry=currentEntries.find(e=>e.path===path);if(entry?.symlink){try{await api('reveal_path',{path})}catch(e){notify($t(String(e)))}}else try{await browse(path)}catch(e){notify($t(String(e)))}}
 async function create(folder=selected){
  if(!await protect())return;
  EditorComponent=(await import('./components/RichEditor.svelte')).default;
  selected=folder;documentFromConversation=false;documentFromSearch=false;documentSearchTerms=[];
  draft={path:(folder?folder+'/':'')+'.draft-'+crypto.randomUUID(),name:$t("未命名.md"),body:'',hash:'',is_new:true};dirty=false;view='editor';
 }
 function draftFolder(value:Draft){return value.is_new&&value.path.startsWith('.draft-')?'':dirname(value.path)}
 function finishSaveFolder(folder:string|null){
  if(folder){try{fileName(draft!.name)}catch(e){saveNameNeeded=true;saveFolderError=$t(String(e));return;}}
  const resolve=chooseSaveFolder!;chooseSaveFolder=null;resolve(folder);
 }
 async function toggleSaveFolder(path:string){
  saveFolderError='';
  if(saveFolderExpanded.has(path)){saveFolderExpanded=new Set([...saveFolderExpanded].filter(item=>item!==path));return;}
  try{cache={...cache,[path]:await api<Entry[]>('list_entries',{path})};saveFolderExpanded=new Set([...saveFolderExpanded,path]);}
  catch(e){saveFolderError=$t(String(e));}
 }
 function changed(body:string){if(!draft)return;draft={...draft,body};dirty=true;clearTimeout(draftTimer);draftTimer=setTimeout(()=>{if(draft)api('put_draft',{draft}).catch(e=>notify($t('草稿保存失败：{value0}',{value0:$t(String(e))})))},500)}
 async function save(){
  if(!draft||saving)return false;
  saving=true;
  try{
   let folder=draftFolder(draft);
   const needsName=!draft.name.trim()||draft.name==='未命名.md'||draft.name==='Untitled.md';
   if(draft.is_new&&(!folder||needsName)){
    newDocumentFolder=folder||roots[0]?.path||'';
    saveFolderExpanded=new Set();saveFolderError='';saveNameNeeded=needsName;
    folder=await new Promise<string|null>(resolve=>chooseSaveFolder=resolve)??'';
    if(!folder)return false;
   }
   const old=draft.path;const name=fileName(draft.name);const path=draft.is_new?folder+'/'+name:draft.path;const doc=await api<Document>('save_document',{path,body:draft.body,expectedHash:draft.is_new?null:draft.hash});clearTimeout(draftTimer);await api('delete_draft',{path:old});draft={path:doc.path,name:doc.name,body:doc.body,hash:doc.hash,is_new:false};selected=dirname(doc.path);dirty=false;await refresh();notify($t("文件已保存，索引正在更新"));return true;
  }catch(e){notify($t(String(e)));return false}finally{saving=false}
 }
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
 async function folderQuery(path:string,next:string,file=false){if(!await protect())return false;await stop(next==='ask'?'ask':'search');if(next==='ask'){editingMessage=null;conversation=null;pending=null;ask='';references=file?[path]:[];draft=null;error='';}scope=[path];searched=false;view=next;menu=null;await tick();if(next==='ask')askInput?.focus();else searchInput?.focus();return true;}
 async function askDocument(question:string){
  let path=(draft??readOnlyDoc)!.path;
  if(draft?.is_new){if(!await save())return;path=draft!.path;}
  try{
   if(!await folderQuery(path,'ask',true))return;
   readOnlyDoc=null;source=null;retryIndex=null;mention=false;documentFromSearch=false;documentFromConversation=false;
   ask=question;await run('ask');
  }catch(e){notify($t(String(e)));}
 }
 async function remove(path:string,file=false){if(!await protect())return;await stop();await api('remove_root',{path});scope=scope.filter(p=>!inside(p,path));references=references.filter(p=>!inside(p,path));expanded=new Set([...expanded].filter(p=>!inside(p,path)));cache=Object.fromEntries(Object.entries(cache).filter(([p])=>!inside(p,path)));if(inside(selected,path)){selected='';view='files';draft=null}if(file&&(draft?.path===path||readOnlyDoc?.path===path)){draft=null;readOnlyDoc=null;view='files'}await refresh();notify($t(file?"已从知识库移除文件，原文件保留":"已移除文件夹，原文件保留"),async()=>{await api('add_root',{path});await refresh();notify($t(file?"已恢复文件":"已恢复文件夹"))});}
 async function confirmRemoveFolder(){removingFolder=true;removeFolderError='';try{await remove(removeFolderPath);removeFolderPath='';}catch(e){removeFolderError=$t(String(e));}finally{removingFolder=false;}}
 async function deleteConversation(c:Conversation){
  try{
   await api('edit_conversation',{id:c.id,title:null,deleted:true});
   if(conversation?.id===c.id){conversation=null;pending=null;references=[];ask='';editingMessage=null;retryIndex=null;error=''}
   await history();
   notify($t("已删除问答记录"),async()=>{await api('edit_conversation',{id:c.id,title:null,deleted:false});await history();notify($t("已恢复问答记录"))});
  }catch(e){notify($t(String(e)))}
 }
 function chatMenu(e:MouseEvent,c:Conversation){
  if((askBusy||cancellingAsk)&&conversation?.id===c.id)return;
  const trigger=e.currentTarget as HTMLElement,rect=trigger.getBoundingClientRect();
  menu={x:Math.min(e.clientX||rect.left,innerWidth-240),y:Math.min((e.clientY||rect.bottom)+8,innerHeight-92),trigger,items:[
   {label:$t("重命名"),glyph:'edit',run:()=>{renameId=c.id;renameTitle=c.title;renameError=''}},
   {label:$t("删除"),glyph:'close',run:()=>deleteConversation(c)}
  ]};
 }
 async function rename(){
  if(renaming||!renameTitle.trim())return;
  renaming=true;renameError='';
  try{await api('edit_conversation',{id:renameId,title:renameTitle.trim(),deleted:null});await history();renameId=''}
  catch(e){renameError=$t(String(e))}finally{renaming=false}
 }
 async function copyMessage(text:string){try{await navigator.clipboard.writeText(text);notify($t("已复制"))}catch(e){notify($t('复制失败：{value0}',{value0:$t(String(e))}))}}
 async function replaceMessages(id:string,messages:Message[]){const updated=await api<Conversation>('replace_messages',{id,messages});if(conversation?.id===id){conversation=updated;pending=null}await history()}
 async function deleteMessage(index:number){try{const before=structuredClone(conversation!);await replaceMessages(before.id,before.messages.filter((_,i)=>i!==index));editingMessage=null;notify($t("已删除此轮问答"),async()=>{await replaceMessages(before.id,before.messages);notify($t("已恢复此轮问答"))})}catch(e){notify($t(String(e)))}}
 async function beginEdit(index:number){editingMessage=index;editingText=displayedMessages[index].question;await tick();messageInput.focus();messageInput.setSelectionRange(editingText.length,editingText.length)}
 async function regenerate(index:number,question:string){const before=conversation?structuredClone(conversation):null;const message=displayedMessages[index];editingMessage=null;retryIndex=null;scope=[...message.scope];references=[...message.references];ask=question;await run('ask',before&&index<before.messages.length?index:undefined);if(before)notify(error?$t("重新生成未完成"):$t("已重新生成回答"),async()=>{await replaceMessages(before.id,before.messages);notify($t("已恢复原问答"))})}
 function retryMessage(index:number){if(index<displayedMessages.length-1)retryIndex=index;else void regenerate(index,displayedMessages[index].question)}
 async function newChat(){if(askBusy||cancellingAsk||!await protect())return;editingMessage=null;await stop('ask');conversation=null;pending=null;ask='';references=[];view='ask';draft=null;await tick();askInput?.focus({preventScroll:true})}
 async function stopSummary(){const id=summaryJob;summaryJob='';summaryBusy=false;if(id)await api('cancel_query',{id});}
 async function summarizeSearch(question:string,candidates:Candidate[]){
  if(settings?.search_summary_enabled===false||!candidates.length||!answerReady)return;
  const id=crypto.randomUUID();summaryJob=id;summaryBusy=true;summaryError='';summaryText='';summaryEvidence=documentResults(candidates).slice(0,settings?.search_summary_sources??10).map(group=>group.first);
  try{const output=await api<{text:string;evidence:Candidate[]}>('summarize_search',{id,question,results:summaryEvidence});if(summaryJob===id){summaryText=output.text;summaryEvidence=output.evidence}}
  catch(e){if(summaryJob===id)summaryError=$t(String(e))}
  finally{if(summaryJob===id){summaryBusy=false;summaryJob='';
   // Without a summary to show, the reader opens the first result instead.
   if(!summaryText&&!summaryError&&!focus&&results.length)void focusResult(results[0]);}}
 }
 async function stop(kind:'ask'|'search'=view==='search'?'search':'ask'){
  if(kind==='ask'){
   const id=askJob;if(!id||cancellingAsk===id)return;
   cancellingAsk=id;toolApproval=null;
   try{
    await api('cancel_query',{id});
    if(askJob===id){askJob='';askBusy=false;askStage='';if(pending)pending={...pending,status:'stopped'}}
   }catch(e){notify($t(String(e)))}
   finally{if(cancellingAsk===id){toolApproval=null;cancellingAsk=''}}
  }
  else{const summaryCancellation=stopSummary();if(searchBusy)searchStopped=true;const id=searchJob;searchJob='';searchBusy=false;paging=false;searchPage=searchPagination?.page??1;searchStage='';await Promise.all([summaryCancellation,...(id?[api('cancel_query',{id})]:[])]);}
 }
 async function resetSearch(focusInput=true){
  await stop('search');passagesOpen=false;summarySelected=false;searchFocusMode=false;rememberedCandidates.clear();summaryText='';summaryError='';summaryEvidence=[];search='';searched=false;results=[];focus=null;previewDocument=null;preview='';previewError='';previewLoading=false;previewRequest++;searchTerms=[];searchQuestion='';searchPagination=null;searchPage=1;searchElapsed=null;searchError='';
  await tick();if(focusInput)searchInput.focus();
 }
 async function clearAllDataState(){
  clearTimeout(draftTimer);draft=null;dirty=false;readOnlyDoc=null;
  await resetSearch(false);searchHistory=[];clearConversationState();clearFoldersState();
  librarySessions={};exploreReturn='';documentFromSearch=false;documentFromConversation=false;view='ask';
  for(const key of Object.keys(localStorage))if(key.startsWith('sag.'))localStorage.removeItem(key);
  dismissedIndexFailures=null;indexStatusHidden=false;
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
 function changeSearchMode(value:string){searchMode=value as 'search'|'deep_search';searchModeChosen=true;localStorage.setItem('sag.search.mode',searchMode);if(searched&&search.trim())void run(searchMode);}
 async function changeSearchPage(page:number){
  if(searchBusy||page===searchPagination?.page)return;
  if(!searchRanked){await run('search',undefined,page);return;}
  searchPage=page;searchPagination={...searchPagination!,page};activeResult=0;
  void focusResult(rankedGroups[(page-1)*20].first);
  await tick();if(searchList)searchList.scrollTop=0;
 }
 function searchKey(e:KeyboardEvent){if(e.key==='Enter'&&imeKey(e)&&!composing&&!e.isComposing)e.preventDefault()}
 function askKey(e:KeyboardEvent){if(imeKey(e)){if(e.key==='Enter'&&!composing&&!e.isComposing)e.preventDefault();return;}if(mention){if(e.key==='Escape'){mention=false;e.preventDefault()}else if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();mentionIndex=(mentionIndex+(e.key==='ArrowDown'?1:-1)+mentions.length)%Math.max(1,mentions.length);tick().then(()=>{document.querySelector<HTMLElement>('.mention-options [aria-selected="true"]')?.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'})})}else if(e.key==='Enter'&&mentions[mentionIndex]){e.preventDefault();reference(mentions[mentionIndex].path);return}else if(e.key==='Enter'){mention=false}else return;if(mention)return}if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();void run('ask')}}
 async function run(kind:'ask'|'search'|'deep_search',replaceFrom?:number,page?:number){
  if(composing||performance.now()-compositionEndedAt<100)return;
  const question=(kind==='ask'?ask:page===undefined?search:searchQuestion).trim();
  if(!question||(kind==='ask'?askBusy||cancellingAsk:searchBusy)||!settings)return;
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
   if(!paging){searchGeneration++;rememberedCandidates.clear();passagesOpen=false;summarySelected=false;previewFindOpen=false;previewFindTerm='';searchFocusMode=false;searchQuestion=question;searchPagination=null;searchPage=1;results=[];searchTerms=[];focus=null;previewDocument=null;preview='';previewError='';previewLoading=false;previewRequest++}
  }
  try{
   if(summaryCancellation)await summaryCancellation;
   if(kind!=='ask'&&page===undefined){const recent=[question,...searchHistory.filter(item=>item!==question)].slice(0,8);localStorage.setItem('sag.search.history',JSON.stringify(recent));searchHistory=recent;}
   if(kind==='ask'&&!answerRenderer)renderAnswers(displayedMessages);
   const output=await api<QueryOutput>('run_query',{query:{id,kind,question,scope,references:kind==='ask'?references:[],model:settings.answer.model,conversation_id:kind==='ask'?conversationId:null,replace_from:replaceFrom??null,page:kind==='search'?(page??1):null}});
   if((kind==='ask'?askJob:searchJob)!==id)return;
   if(kind==='ask'){conversation=output.conversation;pending=null;references=[];await history()}
   else{searchTerms=output.terms||[];results=output.results;searchRanked=kind==='deep_search';searchElapsed=output.elapsed_ms??null;searchPagination=kind==='deep_search'?{total:documentResults(output.results).length,page:1,page_size:20}:output.pagination??null;searchPage=searchPagination?.page??1;await tick();if(searchList)searchList.scrollTop=0;if(page===undefined){activeResult=0;passagesOpen=false}const kept=results.find(c=>c.path===focus?.path);if(kept)void focusResult(kept,true);else if(results.length)void focusResult(results[0]);else{focus=null;previewRequest++}if(page===undefined)void summarizeSearch(question,output.results);}
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
// Search results prioritize source reading; the summary is an explicit secondary view.
 let activeResult=0,resultsFocused=false,passagesOpen=false,passageReturn:HTMLElement|null=null;
 let searchFocusMode=false,summarySelected=false,lastSearchFocus:Candidate|null=null;
 let previewFindOpen=false,previewFindDraft='',previewFindTerm='',previewFindInput:HTMLInputElement;
 const searchSession=crypto.randomUUID();
 let searchGeneration=0,restorePreview=false;
 const rememberedCandidates=new Map<string,Candidate>();
 $:focusedGroup=focus?rankedGroups.find(group=>group.first.path===focus?.path):null;
 $:focusedPassages=focusedGroup?.matches??(focus?[focus]:[]);
 $:passageIndex=Math.max(0,focusedPassages.findIndex(c=>c.id===focus?.id));
 $:readerTerms=previewFindOpen&&previewFindTerm?[previewFindTerm]:searchTerms;
 $:readerMemoryKey=previewDocument?JSON.stringify([searchSession,searchGeneration,previewDocument.path,previewDocument.hash,previewFindTerm]):'';
 $:if(view!=='search'){passagesOpen=false;searchFocusMode=false;}

 let searchListWidth=Math.min(520,Math.max(260,Number(localStorage.getItem('sag.search.listWidth'))||340));
 $:showSummary=settings?.search_summary_enabled!==false&&(summaryBusy||!!summaryText||!!summaryError);
 $:if(summaryText&&!answerRenderer)void loadAnswerRenderer();
 $:renderedSummary=summaryText&&answerRenderer?answerRenderer(summaryText,summaryEvidence):null;
 $:summaryCitations=citedNumbers(renderedSummary?.sources??[]);
 function closePassages(restore=true){
  passagesOpen=false;
  if(restore)tick().then(()=>{if(passageReturn?.isConnected)passageReturn.focus({preventScroll:true});else document.querySelector<HTMLButtonElement>('.search-passage-trigger')?.focus()});
 }
 function togglePassages(path:string){
  if(passagesOpen&&focus?.path===path){closePassages();return;}
  passageReturn=document.activeElement instanceof HTMLElement?document.activeElement:null;
  const candidate=rankedGroups.find(group=>group.first.path===path)?.first??(focus?.path===path?focus:null);if(!candidate)return;
  if(focus?.path!==path)void focusResult(candidate,true);
  passagesOpen=true;
 }
 function selectPassage(candidate:Candidate){closePassages();void focusResult(candidate,false);}
 function stepPassage(step:number){const next=focusedPassages[passageIndex+step];if(next){closePassages(false);void focusResult(next,false);}}
 function leaveSearchReader(){closePassages(false);focus=null;summarySelected=false;previewRequest++;searchFocusMode=false;tick().then(()=>searchList?.focus({preventScroll:true}));}
 function toggleSearchSummary(){
  closePassages(false);searchFocusMode=false;
  if(summarySelected){summarySelected=false;if(lastSearchFocus)void focusResult(lastSearchFocus,true);return;}
  if(!showSummary)return;
  lastSearchFocus=focus;focus=null;previewRequest++;summarySelected=true;previewFindOpen=false;
 }
 async function openPreviewFind(){previewFindOpen=true;await tick();previewFindInput?.focus();}
 function applyPreviewFind(){previewFindTerm=previewFindDraft.trim();restorePreview=false;}
 function closePreviewFind(){previewFindOpen=false;previewFindDraft='';previewFindTerm='';restorePreview=true;tick().then(()=>document.querySelector<HTMLButtonElement>('.search-find-trigger')?.focus());}
 function searchOutside(event:MouseEvent){if(passagesOpen&&event.target instanceof Element&&!event.target.closest('.search-passage-picker,.search-passage-trigger,.search-result-more'))closePassages(false);}

 function showResult(index:number){
  if(!groups.length)return;
  activeResult=(index+groups.length)%groups.length;
  closePassages(false);void focusResult(groups[activeResult].first,true);
  tick().then(()=>document.getElementById(`search-result-${activeResult}`)?.scrollIntoView({block:'nearest'}));
 }
 function resultKeys(e:KeyboardEvent){
  const button=e.target instanceof Element?e.target.closest('button'):null;
  if(button&&(e.key==='Enter'||e.key===' ')&&!e.metaKey&&!e.ctrlKey)return;
  const group=groups[activeResult];if(!group)return;
  if(button&&['ArrowDown','ArrowUp','Home','End','ArrowRight','ArrowLeft'].includes(e.key))searchList.focus({preventScroll:true});
  if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();showResult(activeResult+(e.key==='ArrowDown'?1:-1))}
  else if(e.key==='Home'||e.key==='End'){e.preventDefault();showResult(e.key==='Home'?0:groups.length-1)}
  else if(e.key==='ArrowRight'){e.preventDefault();togglePassages(group.first.path)}
  else if(e.key==='ArrowLeft'&&passagesOpen){e.preventDefault();closePassages()}
  else if(e.key==='Enter'&&(e.metaKey||e.ctrlKey)){e.preventDefault();void edit(group.first.path,searchTerms)}
  else if(e.key==='Enter'){e.preventDefault();showResult(activeResult)}
 }
 function setListWidth(width:number){searchListWidth=Math.round(Math.min(520,Math.max(260,width)));try{localStorage.setItem('sag.search.listWidth',String(searchListWidth))}catch{}}
 function startResize(e:PointerEvent){
  const handle=e.currentTarget as HTMLElement,start=e.clientX,width=searchListWidth;
  handle.setPointerCapture(e.pointerId);
  const move=(event:PointerEvent)=>setListWidth(width+event.clientX-start);
  const end=()=>{handle.removeEventListener('pointermove',move);handle.removeEventListener('pointerup',end);handle.removeEventListener('pointercancel',end)};
  handle.addEventListener('pointermove',move);handle.addEventListener('pointerup',end);handle.addEventListener('pointercancel',end);
 }
 function resizeKey(e:KeyboardEvent){if(e.key==='ArrowLeft'||e.key==='ArrowRight'){e.preventDefault();setListWidth(searchListWidth+(e.key==='ArrowRight'?16:-16))}}
 async function focusResult(candidate:Candidate,restore=false){
   if(focus)rememberedCandidates.set(focus.path,focus);
   const remembered=rememberedCandidates.get(candidate.path);
   const c=restore&&remembered?remembered:candidate;
   if(focus?.id===c.id&&previewDocument&&!previewLoading&&restore){summarySelected=false;return;}
   restorePreview=restore;summarySelected=false;previewFindOpen=false;previewFindDraft='';previewFindTerm='';
  const index=groups.findIndex(group=>group.first.path===c.path);if(index>=0)activeResult=index;
  focus=c;rememberedCandidates.set(c.path,c);previewDocument=null;preview='';previewError='';previewLoading=true;const request=++previewRequest;
  try{
   const doc=await api<Document>('read_document',{path:c.path});
   if(request!==previewRequest||focus?.id!==c.id)return;
   if(isMarkdown(c.path)&&!legacyPreview()){
    const [{markdownModel,candidateRanges},{default:Reader}]=await Promise.all([import('./lib/document-model'),import('./components/MarkdownReader.svelte')]);
    if(request!==previewRequest||focus?.id!==c.id)return;
    const model=markdownModel(doc);
    previewReader={doc,Reader,model,target:candidateRanges(model,c)[0].from};previewDocument=doc;preview='';
    if(doc.hash!==c.hash)previewError=$t('原文件已更新，以下显示当前内容。');
    return;
   }
   const initial=await documentMatchIndex(doc.body,c.path,searchTerms,c);
   const md=isMarkdown(c.path)?await import('./lib/markdown'):null;
   if(request!==previewRequest||focus?.id!==c.id)return;
   previewDocument=doc;previewInitialMatch=initial;preview=md?md.renderMarkdown(doc.body):isHtml(c.path)?'':readonlyHtml(c.path,doc.body);
   if(doc.hash!==c.hash)previewError=$t('原文件已更新，以下显示当前内容。');
  }catch(e){if(request===previewRequest)previewError=$t(String(e))}
  finally{if(request===previewRequest)previewLoading=false}
 }
 async function chooseHistory(c:Conversation){if(conversation?.id===c.id||askBusy||cancellingAsk||!await protect())return;editingMessage=null;await stop('ask');chatScrollKey++;view='ask';draft=null;conversation=c;pending=null;references=[];error='';}
 function setReasoning(value:ReasoningEffort|null){if(!settings)return;const model=settings.answer.model;const profile=settings.model_profiles?.[model]??{system_prompt:'',sag2_enabled:true,mcp_servers:[],mcp_tools:{}};settings={...settings,model_profiles:{...settings.model_profiles,[model]:{...profile,reasoning_effort:value}}};}
 function openAgentSetup(){settingsPage='cli';settingsOpen=true;}
 async function saveReasoning(){if(!settings)return;try{await api('save_settings',{settings,answerKey:'',jevKey:'',mcpCredentials:{}})}catch(e){notify($t('思考强度保存失败：{value0}',{value0:$t(String(e))}))}}
 async function saveSettings(value:Settings,a:string,j:string,credentials:Record<string,McpCredentials>={}){await api('save_settings',{settings:value,answerKey:a,jevKey:j,mcpCredentials:credentials});settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.search_summary_enabled===false)await stopSummary();cache={};await refresh()}
 async function keys(e:KeyboardEvent){if(onboardingOpen)return;if(e.key==='Escape'){if(view==='search'&&passagesOpen){e.preventDefault();closePassages();return;}if(view==='search'&&previewFindOpen){e.preventDefault();closePreviewFind();return;}if(view==='search'&&searchFocusMode){e.preventDefault();searchFocusMode=false;return;}if(leave||renameId||settingsOpen||historySearchOpen||folderPickerOpen||chooseSaveFolder||menu||source||scopeOpen||toolApproval)return;if(mention){mention=false;return;}if(view==='search'&&focus&&showSummary&&!(e.target instanceof Element&&e.target.closest('input,textarea,[contenteditable=true],[role=listbox]:not(.search-results)'))){toggleSearchSummary()}return;}if(view==='search'&&focus&&(e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='f'){e.preventDefault();await openPreviewFind();return;}if(view==='search'&&focus&&(e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='g'){e.preventDefault();previewFindOpen=true;await tick();document.querySelector<HTMLButtonElement>(`.search-preview ${e.shiftKey?'.match-previous':'.match-next'}`)?.click();return}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='s'&&view==='editor'){e.preventDefault();await save()}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='k'){e.preventDefault();await navigate('search')}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='n'){e.preventDefault();await newChat()}}
 onMount(()=>{
  const openLink=(event:Event)=>{void api('open_external_link',{url:(event as CustomEvent<string>).detail}).catch(e=>notify($t(String(e))));};
  window.addEventListener('open-external-link',openLink);
  return()=>window.removeEventListener('open-external-link',openLink);
 });
 onMount(()=>{const cleanup:(()=>void)[]=[];let active=true;const addCleanup=(stop:()=>void)=>{if(active)cleanup.push(stop);else stop();};void(async()=>{try{addCleanup(await listen('quit-requested',requestQuit));addCleanup(await listen<string>('app-error',e=>{if(loading)startupError=e.payload;else notify(e.payload)}));addCleanup(await listen<{path:string;count:number;total:number;stage?:string}>('index-progress',e=>{if(e.payload.count===0)indexStatusHidden=false;indexProgress=e.payload;roots=roots.map(r=>r.path===e.payload.path?{...r,status:'indexing'}:r)}));addCleanup(await subscribeDataChanges(()=>{void refresh().catch(e=>notify($t(String(e))))}));addCleanup(await listen('history-changed',()=>history().catch(e=>notify($t(String(e))))));addCleanup(await listen('settings-changed',async()=>{try{settings=await api<Settings>('get_settings');setLanguage(settings.language??'zh-CN');if(settings.search_summary_enabled===false)await stopSummary()}catch(e){notify($t(String(e)))}}));addCleanup(await listen<ToolApproval>('tool-approval',e=>{if(e.payload.query_id===askJob&&e.payload.query_id!==cancellingAsk)toolApproval=e.payload}));addCleanup(await listen<Progress>('query-progress',e=>{if(e.payload.id===summaryJob){if(e.payload.delta!==null)summaryText=e.payload.replace?e.payload.delta:summaryText+e.payload.delta;return;}if(e.payload.id===searchJob){searchStage=e.payload.stage;return;}if(e.payload.id!==askJob||e.payload.id===cancellingAsk)return;askStage=e.payload.stage;if(pending&&e.payload.tool){const {call,finished}=e.payload.tool;activeToolId=finished?'':call.id;pending={...pending,tool_calls:[...(pending.tool_calls||[]).filter(t=>t.id!==call.id),call]};}if(pending&&e.payload.reasoning_delta)pending={...pending,reasoning:(pending.reasoning||'')+e.payload.reasoning_delta};if(pending&&e.payload.reasoning_block){const block=e.payload.reasoning_block;const blocks=[...(pending.reasoning_blocks||[])];const last=blocks.at(-1);if(last&&last.answer_offset===block.answer_offset&&last.tool_count===block.tool_count)blocks[blocks.length-1]={...last,content:last.content+block.content};else blocks.push(block);pending={...pending,reasoning_blocks:blocks};}if(pending&&e.payload.delta!==null)pending={...pending,answer:e.payload.replace?e.payload.delta:pending.answer+e.payload.delta,reasoning_blocks:e.payload.replace?pending.reasoning_blocks?.map(block=>({...block,answer_offset:Math.min(block.answer_offset,e.payload.delta!.length)})):pending.reasoning_blocks};}));await loadWorkspace();const saved=await api<Draft[]>('list_drafts');if(saved.length){notify($t("有 {value0} 份未保存草稿",{value0:saved.length}),async()=>{draft=saved[0];dirty=true;selected=draftFolder(draft);EditorComponent=(await import('./components/RichEditor.svelte')).default;view='editor';closeNotice()})}onboardingOpen=localStorage.getItem('sag.onboarding.seen')!=='1';}catch(e){startupError=String(e)}finally{if(active)loading=false}})();return()=>{active=false;appDisposed=true;refreshGeneration++;clearTimeout(draftTimer);clearTimeout(noticeTimer);cleanup.forEach(fn=>fn())}});
</script>
<svelte:window on:keydown={keys} on:click={searchOutside} on:contextmenu|preventDefault/>
<div class="desktop-window real-client" class:windows-client={!mac} class:compact-sidebar={collapsed} class:search-workbench={view==='search'} data-sidebar-motion={sidebarMotion}>
 {#if !mac}<DesktopTitlebar/>{/if}
 <div class="shell">
 <aside class="sidebar"><div class="sidebar-titlebar" data-tauri-drag-region><Button.Root class="icon-button" aria-label={collapsed?$t("展开侧边栏"):$t("收起侧边栏")} title={collapsed?$t("展开侧边栏"):$t("收起侧边栏")} aria-expanded={!collapsed} onclick={toggleSidebar}>{@html icon('panel')}</Button.Root></div><div class="brand-row">{@html logo}<span class="brand">{'SAG Wiki'}</span></div>
 <Button.Root class="sidebar-new-chat chat-new" title={$t("新建问答（⌘ / Ctrl + N）")} aria-label={$t("新建问答")} disabled={askBusy||!!cancellingAsk} onclick={newChat}>{@html icon('edit')}<span>{$t("新建问答")}</span><kbd>{mac?'⌘N':'Ctrl N'}</kbd></Button.Root><div class="sidebar-tabs" role="tablist" aria-label={$t("侧边栏内容")}><Button.Root role="tab" id="sidebar-tab-conversations" aria-controls="sidebar-pane-conversations" aria-selected={sidebarTab==='conversations'} class={`sidebar-tab ${sidebarTab==='conversations'?'active':''}`} title={$t("会话")} onclick={(event:MouseEvent)=>{if(collapsed)void toggleSidebar(event);showSidebarTab('conversations')}}>{@html icon('chat')}<span>{$t("会话")}</span></Button.Root><Button.Root role="tab" id="sidebar-tab-folders" aria-controls="sidebar-pane-folders" aria-selected={sidebarTab==='folders'} class={`sidebar-tab ${sidebarTab==='folders'?'active':''}`} title={$t("文件夹")} onclick={(event:MouseEvent)=>{if(collapsed)void toggleSidebar(event);showSidebarTab('folders')}}>{@html icon('folder')}<span>{$t("文件夹")}</span></Button.Root></div>
 <div class="sidebar-pane" id="sidebar-pane-conversations" role="tabpanel" aria-labelledby="sidebar-tab-conversations" hidden={sidebarTab!=='conversations'}><ConversationSidebar {conversations} activeId={conversation?.id??''} generating={askBusy||!!cancellingAsk} onSelect={chooseHistory} onMenu={chatMenu} onStop={()=>stop('ask')}/></div>
 <div class="sidebar-pane" id="sidebar-pane-folders" role="tabpanel" aria-labelledby="sidebar-tab-folders" hidden={sidebarTab!=='folders'}><div class="tree-heading"><Button.Root class="folder-management-link" onclick={()=>libraryBrowse('').catch(e=>notify($t(String(e))))}>{$t("文件夹管理")}</Button.Root><Button.Root class="icon-button" title={$t("添加文件夹")} aria-label={$t("添加文件夹")} onclick={add}>{@html icon('plus')}</Button.Root></div><div class="tree" use:rememberScroll={{key:'sag.sidebar.library.scroll',enabled:!loading&&!collapsed}}><Tree nodes={roots} {expanded} cache={indexedCache} selected={view==='files'?selected:view==='editor'&&draft?indexedSourcePath(draft.path):view==='document'&&readOnlyDoc?indexedSourcePath(readOnlyDoc.path):''} onbrowse={(path)=>browse(path).catch(e=>notify($t(String(e))))} ontoggle={(path)=>toggle(path).catch(e=>notify($t(String(e))))} onmenu={showMenu}/>{#if !loading&&!roots.length}<div class="folder-empty-state"><p>{$t("添加文件夹后，即可对其中的文件提问和搜索。")}</p><Button.Root class="outline" onclick={add}>{@html icon('plus')}<span>{$t("添加文件夹")}</span></Button.Root></div>{/if}</div></div>
 <div class="index-status">{#if !indexStatusHidden&&(indexing||indexPaused||(indexFailed&&indexFailureSignature!==dismissedIndexFailures))}<div class="index-status-row"><Button.Root class="index-status-link" aria-label={$t("查看索引详情")} onclick={()=>indexDetailsOpen=true}><span class="index-status-primary">{indexStatusText}</span>{#if indexing&&indexPercent!==null}<span class="index-status-bar" aria-hidden="true"><span style:width={`${indexPercent}%`}></span></span>{/if}<span class="index-status-meta"><span>{indexStatusCount}</span>{#if indexFailed}<span class="index-status-failure">{indexFailureText}</span>{/if}</span></Button.Root><Button.Root class="index-status-dismiss" aria-label={$t("隐藏索引状态")} onclick={()=>{if(indexFailed){localStorage.setItem('sag.index.dismissed-failures',indexFailureSignature);dismissedIndexFailures=indexFailureSignature;}indexStatusHidden=true}}>{@html icon('close')}</Button.Root></div>{/if}</div><div class="sidebar-bottom"><div class="sidebar-tools"><Button.Root class="sidebar-settings" aria-label={$t("设置")} title={$t("设置")} onclick={()=>{settingsPage='general';settingsOpen=true}}>{@html icon('settings')}<span>{$t("设置")}</span></Button.Root><DoclingDownload/><Button.Root class="sidebar-help" aria-label={$t("打开帮助文档")} title={$t("帮助文档")} onclick={()=>{void api('open_external_link',{url:'http://sag-wiki.zleap.com'}).catch(e=>notify($t(String(e))))}}>{@html icon('help')}</Button.Root><Button.Root aria-label={$t("切换明暗主题")} onclick={()=>{if(settings)saveSettings({...settings,theme:settings.theme==='dark'?'light':'dark'},'','').then(()=>notify($t('设置已保存'))).catch(e=>notify($t(String(e))))}}>{@html icon(settings?.theme==='dark'?'sun':'moon')}</Button.Root></div></div></aside>
 <main class="workspace"><div class="workspace-header" data-tauri-drag-region>{#if view==='search'&&searched}<Button.Root class="icon-button workspace-back" title={$t("返回搜索")} aria-label={$t("返回搜索")} onclick={()=>resetSearch().catch(e=>notify($t(String(e))))}>{@html icon('back')}</Button.Root>{/if}<nav class="workspace-switch" aria-label={$t("问答、搜索与文件")}><Button.Root class={`nav-item ${view==='ask'?'active':''}`} aria-current={view==='ask'?'page':undefined} title={$t("问答")} onclick={()=>navigate('ask')}>{@html icon('chat')}<span>{$t("问答")}</span></Button.Root><Button.Root class={`nav-item ${view==='search'?'active':''}`} aria-current={view==='search'?'page':undefined} title={$t("搜索（⌘ / Ctrl + K）")} onclick={()=>navigate('search')}>{@html icon('search')}<span>{$t("搜索")}</span></Button.Root><Button.Root class={`nav-item ${fileView?'active':''}`} aria-current={fileView?'page':undefined} title={$t("文件")} onclick={()=>navigate('files')}>{@html icon('folder')}<span>{$t("文件")}</span></Button.Root></nav><div class="workspace-actions"><Button.Root type="button" class="outline web-clipper-entry" title={$t("网页剪藏")} aria-label={$t("网页剪藏")} onclick={()=>{settingsPage='clipper';settingsOpen=true}}>{@html icon('globe')}</Button.Root><Button.Root type="button" class="outline agent-connect-entry" title={$t("接入 Agent")} aria-label={$t("接入 Agent")} onclick={openAgentSetup}>{@html icon('plug')}</Button.Root></div></div>
 <div class="content" bind:this={workspaceContent}>
 {#if loading}<div class="empty">{$t("正在打开本地知识库…")}</div>
 {:else if startupError}<div class="empty startup-error"><p>{$t("无法连接本地 SAG 服务")}</p><p class="startup-error-detail">{$t(startupError)}</p><p class="startup-error-hint">{$t("如果旧版本的 SAG 服务仍在运行，请先在终端执行 sag service stop，然后重试。")}</p><Button.Root class="primary" disabled={retrying} onclick={retryStartup}>{$t("重试")}</Button.Root></div>
 {:else if view==='ask'}<div class="ask-layout"><section class="ask-content" class:ask-home={!shownMessages.length&&!pending} class:has-history={conversations.length>0} use:rememberScroll={{key:'sag.history.scroll',enabled:!loading&&!shownMessages.length&&!pending}}>{#if !shownMessages.length&&!pending}<div class="welcome"><div class="welcome-mark" aria-hidden="true">{@html icon('chat')}</div><h1>{roots.length?$t("向你的文件提问"):$t("你的文件，直接成为知识库。")}</h1><p>{roots.length?$t("跨文件查找信息，随时查看原文。"):$t("选一个文件夹，文档和代码自动加入。")}<br>{roots.length?'':$t("无需逐篇上传，无需模型预处理。")}</p>{#if settings&&!answerReady}<p class="setup-hint" role="status">{$t("问答需要先配置模型。")}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="models";settingsOpen=true}}>{$t("前往设置")}</Button.Root></p>{:else if settings&&!jevReady}<p class="setup-hint" role="status">{$t("未配置 Jev 时只能使用本地快速检索；深入检索与多跳问答需要先配置。")}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="models";settingsOpen=true}}>{$t("前往设置")}</Button.Root></p>{/if}</div>{:else}<div class="chat-log" bind:this={chatLog} use:followChat={[chatScrollKey,pending,renderedAnswers]}><div class="chat-messages">{#each displayedMessages as m,index}{@const rendered=renderedAnswers[answerKey(m)]}{@const events=answerEvents(m.tool_calls,m.reasoning_blocks)}<div class="user-turn">
 {#if editingMessage===index}<form class="message-editor" on:submit|preventDefault={()=>regenerate(index,editingText.trim())}>
 <Textarea aria-label={$t("编辑问题")} bind:ref={messageInput} bind:value={editingText} oncompositionstart={compositionStart} oncompositionend={compositionEnd} onkeyup={compositionKeyUp} onkeydown={(event:KeyboardEvent)=>{if(imeKey(event))return;if(event.key==='Escape'){editingMessage=null;event.preventDefault()}else if(event.key==='Enter'&&(event.metaKey||event.ctrlKey)){event.preventDefault();if(editingText.trim())void regenerate(index,editingText.trim())}}}/>
 {#if index<displayedMessages.length-1}<p class="message-edit-note">{$t("重新发送将替换此轮及后续 {value0} 轮问答，可在完成后撤销。",{value0:displayedMessages.length-index-1})}</p>{/if}
 <div class="message-editor-actions"><Button.Root type="button" class="outline" onclick={()=>editingMessage=null}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary" disabled={!editingText.trim()}>{$t("保存并发送")}</Button.Root></div>
 </form>{:else}<div class="user-message">{m.question}</div><div class="message-actions user-actions" aria-label={$t("问题操作")}>
 <Button.Root class="icon-button" title={$t("复制问题")} aria-label={$t("复制问题")} onclick={()=>copyMessage(m.question)}>{@html icon('copy')}</Button.Root>
 <Button.Root class="icon-button" title={$t("编辑问题")} aria-label={$t("编辑问题")} disabled={busy} onclick={()=>beginEdit(index)}>{@html icon('edit')}</Button.Root>
 <Button.Root class="icon-button" title={$t("删除此轮问答")} aria-label={$t("删除此轮问答")} disabled={busy||!conversation} onclick={()=>deleteMessage(index)}>{@html icon('delete')}</Button.Root>
 </div>{/if}</div><article class="answer"><AnswerProcess hasContent={!!(events.length||m.reasoning)} answering={!!m.answer.slice(events.at(-1)?.offset??0).trim()} running={m===pending&&askBusy} startedAt={m.created_at} elapsedMs={m.context?.elapsed_ms}>{#if m.reasoning&&!m.reasoning_blocks?.length}<Reasoning content={m.reasoning} running={m===pending&&busy&&stage==='思考中'&&!m.answer}/>{/if}{#each events as event,i(`${event.kind}:${event.index}`)}{@const html=rendered?.events[i]?.html??escaped(m.answer.slice(event.from,event.offset)).replace(/\n/g,'<br>')}{#if html}<div class="answer-text" role="presentation" on:click={(e)=>{const target=(e.target as HTMLElement).closest<HTMLElement>('[data-citation]');if(target)source=m.evidence[Number(target.dataset.citation)]}} use:answerHtml={html}></div>{/if}{#if event.kind==='reasoning'}{@const block=m.reasoning_blocks![event.index]}<Reasoning content={block.content} running={event.index===m.reasoning_blocks!.length-1&&m===pending&&busy&&stage==='思考中'&&block.answer_offset===m.answer.length&&block.tool_count===(m.tool_calls?.length??0)}/>{:else if !joinsToolRun(m,events,i)}{@const run=toolRun(m,events,i)}<ToolGroup calls={run.map(j=>m.tool_calls![events[j].index])} states={run.map(j=>toolState(m,m.tool_calls![events[j].index],pending,activeToolId,busy))}>{#each run as j(j)}{@const call=m.tool_calls![events[j].index]}<ToolCall {call} state={toolState(m,call,pending,activeToolId,busy)} stage={m===pending&&activeToolId===call.id?searchStageText:''} scope={m.scope.length===0?$t("全部文件"):m.scope.map(basename).join('、')}/>{/each}</ToolGroup>{/if}{/each}{#if m===pending&&busy&&!activeToolId&&!(stage==='思考中'&&(m.reasoning_blocks?.length?m.reasoning_blocks.at(-1)!.answer_offset===m.answer.length&&m.reasoning_blocks.at(-1)!.tool_count===(m.tool_calls?.length??0):m.reasoning))}<div class="answer-progress" role="status"><span class="progress-indicator" aria-hidden="true"></span><span>{$t(stage)}</span></div>{/if}</AnswerProcess><div class="answer-text" role="presentation" on:click={(e)=>{const target=(e.target as HTMLElement).closest<HTMLElement>('[data-citation]');if(target)source=m.evidence[Number(target.dataset.citation)]}} use:answerHtml={rendered?.html??(answerRenderer?'':escaped(m.answer.slice(events.at(-1)?.offset??0)).replace(/\n/g,'<br>'))}></div>{#if m.error}<p class="form-error">{$t(m.error)}{#if m.error==="请在设置中填写问答模型 API 密钥"||m.error.includes("Jev")}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="models";settingsOpen=true}}>{$t("前往设置")}</Button.Root>{/if}</p>{/if}{#if rendered?.sources.length}<div class="source-list">{#each rendered.sources as group}<Button.Root class="source-chip" onclick={()=>source=group.first}><small>{group.number}</small>{@html icon('file')}<span>{group.first.title}</span></Button.Root>{/each}</div>{/if}{#if m.context||m.created_at!=null}<div class="answer-context-reveal"><p class="answer-context">{#if m.context}{$t("{value0} 输入 tokens{value1} · ",{value0:compactNumber(m.context.input_tokens??m.context.estimated_input),value1:m.context.input_tokens===null?$t("（估算）"):''})}{#if m.context.output_tokens===null}{$t("输出 tokens 未返回")}{:else}{$t("{value0} 输出 tokens",{value0:compactNumber(m.context.output_tokens)})}{/if}{#if m.context.elapsed_ms!=null}{$t(" · 耗时 {value0}",{value0:durationLabel(m.context.elapsed_ms)})}{/if}{#if m.context.output_tokens!==null&&m.context.generation_ms} · {compactNumber(m.context.output_tokens*1000/m.context.generation_ms)} tokens/s{/if}{#if m.context.omitted_sources}{$t(" · 已略去 {value0} 个片段以满足预算",{value0:compactNumber(m.context.omitted_sources)})}{/if}{/if}{#if m.created_at!=null}{#if m.context} · {/if}<Timestamp value={m.created_at}/>{/if}</p></div>{/if}{#if !m.tool_calls?.length&&m.retrieval_question!==m.question}<details class="trace"><summary>{$t("检索问题")}</summary>{m.retrieval_question}</details>{/if}
 {#if m.answer||!busy}<div class="message-actions answer-actions" aria-label={$t("回答操作")}>
 <Button.Root class="icon-button" title={$t("复制回答")} aria-label={$t("复制回答")} disabled={!m.answer} onclick={()=>copyMessage(m.answer)}>{@html icon('copy')}</Button.Root>
 {#if m.note_path}<Button.Root class="icon-button note-saved" title={$t("已保存为笔记，点击打开")} aria-label={$t("打开笔记")} onclick={()=>openNote(m.note_path!)}>{@html icon('file')}</Button.Root>{:else}<Button.Root class="icon-button" title={$t("保存为笔记")} aria-label={$t("保存为笔记")} disabled={busy||!conversation||m.status!=='complete'||!m.answer.trim()} onclick={()=>saveNoteFor={index}}>{@html icon('download')}</Button.Root>{/if}
 <Button.Root class="icon-button" title={$t("重新生成回答")} aria-label={$t("重新生成回答")} disabled={busy} onclick={()=>retryMessage(index)}>{@html icon('refresh')}</Button.Root>
 <Button.Root class="icon-button" title={$t("删除此轮问答")} aria-label={$t("删除此轮问答")} disabled={busy||!conversation} onclick={()=>deleteMessage(index)}>{@html icon('delete')}</Button.Root>
 
 </div>{/if}</article>{/each}</div></div>{/if}
 <div class="composer-wrap"><Popover.Root bind:open={mention}><Popover.Portal to=".real-client"><Popover.Content class="mention-picker" customAnchor={composer} collisionBoundary={workspaceContent} side={mentionSide} sideOffset={6} collisionPadding={8} align="start" trapFocus={false} onOpenAutoFocus={event=>event.preventDefault()} onCloseAutoFocus={event=>event.preventDefault()}><div class="mention-heading">{$t("@ 引用文件")}</div>{#if !files.length}<div class="mention-empty" aria-live="polite">{@html icon('folder')}<strong>{$t('知识库还没有可引用的文件')}</strong><p>{$t('添加文件夹并完成索引后，就可以用 @ 选择要引用的文件。')}</p><Button.Root type="button" class="outline mention-add-folder" onclick={()=>{mention=false;add()}}>{@html icon('plus')}{$t('添加文件夹')}</Button.Root></div>{:else if !mentions.length}<div class="mention-empty" aria-live="polite">{@html icon('search')}<strong>{$t('没有匹配的文件')}</strong><p>{$t('试试其他文件名或路径关键词。')}</p></div>{:else}<div class="mention-options" role="listbox" aria-label={$t("引用文件")}>{#each mentions as f,i}<Button.Root role="option" aria-selected={i===mentionIndex} onmousedown={(event:MouseEvent)=>{event.preventDefault();(()=>reference(f.path))();}}>{@html icon('file')}<span><strong>{f.name}</strong><small>{displayPath(f.path)}</small></span></Button.Root>{/each}</div><div class="mention-help">{$t("↑ ↓ 选择 · Enter 引用 · Esc 关闭")}</div>{/if}</Popover.Content></Popover.Portal></Popover.Root><form class="composer" class:busy bind:this={composer} use:mentionPlacement on:submit|preventDefault={()=>run('ask')}><div class="reference-chips">{#each references as path}<span class="reference-chip">{@html icon('file')}<span>{basename(path)}</span><Button.Root type="button" aria-label={$t("移除引用 {value0}",{value0:basename(path)})} onclick={()=>references=references.filter(p=>p!==path)}>{@html icon('close')}</Button.Root></span>{/each}</div><Textarea aria-label={$t("向你的文件提问")} bind:ref={askInput} bind:value={ask} oninput={mentionUpdate} onkeydown={askKey} onkeyup={compositionKeyUp} oncompositionstart={compositionStart} oncompositionend={compositionEnd} placeholder={busy?$t("回答生成中，可以先输入下一个问题…"):$t("向你的文件提问，输入 @ 引用文件…")}/><div class="composer-bottom"><div class="composer-options"><Button.Root class="scope-button" type="button" aria-haspopup="dialog" aria-expanded={scopeOpen} onclick={scopeShow}>{@html icon(scope.length?'folder':'globe')}<span>{scopeLabel}</span>{@html icon('down')}</Button.Root></div><div class="composer-model-controls">{#if settings}<ContextUsage context={busy?pending?.context:conversation?.messages.at(-1)?.context} limit={settings.answer_context_tokens??65536} loading={busy&&!pending?.context}/><ReasoningControl model={settings.answer.model} models={settings.answer.models} onModelChange={model=>{if(settings)settings={...settings,answer:{...settings.answer,model}}}} endpoint={settings.answer.url} value={settings.model_profiles?.[settings.answer.model]?.reasoning_effort} disabled={busy} onChange={setReasoning} onCommit={saveReasoning}/>{/if}{#if busy}<Button.Root type="button" class="send" aria-label={$t("停止生成")} onclick={()=>stop()}>{@html icon('stop')}</Button.Root>{:else}<Button.Root class="send" aria-label={$t("发送问题")} disabled={!ask.trim()}>{@html icon('arrow')}</Button.Root>{/if}</div></div></form></div>{#if !shownMessages.length&&!pending&&conversations.length&&(collapsed||sidebarTab!=='conversations')}<section class="recent-conversations" aria-label={$t("会话列表")}><div class="recent-heading"><span>{$t("会话列表")}</span><Button.Root class="icon-button" title={$t("搜索会话")} aria-label={$t("搜索会话")} onclick={()=>historySearchOpen=true}>{@html icon('search')}</Button.Root></div><div class="recent-list">{#each conversations as c(c.id)}<div class="history-row" class:active={view==='ask'&&conversation?.id===c.id}><Button.Root class="conversation-item" title={c.title} onclick={()=>chooseHistory(c)}>{@html icon('chat')}<span>{c.title}</span></Button.Root><Timestamp value={c.updated}/><Button.Root class="history-more" aria-label={$t("{value0} 的选项",{value0:c.title})} onclick={(e:MouseEvent)=>chatMenu(e,c)}>{@html icon('more')}</Button.Root></div>{:else}<p class="history-empty">{$t("还没有问答记录")}</p>{/each}</div></section>{/if}</section></div>
 {:else if view==='search'}<section class="search-page" class:has-results={searched} aria-busy={busy}>{#if !searched}<div class="welcome search-welcome"><div class="welcome-mark" aria-hidden="true">{@html icon('search')}</div><h1>{$t("搜索你的文件")}</h1><p>{$t("查找文件内容，随时查看原文。")}</p></div>{/if}<div class="search-query-row"><form class="search-composer" class:compact={searched} on:submit|preventDefault={()=>run(searchMode)}><div class="search-entry"><Input aria-label={$t("搜索关键词")} bind:ref={searchInput} bind:value={search} onkeydown={searchKey} onkeyup={compositionKeyUp} oncompositionstart={compositionStart} oncompositionend={compositionEnd} placeholder={$t("输入关键词或问题，搜索你的文件…")}/></div><div class="search-composer-bottom"><Button.Root class="scope-button" type="button" aria-haspopup="dialog" aria-expanded={scopeOpen} onclick={scopeShow}>{@html icon(scope.length?'folder':'globe')}<span>{scopeLabel}</span>{@html icon('down')}</Button.Root><div class="search-composer-actions"><Select class="search-mode" label={$t("搜索模式")} value={searchMode} onValueChange={changeSearchMode} disabled={busy} items={[{value:"deep_search",label:jevReady?$t("深入"):$t("深入（需配置 Jev）")},{value:"search",label:$t("快速")}]}/>{#if busy}<Button.Root type="button" class="send" aria-label={$t("停止搜索")} onclick={()=>stop()}>{@html icon('stop')}</Button.Root>{:else}<Button.Root class="send" aria-label={$t("搜索文件")} disabled={!search.trim()}>{@html icon('arrow')}</Button.Root>{/if}</div></div></form></div>
 {#if !searched&&searchHistory.length}<section class="search-history" aria-label={$t("最近搜索")}><div class="search-history-heading"><span>{$t("最近搜索")}</span><Button.Root class="icon-button" aria-label={$t("清空搜索历史")} title={$t("清空搜索历史")} onclick={clearSearchHistory}>{@html icon('delete')}</Button.Root></div><div class="search-history-items">{#each searchHistory as question}<Button.Root class="search-history-item" title={question} onclick={()=>{search=question;void run(searchMode)}}>{@html icon('search')}<span>{question}</span></Button.Root>{/each}</div></section>{/if}{#if searched}
 <div class="search-options-row"><div class="search-summary"><span aria-live="polite">{searchStopped?$t("已停止搜索 · "):''}{busy&&!paging?$t(searchJobKind==='deep_search'?"深入搜索中":"快速搜索中"):`${searchRanked?$t("深入搜索"):$t("快速搜索")} · ${$t("{value0} 个文件",{value0:searchPagination?.total??groups.length})}`}{#if searchElapsed!==null&&!(busy&&!paging)}{' · '}{searchElapsed<1000?`${searchElapsed} ms`:$t("{value0} 秒",{value0:(searchElapsed/1000).toFixed(1)})}{/if}</span></div><div class="search-options-actions">{#if showSummary}<Button.Root class="search-summary-toggle" aria-label={$t('搜索总结')} aria-pressed={summarySelected} onclick={toggleSearchSummary}>{@html icon('chat')}<span>{$t('搜索总结')}</span>{#if summaryBusy}<span class="summary-running" aria-hidden="true"></span>{/if}</Button.Root>{/if}<Button.Root class="search-new-query" onclick={()=>resetSearch().catch(e=>notify($t(String(e))))}>{$t('重新搜索')}</Button.Root></div></div>
 {#if searchError}<p class="form-error" role="alert">{$t(searchError)}{#if searchError===$t("请在设置中填写 Jev API 密钥")}<Button.Root class="settings-error-link" onclick={()=>{settingsPage="jev";settingsOpen=true}}>{$t("前往设置")}</Button.Root>{/if}</p>{/if}{#if busy&&!paging}
 <div class="search-loading" role="status" aria-live="polite" aria-atomic="true"><div class="search-skeleton" aria-hidden="true">{#each [0,1,2,3] as row}<span style={`--row:${row}`}></span>{/each}</div><strong>{searchStageText}</strong></div>
 {:else if groups.length}
 <div class="search-layout" class:is-paging={paging} class:reading={!!focus||summarySelected} class:focus-mode={searchFocusMode} inert={paging} style={`--search-list-width:${searchListWidth}px`}>
  <section class="search-results-column" aria-label={$t("相关文件")}><header class="search-results-heading"><strong>{$t("相关文件")}</strong></header><div class="search-results" use:quietScroll bind:this={searchList} role="listbox" tabindex="0" aria-label={$t("搜索结果")} aria-activedescendant={groups[activeResult]?`search-result-${activeResult}`:undefined} on:keydown={resultKeys} on:focus={()=>resultsFocused=true} on:blur={()=>resultsFocused=false}>
   {#each groups as g,i (g.first.path)}<SearchResultItem group={g} id={`search-result-${i}`} terms={searchTerms} {roots} selected={focus?.path===g.first.path} active={activeResult===i&&resultsFocused} focused={focus} cited={summaryCitations.get(g.first.path)??[]} expanded={passagesOpen&&focus?.path===g.first.path} onSelect={c=>{activeResult=i;closePassages(false);void focusResult(c,true)}} onToggle={()=>togglePassages(g.first.path)} onFocus={()=>activeResult=i}/>{/each}
  </div></section>
  <!-- A focusable separator with a value is the ARIA splitter widget. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="search-resizer" role="separator" tabindex="0" aria-orientation="vertical" aria-label={$t("调整结果列表宽度")} aria-valuemin={260} aria-valuemax={520} aria-valuenow={searchListWidth} on:pointerdown={startResize} on:keydown={resizeKey}></div>
  <article class="search-preview" class:showing-summary={!focus} class:search-finding={previewFindOpen}>
   {#if focus}
    <div class="search-preview-head">
     <Button.Root class="icon-button search-preview-back list-only" aria-label={$t("返回结果列表")} onclick={leaveSearchReader}>{@html icon('back')}</Button.Root>
     <div class="search-preview-title"><strong title={sourceName(focus)}>{sourceName(focus)}</strong><span class="source-path" title={displayPath(indexedSourcePath(focus.path))}>{sourceFolder(focus.path,roots)}</span></div>
     <div class="search-preview-actions">
      <div class="search-passage-nav" role="group" aria-label={$t('相关片段')}>
       <Button.Root class="search-passage-trigger" aria-expanded={passagesOpen} aria-controls="search-passage-picker" title={$t('选择相关片段')} onclick={()=>togglePassages(focus!.path)}>{@html icon('bulletList')}<span>{$t('相关片段')}</span><b>{passageIndex+1} / {focusedPassages.length}</b>{@html icon('down')}</Button.Root>
       <Button.Root class="icon-button search-passage-step search-passage-previous" aria-label={$t('上一个片段')} title={$t('上一个片段')} disabled={passageIndex<=0} onclick={()=>stepPassage(-1)}>{@html icon('chevron')}</Button.Root>
       <Button.Root class="icon-button search-passage-step search-passage-next" aria-label={$t('下一个片段')} title={$t('下一个片段')} disabled={passageIndex>=focusedPassages.length-1} onclick={()=>stepPassage(1)}>{@html icon('chevron')}</Button.Root>
      </div>
      <Button.Root class="icon-button search-find-trigger" aria-label={$t('在文档中查找')} title={$t('在文档中查找')} aria-expanded={previewFindOpen} onclick={()=>previewFindOpen?closePreviewFind():openPreviewFind()}>{@html icon('search')}</Button.Root>
      <Button.Root class="icon-button search-focus-toggle" aria-label={$t(searchFocusMode?'退出专注阅读':'进入专注阅读')} title={$t(searchFocusMode?'退出专注阅读':'进入专注阅读')} aria-pressed={searchFocusMode} onclick={()=>{closePassages(false);searchFocusMode=!searchFocusMode}}>{@html icon(searchFocusMode?'panel':'maximize')}</Button.Root>
      <Popover.Root><Popover.Trigger class="icon-button" aria-label={$t('文档操作')} title={$t('文档操作')}>{@html icon('more')}</Popover.Trigger><Popover.Portal><Popover.Content class="search-document-menu" side="bottom" align="end" sideOffset={8} collisionPadding={12}><p>{displayPath(indexedSourcePath(focus.path))}</p><Button.Root onclick={()=>api('reveal_path',{path:indexedSourcePath(focus!.path)}).catch(e=>notify($t(String(e))))}>{$t('在文件夹中显示')}</Button.Root><Button.Root onclick={()=>edit(focus!.path,searchTerms)}>{$t('打开文档')}</Button.Root></Popover.Content></Popover.Portal></Popover.Root>
      <Button.Root class="search-open-document" onclick={()=>edit(focus!.path,searchTerms)}>{$t('打开文档')}</Button.Root>
     </div>
    </div>
    {#if previewFindOpen}<form class="search-find-bar" on:submit|preventDefault={applyPreviewFind}><label for="preview-find">{$t('正文查找')}</label><input id="preview-find" bind:this={previewFindInput} bind:value={previewFindDraft} placeholder={$t('输入词语，按 Enter 查找')}/><Button.Root type="submit">{$t('查找')}</Button.Root><Button.Root type="button" class="icon-button" aria-label={$t('关闭正文查找')} onclick={closePreviewFind}>{@html icon('close')}</Button.Root></form>{/if}
    {#if previewError}<p class="small muted search-preview-note" role="status">{previewError}</p>{/if}
    {#if previewLoading}<div class="search-preview-loading" role="status"><span class="search-skeleton" aria-hidden="true">{#each [0,1,2] as row}<span style={`--row:${row}`}></span>{/each}</span><span class="sr-only">{$t("正在读取文件…")}</span></div>{:else if previewDocument}
     {#key `${previewRequest}:${previewFindTerm}`}
      {#if isHtml(focus.path)}<HtmlDocument body={previewDocument.body} terms={readerTerms} initialMatch={previewFindTerm?0:previewInitialMatch} showNavigation={previewFindOpen} memoryKey={readerMemoryKey} restoreScroll={restorePreview}/>
      {:else if previewReader?.doc===previewDocument}<svelte:component this={previewReader.Reader} model={previewReader.model} terms={readerTerms} target={previewFindTerm?0:previewReader.target} path={displayPath(focus.path)} showNavigation={previewFindOpen} memoryKey={readerMemoryKey} restoreScroll={restorePreview}/>
      {:else}<HighlightedDocument html={preview} terms={readerTerms} path={displayPath(focus.path)} initialMatch={previewFindTerm?0:previewInitialMatch} showNavigation={previewFindOpen} memoryKey={readerMemoryKey} restoreScroll={restorePreview}/>{/if}
     {/key}
    {/if}
    {#if passagesOpen}{#key focus.path}<SearchPassagePicker candidates={focusedPassages} selectedId={focus.id} terms={searchTerms} onSelect={selectPassage} onClose={()=>closePassages()}/>{/key}{/if}
   {:else if summarySelected&&showSummary}
    <div class="search-summary-back list-only"><Button.Root class="icon-button" aria-label={$t('返回结果列表')} onclick={leaveSearchReader}>{@html icon('back')}</Button.Root><span>{$t('搜索总结')}</span></div>
    <div class="search-summary-scroll" use:quietScroll><SearchSummary text={summaryText} rendered={renderedSummary} running={summaryBusy} error={summaryError} evidence={summaryEvidence} onSource={candidate=>{closePassages(false);void focusResult(candidate)}} onCopy={copyMessage}/></div>
   {:else}<div class="search-preview-empty">{@html icon('file')}<p>{$t("选择左侧的结果查看原文。")}</p></div>{/if}
  </article>
 </div>
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
 {:else if view==='files'&&librarySessions[selected]}{#key selected}<LibraryWorkspace scope={displayPath(selected)} parent={libraryParent} session={librarySessions[selected]} onModeChange={()=>librarySessions={...librarySessions}} entries={directoryEntries} totalFiles={files.length} indexedFiles={files} notes={noteSummaries} {roots} onIndexDetails={()=>indexDetailsOpen=true} onPauseChange={paused=>indexPaused=paused} oncreate={()=>create()} onadd={add} reading={!!selected&&readingFolder===selected} revision={libraryRevision} onbrowse={libraryBrowse} onopen={(path,terms)=>edit(path,terms,selected)} onmenu={showMenu} onSettings={()=>{settingsPage='files';settingsOpen=true}}/>{/key}
 {:else if view==='document'&&readOnlyDoc}<section class="readonly-document"><div class="editor-head"><Button.Root class="icon-button" title={$t(documentFromConversation?"返回会话":documentFromSearch?"返回搜索":"返回文件夹")} onclick={returnToFiles}>{@html icon('back')}</Button.Root><span>{readOnlyDoc.name}</span><span class="document-status">{#if isRtf(readOnlyDoc.path)}{$t("RTF 正文")} · {/if}{$t("只读")}</span><Button.Root class="outline" onclick={()=>api('reveal_path',{path:readOnlyDoc!.path}).catch(e=>notify($t(String(e))))}>{mac?$t("在 Finder 中打开"):$t("在文件资源管理器中打开")}</Button.Root></div><div class="source-path">{displayPath(readOnlyDoc.path)}</div>{#if isHtml(readOnlyDoc.path)}<HtmlDocument body={readOnlyDoc.body} terms={documentSearchTerms} initialMatch={documentInitialMatch}/>{:else}<HighlightedDocument html={readonlyHtml(readOnlyDoc.path,readOnlyDoc.body)} terms={documentSearchTerms} initialMatch={documentInitialMatch}/>{/if}{#key readOnlyDoc.path}<DocumentQuestion name={readOnlyDoc.name} onAsk={askDocument}/>{/key}</section>
 {:else if view==='editor'&&draft}<section class="editor wysiwyg-editor"><div class="editor-head"><div class="editor-location"><Button.Root class="icon-button" title={$t(documentFromConversation?"返回会话":documentFromSearch?"返回搜索":"返回文件夹")} onclick={returnToFiles}>{@html icon('back')}</Button.Root>{#if selected}<Popover.Root><Popover.Trigger type="button" class="scope-info" title={displayPath(selected)} aria-label={$t("文件夹路径")}>{@html icon('folder')}<span>{basename(selected)}</span></Popover.Trigger><Popover.Portal><Popover.Content class="document-folder-path" aria-label={$t("文件夹路径")} side="bottom" align="start" sideOffset={8} collisionPadding={12}>{displayPath(selected)}</Popover.Content></Popover.Portal></Popover.Root>{/if}</div><div class="document-name-wrap"><Input class="document-name" aria-label={$t("文件名")} title={draft.name} bind:value={draft.name} readonly={!draft.is_new} oninput={()=>dirty=true}/></div><div class="editor-save-actions"><span class="document-status">{dirty?$t("未保存"):draft.is_new?'':$t("已保存")}</span><Button.Root class="primary small" disabled={saving} onclick={save}>{$t("保存")}</Button.Root></div></div>{#if noteDetail&&noteDetail.summary.path===draft.path}<NoteStatus note={noteDetail} checking={noteChecking} onRecheck={recheckNote} onOpenSource={openNoteSource} onReask={reaskNote}/>{/if}{#if EditorComponent}{#key draft.path}<svelte:component this={EditorComponent} body={draft.body} path={draft.path} searchTerms={documentSearchTerms} initialMatch={documentInitialMatch} onChange={changed}/>{/key}{/if}{#key draft.path}<DocumentQuestion name={draft.name} onAsk={askDocument}/>{/key}<div class="editor-footer">{draft.is_new?$t("保存后才创建文件并加入知识库。"):$t("修改后点击保存。")}</div></section>{/if}
 {#if error&&view!=='search'&&!answerHasError}<p class="form-error">{$t(error)}</p>{/if}
 </div></main></div></div>
{#if menu}<ActionMenu {menu} onClose={()=>menu=null}/>{/if}
{#if removeFolderPath}<Modal label={$t("移除文件夹")} onClose={()=>{if(!removingFolder)removeFolderPath=''}}><div class="dialog-head"><h2>{$t("移除文件夹")}</h2></div><div class="dialog-body"><p>{$t("确定移除「{value0}」及其索引吗？",{value0:basename(removeFolderPath)})}</p><p class="small muted" style="overflow-wrap:anywhere">{displayPath(removeFolderPath)}</p><p>{$t("本地原文件和会话记录会保留，之后可重新添加文件夹。")}</p>{#if removeFolderError}<p class="form-error" role="alert">{removeFolderError}</p>{/if}</div><div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消移除文件夹")} disabled={removingFolder} onclick={()=>removeFolderPath=''}>{$t("取消")}</Button.Root><Button.Root type="button" class="primary" aria-label={$t("确认移除文件夹")} disabled={removingFolder} onclick={confirmRemoveFolder}>{$t(removingFolder?"正在移除…":"确认移除")}</Button.Root></div></Modal>{/if}
{#if scopeOpen}<Modal label={$t("选择查询范围")} class="scope-dialog" onClose={()=>scopeOpen=false}><div class="dialog-head"><h2>{$t("选择查询范围")}</h2><Button.Root class="icon-button" aria-label={$t("关闭")} onclick={()=>scopeOpen=false}>{@html icon('close')}</Button.Root></div><div class="dialog-body"><p>{$t("选中的文件夹包含全部子文件夹。")}</p><Button.Root class="scope-all" aria-pressed={!scopeDraft.length} onclick={()=>scopeDraft=[]}>{@html icon('globe')}<span>{$t("全部文件")}</span>{#if !scopeDraft.length}{@html icon('check')}{/if}</Button.Root><div class="scope-tree"><Tree nodes={roots} expanded={scopeExpanded} {cache} mode="scope" checked={scopeDraft} ontoggle={(path)=>toggle(path,true).catch(e=>notify($t(String(e))))} oncheck={scopeCheck}/></div></div><div class="dialog-footer"><Button.Root onclick={()=>scopeOpen=false}>{$t("取消")}</Button.Root><Button.Root class="primary" onclick={applyScope}>{$t("应用")}</Button.Root></div></Modal>{/if}
{#if indexDetailsOpen}<IndexDetails {roots} onClose={()=>indexDetailsOpen=false} onPauseChange={indexPauseChanged}/>{/if}
{#if settingsOpen&&settings}<SettingsPanel onBeforeUpdate={protect} onIndexHistory={()=>indexDetailsOpen=true} onDataCleared={clearAllDataState} onFoldersRemoved={clearFoldersState} onConversationsCleared={clearConversationState} value={settings} initial={settingsPage} onClose={()=>settingsOpen=false} onSave={saveSettings} onSaved={()=>notify($t("设置已保存"))} onShowGuide={()=>{settingsOpen=false;onboardingOpen=true}}/>{/if}
{#if renameId}<Modal label={$t("重命名问答")} onClose={()=>renameId=''}><form on:submit|preventDefault={rename}><div class="dialog-head"><h2>{$t("重命名问答")}</h2></div><div class="dialog-body"><Input data-autofocus aria-label={$t("问答名称")} bind:value={renameTitle} required/>{#if renameError}<p class="form-error" role="alert">{renameError}</p>{/if}</div><div class="dialog-footer"><Button.Root type="button" onclick={()=>renameId=''}>{$t("取消")}</Button.Root><Button.Root class="primary" disabled={renaming||!renameTitle.trim()}>{$t("保存")}</Button.Root></div></form></Modal>{/if}
{#if leave&&!saving}<Modal label={$t("保存文档")} onClose={()=>{void leaveChoice('cancel')}}><div class="dialog-head"><h2>{$t("保存文档？")}</h2></div><div class="dialog-body"><p>{$t("「{value0}」还有未保存内容。",{value0:draft?.name})}</p></div><div class="dialog-footer"><Button.Root onclick={()=>leaveChoice('cancel')}>{$t("取消")}</Button.Root><Button.Root onclick={()=>leaveChoice('discard')}>{$t("不保存")}</Button.Root><Button.Root class="primary" onclick={()=>leaveChoice('save')}>{$t("保存")}</Button.Root></div></Modal>{/if}
{#if source}<Modal label={$t("引用原文")} drawer onClose={()=>source=null}><div class="context-top"><span class="citation-mark" aria-hidden="true">{@html icon('file')}</span><div class="citation-heading"><span class="citation-title" title={source.title}>{citationName(source)}</span><span class="source-path" title={source.path} dir="rtl"><bdi dir="ltr">{source.path}</bdi></span></div><Button.Root class="icon-button citation-close" aria-label={$t("关闭引用")} onclick={()=>source=null}>{@html icon('close')}</Button.Root></div><CitationDocument {source} onOpen={terms=>edit(source!.path,terms,'',source!)}/></Modal>{/if}
<svelte:document on:visibilitychange={scheduleNotice} on:click={externalLinkClick}/>
{#if toast}<div id="notice" role="status" style:display="flex" on:mouseenter={()=>clearTimeout(noticeTimer)} on:mouseleave={scheduleNotice} on:focusin={()=>clearTimeout(noticeTimer)} on:focusout={()=>queueMicrotask(scheduleNotice)}><span>{$t(toast)}</span>{#if undo}<Button.Root onclick={()=>undo?.()}>{$t("恢复")}</Button.Root>{/if}<Button.Root aria-label={$t("关闭提示")} onclick={closeNotice}>{@html icon('close')}</Button.Root></div>{/if}

{#if retryIndex!==null}<Modal label={$t("重新生成回答")} onClose={()=>retryIndex=null}><form on:submit|preventDefault={()=>regenerate(retryIndex!,displayedMessages[retryIndex!].question)}><div class="dialog-head"><h2>{$t("重新生成回答")}</h2></div><div class="dialog-body"><p>{$t("此轮及后续 {value0} 轮问答将被替换。完成后可以撤销，恢复原问答。",{value0:displayedMessages.length-retryIndex-1})}</p></div><div class="dialog-actions"><Button.Root type="button" class="outline" onclick={()=>retryIndex=null}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary">{$t("重新生成")}</Button.Root></div></form></Modal>{/if}

{#if toolApproval}<Modal label={$t("确认 MCP 工具调用")} onClose={()=>answerApproval(false)}><div class="dialog-head"><h2>{$t("确认工具调用")}</h2></div><div class="dialog-body"><p><strong>{toolApproval.server_name}</strong> · {toolApproval.tool_name}</p><pre class="tool-approval-arguments">{JSON.stringify(toolApproval.arguments,null,2)}</pre><p class="small muted">{$t("只有允许后才会执行本次调用。")}</p></div><div class="dialog-footer"><Button.Root onclick={()=>answerApproval(false)}>{$t("拒绝")}</Button.Root><Button.Root class="primary" onclick={()=>answerApproval(true)}>{$t("允许本次")}</Button.Root></div></Modal>{/if}

{#if onboardingOpen}<Onboarding onClose={closeOnboarding}/>{/if}

{#if saveNoteFor&&conversation&&displayedMessages[saveNoteFor.index]}<SaveNoteDialog message={displayedMessages[saveNoteFor.index]} conversationId={conversation.id} messageIndex={saveNoteFor.index} {roots} {cache} defaultFolder={settings?.notes?.folder} onClose={()=>saveNoteFor=null} onAddFolder={()=>{saveNoteFor=null;add()}} onSaved={note=>{const index=saveNoteFor!.index;saveNoteFor=null;const path=note.summary.path;if(conversation)conversation={...conversation,messages:conversation.messages.map((m,i)=>i===index?{...m,note_path:path}:m)};void history();notify($t('已保存笔记《{value0}》',{value0:note.summary.title}),async()=>{closeNotice();await openNote(path)});}}/>{/if}
{#if historySearchOpen}<HistorySearch {conversations} onClose={()=>historySearchOpen=false} onSelect={async c=>{await chooseHistory(c);if(conversation?.id===c.id)historySearchOpen=false}}/>{/if}

{#if chooseSaveFolder&&!folderPickerOpen&&draft}<Modal label={$t("保存文档")} class="new-document-dialog" onClose={()=>finishSaveFolder(null)}>
 <form class="save-document-form" on:submit|preventDefault={()=>finishSaveFolder(newDocumentFolder)}>
  <div class="dialog-head"><h2>{$t("保存文档")}</h2><Button.Root type="button" class="icon-button" aria-label={$t("关闭")} onclick={()=>finishSaveFolder(null)}>{@html icon('close')}</Button.Root></div>
  <div class="dialog-body">
   {#if saveNameNeeded}<label class="save-document-name"><span>{$t("文件名")}</span><Input data-autofocus aria-label={$t("文件名")} bind:value={draft.name} required onfocus={event=>event.currentTarget.select()} oninput={()=>{dirty=true;saveFolderError='';}}/></label>{/if}
   {#if draftFolder(draft)}<p class="save-document-location"><span>{$t("保存位置")}</span><strong>{basename(newDocumentFolder)}</strong></p>
   {:else if roots.length}<p>{$t("选择文档的保存文件夹。")}</p><div class="save-folder-tree" role="group" aria-label={$t("保存位置")}><Tree nodes={roots} expanded={saveFolderExpanded} {cache} mode="folder" selected={newDocumentFolder} onbrowse={path=>newDocumentFolder=path} ontoggle={toggleSaveFolder}/></div>
   {:else}<p>{$t("请先添加一个文件夹，再保存文档。")}</p><Button.Root type="button" class="outline" onclick={add}>{@html icon('plus')}{$t("添加文件夹")}</Button.Root>{/if}
   {#if saveFolderError}<p class="form-error" role="alert">{saveFolderError}</p>{/if}
  </div>
  <div class="dialog-footer"><Button.Root type="button" onclick={()=>finishSaveFolder(null)}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary" disabled={!newDocumentFolder||!draft.name.trim()}>{$t("保存")}</Button.Root></div>
 </form>
</Modal>{/if}
{#if folderPickerOpen}<FolderPicker defaults={{file_types:{markdown:true,text:true,code:false,config:false,...settings?.file_types},convert_other_formats:settings?.convert_other_formats??false,conversion_formats:settings?.conversion_formats}} roots={roots.map(root=>root.path)} onClose={()=>folderPickerOpen=false} onAdd={addFolders}/>{/if}

{#if folderIndexPath}<FolderIndexSettings path={folderIndexPath} onClose={()=>folderIndexPath=''} onSaved={rebuild=>{void refresh().catch(e=>notify($t(String(e))));notify($t(rebuild?'正在重新索引当前文件夹':'索引设置已保存，正在更新索引'))}}/>{/if}

{#if settings}<UpdateNotification config={settings.updates} onOpen={()=>{settingsPage='about';settingsOpen=true}}/>{/if}
