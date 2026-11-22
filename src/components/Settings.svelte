<script lang="ts">
 import {version as appVersion} from '../../src-tauri/tauri.conf.json';
 import {t,setLanguage,languageOptions,resolveLanguage} from '../lib/i18n';
 import {onDestroy} from 'svelte';
 import Input from "./ui/Input.svelte";
 import Textarea from "./ui/Textarea.svelte";
 import Select from "./ui/Select.svelte";
 import ThemePicker from './ThemePicker.svelte';
 import {applyTheme} from '../lib/theme';
 import {web,copyText} from '../lib/web-connection';
 import SagTools from './SagTools.svelte';
 import Switch from "./ui/Switch.svelte";
 import ReasoningControl from "./ReasoningControl.svelte";
 import{Button,Checkbox}from 'bits-ui';
 import type{Settings,ModelConfig,Retrieval,FileTypes,CliAccess,ConversionStorage}from '../lib/types';import{normalizeModelProxy}from '../lib/types';import{icon,logo}from '../lib/icons';import{api}from '../lib/api';
 import {open} from '@tauri-apps/plugin-dialog';
 import {displayPath} from '../lib/domain';
 import Modal from './ui/Modal.svelte';import McpSettings from './McpSettings.svelte';import{parseMcpCredentials}from '../lib/mcp-config';import type{McpCredentials,McpServer,McpToolInfo}from '../lib/types';
 import IndexSettings from './IndexSettings.svelte';
 import MineruSettings from './MineruSettings.svelte';
 import RuntimeSettings from './RuntimeSettings.svelte';
 import OtherFormats from './OtherFormats.svelte';
 import {defaultConversionWorkers} from '../lib/docling';
 import RemovedDirectories from './RemovedDirectories.svelte';
 import CliSettings from './CliSettings.svelte';
 import ApiSettings from './ApiSettings.svelte';
 import WebClipper from './WebClipper.svelte';
 import DataImport from './DataImport.svelte';
 import DataDirectory from './DataDirectory.svelte';
 import AppUpdates from './AppUpdates.svelte';
 import QuickSearchSettings from './QuickSearchSettings.svelte';
 import NetworkSettings from './NetworkSettings.svelte';
 import {defaultUpdateSettings,updateSettingsForLanguage} from '../lib/updates';
 import {providers,providerFor,sameProviderOrigin,type ModelKind} from '../lib/providers';
 import pdfRecognitionPrompt from '../../protocol/src/pdf-recognition-prompt.txt?raw';
 import imageRecognitionPrompt from '../../protocol/src/image-recognition-prompt.txt?raw';
 export let onShareReceived:((path:string)=>void)|undefined=undefined;
 export let onChatImported:(()=>Promise<void>)|undefined=undefined;
 export let onOpenImportedFolder:((path:string)=>Promise<void>)|undefined=undefined;
 export let narrowLayout=false;
 export let onShowGuide:(()=>void)|undefined=undefined;
 export let onIndexHistory:(()=>void)|undefined=undefined;
 export let onBeforeUpdate:(()=>Promise<boolean>)|undefined=undefined;
 export let onSaved:(()=>void)|undefined=undefined;
 export let onConversationsCleared:(()=>void)|undefined=undefined;
 export let onFoldersRemoved:(()=>void)|undefined=undefined;
 export let onDataCleared:(()=>Promise<void>)|undefined=undefined;
 let confirmClearData=false,clearingData=false,clearDataError='';
 async function clearData(){clearingData=true;clearDataError='';try{await api('clear_all_data');languageSaved=true;await onDataCleared?.();confirmClearData=false;onClose()}catch(e){clearDataError=$t(String(e))}finally{clearingData=false}}
 let confirmClear=false,clearing=false,clearError='';
 let confirmRemoveRoots=false,removingRoots=false,removeRootsError='';
 async function removeRoots(){removingRoots=true;removeRootsError='';try{await api('remove_all_roots');onFoldersRemoved?.();confirmRemoveRoots=false;status=$t("所有文件夹及索引已移除，本地文件已保留。")}catch(e){removeRootsError=$t(String(e))}finally{removingRoots=false}}
 async function clearConversations(){clearing=true;clearError='';try{await api('clear_conversations');onConversationsCleared?.();confirmClear=false;status=$t("所有对话和日志已永久清空")}catch(e){clearError=$t(String(e))}finally{clearing=false}}
 export let value:Settings;export let initial='general';export let onClose:()=>void;export let onSave:(settings:Settings,answer:string,jev:string,credentials?:Record<string,McpCredentials>,mineruKey?:string)=>Promise<void>;
 let settings={...structuredClone(value),model_proxy:normalizeModelProxy(value.model_proxy),show_index_details:value.show_index_details??false,runtime:{office_text_only:false,...structuredClone(value.runtime??{})},notes:{folder:(value.notes?.folder??'') as string|null,ask_location:value.notes?.ask_location??false},updates:structuredClone(value.updates??defaultUpdateSettings),convert_other_formats:value.convert_other_formats??false,search_summary_enabled:value.search_summary_enabled??true,search_summary_model:value.search_summary_model??'',search_summary_sources:value.search_summary_sources??10,language:resolveLanguage(value.language),max_tool_calls:value.max_tool_calls??20,index_workers:value.index_workers??8,conversion_workers:value.conversion_workers??defaultConversionWorkers,file_types:{markdown:true,text:true,code:false,config:false,...value.file_types},model_profiles:structuredClone(value.model_profiles??{}),mcp_servers:structuredClone(value.mcp_servers??[])},profileModel=value.answer.model,mcpDrafts:Record<string,string>={},page=initial==='jev'?'models':initial,answerKey='',jevKey='',mineruKey='',models=settings.answer.models.join('\n'),error='',status='',busy=false,showAnswer=false,showJev=false,mcpTesting='',mcpErrors:Record<string,string>={};
 let cliAccess:CliAccess|undefined=undefined,cliBusy=false,apiBusy=false;
 let dataDirectoryBusy=false;
 let serviceInfo:{data_directory:string;version:string|null}|null=null;
 $:if(web&&(page==='privacy'||page==='about')&&!serviceInfo)void api<{data_directory:string;version:string|null}>('get_service_info').then(info=>serviceInfo=info).catch(e=>error=String(e));
 let documentFormats:OtherFormats;
 let mineruConfigOpen=false;
 let visionConfigOpen=false;
 const visionPromptFields=[
  {kind:'pdf',label:'PDF 提示词',defaultPrompt:pdfRecognitionPrompt.trimEnd()},
  {kind:'image',label:'图片提示词',defaultPrompt:imageRecognitionPrompt.trimEnd()}
 ] as const;
 let visionPrompts={
  pdf:value.document_parser_pdf_prompt?.trim()||visionPromptFields[0].defaultPrompt,
  image:value.document_parser_image_prompt?.trim()||visionPromptFields[1].defaultPrompt,
 };
 $:documentParser=settings.document_parser??(value.mineru?.enabled?'mineru':'docling');
 $:remoteParsing=documentParser==='mineru'&&!!(settings.mineru?.has_key||mineruKey.trim());
 let storage:ConversionStorage=value.conversion_storage??{mode:'library'};
 let customDirectory=storage.mode==='custom'?storage.directory:'';
 $:storageChanged=JSON.stringify(storage)!==JSON.stringify(value.conversion_storage??{mode:'library'});
 async function pickConversionDirectory(){
  error='';
  try{const directory=await open({directory:true,multiple:false,title:$t('选择转换结果目录')});if(directory){customDirectory=directory;storage={mode:'custom',directory};}}
  catch(reason){error=$t(String(reason));}
 }
 let modelCatalog:{items?:string[];error?:string;existing:Set<string>}|null=null,modelFilter='';
 $: visibleModels=modelCatalog?.items?.filter(model=>model.toLowerCase().includes(modelFilter.trim().toLowerCase()));
 $: answerModels=[...new Set(models.split('\n').map(model=>model.trim()).filter(Boolean))];
 $: parserItems=[{value:'docling',label:'Docling'},{value:'mineru',label:'MinerU'},...answerModels.map(model=>({value:`model:${model}`,label:model}))];
 function selectParser(next:string){
  if(next==='docling'||next==='mineru'){settings.document_parser=next;settings.document_parser_model='';}
  else if(next.startsWith('model:')){settings.document_parser='vision';settings.document_parser_model=next.slice('model:'.length);}
 }
 async function testVision(){
  busy=true;error='';status=$t('正在测试连接…');
  try{checkProviderKey('answer');status=await api<string>('test_model',{kind:'vision',config:{...settings.answer,model:settings.document_parser_model,reasoning_effort:undefined},key:answerKey,modelProxy:structuredClone(settings.model_proxy)});}
  catch(reason){error=$t(String(reason));status='';}finally{busy=false;}
 }
 function closeVisionConfig(){
  if(busy)return;
  for(const {kind,defaultPrompt} of visionPromptFields)visionPrompts[kind]=visionPrompts[kind].trim()||defaultPrompt;
  visionConfigOpen=false;showAnswer=false;error='';status='';
 }
 async function fetchModels(){
  const catalog={existing:new Set(models.split('\n').map(model=>model.trim()))};
  modelFilter='';modelCatalog=catalog;
  try{
   checkProviderKey('answer');
   const items=await api<string[]>('list_answer_models',{config:structuredClone(settings.answer),key:answerKey,modelProxy:structuredClone(settings.model_proxy)});
   if(modelCatalog===catalog)modelCatalog={...catalog,items};
  }catch(e){if(modelCatalog===catalog)modelCatalog={...catalog,error:$t(String(e))};}
 }
 function toggleModel(model:string,checked:boolean){
  const current=models.split('\n');
  if(checked){
   if(!current.some(line=>line.trim()===model))models=[models.trimEnd(),model].filter(Boolean).join('\n');
  }else models=current.filter(line=>line.trim()!==model).join('\n');
 }
 let cliConnection='',cliConnectionOpen=false,cliConnectionSuccessUntil=0;
 async function openAboutLink(event:MouseEvent,target:'project'|'company'|'302-api-keys'|'zleap-console'){event.preventDefault();error='';try{await api('open_about_link',{target})}catch(e){error=String(e)}}
 let languageSaved=false;
 let providerChoice={answer:providerFor('answer',value.answer.url),jev:providerFor('jev',value.jev.url)};
 let autoFilledKey:ModelKind|null=null;
 $: savedKeys={answer:settings.answer.has_key&&sameProviderOrigin(value.answer.url,settings.answer.url),jev:settings.jev.auth_mode!=='none'&&value.jev.auth_mode!=='none'&&settings.jev.has_key&&sameProviderOrigin(value.jev.url,settings.jev.url)};
 function checkProviderKey(kind:ModelKind){if(kind==='jev'&&settings.jev.auth_mode==='none')return;if(settings[kind].has_key&&!savedKeys[kind]&&!(kind==='answer'?answerKey:jevKey).trim())throw new Error($t('切换供应商后，请填写对应的 API 密钥。'))}
 function changeJevAuth(mode:string){settings.jev.auth_mode=mode as 'bearer'|'none';jevKey='';autoFilledKey=null;status='';error='';}
 function changeKey(kind:ModelKind,key:string){
  if(kind==='answer')answerKey=key;else jevKey=key;
  if(autoFilledKey===kind)autoFilledKey=null;
  const other=kind==='answer'?'jev':'answer',otherKey=other==='answer'?answerKey:jevKey;
  if(settings.jev.auth_mode!=='none'&&providerChoice.answer==='302'&&providerChoice.jev==='302'&&(autoFilledKey===other||!otherKey.trim()&&!savedKeys[other])){
   if(other==='answer')answerKey=key;else jevKey=key;
   autoFilledKey=key?other:null;
  }
 }
 function changeAddress(kind:ModelKind,address:string){if(!sameProviderOrigin(settings[kind].url,address)){if(kind==='answer')answerKey='';else jevKey='';autoFilledKey=null;}settings[kind].url=address;if(providerChoice[kind]!=='custom')providerChoice[kind]=providerFor(kind,address);status='';error='';}
 function changeProvider(kind:ModelKind,id:string){const current=providerChoice[kind];providerChoice[kind]=id;if(id===current)return;if(kind==='jev'){settings.jev.model=id==='zleap'?'Zev-1':id==='openai'?'gpt-6-luna':id==='302'||id==='official'?'jev-latest':'';if(id==='openai'&&settings.jev.auth_mode==='none')changeJevAuth('bearer');}if(id==='custom')return;changeAddress(kind,providerFor(kind,value[kind].url)===id?value[kind].url:providers[kind].find(provider=>provider.value===id)!.url);}
 $:applyTheme(settings.theme);
 onDestroy(()=>{if(!languageSaved)setLanguage(value.language);applyTheme(value.theme)});
 type SettingsPage=[id:string,label:string,glyph:string];
 let pageGroups:{label:string;pages:SettingsPage[]}[];
 $:pageGroups=[
  {label:$t('settings.group.app'),pages:[
   ['general',$t('通用'),'settings'],
   ['appearance',$t('外观'),'sun'],
   ['shortcuts',$t('快捷键'),'panel'],
  ]},
  {label:$t('知识库'),pages:[
   ['imports',$t('数据源'),'plug'],
   ['files',$t('文件设置'),'folder'],
   ...(!web?[['clipper',$t('网页剪藏'),'globe'] as SettingsPage]:[]),
  ]},
  {label:$t('settings.group.searchAndChat'),pages:[
   ['models',$t('模型设置'),'plug'],
   ['answer',$t('问答设置'),'chat'],
   ['retrieval',$t('搜索设置'),'search'],
  ]},
  {label:$t('settings.group.connections'),pages:[
   ['cli',$t('CLI设置'),'codeBlock'],
   ['api',$t('远程服务'),'network'],
   ['network',$t('网络代理'),'route'],
  ]},
  {label:$t('其他'),pages:[
   ['privacy',$t('数据与隐私'),'globe'],
   ['about',$t('关于'),'info'],
  ]},
 ];
 $:pages=pageGroups.flatMap(group=>group.pages);
 let descriptions:Record<string,string>; $:descriptions={api:$t('供本机与局域网程序搜索知识库。'),clipper:$t('将网页保存为本地 Markdown 文件。'),general:$t("设置启动方式与日常使用习惯。"),appearance:$t("选择适合你的外观与阅读字号。"),files:$t("原文件保留在本地，内容变化自动同步。"),shortcuts:$t("使用键盘完成常用操作。"),models:$t("配置问答模型与 Jev 的连接信息。"),answer:$t("配置提示词、工具绑定与问答请求参数。"),retrieval:$t("调整 SAG2 的召回、扩展与片段设置。"),cli:$t("让 Agent 通过 SAG 搜索本地项目。"),network:$t("设置代理模式、认证和直连地址。"),privacy:$t("查看数据保存方式与本地存储位置。"),about:$t("你的文件，直接成为知识库。")};
 let parameters:[keyof Retrieval,string,string,number,number][]; $:parameters=[['candidate_k',$t("每轮候选数量"),$t("问答每轮的新候选上限；深入搜索固定为 100"),10,100],['seed_k',$t("扩展起点数量"),$t("每轮尚未扩展的起点"),1,20],['expansion_term_limit',$t("扩展词上限"),$t("包括词项和完整名称短语"),1,500],['max_hops',$t("扩展轮数"),$t("默认扩展 1 次；加上首次召回，共 2 轮检索"),0,2],['top_k',$t("返回片段数量"),$t("问答工具返回的片段数；深入搜索最多展示 100 个文件"),1,100],['chunk_chars',$t("片段目标字符数"),$t("默认约 512，保留完整句子及相邻上下文；长句可超过目标长度。修改后自动重建索引"),400,12000]];
 let fileTypeOptions:[keyof FileTypes,string,string][]; $:fileTypeOptions=[['markdown','Markdown','.md · .markdown'],['text',$t("文本与文档"),'.txt · .rtf · .docx · .pptx · .xlsx · .odt · .epub · .pdf'],['code',$t("代码"),'.py · .js · .ts · .rs · .html · .css'],['config',$t("配置文件"),'.json · .yaml · .toml · .xml · .ini']];
 $:indexSettingsChanged=settings.runtime.office_text_only!==(value.runtime?.office_text_only??false)||JSON.stringify(settings.conversion_formats??null)!==JSON.stringify(value.conversion_formats??null)||settings.convert_other_formats!==(value.convert_other_formats??false)||fileTypeOptions.some(([kind])=>settings.file_types[kind]!==({markdown:true,text:true,code:false,config:false,...value.file_types})[kind])||JSON.stringify(settings.excluded)!==JSON.stringify(value.excluded)||settings.retrieval.chunk_chars!==value.retrieval.chunk_chars;
 $:profileNames=models.split('\n').map(name=>name.trim()).filter(Boolean);
 $:if(profileNames.length&&!profileNames.includes(profileModel))profileModel=profileNames[0];
 $:if(profileModel&&!settings.model_profiles[profileModel])settings.model_profiles[profileModel]={system_prompt:'',sag2_enabled:true,mcp_servers:[],mcp_tools:{}};
 async function testMcp(server:McpServer){
  mcpTesting=server.id;mcpErrors={...mcpErrors,[server.id]:''};
  try {
   const credentials=parseMcpCredentials(server,mcpDrafts[server.id]);
   const tools=await api<McpToolInfo[]>('test_mcp',{server,credentials:credentials??null});
   for(const profile of Object.values(settings.model_profiles))if(profile.mcp_servers.includes(server.id)){
    profile.mcp_tools??={};
    if(!profile.mcp_tools[server.id])profile.mcp_tools[server.id]=tools.map(tool=>tool.name);
   }
   settings={...settings,mcp_servers:settings.mcp_servers.map(item=>item.id===server.id?{...item,tools}:item)};
  }catch(error){mcpErrors={...mcpErrors,[server.id]:$t(String(error))}}finally{mcpTesting=''}
 }
 function removeMcp(id:string){
  const bindings=Object.entries(settings.model_profiles).map(([model,profile])=>({model,index:profile.mcp_servers.indexOf(id),tools:profile.mcp_tools?.[id]?.slice()}));
  for(const profile of Object.values(settings.model_profiles)){profile.mcp_servers=profile.mcp_servers.filter(serverId=>serverId!==id);delete profile.mcp_tools?.[id]}
  return ()=>{
   for(const binding of bindings){
    const profile=settings.model_profiles[binding.model];if(!profile)continue;
    if(binding.index>=0&&!profile.mcp_servers.includes(id))profile.mcp_servers.splice(Math.min(binding.index,profile.mcp_servers.length),0,id);
    if(binding.tools){profile.mcp_tools??={};profile.mcp_tools[id]=binding.tools;}
   }
   settings={...settings};
  };
 }
 async function save(){
  settings.document_parser_pdf_prompt=visionPrompts.pdf===visionPromptFields[0].defaultPrompt?'':visionPrompts.pdf;
  settings.document_parser_image_prompt=visionPrompts.image===visionPromptFields[1].defaultPrompt?'':visionPrompts.image;
  busy=true;error='';status=storageChanged?$t('正在迁移转换结果…'):'';try{if(storage.mode==='custom'&&!storage.directory)throw new Error($t('请先选择转换结果目录'));settings.conversion_storage=storage;if(page==='files'||mineruKey.trim())settings.document_parser=documentParser;checkProviderKey('answer');checkProviderKey('jev');if(settings.model_proxy.mode==='custom'&&!settings.model_proxy.url.trim())throw new Error($t('请填写代理地址，例如 http://127.0.0.1:7890'));if(settings.model_proxy.mode==='custom'&&settings.model_proxy.password&&!settings.model_proxy.username.trim())throw new Error($t('请填写代理用户名'));settings.model_proxy={...settings.model_proxy,url:settings.model_proxy.url.trim(),username:settings.model_proxy.username.trim(),bypass:settings.model_proxy.bypass.trim()};settings.notes={...settings.notes,folder:settings.notes.folder?.trim()||null};settings.answer.models=answerModels;if(!answerModels.includes(settings.search_summary_model))settings.search_summary_model='';if(!settings.answer.models.includes(settings.answer.model))settings.answer.model=settings.answer.models[0]||'';const credentials:Record<string,McpCredentials>={};for(const server of settings.mcp_servers){const parsed=parseMcpCredentials(server,mcpDrafts[server.id]);if(parsed)credentials[server.id]=parsed;}await onSave(settings,answerKey,settings.jev.auth_mode==='none'?'':jevKey,credentials,mineruKey);languageSaved=true;answerKey='';jevKey='';if(cliAccess){try{await api('save_cli_settings',{access:cliAccess})}catch(e){throw new Error($t('其他设置已保存，但 CLI 权限未保存：{value0}',{value0:String(e)}))}}onSaved?.();onClose()}catch(e){error=$t(String(e))}finally{busy=false;status=''}
 }
 async function clearKey(kind:'answer'|'jev'){try{await api('clear_model_key',{kind});settings[kind].has_key=false;status=$t("已移除保存的密钥")}catch(e){error=$t(String(e))}}
 async function test(kind:'answer'|'jev'){busy=true;error='';status=$t("正在测试连接…");try{checkProviderKey(kind);const config:ModelConfig=structuredClone(settings[kind]);if(kind==='answer')config.model=models.split('\n')[0].trim();status=await api<string>('test_model',{kind,config,key:kind==='answer'?answerKey:settings.jev.auth_mode==='none'?'':jevKey,modelProxy:structuredClone(settings.model_proxy)})}catch(e){error=$t(String(e));status=''}finally{busy=false}}
