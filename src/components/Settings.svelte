<script lang="ts">
 import {version as appVersion} from '../../src-tauri/tauri.conf.json';
 import {t,setLanguage,type Language} from '../lib/i18n';
 import {onDestroy} from 'svelte';
 import Input from "./ui/Input.svelte";
 import Textarea from "./ui/Textarea.svelte";
 import Select from "./ui/Select.svelte";
 import ThemePicker from './ThemePicker.svelte';
 import {applyTheme} from '../lib/theme';
 import Switch from "./ui/Switch.svelte";
 import ReasoningControl from "./ReasoningControl.svelte";
 import{Button,Checkbox}from 'bits-ui';
 import type{Settings,ModelConfig,Retrieval,FileTypes,CliAccess}from '../lib/types';import{icon,logo}from '../lib/icons';import{api}from '../lib/api';
 import Modal from './ui/Modal.svelte';import McpSettings from './McpSettings.svelte';import{parseMcpCredentials}from '../lib/mcp-config';import type{McpCredentials,McpServer,McpToolInfo}from '../lib/types';
 import IndexSettings from './IndexSettings.svelte';
 import OtherFormats from './OtherFormats.svelte';
 import RemovedDirectories from './RemovedDirectories.svelte';
 import CliSettings from './CliSettings.svelte';
 import WebClipper from './WebClipper.svelte';
 import AppUpdates from './AppUpdates.svelte';
 import {defaultUpdateSettings} from '../lib/updates';
 import {providers,providerFor,sameProviderOrigin,type ModelKind} from '../lib/providers';
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
 export let value:Settings;export let initial='general';export let onClose:()=>void;export let onSave:(settings:Settings,answer:string,jev:string,credentials?:Record<string,McpCredentials>)=>Promise<void>;
 let settings={...structuredClone(value),updates:structuredClone(value.updates??defaultUpdateSettings),convert_other_formats:value.convert_other_formats??false,search_summary_enabled:value.search_summary_enabled??true,search_summary_sources:value.search_summary_sources??10,language:value.language??'zh-CN',max_tool_calls:value.max_tool_calls??10,index_workers:value.index_workers??8,conversion_workers:value.conversion_workers??1,file_types:{markdown:true,text:true,code:false,config:false,...value.file_types},model_profiles:structuredClone(value.model_profiles??{}),mcp_servers:structuredClone(value.mcp_servers??[])},profileModel=value.answer.model,mcpDrafts:Record<string,string>={},page=initial==='jev'?'models':initial,answerKey='',jevKey='',models=settings.answer.models.join('\n'),error='',status='',busy=false,showAnswer=false,showJev=false,mcpTesting='',mcpErrors:Record<string,string>={};
 let cliAccess:CliAccess|undefined=undefined,cliBusy=false;
 async function openAboutLink(event:MouseEvent,target:'project'|'company'|'302-api-keys'){event.preventDefault();error='';try{await api('open_about_link',{target})}catch(e){error=String(e)}}
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
 function changeProvider(kind:ModelKind,id:string){const current=providerChoice[kind];providerChoice[kind]=id;if(id==='custom'||id===current)return;changeAddress(kind,providerFor(kind,value[kind].url)===id?value[kind].url:providers[kind].find(provider=>provider.value===id)!.url);}
 $:applyTheme(settings.theme);
 onDestroy(()=>{if(!languageSaved)setLanguage(value.language??'zh-CN');applyTheme(value.theme)});
 $:pages=[['general',$t("通用"),'settings'],['appearance',$t("外观"),'sun'],['files',$t("文件设置"),'folder'],['clipper',$t('网页剪藏'),'globe'],['shortcuts',$t("快捷键"),'panel'],['models',$t("模型设置"),'plug'],['answer',$t("问答设置"),'chat'],['retrieval',$t("搜索设置"),'search'],['cli',$t('CLI设置'),'codeBlock'],['privacy',$t("数据与隐私"),'globe'],['about',$t("关于"),'info']];
 let descriptions:Record<string,string>; $:descriptions={clipper:$t('将网页保存为本地 Markdown 文件。'),general:$t("设置启动方式与日常使用习惯。"),appearance:$t("选择适合你的外观与阅读字号。"),files:$t("原文件保留在本地，内容变化自动同步。"),shortcuts:$t("使用键盘完成常用操作。"),models:$t("配置问答模型与 Jev 的连接信息。"),answer:$t("配置提示词、工具绑定与问答请求参数。"),retrieval:$t("调整 SAG2 的召回、扩展与片段设置。"),cli:$t("让 Agent 通过 SAG 搜索本地项目。"),privacy:$t("查看数据保存方式与本地存储位置。"),about:$t("你的文件，直接成为知识库。")};
 let parameters:[keyof Retrieval,string,string,number,number][]; $:parameters=[['candidate_k',$t("每轮候选数量"),$t("问答每轮的新候选上限；深入搜索固定为 100"),10,100],['seed_k',$t("扩展起点数量"),$t("每轮尚未扩展的起点"),1,20],['expansion_term_limit',$t("扩展词上限"),$t("包括词项和完整名称短语"),1,500],['max_hops',$t("扩展轮数"),$t("默认扩展 1 次；加上首次召回，共 2 轮检索"),0,2],['top_k',$t("返回片段数量"),$t("问答工具返回的片段数；深入搜索最多展示 100 个文件"),1,100],['chunk_chars',$t("片段目标字符数"),$t("默认约 512，保留完整句子及相邻上下文；长句可超过目标长度。修改后自动重建索引"),400,12000]];
 let fileTypeOptions:[keyof FileTypes,string,string][]; $:fileTypeOptions=[['markdown','Markdown','.md · .markdown'],['text',$t("文本与文档"),'.txt · .rtf · .docx · .pptx · .xlsx · .odt · .epub · .pdf'],['code',$t("代码"),'.py · .js · .ts · .rs · .html · .css'],['config',$t("配置文件"),'.json · .yaml · .toml · .xml · .ini']];
 $:indexSettingsChanged=JSON.stringify(settings.conversion_formats??null)!==JSON.stringify(value.conversion_formats??null)||settings.convert_other_formats!==(value.convert_other_formats??false)||fileTypeOptions.some(([kind])=>settings.file_types[kind]!==({markdown:true,text:true,code:false,config:false,...value.file_types})[kind])||JSON.stringify(settings.excluded)!==JSON.stringify(value.excluded)||settings.retrieval.chunk_chars!==value.retrieval.chunk_chars;
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
 async function save(){busy=true;error='';try{checkProviderKey('answer');checkProviderKey('jev');settings.answer.models=[...new Set(models.split('\n').map(s=>s.trim()).filter(Boolean))];if(!settings.answer.models.includes(settings.answer.model))settings.answer.model=settings.answer.models[0]||'';const credentials:Record<string,McpCredentials>={};for(const server of settings.mcp_servers){const parsed=parseMcpCredentials(server,mcpDrafts[server.id]);if(parsed)credentials[server.id]=parsed;}await onSave(settings,answerKey,settings.jev.auth_mode==='none'?'':jevKey,credentials);languageSaved=true;answerKey='';jevKey='';if(cliAccess){try{await api('save_cli_settings',{access:cliAccess})}catch(e){throw new Error($t('其他设置已保存，但 CLI 权限未保存：{value0}',{value0:String(e)}))}}onSaved?.();onClose()}catch(e){error=$t(String(e))}finally{busy=false}}
 async function clearKey(kind:'answer'|'jev'){try{await api('clear_model_key',{kind});settings[kind].has_key=false;status=$t("已移除保存的密钥")}catch(e){error=$t(String(e))}}
 async function test(kind:'answer'|'jev'){busy=true;error='';status=$t("正在测试连接…");try{checkProviderKey(kind);const config:ModelConfig=structuredClone(settings[kind]);if(kind==='answer')config.model=models.split('\n')[0].trim();status=await api<string>('test_model',{kind,config,key:kind==='answer'?answerKey:settings.jev.auth_mode==='none'?'':jevKey})}catch(e){error=$t(String(e));status=''}finally{busy=false}}
