<script lang="ts">
 import {onMount,tick} from 'svelte';
 import {Button} from 'bits-ui';
 import {open} from '@tauri-apps/plugin-dialog';
 import Modal from './ui/Modal.svelte';
 import ApiSourceFields from './ApiSourceFields.svelte';
 import type {Root} from '../lib/types';
 import {api} from '../lib/api';
 import {copyText,web} from '../lib/web-connection';
 import {icon} from '../lib/icons';
 import {language,t} from '../lib/i18n';
 import {externalLinkClick} from '../lib/external-links';
 import {renderMarkdown,splitFrontmatter} from '../lib/markdown';
 import {stripDocsScreenshots} from '../lib/markdown-source';
 import {onlinePlatforms,onlineSchedules,type OnlineList,type OnlinePage,type OnlinePlatform,type OnlineReport,type OnlineResource,type OnlineSchedule,type OnlineSource} from '../lib/online-import';
 export let platform:OnlinePlatform;
 export let onClose:()=>void;
 export let onImported:()=>Promise<void>;
 export let onOpenImportedFolder:((path:string)=>Promise<void>)|undefined=undefined;
 export let onNotionZip:()=>void;

 const guides=import.meta.glob<string>('../../docs-site/content/docs/data-source-*.mdx',{eager:true,query:'?raw',import:'default'});
 const usesAccountLogin=platform==='feishu'||platform==='lark';
 const github=platform==='github';
 const gitlab=platform==='gitlab';
 const repository=github||gitlab;
 const dingtalk=platform==='dingtalk';
 const ima=platform==='ima';
 const apiPlatform=platform==='api';
 const usesAddressOnly=platform==='rss'||repository;
 const desktopAccount=usesAccountLogin&&!web;
 const webAccount=usesAccountLogin&&web;
 const name=onlinePlatforms[platform];
 $: rssExample=$language.startsWith('zh-')?'https://zleap.com/blog/rss.xml':'https://zleap.com/en/blog/rss.xml';
 const accountCopy=platform==='lark'?{
  authorize:'使用 Lark 账号授权',reauthorize:'重新授权 Lark 账号',authorized:'Lark 账号已授权',
  waiting:'请在浏览器中完成 Lark 授权',desktop:'请在桌面端配置 Lark 账号授权。',
  drive:'使用 Lark 账号授权，选择你有权限访问的云盘文件夹。',
  wiki:'使用 Lark 账号授权，选择你有权限访问的知识库或页面。',
 }:{
  authorize:'使用飞书账号授权',reauthorize:'重新授权飞书账号',authorized:'飞书账号已授权',
  waiting:'请在浏览器中完成飞书授权',desktop:'请在桌面端配置飞书账号授权。',
  drive:'使用飞书账号授权，选择你有权限访问的云盘文件夹。',
  wiki:'使用飞书账号授权，选择你有权限访问的知识库或页面。',
 };
 const statusNames:Record<string,string>={running:'正在同步',success:'同步完成',failed:'同步失败',partial:'部分完成',canceled:'已取消',interrupted:'已中断'};
 const help:Record<Exclude<OnlinePlatform,'feishu'|'lark'>,{text:string;url:string}>={
  notion:{text:'创建 Notion 内部集成，开启读取内容，并在需要导入的页面中添加该连接。',url:'https://www.notion.so/my-integrations'},
  yuque:{text:'在语雀的账号设置中创建 API Token，确保该账号可以读取要导入的知识库。',url:'https://www.yuque.com/settings/tokens'},
  github:{text:'公开仓库可直接连接。私有仓库需要拥有该仓库 Contents 读取权限的令牌。',url:''},
  gitlab:{text:'公开项目可直接连接。私有项目需要有 read_api 权限的访问令牌。',url:''},
  dingtalk:{text:'使用企业内部应用凭据和有权访问目标知识库的操作人 Union ID。',url:''},
  ima:{text:'在腾讯 ima 开放接口中获取 Client ID 和 API Key。',url:'https://ima.qq.com/agent-interface'},
  api:{text:'',url:''},
  rss:{text:'填入站点提供的 RSS 或 Atom 订阅地址。',url:''},
 };
 let list:OnlineList={sources:[],documents:null},draft:OnlineSource|null=null,secret='',loading=true,busy=false,error='';
 let apiReady=false,webRoots:Root[]=[];
 let tested=false,page:OnlinePage|null=null,parents:OnlineResource[]=[],selected:Record<string,string>={};
 const browsedPages=new Map<string|undefined,OnlinePage>();
 let history:OnlineReport[]|null=null,historyName='',removeId:string|null=null;
 let guideOpen=false,guideArticle:HTMLElement|null=null,guideButton:HTMLButtonElement|null=null,guideCopyError='';
 let seenFinished=new Map<string,number>();
 let firstImportId='';
 let refreshing=false;
 let disposed=false;
 let loginSession='',authorizing=false,enterSecret=false,loginTimer:ReturnType<typeof setTimeout>|undefined;
 $: accountAuthorized=!!draft?.connection.user_access;
 $: connectionName=draft?.name.trim()||(desktopAccount?$t(name):'');
 $: selectionCount=Object.keys(selected).length;
 $: canBrowse=!!draft
  && (!(usesAddressOnly||dingtalk)||!!draft.connection.address.trim())
  && (!(dingtalk||ima)||!!draft.connection.app_id.trim())
  && (usesAddressOnly||!!draft.id||!!secret.trim());
 $: saveConnectionOnly=desktopAccount&&accountAuthorized&&!selectionCount;
 $: guideHtml=guideOpen?renderGuide($language==='zh-CN'?'':'.en',$t('复制权限')):'';
 function renderGuide(suffix:string,copyLabel:string){
  const html=renderMarkdown([platform,'scheduled-sync'].map(slug=>{
   const source=guides[`../../docs-site/content/docs/data-source-${slug}${suffix}.mdx`];
   const {prefix,body}=splitFrontmatter(source);
   const title=prefix.match(/^title: "(.+)"$/m)![1];
   const content=stripDocsScreenshots(body).trim().replace(/^## (.+?)(?: \[#[\w-]+\])?$/gm,'#### $1');
   return `\n\n### ${title}\n\n${content}`;
  }).join(''));
  if(!usesAccountLogin)return html;
  const guide=new DOMParser().parseFromString(html,'text/html');
  for(const code of guide.querySelectorAll('pre > code.language-json')){
   const pre=code.parentElement!;
   const block=guide.createElement('div');
   block.className='guide-code';
   const button=guide.createElement('button');
   button.type='button';
   button.className='outline';
   button.dataset.copyPermissions='';
   button.textContent=copyLabel;
   pre.replaceWith(block);
   block.append(button,pre);
  }
  return guide.body.innerHTML;
 }
 async function onGuideClick(event:MouseEvent){
  if(!(event.target instanceof HTMLButtonElement)||!event.target.hasAttribute('data-copy-permissions')){
   externalLinkClick(event);
   return;
  }
  const button=event.target;
  const code=button.parentElement!.querySelector('pre > code')!;
  guideCopyError='';
  try{
   await copyText(code.textContent!.trim());
   button.textContent=$t('已复制');
  }catch(error){guideCopyError=$t('复制失败：{value0}',{value0:String(error)});}
 }
 async function showGuide(open:boolean){
  guideOpen=open;
  guideCopyError='';
  await tick();
  if(open)guideArticle?.focus();else guideButton?.focus();
 }
 function close(){if(busy)return;if(guideOpen)void showGuide(false);else onClose();}
 function call<T>(action:Record<string,unknown>){return api<T>('online_import',{action});}
 function cancelLogin(){
  clearTimeout(loginTimer);
  const session=loginSession;
  loginSession='';authorizing=false;
  if(session)void call({action:'cancel_account_login',session}).catch(e=>{if(!disposed)error=String(e);});
 }
 async function checkLogin(session:string){
  try{
   const result=await call<{ready:boolean}>({action:'account_login_status',session});
   if(disposed||loginSession!==session||!draft)return;
   if(!result.ready){loginTimer=setTimeout(()=>void checkLogin(session),800);return;}
   authorizing=false;secret='';enterSecret=false;
   draft={...draft,connection:{...draft.connection,user_access:true,address:''}};
   parents=[];
   await browse();
  }catch(e){if(!disposed&&loginSession===session){cancelLogin();error=String(e);}}
 }
 async function login(){
  if(!draft)return;
  if(accountAuthorized&&!enterSecret){enterSecret=true;return;}
  if(loginSession)draft={...draft,connection:{...draft.connection,user_access:false}};
  cancelLogin();resetConnection(true);
  busy=true;error='';
  try{
   const {session,url}=await call<{session:string;url:string}>({action:'account_login',platform,app_id:draft.connection.app_id,app_secret:secret,drive:draft.connection.drive});
   if(disposed){await call({action:'cancel_account_login',session});return;}
   loginSession=session;authorizing=true;
   await api('open_external_link',{url});
   void checkLogin(session);
  }catch(e){cancelLogin();error=String(e);}
  finally{busy=false;}
 }
 function blank():OnlineSource{
  return {
   id:'',name:desktopAccount?$t(name):'',connection:{platform,drive:false,app_id:'',address:''},
   resources:[],directory:'',schedule:'manual',daily_time:'09:00',paused:false,
   ...(apiPlatform?{api:{request:{method:'GET',url:'',query:[],headers:[],secret_header:'',body:null,allow_lan:false},records_path:'',fields:[],dedup_field:null,cleanup:null}}:{})
  };
 }
 function rssAddressKeydown(event:KeyboardEvent){
  if(!draft||draft.connection.address.trim()||event.isComposing||event.altKey||event.ctrlKey||event.metaKey)return;
  if(event.key==='Enter')event.preventDefault();
  else if(event.key!=='Tab'||event.shiftKey)return;
  draft.connection.address=rssExample;
  resetConnection(true);
 }
 async function refresh(initial=false){
  if(refreshing)return;
  refreshing=true;
  try{
   const value=await call<OnlineList>({action:'list',platform});if(disposed)return;
   if(initial&&apiPlatform&&web)webRoots=await api<Root[]>('list_roots');
   const firstImport=value.sources.find(({source})=>source.id===firstImportId);
   if(firstImport?.last?.finished){
    firstImportId='';
    const result=firstImport.last.result;
    if(onOpenImportedFolder&&result.status==='success'&&(result.imported||result.updated)){
     list=value;await onOpenImportedFolder(firstImport.source.directory);return;
    }
   }
   if(!initial&&value.sources.some(({source,last})=>last?.finished&&seenFinished.get(source.id)!==last.finished&&(last.result.imported||last.result.updated))) await onImported();
   list=value;seenFinished=new Map(value.sources.flatMap(({source,last})=>last?.finished?[[source.id,last.finished]]:[]));
   if(initial&&!list.sources.length&&platform!=='notion')edit(blank());
  }catch(e){if(!disposed)error=String(e);}finally{loading=false;refreshing=false;}
 }
 onMount(()=>{void refresh(true);const timer=setInterval(()=>{if(!busy&&!draft&&!history)void refresh();},1500);return()=>{disposed=true;clearInterval(timer);cancelLogin();};});
 function edit(source:OnlineSource){cancelLogin();resetConnection();apiReady=false;draft=structuredClone(source);secret='';enterSecret=false;tested=!!source.id;selected=Object.fromEntries(source.resources.map(id=>[id,id]));error='';}
 function resetConnection(clear=false){tested=false;page=null;parents=[];browsedPages.clear();if(clear)selected={};}
 function selectImportType(drive:boolean){cancelLogin();enterSecret=false;if(draft)draft={...draft,connection:{...draft.connection,drive,user_access:false,address:''}};resetConnection(true);}
 function resetApp(){
  cancelLogin();
  if(draft?.connection.user_access)draft={...draft,connection:{...draft.connection,user_access:false}};
  enterSecret=false;resetConnection();
 }
 function selectSchedule(value:string){if(draft&&Object.hasOwn(onlineSchedules,value))draft={...draft,schedule:value as OnlineSchedule};}
 function toggle(item:OnlineResource,checked:boolean){
  if(checked){
   if(github){
    const tree='github:tree:';
    const blob='github:blob:';
    const scope=item.id.slice(tree.length);
    const childPrefix=scope.indexOf(':')===scope.length-1?scope:scope+'/';
    selected=Object.fromEntries(Object.entries(selected).filter(([id])=>
     (id.startsWith(tree)||id.startsWith(blob))
      && !parents.some(parent=>parent.id===id)
      && !(item.id.startsWith(tree)&&id.slice(tree.length).startsWith(childPrefix))
    ));
   }
   selected={...selected,[item.id]:item.title};
  }else {const next={...selected};delete next[item.id];selected=next;}
 }
 function normalizeGithubAddress(){
  if(github&&draft)draft.connection.address=draft.connection.address.split('#',1)[0].trim();
 }
 async function browse(target=parents,append=false){
  if(!draft)return;
  normalizeGithubAddress();
  error='';
  const parent=target.at(-1)?.id;
  const cached=!append&&browsedPages.get(parent);
  if(cached){parents=target;page=cached;return;}
  busy=true;
  try{
   const result=await call<OnlinePage>({action:'browse',id:draft.id||null,connection:draft.connection,secret:desktopAccount?null:secret||null,parent:parent??null,cursor:append?page?.cursor:null,...(loginSession?{auth_session:loginSession}:{})});
   if(append&&result.cursor&&result.cursor===page?.cursor)throw new Error($t('数据源分页返回了重复内容'));
   page=append&&page?{items:[...page.items,...result.items.filter(item=>!page!.items.some(existing=>existing.id===item.id))],cursor:result.cursor}:result;
   parents=target;tested=true;
   if(desktopAccount)browsedPages.set(parent,page);
   if(platform==='rss'&&result.items[0]){const feed=result.items[0];selected={[feed.id]:feed.title};if(!draft.name.trim())draft={...draft,name:feed.title};}
   if(repository&&!target.length&&!draft.name.trim()){
    const parts=draft.connection.address.trim().split('/').filter(Boolean);
    draft={...draft,name:parts.slice(github?-2:-1).join('/').replace(/\.git$/, '')};
   }
  }catch(e){error=String(e);}
  finally{busy=false;}
 }
 function enter(resource:OnlineResource){void browse([...parents,resource]);}
 function up(){void browse(parents.slice(0,-1));}
 async function pickDirectory(){
  if(!draft)return;
  try{const picked=await open({directory:true,multiple:false});if(typeof picked==='string')draft={...draft,directory:picked};}catch(e){error=String(e);}
 }
 async function save(){
  if(!draft)return;
  const resources=Object.keys(selected);
  const firstImport=!draft.resources.length&&(apiPlatform||resources.length>0);
  busy=true;error='';
  try{
   const saved=await call<OnlineSource>({action:'save',source:{...draft,name:connectionName,resources,...(draft.api?{api:JSON.parse(JSON.stringify(draft.api))}:{})},secret:desktopAccount?null:secret||null,...(loginSession?{auth_session:loginSession}:{})});
   if(firstImport&&onOpenImportedFolder)firstImportId=saved.id;
   loginSession='';draft=null;secret='';await refresh();
  }
  catch(e){error=String(e);}finally{busy=false;}
 }
 async function operate(action:Record<string,unknown>){busy=true;error='';try{await call(action);removeId=null;await refresh();}catch(e){error=String(e);}finally{busy=false;}}
 async function showHistory(source:OnlineSource){busy=true;error='';try{history=await call<OnlineReport[]>({action:'history',id:source.id});historyName=source.name;}catch(e){error=String(e);}finally{busy=false;}}
 function date(value:number|null){return value?new Intl.DateTimeFormat($language,{dateStyle:'short',timeStyle:'short'}).format(value*1000):'—';}
</script>

<Modal label={guideOpen?$t('配置教程'):$t('配置 {value0}',{value0:$t(name)})} class="online-import" onClose={close}>
 <header><div><div class="online-title"><h2>{guideOpen?$t('配置教程'):$t(name)}</h2>{#if !guideOpen}<Button.Root bind:ref={guideButton} type="button" class="text-button online-guide-button" disabled={busy} onclick={()=>showGuide(true)}>{@html icon('help')}{$t('配置教程')}</Button.Root>{/if}</div>{#if history&&!guideOpen}<span>{historyName} · {$t('同步记录')}</span>{/if}</div><Button.Root class="icon-button" aria-label={$t('关闭')} disabled={busy} onclick={close}>{@html icon('close')}</Button.Root></header>
 <div class="online-body">
  {#if guideOpen}
   <article class="online-guide" bind:this={guideArticle} tabindex="-1" lang={$language==='zh-CN'?'zh-CN':'en'}>
    <div role="presentation" onclick={onGuideClick}>{@html guideHtml}</div>
    {#if guideCopyError}<p class="error" role="alert">{guideCopyError}</p>{/if}
   </article>
  {:else if loading}<p role="status">{$t('正在读取…')}</p>
  {:else if history}
   {#if !history.length}<p>{$t('暂无同步记录')}</p>{/if}
   {#each history as row}
    <section class="connection-card"><strong>{$t(statusNames[row.result.status]??row.result.status)}</strong><span class="secondary">{date(row.started)}</span>
     <p class="secondary">{$t('新增 {value0} · 更新 {value1} · 跳过 {value2} · 冲突 {value3} · 失败 {value4}',{value0:row.result.imported,value1:row.result.updated,value2:row.result.skipped,value3:row.result.conflicts,value4:row.result.failed})}</p>
     {#if row.result.issues.length}<ul>{#each row.result.issues as issue}<li>{issue.title ? `${issue.title}：` : ''}{$t(issue.reason)}</li>{/each}</ul>{/if}
    </section>
   {/each}
  {:else if draft}
   <fieldset disabled={busy} class:api-form={!!draft.api}>
    {#if draft.api}
     <ApiSourceFields bind:config={draft.api} bind:connectionName={draft.name} id={draft.id} bind:secret bind:busy bind:error bind:valid={apiReady} />
    {:else}
    <legend>{$t('连接与范围')}</legend>
    <label>{$t('连接名称')}<input bind:value={draft.name} placeholder={$t(name)} maxlength="100" /></label>
    {#if usesAccountLogin}
     <label>{$t('导入类型')}<select value={draft.connection.drive?'drive':'wiki'} disabled={!!draft.id} onchange={event=>selectImportType(event.currentTarget.value==='drive')}><option value="wiki">{$t('知识库')}</option><option value="drive">{$t('云盘')}</option></select></label>
     {#if !webAccount}<label>App ID<input bind:value={draft.connection.app_id} oninput={resetApp} autocomplete="off" /></label>{/if}
    {/if}
    {#if repository}<label>{$t(github?'GitHub 仓库链接':'GitLab 项目链接')}<input bind:value={draft.connection.address} disabled={!!draft.id} oninput={()=>resetConnection(true)} onblur={normalizeGithubAddress} placeholder={github?'https://github.com/owner/repo':'https://gitlab.com/group/project'} autocomplete="off" /></label>{/if}
    {#if dingtalk||ima}<label>Client ID<input bind:value={draft.connection.app_id} oninput={()=>resetConnection(true)} autocomplete="off" /></label>{/if}
    {#if platform!=='rss'&&!webAccount&&(!desktopAccount||!accountAuthorized||enterSecret)}<label>{usesAccountLogin?'App Secret':repository?$t('访问令牌（选填）'):dingtalk?'Client Secret':ima?'API Key':'API Token'}<input type="password" bind:value={secret} oninput={()=>{if(!desktopAccount)resetConnection();}} placeholder={draft.id&&!desktopAccount?$t('留空保留已有凭据'):''} autocomplete="off" /></label>{/if}
    {#if dingtalk}<label>{$t('操作人 Union ID')}<input bind:value={draft.connection.address} oninput={()=>resetConnection(true)} autocomplete="off" /></label>{/if}
    {#if desktopAccount}
     {#if !accountAuthorized&&!authorizing}<p class="secondary">{$t(draft.connection.drive?accountCopy.drive:accountCopy.wiki)}</p>{/if}
     <div class="actions">
      {#if authorizing}<span class="secondary" role="status">{$t(accountCopy.waiting)}</span><Button.Root class="text-button" onclick={cancelLogin}>{$t('取消')}</Button.Root>
      {:else}
       {#if accountAuthorized}<span class="secondary" role="status">{$t(accountCopy.authorized)}</span>{/if}
       <Button.Root class="outline" disabled={(!accountAuthorized||enterSecret)&&(!draft.connection.app_id.trim()||!secret.trim())} onclick={login}>{$t(accountAuthorized&&!enterSecret?accountCopy.reauthorize:accountCopy.authorize)}</Button.Root>
       {#if accountAuthorized}<Button.Root class="text-button" onclick={()=>browse([])}>{$t(draft.connection.drive?'浏览云盘文件夹':'浏览知识库')}</Button.Root>{/if}
      {/if}
     </div>
    {:else if webAccount}<p class="secondary">{$t(accountCopy.desktop)}</p>{/if}
    {#if (draft.connection.drive&&!webAccount&&(!desktopAccount||accountAuthorized))||platform==='rss'}<label>{$t(draft.connection.drive?'云盘文件夹链接或标识':'订阅地址')}<input bind:value={draft.connection.address} oninput={()=>resetConnection(!desktopAccount)} onkeydown={platform==='rss'?rssAddressKeydown:undefined} placeholder={platform==='rss'?rssExample:'https://…/drive/folder/…'} autocomplete="off" /></label>{/if}
    {#if platform!=='feishu'&&platform!=='lark'}
    <p class="secondary">{$t(help[platform].text)} {#if help[platform].url}<Button.Root class="text-button" onclick={()=>{void api('open_external_link',{url:help[platform].url}).catch(e=>error=String(e));}}>{$t('打开平台设置')}</Button.Root>{/if}</p>
    <Button.Root class="outline" disabled={!canBrowse} onclick={()=>browse([])}>{$t(repository?'浏览分支':'测试连接')}</Button.Root>
    {/if}
    {#if page}
     <div class="resources">
      <div class="resource-heading"><strong>{$t(repository&&!parents.length?'分支':'导入范围')}</strong>{#if parents.length}<Button.Root class="text-button" onclick={up}>{$t('返回上层')}</Button.Root>{/if}</div>
      {#if parents.length}<p class="secondary">{parents.map(parent=>parent.title).join(' / ')}</p>{/if}
      {#if github&&parents.length===1}
       <div class="resource-row"><label><input type="checkbox" checked={Object.hasOwn(selected,parents[0].id)} onchange={event=>toggle(parents[0],event.currentTarget.checked)} />{$t('整个分支')}</label></div>
      {/if}
      {#each page.items as item (item.id)}
       <div class="resource-row">
        {#if github&&!parents.length}
         <Button.Root class="text-button github-branch-open" aria-label={$t('打开 {value0}',{value0:item.title})} onclick={()=>enter(item)}><span>{item.title}</span>{@html icon('chevron')}</Button.Root>
        {:else}
         {#if !item.id.startsWith('group:')}
          <label><input type="checkbox" checked={Object.hasOwn(selected,item.id)} onchange={event=>toggle(item,event.currentTarget.checked)} />{item.title}</label>
         {:else}<span>{item.title}</span>{/if}
         {#if item.expandable}<Button.Root class="text-button" aria-label={$t('打开 {value0}',{value0:item.title})} onclick={()=>enter(item)}>{@html icon('chevron')}</Button.Root>{/if}
        {/if}
       </div>
      {:else}<p class="secondary">{$t('没有可读取的资源')}</p>{/each}
      {#if page.cursor}<Button.Root class="text-button" onclick={()=>browse(parents,true)}>{$t('加载更多')}</Button.Root>{/if}
     </div>
    {/if}
    {#if selectionCount}<p class="secondary">{$t('已选 {value0} 项',{value0:selectionCount})}</p>{/if}
    {/if}
    {#if draft.api}
     <fieldset>
      <legend>{$t('保存位置')}</legend>
      {#if web&&!draft.id}
       <select aria-label={$t('保存位置')} bind:value={draft.directory}>
        <option value="">{`${list.documents??''}/sag-wiki/data-sources/api/`}</option>
        {#each webRoots as root (root.path)}<option value={root.path}>{root.path}</option>{/each}
       </select>
      {:else}
       <div class="destination"><span class="secondary">{draft.directory||`${list.documents??''}/sag-wiki/data-sources/api/`}</span>{#if !draft.id&&!web}<Button.Root class="text-button" onclick={pickDirectory}>{$t('更改')}</Button.Root>{/if}</div>
      {/if}
     </fieldset>
    {:else}
    <div class="destination"><div><strong>{$t('保存位置')}</strong><span class="secondary">{draft.directory||`${list.documents??''}/sag-wiki/data-sources/${platform}/`}</span></div>{#if !draft.id&&!web}<Button.Root class="text-button" onclick={pickDirectory}>{$t('更改')}</Button.Root>{/if}</div>
    {/if}
   </fieldset>
   <fieldset disabled={busy}>
    <legend>{$t('同步计划')}</legend>
    <div class="schedule"><select aria-label={$t('同步频率')} value={draft.schedule} onchange={event=>selectSchedule(event.currentTarget.value)}>
    {#each Object.entries(onlineSchedules) as [value,label]}<option {value}>{$t(label)}</option>{/each}
    </select>
    {#if draft.schedule==='daily'}<input aria-label={$t('每日同步时间')} type="time" bind:value={draft.daily_time} required />{/if}</div>
    {#if draft.schedule!=='manual'}<label class="checkbox-row"><input type="checkbox" bind:checked={draft.paused} />{$t('暂停定时同步')}</label><p class="secondary">{$t('后台服务运行时按本机时间同步。')}</p>{/if}
   </fieldset>
  {:else}
   {#if platform==='notion'&&!web}<div class="notion-zip"><Button.Root class="outline" onclick={onNotionZip}>{$t('从导出文件导入')}</Button.Root></div>{/if}
   {#if !list.sources.length}<p class="secondary">{$t('尚未添加在线连接')}</p>{/if}
   {#each list.sources as row (row.source.id)}
     {@const actionNeeded=row.last?.result.issues.find(issue=>['请重新授权飞书账号','请重新授权 Lark 账号','飞书本月的 API 调用额度已用完。','Lark 本月的 API 调用额度已用完。'].includes(issue.reason))}
    <section class="connection-card">
     <div class="connection-title"><strong>{row.source.name}</strong>{#if usesAccountLogin}<span class="secondary">{$t(row.source.connection.drive?'云盘':'知识库')}</span>{/if}</div>
     <p class="secondary path" title={row.source.directory}>{row.source.directory}</p>
     <p class="secondary" role={row.running?'status':undefined}>{$t(row.source.resources.length?(row.last?statusNames[row.last.result.status]??row.last.result.status:'尚未同步'):(row.source.connection.drive?'未选择文件夹':'未选择知识库或页面'))}{#if row.source.resources.length&&row.last?.finished} · {date(row.last.finished)}{/if}{#if row.next_at} · {$t('下次同步')} {date(row.next_at)}{/if}</p>
     {#if actionNeeded}<p class="secondary">{$t(actionNeeded.reason)}</p>{/if}
     {#if removeId===row.source.id}<p>{$t('移除连接后，本地文件会保留。')}</p><div class="actions"><Button.Root class="outline" disabled={busy} onclick={()=>removeId=null}>{$t('取消')}</Button.Root><Button.Root class="outline" disabled={busy} onclick={()=>operate({action:'remove',id:row.source.id})}>{$t('移除连接')}</Button.Root></div>
     {:else}<div class="actions">
      {#if row.running}<Button.Root class="outline" disabled={busy} onclick={()=>operate({action:'cancel',id:row.source.id})}>{$t('停止同步')}</Button.Root>
      {:else}
       {#if row.source.resources.length}<Button.Root class="outline" disabled={busy} onclick={()=>operate({action:'run',id:row.source.id,retry:false})}>{$t('立即同步')}</Button.Root>{/if}
       <Button.Root class="text-button" disabled={busy} onclick={()=>edit(row.source)}>{$t('配置')}</Button.Root>
       {#if row.source.resources.length&&(row.last?.result.failed||row.pending)}<Button.Root class="text-button" disabled={busy} onclick={()=>operate({action:'run',id:row.source.id,retry:true})}>{$t('重试失败项')}</Button.Root>{/if}
       <Button.Root class="text-button" disabled={busy} onclick={()=>removeId=row.source.id}>{$t('移除连接')}</Button.Root>
      {/if}
      {#if row.last}<Button.Root class="text-button" disabled={busy} onclick={()=>showHistory(row.source)}>{$t('同步记录')}</Button.Root>{/if}
     </div>{/if}
    </section>
   {/each}
  {/if}
  {#if !guideOpen&&error}<p class="error" role="alert">{$t(error.replace(/^Error: /,''))}</p>{/if}
  {#if !guideOpen&&busy}<p class="secondary" role="status">{$t('正在处理…')}</p>{/if}
 </div>
 <footer>
  {#if guideOpen}<Button.Root class="outline" onclick={()=>showGuide(false)}>{$t('返回')}</Button.Root>
  {:else if history}<Button.Root class="outline" onclick={()=>history=null}>{$t('返回')}</Button.Root>
  {:else if draft}
   <Button.Root class="outline" disabled={busy} onclick={()=>{cancelLogin();draft=null;secret='';error='';}}>{$t('取消')}</Button.Root>
   <Button.Root class="primary" disabled={busy||authorizing||(apiPlatform?!apiReady:(!saveConnectionOnly&&(!tested||!selectionCount)))||!connectionName||(draft.schedule==='daily'&&!draft.daily_time)||webAccount||(desktopAccount&&!accountAuthorized)} onclick={save}>{$t(saveConnectionOnly?'保存连接':draft.resources.length?'保存':'保存并导入')}</Button.Root>
  {:else}<Button.Root class="outline" disabled={busy||loading} onclick={onClose}>{$t('完成')}</Button.Root><Button.Root class="primary" disabled={busy||loading} onclick={()=>edit(blank())}>{$t('添加连接')}</Button.Root>{/if}
 </footer>
</Modal>

<style>
 :global(.dialog-panel.online-import){width:590px;max-width:calc(100vw - 32px);max-height:calc(100dvh - 48px);display:flex;flex-direction:column;overflow:hidden;background:var(--color-paper)}
 header,footer{padding:16px 20px;display:flex;align-items:center;gap:10px}
 header{justify-content:space-between;border-bottom:1px solid var(--color-rule)}
 h2{margin:0;font-size:18px;font-weight:600}
 .online-title{display:flex;align-items:center;flex-wrap:wrap;gap:8px}
 .online-title :global(.online-guide-button){display:inline-flex;align-items:center;gap:5px;color:var(--color-muted);font-size:13px}
 .online-title :global(.online-guide-button svg){width:14px;height:14px}
 .online-guide{font-size:14px;line-height:1.7;overflow-wrap:anywhere;outline:none}
 .online-guide :global(h3){font-size:16px;margin:0 0 9px}
 .online-guide :global(h3:not(:first-child)){margin-top:20px}
 .online-guide :global(h4){font-size:14px;margin:14px 0 8px}
 .online-guide :global(ol){padding-inline-start:22px;margin:0 0 10px}
 .online-guide :global(li){padding-inline-start:2px;margin-bottom:7px}
 .online-guide :global(a){color:var(--color-ink);text-decoration:underline;text-underline-offset:3px}
 .online-guide :global(.guide-code){display:grid;justify-items:end;gap:6px;margin:8px 0 12px;padding:8px 10px;background:var(--color-panel);border:1px solid var(--color-rule);border-radius:8px}
 .online-guide :global(.guide-code pre){width:100%;margin:0;overflow:auto;font-size:13px}
 .online-guide :global(.guide-code button){font-size:12px;padding:4px 8px;min-height:26px}
 header span,.secondary{font-size:13px;color:var(--color-muted)}
 footer{justify-content:flex-end;border-top:1px solid var(--color-rule)}
 .online-body{min-height:100px;overflow:auto;padding:18px 20px;display:grid;align-content:start;gap:12px}
 fieldset,.api-form :global(.api-section){margin:0;padding:12px 14px 14px;display:grid;gap:10px;border:1px solid var(--color-rule);border-radius:9px;min-width:0}
 fieldset.api-form{border:0;padding:0;gap:12px}
 legend,.api-form :global(.api-section>legend){font-weight:600;padding:0 5px}
 label{display:grid;gap:5px;font-size:13px}
 input:not([type=checkbox]),select{min-width:0;color:var(--color-ink);background:var(--color-paper);font:inherit;border:1px solid var(--color-rule);border-radius:6px;padding:7px 9px}
 .schedule,.actions,.connection-title,.destination,.resource-heading,.resource-row{display:flex;align-items:center;gap:8px}
 .actions{flex-wrap:wrap;margin-top:6px}
 .schedule select{min-width:120px}
 .checkbox-row,.resource-row label{display:flex;align-items:center;gap:7px}
 .resource-row label{flex:1;overflow-wrap:anywhere}
 .resources{border:1px solid var(--color-rule);border-radius:6px;padding:8px 10px;display:grid;gap:5px;max-height:240px;overflow:auto}
 .resource-heading,.resource-row,.destination{justify-content:space-between}
 .resource-row{min-height:30px;border-top:1px solid var(--color-rule)}
 .resource-row :global(.github-branch-open){display:flex;width:100%;justify-content:space-between;align-items:center;gap:8px;text-align:left}
 .resource-row :global(.github-branch-open span){min-width:0;overflow-wrap:anywhere}
 .destination>div{display:grid;gap:4px;min-width:0}
 .path,.destination span{overflow-wrap:anywhere}
 .connection-card{padding:11px 13px;border:1px solid var(--color-rule);border-radius:8px;display:grid;gap:4px}
 p{margin:0}
 .error{color:var(--color-danger,#ba3838);overflow-wrap:anywhere}
 ul{margin:6px 0 0;padding-inline-start:20px;font-size:13px;overflow-wrap:anywhere}
</style>