</script>

{#snippet modelEndpoint(kind:ModelKind)}
 <label class="field-label provider-field">{$t('供应商')}<Select label={kind==='answer'?$t('LLM 供应商'):$t('Jev 供应商')} value={providerChoice[kind]} placeholder={kind==='jev'?$t('已保存的接口'):''} items={providers[kind].map(provider=>({value:provider.value,label:$t(provider.label)}))} onValueChange={id=>changeProvider(kind,id)}/></label>
 <label class="field-label">{$t('接口地址')}<Input type="url" required value={settings[kind].url} oninput={event=>changeAddress(kind,event.currentTarget.value)} spellcheck="false"/></label>
{/snippet}

{#snippet answerCredential()}
 <label class="field-label"><span class="key-field-label">{$t('API 密钥')}{#if providerChoice.answer==='302'}<a class="settings-key-link" href="https://302.ai/api-keys/list" target="_blank" rel="noreferrer" on:click={event=>openAboutLink(event,'302-api-keys')}>{$t('获取 API Key')}</a>{/if}</span><div class="key-field"><Input type={showAnswer?'text':'password'} value={answerKey} oninput={event=>changeKey('answer',event.currentTarget.value)} placeholder={savedKeys.answer?$t('已保存在本机；留空保持原密钥'):$t('填写 API 密钥')} autocomplete="off" spellcheck="false"/><Button.Root type="button" onclick={()=>showAnswer=!showAnswer}>{showAnswer?$t('隐藏'):$t('显示')}</Button.Root></div></label>
{/snippet}

<Modal label={$t("设置")} class="settings-dialog settings-window" overlayClass="settings-backdrop" onClose={()=>{if(!apiBusy&&!busy&&!dataDirectoryBusy)onClose();}}>
  <div class="dialog-head">
   <h2>{$t("设置")}</h2>
   <Button.Root class="icon-button settings-close" aria-label={$t("关闭设置")} disabled={apiBusy||busy||dataDirectoryBusy} onclick={onClose}>{@html icon('close')}</Button.Root>
   <Button.Root class="icon-button settings-back" aria-label={$t("返回上一层")} disabled={apiBusy||busy||dataDirectoryBusy} onclick={onClose}>{@html icon('back')}</Button.Root>
  </div>
  <form on:submit|preventDefault={()=>{if(page!=='api'&&!dataDirectoryBusy)void save();}} class="settings-form">
   <nav class="settings-nav" aria-label={$t("设置分类")}>
    {#each pageGroups as group}
     <div class="settings-nav-group" role="group" aria-label={group.label}>
      <p class="settings-nav-heading" aria-hidden="true">{group.label}</p>
      {#each group.pages as [id,label,glyph] (id)}
       <Button.Root type="button" disabled={cliBusy||apiBusy||dataDirectoryBusy} aria-current={page===id?'page':undefined} class={page===id?'active':''} onclick={()=>{page=id;status='';error=''}}>{@html icon(glyph)}<span>{label}</span></Button.Root>
      {/each}
     </div>
    {/each}
   </nav>
   {#if narrowLayout}
    <div class="settings-category-picker">
     <Select label={$t("设置分类")} value={page} items={pages.map(([value,label])=>({value,label}))} disabled={cliBusy||apiBusy||dataDirectoryBusy} onValueChange={next=>{page=next;status='';error=''}}/>
    </div>
   {/if}
   <div class="settings-content">
    {#if page!=='api'}<header class="settings-page-heading"><h2>{pages.find(p=>p[0]===page)?.[1]}</h2>{#if descriptions[page]}<p>{descriptions[page]}</p>{/if}</header>{/if}
    {#if page==='general'}
     <section class="settings-group">
      {#if !web}<div class="setting-row"><div><strong>{$t("启动页面")}</strong><p>{$t("打开应用时首先显示的页面")}</p></div><Select label={$t("启动页面")} class="start-page-select" bind:value={settings.start_page} items={[{value:"ask",label:$t("问答")},{value:"search",label:$t("搜索")}]}/></div>{/if}
      <div class="setting-row">
       <strong>{$t('文档默认打开方式')}</strong>
       <Select label={$t('文档默认打开方式')} value={settings.editor_open_mode??'edit'} onValueChange={mode=>settings.editor_open_mode=mode as Settings['editor_open_mode']} items={[{value:'edit',label:$t('编辑')},{value:'view',label:$t('查看')}]}/>
      </div>
      <div class="setting-row"><div><strong>{$t("settings.interfaceLanguage")}</strong></div><Select label={$t("settings.interfaceLanguage")} value={settings.language} onValueChange={next=>{settings.language=resolveLanguage(next);setLanguage(settings.language)}} items={languageOptions}/></div>
      {#if !web}<div class="setting-row"><div><strong>{$t("关闭窗口后")}</strong><p>{$t("从托盘或菜单栏重新打开，也可在那里退出。")}</p></div><span class="setting-value">{$t("后台继续运行")}</span></div>{/if}
     </section>
     {#if onShowGuide}<section class="settings-group"><div class="setting-row"><div><strong>{$t('快速入门')}</strong><p>{$t('重新查看文件接入、搜索与问答教程')}</p></div><Button.Root type="button" class="outline" onclick={onShowGuide}>{$t('查看教程')}</Button.Root></div></section>{/if}
    {:else if page==='appearance'}
     <h3 class="settings-section-title">{$t('视觉风格')}</h3>
     <section class="settings-group">
      <div class="setting-row appearance-theme-row"><div><strong>{$t('模式')}</strong><p>{$t("设置应用的明暗外观")}</p></div><ThemePicker bind:value={settings.theme}/></div>
      <div class="setting-row"><div><strong>{$t("文档字号")}</strong><p>{$t("调整正文大小")}</p></div><Select label={$t("文档字号")} value={String(settings.editor_size)} onValueChange={value=>settings.editor_size=Number(value)} items={[14,15,16,18].map(n=>({value:String(n),label:`${n} px`}))}/></div>
     </section>
     <h3 class="settings-section-title">{$t('正文预览')}</h3>
     <section class="settings-type-preview"><p style:font-size={`${settings.editor_size}px`}>{$t("你的文件，直接成为知识库。")}</p></section>
    {:else if page==='files'}
     <h3 class="settings-section-title">{$t('文档解析')}</h3>
     <section class="settings-group conversion-storage">
      <div class="setting-row parser-setting">
       <div><strong>{$t('优先解析方式')}</strong>{#if documentParser==='vision'}<p>{$t('将使用所选模型的多模态能力解析 PDF 和图片。')}</p>{:else if documentParser==='mineru'&&!remoteParsing}<p>{$t('未配置密钥，暂用本地解析。')}</p>{/if}</div>
       <div class="parser-actions">
        <Select label={$t('优先解析方式')} value={documentParser==='vision'?`model:${settings.document_parser_model}`:documentParser} disabled={busy} onValueChange={selectParser} items={parserItems}/>
        {#if documentParser==='vision'}<Button.Root type="button" class="outline" disabled={busy} onclick={()=>{showAnswer=false;error='';status='';visionConfigOpen=true}}>{$t('配置模型')}</Button.Root>{:else}
        <Button.Root type="button" class="outline" disabled={busy} onclick={()=>documentParser==='docling'?documentFormats.showDetails(true):mineruConfigOpen=true}>{$t(documentParser==='docling'?'配置 Docling':'配置 MinerU')}</Button.Root>
        {/if}
       </div>
      </div>
      <div class="setting-row">
       <div><strong>{$t('转换结果存放位置')}</strong>{#if storageChanged}<p>{$t('保存后迁移已有转换结果，无需重新识别。')}</p>{/if}</div>
       <Select class="conversion-storage-select" label={$t('转换结果存放位置')} value={storage.mode} disabled={busy} onValueChange={mode=>storage=mode==='custom'?{mode,directory:customDirectory}:{mode:mode as 'app_data'|'library'}} items={[{value:'app_data',label:$t('应用数据目录')},{value:'library',label:$t('资料文件夹内')},...(!web||value.conversion_storage?.mode==='custom'?[{value:'custom',label:$t('自定义目录')}]:[])]}/>
      </div>
      {#if storage.mode==='custom'}
       <div class="setting-row"><div><strong>{$t('自定义目录')}</strong><p class="conversion-directory">{storage.directory?displayPath(storage.directory):$t('请选择资料文件夹之外的目录')}</p></div>{#if !web}<Button.Root type="button" class="outline" disabled={busy} onclick={pickConversionDirectory}>{$t('选择目录')}</Button.Root>{/if}</div>
      {/if}
      <RuntimeSettings group="files" bind:value={settings.runtime}/>
     </section>
     <header class="settings-group-heading"><div><h3>{$t('索引文件类型')}</h3><p class="settings-help">{$t('默认启用常用文本、Office、PDF 与电子书，无需下载组件。取消勾选后不再检索该类文件，原文件和文件浏览不受影响。')}</p></div></header>
     <section class="settings-group file-types">
      <IndexSettings settingsChanged={indexSettingsChanged}/>
      {#each fileTypeOptions as [kind,label,description]}
       <div class="setting-row"><label for={`file-type-${kind}`}><strong>{label}</strong><p>{description}</p></label><Checkbox.Root id={`file-type-${kind}`} class="ui-checkbox" aria-label={$t("索引 {value0}",{value0:label})} checked={settings.file_types[kind]} onCheckedChange={checked=>settings.file_types={...settings.file_types,[kind]:checked}}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root></div>
      {/each}
      <OtherFormats bind:this={documentFormats} {remoteParsing} visionParsing={documentParser==='vision'} remoteProvider={settings.mineru?.provider} bind:workers={settings.conversion_workers} bind:enabled={settings.convert_other_formats} bind:formats={settings.conversion_formats} disabled={busy}/>
      <div class="setting-row"><div><strong>{$t("Office 仅提取文字")}</strong><p>{$t("仅适用于 DOCX/PPTX。跳过图片和内嵌对象。")}</p></div><Switch bind:checked={settings.runtime.office_text_only} label={$t("Office 仅提取文字")}/></div>
     </section>
     <h3 class="settings-section-title">{$t('索引与显示')}</h3>
     <section class="settings-group">
      <div class="setting-row"><div><strong>{$t("自动监听文件变化")}</strong><p>{$t("新增、修改、删除和重命名后自动更新索引；退出应用后停止监听")}</p></div><Switch bind:checked={settings.watch} label={$t("自动监听文件变化")}/></div>
      <div class="setting-row hidden-folder-settings"><div><strong>{$t("显示隐藏文件夹")}</strong><p>{$t("仅在右侧文件列表显示，左侧始终隐藏；不影响索引。")}</p></div><Switch bind:checked={settings.show_hidden} label={$t("显示隐藏文件夹")}/></div>
      <div class="setting-row"><label for="index-workers"><strong>{$t("索引并行数")}</strong><p>{$t("默认 8，可设为 1–64；下次索引任务生效，不会重建已有索引")}</p></label><Input id="index-workers" aria-label={$t("索引并行数")} type="number" min="1" max="64" required bind:value={settings.index_workers}/></div>
      <div class="settings-fields file-excluded"><label class="field-label">{$t("排除索引的目录名称")}<Textarea value={settings.excluded.join('\n')} oninput={(e)=>settings.excluded=e.currentTarget.value.split('\n').filter(Boolean)} rows={4}/></label><p class="settings-help">{$t("每行一个名称。排除索引不影响文件树中的浏览。文档使用手动保存。")}</p><p class="settings-help">{$t("项目忽略规则自动读取 .gitignore 和 .sagignore。可用 *.log、目录名/ 和 !文件名 编写规则；.sagignore 优先。修改后自动更新索引，原文件仍可浏览。")}</p></div>
      {#if onIndexHistory}<div class="setting-row index-history-settings"><div><strong>{$t('索引历史')}</strong></div><Button.Root type="button" class="outline settings-index-history" onclick={onIndexHistory}>{$t('查看')}</Button.Root></div>{/if}
      <div class="setting-row"><div><strong>{$t('显示索引详情')}</strong></div><Switch bind:checked={settings.show_index_details} label={$t('显示索引详情')}/></div>
     </section>
     {#if !web}<RemovedDirectories/>{/if}

    {:else if page==='models'}
     {#if value.credential_error}<p class="form-error">{$t(value.credential_error)}</p>{/if}
     <div class="settings-info-box">{$t("问答模型按需调用 SAG2 检索工具，Jev 判断原文相关性。问答模型需支持工具调用，问题和相关原文会发送至所配置的服务。")}</div>
     {#each ['answer','jev'] as kind}
      <header class="settings-group-heading"><h3>{kind==='answer'?$t("问答模型（LLM）"):$t("Jev 模型")}</h3><span>{kind==='answer'?$t("生成回答"):$t("判断相关性")}</span></header>
      <section class="settings-group settings-fields model-settings-card" data-model={kind}>
       {@render modelEndpoint(kind as ModelKind)}
       {#if kind==='answer'}
        <div class="field-label">
         <div class="key-field-label"><label for="answer-models">{$t("可选模型 · 每行一个")}</label><Button.Root type="button" class="settings-key-link" disabled={busy} onclick={fetchModels}>{$t('获取模型列表')}</Button.Root></div>
         <Textarea id="answer-models" required bind:value={models} rows={3} spellcheck="false"/>
        </div>
        {@render answerCredential()}
       {:else}
        <label class="field-label">{$t("模型名称")}<Input required bind:value={settings.jev.model} spellcheck="false"/></label>
        <label class="field-label auth-field">{$t("鉴权方式")}<Select label={$t("Jev 鉴权方式")} value={settings.jev.auth_mode??'bearer'} items={[{value:'bearer',label:$t('API Key')},{value:'none',label:$t('无需鉴权')}]} onValueChange={changeJevAuth}/></label>
        {#if settings.jev.auth_mode!=='none'}
        <label class="field-label"><span class="key-field-label">{$t("API 密钥")}{#if providerChoice.jev==='zleap'}<a class="settings-key-link" href="https://platform.zleap.com/console" target="_blank" rel="noreferrer" on:click={event=>openAboutLink(event,'zleap-console')}>{$t("获取 API Key")}</a>{:else if providerChoice.jev==='302'}<a class="settings-key-link" href="https://302.ai/api-keys/list" target="_blank" rel="noreferrer" on:click={event=>openAboutLink(event,'302-api-keys')}>{$t("获取 API Key")}</a>{/if}</span><div class="key-field"><Input data-autofocus={initial==='jev'?true:undefined} type={showJev?'text':'password'} value={jevKey} oninput={event=>changeKey('jev',event.currentTarget.value)} placeholder={savedKeys.jev?$t("已保存在本机；留空保持原密钥"):$t("填写 API 密钥")} autocomplete="off" spellcheck="false"/><Button.Root type="button" onclick={()=>showJev=!showJev}>{showJev?$t("隐藏"):$t("显示")}</Button.Root></div></label>
        {/if}
       {/if}
       <div class="settings-card-actions">{#if settings[kind as ModelKind].has_key&&savedKeys[kind as ModelKind]}<Button.Root type="button" class="settings-text-button" onclick={()=>clearKey(kind as 'answer'|'jev')}>{$t("移除保存的密钥")}</Button.Root>{/if}<Button.Root type="button" class="outline settings-model-test" disabled={busy} onclick={()=>test(kind as 'answer'|'jev')}>{$t("测试连接")}</Button.Root></div>
      </section>
     {/each}
     <RuntimeSettings group="models" bind:value={settings.runtime}/>
    {:else if page==='answer'}
     <h3 class="settings-section-title">{$t('提示词')}</h3>
     <section class="settings-group settings-fields model-profile-settings">
     <div class="answer-model-selection">
      <label class="field-label">{$t("问答模型")}<Select label={$t("配置问答模型")} bind:value={profileModel} items={profileNames.map(model=>({value:model,label:model}))}/></label>
      <p class="settings-help">{$t("提示词和工具绑定按模型分别保存；上下文与请求设置对所有模型生效。")}</p>
     </div>
      {#if settings.model_profiles[profileModel]}
       <div class="setting-row model-reasoning-row"><div><strong>{$t("思考强度")}</strong><p>{$t("按模型单独保存；强度越高，回答通常需要更长时间。")}</p></div><ReasoningControl model={profileModel} endpoint={settings.answer.url} value={settings.model_profiles[profileModel].reasoning_effort} onChange={value=>settings.model_profiles[profileModel].reasoning_effort=value}/></div>
       <label class="field-label">{$t("系统提示词")}<Textarea aria-label={$t("系统提示词")} bind:value={settings.model_profiles[profileModel].system_prompt} rows={6} placeholder={$t("留空使用内置提示词；可设置回答角色、语言和风格。")}/></label>
       <p class="settings-help">{$t("此提示词只用于当前选择的模型。清空后恢复内置提示词。")}</p>
      {/if}
     </section>
     <h3 class="settings-section-title">{$t('工具绑定')}</h3>
     <section class="settings-group settings-fields model-tools-settings">
      {#if settings.model_profiles[profileModel]}
        {#if web}<SagTools profile={settings.model_profiles[profileModel]} onChange={profile=>settings.model_profiles[profileModel]=profile}/>{:else}
         <McpSettings model={profileModel} bind:profile={settings.model_profiles[profileModel]} bind:servers={settings.mcp_servers} bind:drafts={mcpDrafts} onTest={testMcp} testing={mcpTesting} errors={mcpErrors} onRemove={removeMcp}/>
        {/if}
      {/if}
     </section>
     <h3 class="settings-section-title">{$t('上下文与请求')}</h3>
     <section class="settings-group model-request-settings">
      <div class="setting-row"><label for="max-tool-calls"><strong>{$t("每轮工具调用上限")}</strong><p>{$t("默认 20；填 0 表示不限制；SAG 和 MCP 合计")}</p></label><Input id="max-tool-calls" aria-label={$t("每轮工具调用上限")} type="number" min="0" max="100" required bind:value={settings.max_tool_calls}/></div>
      <div class="setting-row"><label for="answer-context"><strong>{$t("回答上下文预算（tokens）")}</strong><p>{$t("控制问题、历史、工具记录和引用的输入规模。默认 64K，无固定最大值，请按模型支持的上下文大小填写。输出长度由模型服务决定。")}</p></label><Input id="answer-context" aria-label={$t("回答上下文预算（tokens）")} type="number" min="4096" step="1" required bind:value={settings.answer_context_tokens}/></div>
      <div class="setting-row"><label for="model-timeout"><strong>{$t("请求超时")}</strong><p>{$t("单位：秒")}</p></label><Input id="model-timeout" aria-label={$t("请求超时")} type="number" min="10" max="600" bind:value={settings.timeout_secs}/></div>
     </section>
     <h3 class="settings-section-title">{$t('笔记')}</h3>
     <section class="settings-group note-settings">
      <div class="setting-row"><label for="notes-folder"><strong>{$t("默认保存文件夹")}</strong><p>{$t("保存回答为笔记时的默认位置。留空使用文稿/sag-wiki/notes，不存在时自动创建并加入知识库。")}</p></label><Input id="notes-folder" aria-label={$t("默认保存文件夹")} placeholder={$t("文稿/sag-wiki/notes")} bind:value={settings.notes.folder}/></div>
      <div class="setting-row"><div><strong>{$t("每次询问保存位置")}</strong><p>{$t("关闭后，记住位置的保存会直接写入默认文件夹。")}</p></div><Switch bind:checked={settings.notes.ask_location} label={$t("每次询问保存位置")}/></div>
     </section>

     <RuntimeSettings group="answer" bind:value={settings.runtime}/>
    {:else if page==='retrieval'}
     <section class="settings-group">
      <div class="setting-row"><strong>{$t("搜索结果自动总结")}</strong><Switch bind:checked={settings.search_summary_enabled} label={$t("搜索结果自动总结")}/></div>
      <div class="setting-row"><strong>{$t("总结模型")}</strong><Select class="summary-model-select" label={$t("总结模型")} bind:value={settings.search_summary_model} items={[{value:'',label:$t("跟随默认问答模型")},...answerModels.map(model=>({value:model,label:model}))]}/></div>
      <div class="setting-row"><div><strong>{$t("自动总结参考数量")}</strong><p>{$t("默认 10 条，不设最大值；超过模型上下文预算时自动裁剪。")}</p></div><Input type="number" min="1" step="1" required bind:value={settings.search_summary_sources} aria-label={$t("自动总结参考数量")}/></div>
     </section>
     <section class="settings-group">{#each parameters as [key,title,desc,min,max]}<div class="setting-row"><div><strong>{title}</strong><p>{desc}</p></div><Input type="number" {min} {max} required bind:value={settings.retrieval[key]} aria-label={title}/></div>{/each}</section>
     <div class="settings-section-actions"><Button.Root type="button" class="outline" onclick={()=>settings.retrieval={candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512}}>{$t("恢复默认参数")}</Button.Root></div>
     <details class="settings-limits"><summary>{$t("算法固定规则")}</summary><p>{$t("每次检索最多 3 轮 Jev 评分，本地排序不增加模型请求。单轮预算为 64,000 tokens，单候选与共享证据合计不超过 32,000 tokens；超限时提示调整候选数量，原文不截断。")}</p></details>
     <RuntimeSettings group="retrieval" bind:value={settings.runtime}/>
    {:else if page==='imports'}
     <DataImport {onShareReceived} onImported={onChatImported} {onOpenImportedFolder} onDestinationChange={folder=>settings.chat_import_folder=folder}/>
    {:else if page==='clipper'&&!web}
     <WebClipper/>
    {:else if page==='api'}
     <ApiSettings bind:pending={apiBusy}/>
    {:else if page==='network'}
     <NetworkSettings bind:value={settings.model_proxy} testUrl={value.answer.url} saving={busy}/>
    {:else if page==='cli'}
     <CliSettings bind:access={cliAccess} bind:pending={cliBusy} bind:connection={cliConnection} bind:connectionOpen={cliConnectionOpen} bind:connectionSuccessUntil={cliConnectionSuccessUntil} saving={busy}/>
    {:else if page==='privacy'}
     {#if web}<section class="settings-group"><div class="setting-row"><div><strong>{$t("应用数据位置")}</strong><p>{$t("SQLite 索引、配置与问答历史")}</p>{#if serviceInfo}<p class="conversion-directory">{serviceInfo.data_directory}</p>{/if}</div><Button.Root type="button" class="outline" disabled={!serviceInfo} onclick={()=>copyText(serviceInfo!.data_directory).catch(e=>error=$t(String(e)))}>{$t("复制")}</Button.Root></div></section>{:else}<DataDirectory bind:pending={dataDirectoryBusy} disabled={busy} onBeforeRestart={onBeforeUpdate}/>{/if}
     <section class="settings-group data-cleanup-settings">
      <div class="setting-row clear-conversations-settings"><div><strong>{$t("清空所有对话")}</strong><p>{$t("永久删除全部对话、思考过程和工具调用日志，包括此前删除的对话。文件与索引保留。")}</p></div><Button.Root type="button" class="clear-conversations-button" onclick={()=>{clearError='';confirmClear=true}}>{$t("清空所有对话")}</Button.Root></div>
     <div class="setting-row remove-roots-settings"><div><strong>{$t("移除所有文件夹")}</strong><p>{$t("移除全部已添加的文件夹，同时删除索引。本地文件和会话会保留。")}</p></div><Button.Root type="button" class="remove-roots-button" onclick={()=>{removeRootsError='';confirmRemoveRoots=true}}>{$t("移除所有文件夹")}</Button.Root></div>
     <div class="setting-row clear-data-settings"><div><strong>{$t("清除所有数据")}</strong><p>{$t("清空所有对话、文件夹及索引、搜索记录、模型设置、密钥和草稿，所有设置恢复默认。本地原文件保留。")}</p></div><Button.Root type="button" class="clear-conversations-button" onclick={()=>{clearDataError='';confirmClearData=true}}>{$t("清除所有数据")}</Button.Root></div>
     </section>
    {:else if page==='shortcuts'}
     {#if !web}<QuickSearchSettings/>{/if}
     <section class="settings-group settings-shortcuts">{#each [[$t("搜索文件"),'⌘ / Ctrl + K'],[$t("新建问答"),'⌘ / Ctrl + N'],[$t("保存文档"),'⌘ / Ctrl + S'],[$t("发送或搜索"),'Enter'],[$t("问题换行"),'Shift + Enter'],[$t("引用文件"),'@'],[$t("选择引用"),'↑ ↓ / Enter / Esc']] as [label,key]}<div class="setting-row"><strong>{label}</strong><kbd>{key}</kbd></div>{/each}</section>
    {:else}
     <section class="settings-about" aria-label={$t('关于 SAG Wiki')}>
      <div class="settings-about-mark">{@html logo}</div>
      <div class="settings-about-copy"><h3>SAG Wiki</h3><p>{$t("向你的文件提问")}</p>{#if !web}<span>{$t('Tauri + Rust + SQLite')}</span>{/if}</div>
      {#if !web||serviceInfo?.version}<span class="settings-about-version">v{web?serviceInfo?.version:appVersion}</span>{/if}
      <div class="settings-about-links"><a href="https://github.com/Zleap-AI/SAG-Wiki" target="_blank" rel="noopener noreferrer" on:click={event=>openAboutLink(event,'project')}>{$t('开源项目')} · GitHub</a><a href="https://zleap.com" target="_blank" rel="noopener noreferrer" on:click={event=>openAboutLink(event,'company')}>{$t('开发公司')} · Zleap</a></div>
     </section>
     {#if !web}<AppUpdates config={updateSettingsForLanguage(settings.updates,value.language)} onBeforeInstall={onBeforeUpdate}/>{/if}
    {/if}
   </div>
   {#if page==='imports'}<div class="settings-footer"><Button.Root type="button" class="primary" onclick={onClose}>{$t('完成')}</Button.Root></div>{:else if page==='api'}<div class="settings-footer api-settings-footer"><span class="settings-status">{$t('更改即时生效')}</span><Button.Root type="button" class="primary" disabled={apiBusy||busy} onclick={onClose}>{$t('完成')}</Button.Root></div>{:else}
   <div class="settings-footer"><div class="settings-feedback"><span class="form-error" role="alert">{$t(error)}</span><span class="settings-status" role="status">{$t(status)}</span></div><Button.Root type="button" class="settings-cancel" disabled={busy||dataDirectoryBusy} onclick={onClose}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary" disabled={busy||dataDirectoryBusy||!!mcpTesting||cliBusy}>{$t("保存设置")}</Button.Root></div>{/if}
  </form>
</Modal>
{#if mineruConfigOpen}
 <Modal label={$t('配置 MinerU')} class="mineru-config-dialog" onClose={()=>mineruConfigOpen=false}>
  <div class="dialog-head"><h2>{$t('配置 MinerU')}</h2></div>
  <div class="dialog-body"><MineruSettings bind:value={settings.mineru} bind:apiKey={mineruKey} modelProxy={settings.model_proxy}/></div>
  <div class="dialog-footer"><Button.Root type="button" class="primary" onclick={()=>mineruConfigOpen=false}>{$t('完成')}</Button.Root></div>
 </Modal>
{/if}

{#if visionConfigOpen}
 <Modal label={$t('配置模型')} class="vision-config-dialog" onClose={closeVisionConfig}>
  <div class="dialog-head"><h2>{settings.document_parser_model}</h2></div>
  <form on:submit|preventDefault={closeVisionConfig}>
   <div class="dialog-body">
    {@render modelEndpoint('answer')}
    {@render answerCredential()}
    {#each visionPromptFields as {kind,label,defaultPrompt}}
     <div class="field-label">
      <div class="key-field-label">
       <label for={`vision-${kind}-prompt`} id={`vision-${kind}-label`}>{$t(label)}</label>
       <Button.Root type="button" class="settings-key-link" aria-describedby={`vision-${kind}-label`} disabled={visionPrompts[kind]===defaultPrompt} onclick={()=>visionPrompts[kind]=defaultPrompt}>{$t('恢复默认')}</Button.Root>
      </div>
      <Textarea id={`vision-${kind}-prompt`} rows={4} bind:value={visionPrompts[kind]}/>
     </div>
    {/each}
    {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
    {#if status}<p class="settings-status" role="status">{$t(status)}</p>{/if}
   </div>
   <div class="dialog-footer">
    <Button.Root type="button" class="outline" disabled={busy} onclick={testVision}>{$t('测试连接')}</Button.Root>
    <Button.Root type="submit" class="primary" disabled={busy}>{$t('完成')}</Button.Root>
   </div>
  </form>
 </Modal>
{/if}



{#if modelCatalog}<Modal label={$t('模型列表')} onClose={()=>modelCatalog=null} class="model-catalog" overlayClass="model-catalog-backdrop">
 <div class="dialog-head"><h2>{$t('模型列表')}</h2></div>
 <div class="dialog-body">
  {#if modelCatalog.error}<p class="form-error" role="alert">{modelCatalog.error}</p>
  {:else if !visibleModels}<p role="status">{$t('正在获取模型列表…')}</p>
  {:else if !modelCatalog.items?.length}<p role="status">{$t('供应商未返回模型。')}</p>
  {:else}
   <Input type="search" data-autofocus aria-label={$t('搜索模型')} placeholder={$t('搜索模型')} bind:value={modelFilter}/>
   {#if visibleModels.length}
    <div class="model-options">
     {#each visibleModels as model (model)}
      <label><input type="checkbox" checked={models.split('\n').some(line=>line.trim()===model)} disabled={modelCatalog.existing.has(model)} on:change={event=>toggleModel(model,event.currentTarget.checked)}/><span>{model}</span></label>
     {/each}
    </div>
   {:else}<p role="status">{$t('没有匹配的模型。')}</p>{/if}
  {/if}
 </div>
 <div class="dialog-footer"><Button.Root type="button" class="primary" onclick={()=>modelCatalog=null}>{$t('完成')}</Button.Root></div>
</Modal>{/if}

{#if confirmClear}<Modal label={$t("清空所有对话")} onClose={()=>{if(!clearing)confirmClear=false}}>
 <div class="dialog-head"><h2>{$t("清空所有对话")}</h2></div>
 <div class="dialog-body"><p>{$t("所有问答历史、思考过程和工具调用日志将被永久删除，无法恢复。原文件、知识库索引、模型设置和密钥会保留。")}</p>{#if clearError}<p class="form-error clear-history-error" role="alert">{clearError}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消清空对话")} disabled={clearing} onclick={()=>confirmClear=false}>{$t("取消")}</Button.Root><Button.Root type="button" class="clear-conversations-button" aria-label={$t("确认清空所有对话")} disabled={clearing} onclick={clearConversations}>{clearing?$t("正在清空…"):$t("确认清空")}</Button.Root></div>
</Modal>{/if}

<style>
 :global(.model-catalog-backdrop){z-index:45}
 :global(.model-catalog.dialog-panel){display:flex;flex-direction:column;max-height:calc(100dvh - 48px)}
 :global(.model-catalog .dialog-body){display:flex;flex-direction:column;min-height:0;gap:12px}
 .model-options{min-height:0;max-height:320px;overflow-y:auto}
 .model-options label{display:flex;align-items:center;gap:10px;min-height:36px;margin:0;padding:4px 8px;cursor:pointer}
 .model-options label:hover{background:var(--color-hover)}
 .model-options input{flex:none;width:16px;height:16px;margin:0;accent-color:var(--color-accent)}
 .model-options span{min-width:0;overflow-wrap:anywhere}
</style>

{#if confirmRemoveRoots}<Modal label={$t("移除所有文件夹")} onClose={()=>{if(!removingRoots)confirmRemoveRoots=false}}>
 <div class="dialog-head"><h2>{$t("移除所有文件夹")}</h2></div>
 <div class="dialog-body"><p>{$t("将移除全部已添加的文件夹，同时删除这些文件夹的索引。本地文件、会话、草稿和模型设置会保留。之后可重新添加文件夹。")}</p>{#if removeRootsError}<p class="form-error remove-roots-error" role="alert">{removeRootsError}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消移除文件夹")} disabled={removingRoots} onclick={()=>confirmRemoveRoots=false}>{$t("取消")}</Button.Root><Button.Root type="button" class="remove-roots-button" aria-label={$t("确认移除所有文件夹")} disabled={removingRoots} onclick={removeRoots}>{removingRoots?$t("正在移除…"):$t("确认移除")}</Button.Root></div>
</Modal>{/if}

{#if confirmClearData}<Modal label={$t("清除所有数据")} onClose={()=>{if(!clearingData)confirmClearData=false}}>
 <div class="dialog-head"><h2>{$t("清除所有数据")}</h2></div>
 <div class="dialog-body"><p>{$t("所有对话、思考过程、工具调用日志、已添加的文件夹、索引、搜索记录、模型设置、密钥和未保存草稿将被清除，无法恢复。所有设置恢复默认，仅保留本地原文件。")}</p>{#if clearDataError}<p class="form-error clear-data-error" role="alert">{clearDataError}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消清除所有数据")} disabled={clearingData} onclick={()=>confirmClearData=false}>{$t("取消")}</Button.Root><Button.Root type="button" class="clear-conversations-button" aria-label={$t("确认清除所有数据")} disabled={clearingData} onclick={clearData}>{clearingData?$t("正在清除…"):$t("确认清除")}</Button.Root></div>
</Modal>{/if}