</script>

<Modal label={$t("设置")} class="settings-dialog settings-window" overlayClass="settings-backdrop" {onClose}>
  <div class="dialog-head"><h2>{$t("设置")}</h2><Button.Root class="icon-button" aria-label={$t("关闭设置")} onclick={onClose}>{@html icon('close')}</Button.Root></div>
  <form on:submit|preventDefault={save} class="settings-form">
   <nav class="settings-nav" aria-label={$t("设置分类")}>
    {#each pages as [id,label,glyph]}
     <Button.Root type="button" disabled={cliBusy} aria-current={page===id?'page':undefined} class={page===id?'active':''} onclick={()=>{page=id;status='';error=''}}>{@html icon(glyph)}<span>{label}</span></Button.Root>
    {/each}
   </nav>
   <div class="settings-content">
    <header class="settings-page-heading"><h2>{pages.find(p=>p[0]===page)?.[1]}</h2><p>{descriptions[page]}</p></header>
    {#if page==='general'}
     <section class="settings-group">
      <div class="setting-row"><div><strong>{$t("启动页面")}</strong><p>{$t("打开应用时首先显示的页面")}</p></div><Select label={$t("启动页面")} bind:value={settings.start_page} items={[{value:"ask",label:$t("问答")},{value:"search",label:$t("搜索")}]}/></div>
      <div class="setting-row"><div><strong>{$t("界面语言")}</strong></div><Select label={$t("界面语言")} value={settings.language} onValueChange={next=>{settings.language=next as Language;setLanguage(settings.language)}} items={[{value:'zh-CN',label:'简体中文'},{value:'en',label:'English'}]}/></div>
      <div class="setting-row"><div><strong>{$t("关闭窗口后")}</strong><p>{$t("从托盘或菜单栏重新打开，也可在那里退出。")}</p></div><span class="setting-value">{$t("后台继续运行")}</span></div>
     </section>
     {#if onShowGuide}<section class="settings-group"><div class="setting-row"><div><strong>{$t('快速入门')}</strong><p>{$t('重新查看文件接入、搜索与问答教程')}</p></div><Button.Root type="button" class="outline" onclick={onShowGuide}>{$t('查看教程')}</Button.Root></div></section>{/if}
    {:else if page==='appearance'}
     <h3 class="appearance-section-title">{$t('视觉风格')}</h3>
     <section class="settings-group">
      <div class="setting-row appearance-theme-row"><div><strong>{$t('模式')}</strong><p>{$t("设置应用的明暗外观")}</p></div><ThemePicker bind:value={settings.theme}/></div>
      <div class="setting-row"><div><strong>{$t("文档字号")}</strong><p>{$t("调整正文大小")}</p></div><Select label={$t("文档字号")} value={String(settings.editor_size)} onValueChange={value=>settings.editor_size=Number(value)} items={[14,15,16,18].map(n=>({value:String(n),label:`${n} px`}))}/></div>
     </section>
     <section class="settings-type-preview"><span>{$t("正文预览")}</span><p style:font-size={`${settings.editor_size}px`}>{$t("你的文件，直接成为知识库。")}</p></section>
    {:else if page==='files'}
     <div class="settings-info-box">{$t("支持 Markdown、纯文本、RTF、常见代码及 JSON / YAML / TOML 等配置文件，并内置转换 PDF、Office、EPUB 等文档。首次加入无需模型预处理。")}</div>
     <section class="settings-group file-types">
      <IndexSettings settingsChanged={indexSettingsChanged}/>
      {#each fileTypeOptions as [kind,label,description]}
       <div class="setting-row"><label for={`file-type-${kind}`}><strong>{label}</strong><p>{description}</p></label><Checkbox.Root id={`file-type-${kind}`} class="ui-checkbox" aria-label={$t("导入 {value0}",{value0:label})} checked={settings.file_types[kind]} onCheckedChange={checked=>settings.file_types={...settings.file_types,[kind]:checked}}>{#snippet children({checked})}{#if checked}{@html icon('check')}{/if}{/snippet}</Checkbox.Root></div>
      {/each}
      <OtherFormats bind:workers={settings.conversion_workers} bind:enabled={settings.convert_other_formats} bind:formats={settings.conversion_formats} disabled={busy}/>
     </section>
     <section class="settings-group">
      <div class="setting-row"><div><strong>{$t("自动监听文件变化")}</strong><p>{$t("新增、修改、删除和重命名后自动更新索引；退出应用后停止监听")}</p></div><Switch bind:checked={settings.watch} label={$t("自动监听文件变化")}/></div>
      <div class="setting-row hidden-folder-settings"><div><strong>{$t("显示隐藏文件夹")}</strong><p>{$t("仅在右侧文件列表显示，左侧始终隐藏；不影响索引。")}</p></div><Switch bind:checked={settings.show_hidden} label={$t("显示隐藏文件夹")}/></div>
      <div class="setting-row"><label for="index-workers"><strong>{$t("索引并行数")}</strong><p>{$t("默认 8，可设为 1–64；下次索引任务生效，不会重建已有索引")}</p></label><Input id="index-workers" aria-label={$t("索引并行数")} type="number" min="1" max="64" required bind:value={settings.index_workers}/></div>
     </section>
     <section class="settings-group settings-fields"><label class="field-label">{$t("排除索引的目录名称")}<Textarea value={settings.excluded.join('\n')} oninput={(e)=>settings.excluded=e.currentTarget.value.split('\n').filter(Boolean)} rows={4}/></label><p class="settings-help">{$t("每行一个名称。排除索引不影响文件树中的浏览。文档使用手动保存。")}</p><p class="settings-help">{$t("项目忽略规则自动读取 .gitignore 和 .sagignore。可用 *.log、目录名/ 和 !文件名 编写规则；.sagignore 优先。修改后自动更新索引，原文件仍可浏览。")}</p></section>
     <RemovedDirectories/>
     {#if onIndexHistory}<section class="settings-group index-history-settings"><div class="setting-row"><div><strong>{$t('索引历史')}</strong><p>{$t('查看索引进度和历史记录')}</p></div><Button.Root type="button" class="outline settings-index-history" onclick={onIndexHistory}>{$t('查看')}</Button.Root></div></section>{/if}

    {:else if page==='models'}
     {#if value.credential_error}<p class="form-error">{$t(value.credential_error)}</p>{/if}
     <div class="settings-info-box">{$t("问答模型按需调用 SAG2 检索工具，Jev 判断原文相关性。问答模型需支持工具调用，问题和相关原文会发送至所配置的服务。")}</div>
     {#each ['answer','jev'] as kind}
      <section class="settings-group settings-fields model-settings-card" data-model={kind}>
       <div class="settings-group-heading"><h3>{kind==='answer'?$t("问答模型（LLM）"):$t("Jev 模型")}</h3><span>{kind==='answer'?$t("生成回答"):$t("判断相关性")}</span></div>
       <label class="field-label provider-field">{$t('供应商')}<Select label={kind==='answer'?$t('LLM 供应商'):$t('Jev 供应商')} value={providerChoice[kind as ModelKind]} placeholder={kind==='jev'?$t('已保存的接口'):''} items={providers[kind as ModelKind].map(provider=>({value:provider.value,label:$t(provider.label)}))} onValueChange={id=>changeProvider(kind as ModelKind,id)}/></label>
       <label class="field-label">{$t("接口地址")}<Input type="url" required value={settings[kind as ModelKind].url} oninput={event=>changeAddress(kind as ModelKind,event.currentTarget.value)} spellcheck="false"/></label>
       {#if kind==='answer'}
        <label class="field-label">{$t("可选模型 · 每行一个")}<Textarea required bind:value={models} rows={3} spellcheck="false"/></label>
        <label class="field-label"><span class="key-field-label">{$t("API 密钥")}{#if providerChoice.answer==='302'}<a class="settings-key-link" href="https://302.ai/api-keys/list" target="_blank" rel="noreferrer" on:click={event=>openAboutLink(event,'302-api-keys')}>{$t("获取 API Key")}</a>{/if}</span><div class="key-field"><Input type={showAnswer?'text':'password'} value={answerKey} oninput={event=>changeKey('answer',event.currentTarget.value)} placeholder={savedKeys.answer?$t("已保存在本机；留空保持原密钥"):$t("填写 API 密钥")} autocomplete="off" spellcheck="false"/><Button.Root type="button" onclick={()=>showAnswer=!showAnswer}>{showAnswer?$t("隐藏"):$t("显示")}</Button.Root></div></label>
       {:else}
        <label class="field-label">{$t("模型名称")}<Input required bind:value={settings.jev.model} spellcheck="false"/></label>
        <label class="field-label auth-field">{$t("鉴权方式")}<Select label={$t("Jev 鉴权方式")} value={settings.jev.auth_mode??'bearer'} items={[{value:'bearer',label:$t('API Key')},{value:'none',label:$t('无需鉴权')}]} onValueChange={changeJevAuth}/></label>
        {#if settings.jev.auth_mode!=='none'}
        <label class="field-label"><span class="key-field-label">{$t("API 密钥")}{#if providerChoice.jev==='302'}<a class="settings-key-link" href="https://302.ai/api-keys/list" target="_blank" rel="noreferrer" on:click={event=>openAboutLink(event,'302-api-keys')}>{$t("获取 API Key")}</a>{/if}</span><div class="key-field"><Input data-autofocus={initial==='jev'?true:undefined} type={showJev?'text':'password'} value={jevKey} oninput={event=>changeKey('jev',event.currentTarget.value)} placeholder={savedKeys.jev?$t("已保存在本机；留空保持原密钥"):$t("填写 API 密钥")} autocomplete="off" spellcheck="false"/><Button.Root type="button" onclick={()=>showJev=!showJev}>{showJev?$t("隐藏"):$t("显示")}</Button.Root></div></label>
        {/if}
       {/if}
       <div class="settings-card-actions"><Button.Root type="button" class="outline" disabled={busy} onclick={()=>test(kind as 'answer'|'jev')}>{$t("测试连接")}</Button.Root>{#if settings[kind as ModelKind].has_key&&savedKeys[kind as ModelKind]}<Button.Root type="button" class="settings-text-button" onclick={()=>clearKey(kind as 'answer'|'jev')}>{$t("移除保存的密钥")}</Button.Root>{/if}</div>
      </section>
     {/each}
     <p class="settings-help settings-privacy-note">{$t("密钥保存在当前用户的应用数据目录。macOS 文件权限为仅当前用户读写；同一账户运行的其他程序仍可能读取。留空保持已保存的密钥。")}</p>
    {:else if page==='answer'}
     <section class="settings-group settings-fields model-profile-settings">
      <div class="settings-group-heading"><h3>{$t("提示词")}</h3></div>
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
     <section class="settings-group settings-fields model-tools-settings">
      <div class="settings-group-heading"><h3>{$t("工具绑定")}</h3></div>
      {#if settings.model_profiles[profileModel]}
        <McpSettings model={profileModel} bind:profile={settings.model_profiles[profileModel]} bind:servers={settings.mcp_servers} bind:drafts={mcpDrafts} onTest={testMcp} testing={mcpTesting} errors={mcpErrors} onRemove={removeMcp}/>
      {/if}
     </section>
     <section class="settings-group settings-fields model-request-settings">
      <div class="settings-group-heading"><h3>{$t("上下文与请求")}</h3></div>
      <div class="setting-row"><label for="max-tool-calls"><strong>{$t("每轮工具调用上限")}</strong><p>{$t("默认 10 次；0 表示不限制，SAG 和 MCP 合计")}</p></label><Input id="max-tool-calls" aria-label={$t("每轮工具调用上限")} type="number" min="0" max="100" required bind:value={settings.max_tool_calls}/></div>
      <div class="setting-row"><label for="answer-context"><strong>{$t("回答上下文预算（tokens）")}</strong><p>{$t("控制问题、历史、工具记录和引用的输入规模。默认 64K，无固定最大值，请按模型支持的上下文大小填写。输出长度由模型服务决定。")}</p></label><Input id="answer-context" aria-label={$t("回答上下文预算（tokens）")} type="number" min="4096" step="1" required bind:value={settings.answer_context_tokens}/></div>
      <div class="setting-row"><label for="model-timeout"><strong>{$t("请求超时")}</strong><p>{$t("单位：秒")}</p></label><Input id="model-timeout" aria-label={$t("请求超时")} type="number" min="10" max="600" bind:value={settings.timeout_secs}/></div>
     </section>

    {:else if page==='retrieval'}
     <section class="settings-group"><div class="setting-row"><div><strong>{$t("搜索结果自动总结")}</strong><p>{$t("搜索后使用问答模型，根据问题和参考结果生成简短总结。")}</p></div><Switch bind:checked={settings.search_summary_enabled} label={$t("搜索结果自动总结")}/></div><div class="setting-row"><div><strong>{$t("自动总结参考数量")}</strong><p>{$t("默认 10 条，不设最大值；超过模型上下文预算时自动裁剪。")}</p></div><Input type="number" min="1" step="1" required bind:value={settings.search_summary_sources} aria-label={$t("自动总结参考数量")}/></div></section>
     <section class="settings-group">{#each parameters as [key,title,desc,min,max]}<div class="setting-row"><div><strong>{title}</strong><p>{desc}</p></div><Input type="number" {min} {max} required bind:value={settings.retrieval[key]} aria-label={title}/></div>{/each}</section>
     <div class="settings-section-actions"><Button.Root type="button" class="outline" onclick={()=>settings.retrieval={candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:512}}>{$t("恢复默认参数")}</Button.Root></div>
     <details class="settings-limits"><summary>{$t("算法固定规则")}</summary><p>{$t("每次检索最多 3 轮 Jev 评分，本地排序不增加模型请求。单轮预算为 64,000 tokens，单候选与共享证据合计不超过 32,000 tokens；超限时提示调整候选数量，原文不截断。")}</p></details>
    {:else if page==='clipper'}
     <WebClipper/>
    {:else if page==='cli'}
     <CliSettings bind:access={cliAccess} bind:pending={cliBusy} saving={busy}/>
    {:else if page==='privacy'}
     <section class="settings-group"><div class="setting-row"><div><strong>{$t("应用数据位置")}</strong><p>{$t("SQLite 索引、配置与问答历史")}</p></div><Button.Root type="button" class="outline" onclick={()=>api('open_data_directory').catch(e=>error=$t(String(e)))}>{$t("打开目录")}</Button.Root></div></section>
     <section class="settings-group data-cleanup-settings">
      <div class="setting-row clear-conversations-settings"><div><strong>{$t("清空所有对话")}</strong><p>{$t("永久删除全部对话、思考过程和工具调用日志，包括此前删除的对话。文件与索引保留。")}</p></div><Button.Root type="button" class="clear-conversations-button" onclick={()=>{clearError='';confirmClear=true}}>{$t("清空所有对话")}</Button.Root></div>
     <div class="setting-row remove-roots-settings"><div><strong>{$t("移除所有文件夹")}</strong><p>{$t("移除全部已添加的文件夹，同时删除索引。本地文件和会话会保留。")}</p></div><Button.Root type="button" class="remove-roots-button" onclick={()=>{removeRootsError='';confirmRemoveRoots=true}}>{$t("移除所有文件夹")}</Button.Root></div>
     <div class="setting-row clear-data-settings"><div><strong>{$t("清除所有数据")}</strong><p>{$t("清空所有对话、文件夹及索引、搜索记录、模型设置、密钥和草稿，所有设置恢复默认。本地原文件保留。")}</p></div><Button.Root type="button" class="clear-conversations-button" onclick={()=>{clearDataError='';confirmClearData=true}}>{$t("清除所有数据")}</Button.Root></div>
     </section>
    {:else if page==='shortcuts'}
     <section class="settings-group settings-shortcuts">{#each [[$t("搜索文件"),'⌘ / Ctrl + K'],[$t("新建问答"),'⌘ / Ctrl + N'],[$t("保存文档"),'⌘ / Ctrl + S'],[$t("发送或搜索"),'Enter'],[$t("问题换行"),'Shift + Enter'],[$t("引用文件"),'@'],[$t("选择引用"),'↑ ↓ / Enter / Esc']] as [label,key]}<div class="setting-row"><strong>{label}</strong><kbd>{key}</kbd></div>{/each}</section>
    {:else}
     <section class="settings-about" aria-label={$t('关于 SAG Wiki')}>
      <div class="settings-about-mark">{@html logo}</div>
      <div class="settings-about-copy"><h3>SAG Wiki</h3><p>{$t("向你的文件提问")}</p><span>{$t('Tauri + Rust + SQLite')}</span></div>
      <span class="settings-about-version">v{appVersion}</span>
      <div class="settings-about-links"><a href="https://github.com/Zleap-AI/SAG-Wiki" target="_blank" rel="noopener noreferrer" on:click={event=>openAboutLink(event,'project')}>{$t('开源项目')} · GitHub</a><a href="https://zleap.com" target="_blank" rel="noopener noreferrer" on:click={event=>openAboutLink(event,'company')}>{$t('开发公司')} · Zleap</a></div>
     </section>
     <AppUpdates bind:config={settings.updates} onBeforeInstall={onBeforeUpdate}/>
    {/if}
   </div>
   <div class="settings-footer"><div class="settings-feedback"><span class="form-error" role="alert">{$t(error)}</span><span class="settings-status" role="status">{$t(status)}</span></div><Button.Root type="button" class="settings-cancel" onclick={onClose}>{$t("取消")}</Button.Root><Button.Root type="submit" class="primary" disabled={busy||!!mcpTesting||cliBusy}>{$t("保存设置")}</Button.Root></div>
  </form>
</Modal>

{#if confirmClear}<Modal label={$t("清空所有对话")} onClose={()=>{if(!clearing)confirmClear=false}}>
 <div class="dialog-head"><h2>{$t("清空所有对话")}</h2></div>
 <div class="dialog-body"><p>{$t("所有问答历史、思考过程和工具调用日志将被永久删除，无法恢复。原文件、知识库索引、模型设置和密钥会保留。")}</p>{#if clearError}<p class="form-error clear-history-error" role="alert">{clearError}</p>{/if}</div>
 <div class="dialog-footer"><Button.Root type="button" aria-label={$t("取消清空对话")} disabled={clearing} onclick={()=>confirmClear=false}>{$t("取消")}</Button.Root><Button.Root type="button" class="clear-conversations-button" aria-label={$t("确认清空所有对话")} disabled={clearing} onclick={clearConversations}>{clearing?$t("正在清空…"):$t("确认清空")}</Button.Root></div>
</Modal>{/if}

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
