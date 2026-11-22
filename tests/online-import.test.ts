// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import DataImport from '../src/components/DataImport.svelte';
import {setLanguage} from '../src/lib/i18n';
import type {OnlineSource} from '../src/lib/online-import';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let mounted:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(mounted)await unmount(mounted);mounted=undefined;document.body.innerHTML='';setLanguage('zh-CN');vi.restoreAllMocks();vi.resetAllMocks();});

function button(label:string){
 return [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.getAttribute('aria-label')===label||button.textContent?.trim()===label)!;
}
function input(label:string){
 return [...document.querySelectorAll('label')].find(element=>element.textContent?.trim()===label)?.querySelector('input')!;
}
async function fill(field:HTMLInputElement,value:string){field.value=value;field.dispatchEvent(new Event('input',{bubbles:true}));await tick();}

it('accepts the Apify example with Enter or Tab only when the field is blank',async()=>{
 invoke.mockImplementation(async(_command,{action})=>action.action==='list'
  ?{sources:[],documents:'/Documents'}
  :{id:'custom',slug:'example/custom',title:'Custom',preset:false,schema:null});
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(input('Store 链接或 Actor ID')).toBeDefined());
 expect(button('添加连接')).toBeUndefined();
 expect(button('使用官方网页爬虫')).toBeUndefined();
 button('配置教程').click();
 const guide=()=>document.querySelector('.apify-import .guide');
 await vi.waitFor(()=>expect(guide()?.textContent).toContain('首次配置会直接打开新增表单'));
 expect(guide()?.textContent).not.toContain('填写 Token 和网站起始地址');
 button('返回').click();
 await vi.waitFor(()=>expect(input('Store 链接或 Actor ID')).toBeDefined());
 const reference=input('Store 链接或 Actor ID');
 reference.focus();
 expect(reference.value).toBe('');
 expect(reference.placeholder).toBe('apify/website-content-crawler');
 reference.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true}));
 await tick();
 expect(reference.value).toBe('');
 for(const key of ['Enter','Tab']){
  await fill(reference,'');
  reference.focus();
  const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});
  reference.dispatchEvent(event);
  await tick();
  expect(reference.value).toBe(reference.placeholder);
  expect(event.defaultPrevented).toBe(key==='Enter');
 }
 expect(invoke.mock.calls.some(([,args])=>args.action.request?.action==='describe')).toBe(false);
 await fill(reference,'example/custom');
 reference.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true}));
 await tick();
 expect(reference.value).toBe('example/custom');
 reference.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({request:{action:'describe',actor:'example/custom'}})}));
});

it('browses popular Apify Actors, pages and searches, then loads the chosen Actor by ID',async()=>{
 const item=(id:string,title:string)=>({id,title,username:'example',name:id,description:'Public records'});
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='list')return {sources:[],documents:'/Documents'};
  switch(action.request.action){
   case 'store':{
    const {search,offset}=action.request;
    const actor=search?item('matching123','Matching Actor'):offset===0?item('popular123','Popular Actor'):item('second123','Second Page Actor');
    return {items:[actor],offset,limit:20,next:search||offset!==0?null:20};
   }
   case 'describe':return {id:'matching123',slug:'example/records',title:'Matching Actor',preset:false,schema:{properties:{query:{type:'string',title:'Query'}}}};
   default:throw new Error('Unexpected Apify action');
  }
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(button('查看 Actor')).toBeDefined());
 button('查看 Actor').click();
 await vi.waitFor(()=>expect(document.querySelector('.store-actor strong')?.textContent).toBe('Popular Actor'));
 expect(button('上一页').disabled).toBe(true);
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'apify',token:'',connection:null,request:{action:'store',search:'',offset:0}}});
 button('下一页').click();
 await vi.waitFor(()=>expect(document.querySelector('.store-actor strong')?.textContent).toBe('Second Page Actor'));
 expect(button('下一页').disabled).toBe(true);
 button('上一页').click();
 await vi.waitFor(()=>expect(document.querySelector('.store-actor strong')?.textContent).toBe('Popular Actor'));
 await fill(document.querySelector<HTMLInputElement>('input[aria-label="搜索 Actor"]')!,'records');
 document.querySelector('form.store-search')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.store-actor strong')?.textContent).toBe('Matching Actor'));
 expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({request:{action:'store',search:'records',offset:0}})});
 document.querySelector<HTMLButtonElement>('.store-actor')!.click();
 await vi.waitFor(()=>expect(input('Query')).toBeDefined());
 expect(input('Store 链接或 Actor ID').value).toBe('matching123');
 expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({request:{action:'describe',actor:'matching123'}})});
 expect(invoke.mock.calls.some(([,arg])=>arg.action.request?.action==='start')).toBe(false);
});

it('loads Apify parameters and imports selected fields from the previewed run',async()=>{
 const storeLink='https://apify.com/example/records';
 let finishRun!:()=>void;
 const status=new Promise<{status:string}>(resolve=>{finishRun=()=>resolve({status:'SUCCEEDED'})});
 let savedRequest:Record<string,unknown>|undefined;
 const saved={id:'connection1',name:'Records',directory:'/Documents/sag-wiki/data-sources/apify/records',connection:{app_id:'actor123',address:'example/records'},schedule:'manual',daily_time:'09:00',paused:false};
 let hasMapping=false;
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='list')return {sources:savedRequest?[{source:saved}]:[],documents:'/Documents'};
  switch(action.request.action){
   case 'describe':return {id:'actor123',slug:'example/records',title:'Records',preset:false,schema:{
    properties:{query:{type:'string',title:'Query'},limit:{type:'integer',title:'Limit',default:5}},
   }};
   case 'save_connection':savedRequest=action.request.input;return saved;
   case 'get_connection':return {input:savedRequest,has_mapping:hasMapping};
   case 'start':return {id:'run123'};
   case 'status':return status;
   case 'preview':return {fields:[['/name','name'],['/summary','summary']],samples:['{"name":"Example","summary":"Content"}'],total:26};
   case 'import':
    hasMapping=action.request.offset===20;
    return {directory:'/Documents/sag-wiki/data-sources/apify/records',next:hasMapping?null:20,
     imported:hasMapping?6:20,updated:0,skipped:0,conflicts:0,empty:0};
   default:throw new Error('Unexpected Apify action');
  }
 });
 const onImported=vi.fn();
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false,onImported}});
 await tick();
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(input('Store 链接或 Actor ID')).toBeDefined());
 const paste=new Event('paste',{bubbles:true,cancelable:true});
 Object.defineProperty(paste,'clipboardData',{value:{getData:()=>storeLink}});
 input('Store 链接或 Actor ID').dispatchEvent(paste);
 await vi.waitFor(()=>expect(input('Query')).toBeDefined());
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'apify',token:'',connection:null,request:{action:'describe',actor:storeLink}}});
 await fill(input('Apify API Token'),'fixture-token');
 await fill(input('Query'),'guides');
 expect([...document.querySelectorAll('.apify-import legend')].map(legend=>legend.textContent)).toEqual(['连接与范围','运行参数','保存位置','同步计划']);
 const frequency=()=>document.querySelector<HTMLSelectElement>('select[aria-label="同步频率"]')!;
 expect(frequency().value).toBe('manual');
 expect(frequency().disabled).toBe(true);
 button('保存并运行').click();
 await vi.waitFor(()=>expect(button('取消运行')).toBeDefined());
 expect(button('关闭').disabled).toBe(false);
 button('关闭').click();
 await vi.waitFor(()=>expect(document.querySelector('.apify-import')).toBeNull());
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(document.querySelector('.connection-card')?.textContent).toContain('正在运行 Apify'));
 expect(button('添加连接').disabled).toBe(false);
 expect(button('取消运行').disabled).toBe(false);
 expect(button('立即同步')).toBeUndefined();
 finishRun();
 await vi.waitFor(()=>expect(button('结果预览')).toBeDefined());
 button('结果预览').click();
 await vi.waitFor(()=>expect(document.querySelector('pre')?.textContent).toContain('Example'));
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'apify',token:'',connection:'connection1',request:{action:'start',actor:'actor123',input:{query:'guides',limit:5}}}});
 expect(button('导入').disabled).toBe(true);
 input('summary').click();
 await tick();
 button('导入').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('新增 26'));
 const requests=invoke.mock.calls.map(([,arg])=>arg.action.request).filter(request=>request?.action==='import');
 expect(requests.map(request=>request.offset)).toEqual([0,20]);
 expect(requests[1]).toMatchObject({run:'run123',preset:false,mapping:{title:'/name',content:['/summary']}});
 expect(invoke.mock.calls.filter(([,arg])=>arg.action.request?.action==='start')).toHaveLength(1);
 expect(onImported).toHaveBeenCalledOnce();
 button('完成').click();
 await vi.waitFor(()=>expect(button('立即同步')).toBeDefined());
 button('完成').click();
 await vi.waitFor(()=>expect(document.querySelector('.apify-import')).toBeNull());
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(button('立即同步')).toBeDefined());
 expect(button('添加连接')?.disabled).toBe(false);
 expect(input('Store 链接或 Actor ID')).toBeUndefined();
 expect(document.querySelector('.apify-import .connection-card')?.textContent).toContain(saved.directory);
 document.querySelector<HTMLButtonElement>('.apify-import .connection-card .text-button')!.click();
 await vi.waitFor(()=>expect(input('Query')?.value).toBe('guides'));
 expect(input('Apify API Token').value).toBe('');
 expect(frequency().disabled).toBe(false);
});

it('imports the official Apify crawler Markdown after dismissing the dialog during startup',async()=>{
 let finishStarting!:()=>void;
 const run=new Promise<{id:string}>(resolve=>{finishStarting=()=>resolve({id:'crawler-run'})});
 const saved={id:'crawler',name:'Website Content Crawler',directory:'/Documents/sag-wiki/data-sources/apify/crawler',connection:{app_id:'official',address:'apify/website-content-crawler'},schedule:'manual',daily_time:'09:00',paused:false};
 let configured=false;
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='list')return {sources:configured?[{source:saved}]:[],documents:'/Documents'};
  switch(action.request.action){
   case 'describe':return {id:'official',slug:'apify/website-content-crawler',title:'Website Content Crawler',preset:true,schema:null};
   case 'save_connection':configured=true;return saved;
   case 'start':return run;
   case 'status':return {id:'crawler-run',status:'SUCCEEDED'};
   case 'import':return {directory:'/Documents/sag-wiki/data-sources/apify/crawler',next:null,imported:1,updated:0,skipped:0,conflicts:0,empty:0};
   default:throw new Error('Unexpected Apify action');
  }
 });
 const onImported=vi.fn();
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false,onImported}});
 await tick();
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(input('Store 链接或 Actor ID')).toBeDefined());
 const reference=input('Store 链接或 Actor ID');
 reference.focus();
 reference.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
 await tick();
 button('加载参数').click();
 await vi.waitFor(()=>expect(input('网站起始地址')).toBeDefined());
 await fill(input('Apify API Token'),'fixture-token');
 await fill(input('网站起始地址'),'https://example.org/docs');
 button('保存并导入').click();
 await vi.waitFor(()=>expect(button('后台运行')).toBeDefined());
 button('后台运行').click();
 await vi.waitFor(()=>expect(document.querySelector('.apify-import')).toBeNull());
 finishStarting();
 await vi.waitFor(()=>expect(onImported).toHaveBeenCalledOnce());
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(document.querySelector('.connection-card')?.textContent).toContain('导入完成'));
 expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({
  request:{action:'start',actor:'official',input:expect.objectContaining({
   startUrls:[{url:'https://example.org/docs'}],saveMarkdown:true,maxCrawlDepth:0,maxCrawlPages:1,
  })},
 })});
 expect(invoke.mock.calls.some(([,arg])=>arg.action.request?.action==='preview')).toBe(false);
});

it('cancels a background Apify run from the reopened connection list',async()=>{
 const saved={id:'cancel-job',name:'Records',directory:'/Documents/records',connection:{app_id:'actor123',address:'example/records'},schedule:'manual',daily_time:'09:00',paused:false};
 let confirmCancellation!:()=>void;
 const status=new Promise<{status:string}>(resolve=>{confirmCancellation=()=>resolve({status:'ABORTED'})});
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='list')return {sources:[{source:saved}],documents:'/Documents'};
  switch(action.request.action){
   case 'describe':return {id:'actor123',slug:'example/records',title:'Records',preset:false,schema:null};
   case 'get_connection':return {input:{},has_mapping:false};
   case 'start':return {id:'background-run'};
   case 'status':return status;
   case 'cancel':return {};
   default:throw new Error('Unexpected Apify action');
  }
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(button('立即同步')).toBeDefined());
 button('立即同步').click();
 await vi.waitFor(()=>expect(button('取消运行')).toBeDefined());
 button('后台运行').click();
 await vi.waitFor(()=>expect(document.querySelector('.apify-import')).toBeNull());
 button('配置 Apify').click();
 await vi.waitFor(()=>expect(button('取消运行')).toBeDefined());
 button('取消运行').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({
  connection:'cancel-job',request:{action:'cancel',run:'background-run'},
 })}));
 confirmCancellation();
 await vi.waitFor(()=>expect(document.querySelector('.connection-card')?.textContent).toContain('已取消'));
 expect(button('立即同步').disabled).toBe(false);
});

it('opens Apify from the schedule shortcut and runs manually while the schedule is paused',async()=>{
 let saved={id:'scheduled-crawler',name:'Website Content Crawler',directory:'/Documents/website',
  connection:{app_id:'official',address:'apify/website-content-crawler'},schedule:'manual',daily_time:'09:00',paused:false};
 let running=false;
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='list')return {sources:[{source:saved,running}],documents:'/Documents'};
  if(action.action==='run'){running=true;return true}
  switch(action.request.action){
   case 'describe':return {id:'official',slug:saved.connection.address,title:saved.name,preset:true,schema:null};
   case 'get_connection':return {input:{startUrls:[{url:'https://example.org'}]},has_mapping:false};
   case 'save_connection':
    saved={...saved,schedule:action.request.schedule,daily_time:action.request.daily_time,paused:action.request.paused};
    return saved;
   default:throw new Error('Unexpected Apify action');
  }
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 Apify 定时同步').click();
 await vi.waitFor(()=>expect(button('立即同步')).toBeDefined());
 document.querySelector<HTMLButtonElement>('.apify-import .connection-card .text-button')!.click();
 await vi.waitFor(()=>expect(input('网站起始地址').value).toBe('https://example.org'));
 const frequency=document.querySelector<HTMLSelectElement>('select[aria-label="同步频率"]')!;
 expect(frequency.value).toBe('manual');
 expect([...frequency.options].map(option=>option.textContent)).toEqual(['仅手动','每 30 分钟','每小时','每 6 小时','每 12 小时','每天']);
 frequency.value='daily';frequency.dispatchEvent(new Event('change',{bubbles:true}));
 await tick();
 await fill(document.querySelector<HTMLInputElement>('input[aria-label="每日同步时间"]')!,'15:45');
 input('暂停定时同步').click();await tick();
 button('保存').click();
 await vi.waitFor(()=>expect(button('立即同步')).toBeDefined());
 expect(saved).toMatchObject({schedule:'daily',daily_time:'15:45',paused:true});
 expect(invoke.mock.calls.some(([,arg])=>arg.action.action==='run')).toBe(false);
 button('立即同步').click();
 await vi.waitFor(()=>expect(button('停止同步')).toBeDefined());
 expect(button('立即同步')).toBeUndefined();
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'run',id:saved.id,retry:false}});
});

it.each([
 ['zh-CN','配置 {name}','配置教程','定时同步',['飞书','Lark','Notion','语雀','GitHub','GitLab','钉钉文档','腾讯 ima','RSS / Atom','API 请求']],
 ['en','Configure {name}','Setup guide','Scheduled sync',['Feishu','Lark','Notion','Yuque','GitHub','GitLab','DingTalk Docs','Tencent ima','RSS / Atom','API Request']],
] as const)('opens each provider guide before sync instructions in %s',async(locale,configure,guide,schedule,providers)=>{
 setLanguage(locale);
 invoke.mockResolvedValue({sources:[],documents:'/Documents'});
 const clipboard=vi.spyOn(navigator.clipboard,'writeText').mockResolvedValue(undefined);
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 for(const name of providers){
  button(configure.replace('{name}',name)).click();
  await vi.waitFor(()=>expect(button(guide)).toBeDefined());
  button(guide).click();
  await vi.waitFor(()=>expect([...document.querySelectorAll('.online-guide h3')].map(heading=>heading.textContent)).toEqual([name,schedule]));
  const copyButtons=[...document.querySelectorAll<HTMLButtonElement>('.online-guide [data-copy-permissions]')];
  if(name==='飞书'||name==='Feishu'||name==='Lark'){
   expect(copyButtons).toHaveLength(2);
   const common=['docx:document:readonly','drive:drive:readonly','drive:export:readonly'];
   for(const [index,copy] of copyButtons.entries()){
    expect(copy.textContent).toBe(locale==='en'?'Copy permissions':'复制权限');
    copy.click();
    await vi.waitFor(()=>expect(copy.textContent).toBe(locale==='en'?'Copied':'已复制'));
    const {scopes}=JSON.parse(clipboard.mock.lastCall![0]);
    const wikiScopes=['wiki:space:retrieve','wiki:node:retrieve','wiki:node:read'];
    const feishu=name!=='Lark';
    const wiki=feishu?wikiScopes:['wiki:wiki:readonly'];
    expect(scopes.tenant).toEqual([]);
    expect([...scopes.user].sort()).toEqual([...common,'offline_access',...(index===0?wiki:[])].sort());
   }
  }else expect(copyButtons).toHaveLength(0);
  const close=locale==='en'?'Close':'关闭';
  button(close).click();await tick();
  button(close).click();await tick();
 }
});

it('opens a GitLab branch and saves its selected directory',async()=>{
 const address='https://gitlab.com/group/project';
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='browse')return {cursor:null,items:action.parent
   ?[{id:'gitlab:tree:release/docs:manual',title:'manual',expandable:true}]
   :[{id:'gitlab:tree:release/docs:',title:'release/docs',expandable:true}]};
  return {sources:[],documents:'/Documents'};
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 const rows=[...document.querySelectorAll('.data-import-row strong')].map(row=>row.textContent);
 expect(rows[rows.indexOf('GitHub')+1]).toBe('GitLab');
 button('配置 GitLab').click();
 await vi.waitFor(()=>expect(input('GitLab 项目链接')).toBeDefined());
 await fill(input('GitLab 项目链接'),address);
 button('浏览分支').click();
 await vi.waitFor(()=>expect(button('打开 release/docs')).toBeDefined());
 button('打开 release/docs').click();
 await vi.waitFor(()=>expect(input('manual')).toBeDefined());
 input('manual').click();
 await tick();
 button('保存并导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:{
  action:'save',secret:null,source:expect.objectContaining({
   connection:expect.objectContaining({platform:'gitlab',address}),resources:['gitlab:tree:release/docs:manual'],
  }),
 }}));
});

it.each([
 ['钉钉文档','dingtalk','Client Secret','operator'],
 ['腾讯 ima','ima','API Key',''],
])('passes %s credentials to the connector',async(name,platform,credential,operator)=>{
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='browse')return {items:[{id:'base',title:'资料',expandable:false}],cursor:null};
  return {sources:[],documents:'/Documents'};
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button(`配置 ${name}`).click();
 await vi.waitFor(()=>expect(input('Client ID')).toBeDefined());
 await fill(input('Client ID'),'test-client');
 await fill(input(credential),'test-secret');
 if(operator)await fill(input('操作人 Union ID'),operator);
 button('测试连接').click();
 await vi.waitFor(()=>expect(input('资料')).toBeDefined());
 expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({
  action:'browse',secret:'test-secret',connection:expect.objectContaining({platform,app_id:'test-client',address:operator}),
 })});
});

it('opens a GitHub branch without selecting it and replaces a broad selection with one file',async()=>{
 const address='https://github.com/owner/repo';
 invoke.mockImplementation(async(_command,{action})=>{
  if(action.action==='browse')return {cursor:null,items:action.parent
   ?[{id:'github:tree:release/docs:guides',title:'guides',expandable:true},{id:'github:blob:release/docs:README.md',title:'README.md',expandable:false}]
   :[{id:'github:tree:release/docs:',title:'release/docs',expandable:true}]};
  return {sources:[],documents:'/Documents'};
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 GitHub').click();
 await vi.waitFor(()=>expect(input('GitHub 仓库链接')).toBeDefined());
 await fill(input('GitHub 仓库链接'),address+'#user-guide');
 button('浏览分支').click();
 await vi.waitFor(()=>expect(button('打开 release/docs')).toBeDefined());
 expect(input('GitHub 仓库链接').value).toBe(address);
 expect(input('连接名称').value).toBe('owner/repo');
 expect(input('release/docs')).toBeUndefined();
 button('打开 release/docs').click();
 await vi.waitFor(()=>expect(input('README.md')).toBeDefined());
 expect(input('整个分支').checked).toBe(false);
 input('整个分支').click();
 await tick();
 input('README.md').click();
 await tick();
 expect(input('整个分支').checked).toBe(false);
 expect(input('README.md').checked).toBe(true);
 button('保存并导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:{
  action:'save',secret:null,source:expect.objectContaining({
   connection:expect.objectContaining({platform:'github',address}),
   resources:['github:blob:release/docs:README.md'],
  }),
 }}));
});

it.each([
 ['zh-CN','配置 RSS / Atom','订阅地址','https://zleap.com/blog/rss.xml'],
 ['zh-TW','Configure RSS / Atom','Feed URL','https://zleap.com/blog/rss.xml'],
 ['en','Configure RSS / Atom','Feed URL','https://zleap.com/en/blog/rss.xml'],
] as const)('offers an RSS feed example that can be accepted with Enter or Tab in %s',async(locale,configure,address,feed)=>{
 setLanguage(locale);
 invoke.mockResolvedValue({sources:[],documents:'/Documents'});
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button(configure).click();
 await vi.waitFor(()=>expect(input(address)?.placeholder).toBe(feed));
 const field=input(address);
 field.focus();
 expect(field.value).toBe('');
 for(const key of ['Enter','Tab']){
  await fill(field,'');
  const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});
  field.dispatchEvent(event);
  await tick();
  expect(field.value).toBe(feed);
  expect(event.defaultPrevented).toBe(key==='Enter');
 }
 expect(invoke).not.toHaveBeenCalledWith('online_import',{action:expect.objectContaining({action:'browse'})});
 const custom='https://example.org/feed.xml';
 await fill(field,custom);
 for(const key of ['Enter','Tab'])field.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true}));
 await tick();
 expect(field.value).toBe(custom);
});

it.each((['feishu','lark'] as const).flatMap(platform=>[
 {platform,name:'Team Drive',drive:true,id:'drive:folder',title:'Team folder'},
 {platform,name:'Team Wiki',drive:false,id:'wiki:space:page',title:'Team page'},
]))('authorizes and saves $name on $platform with daily sync',async({platform,name,drive,id,title})=>{
 const isLark=platform==='lark';
 const provider=isLark?'Lark':'飞书';
 const authorizationUrl=isLark?'https://accounts.larksuite.com/':'https://accounts.feishu.cn/';
 const limited=isLark?'Lark 暂时限制了请求频率，请稍后重试。':'飞书暂时限制了请求频率，请稍后重试。';
 let rootRequests=0;
 invoke.mockImplementation(async (command,args)=>{
  if(command==='open_external_link')return;
  const {action}=args;
  if(action.action==='list')return {sources:[],documents:'/Documents'};
  if(action.action==='account_login')return {session:'login',url:authorizationUrl};
  if(action.action==='account_login_status')return {ready:true};
  if(action.action==='browse'){
   if(!action.parent&&++rootRequests>1)throw '数据源请求失败（HTTP 429）';
   if(action.parent==='wiki:limited')throw limited;
   const item=!drive&&!action.parent?{id:'wiki:space',title:'Team space',expandable:true}:{id,title,expandable:false};
   return {items:!drive&&!action.parent?[item,{id:'wiki:limited',title:'Limited space',expandable:true}]:[item],cursor:null};
  }
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 expect(document.body.textContent).not.toContain('飞书云盘');
 expect(button('配置 Lark')).toBeDefined();
 expect(document.querySelectorAll('.data-import-clock')).toHaveLength(11);
 button(`配置 ${provider} 定时同步`).click();
 await vi.waitFor(()=>expect(input('App ID')).toBeDefined());
 const format=[...document.querySelectorAll('select')].find(element=>element.options[1]?.text==='云盘')!;
 format.selectedIndex=drive?1:0;format.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 await fill(input('连接名称'),name);
 button('配置教程').click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.online-guide h3')].map(heading=>heading.textContent)).toEqual([provider,'定时同步']));
 expect([...document.querySelectorAll('.online-guide h4')].map(heading=>heading.textContent)).toEqual(['知识库','云盘']);
 button('返回').click();
 await vi.waitFor(()=>expect(input('连接名称')?.value).toBe(name));
 await fill(input('App ID'),'test-app');await fill(input('App Secret'),'fixture-secret');
 expect(input('云盘文件夹链接或标识')).toBeUndefined();
 button(isLark?'使用 Lark 账号授权':'使用飞书账号授权').click();
 if(!drive){
  await vi.waitFor(()=>expect(input('Team space')).toBeDefined());
  button('打开 Team space').click();
 }
 await vi.waitFor(()=>expect(input(title)).toBeDefined());
 expect(invoke).toHaveBeenCalledWith('open_external_link',{url:authorizationUrl});
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'account_login',platform,app_id:'test-app',app_secret:'fixture-secret',drive}});
 expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({action:'browse',secret:null,auth_session:'login',parent:drive?null:'wiki:space',connection:expect.objectContaining({drive,user_access:true,address:''})})});
 input(title).click();await tick();
 if(!drive){
  button('返回上层').click();
  await vi.waitFor(()=>expect(input('Team space')).toBeDefined());
  expect(rootRequests).toBe(1);
  button('打开 Limited space').click();
  await vi.waitFor(()=>expect(document.body.textContent).toContain(limited));
  expect(input('Team space')).toBeDefined();
  button('打开 Team space').click();
  await vi.waitFor(()=>expect(input(title)?.checked).toBe(true));
 }
 expect(input('App Secret')).toBeUndefined();
 expect(document.body.textContent).toContain(isLark?'Lark 账号已授权':'飞书账号已授权');
 button(isLark?'重新授权 Lark 账号':'重新授权飞书账号').click();
 await vi.waitFor(()=>expect(input('App Secret')?.value).toBe(''));
 expect(input(title)?.checked).toBe(true);
 const frequency=document.querySelector<HTMLSelectElement>('select[aria-label="同步频率"]')!;
 expect([...frequency.options].map(option=>option.value)).toEqual(['manual','half_hourly','hourly','every_six_hours','every_twelve_hours','daily']);
 frequency.value='every_six_hours';frequency.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 expect(document.querySelector('input[type="time"]')).toBeNull();
 frequency.value='daily';frequency.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 await fill(document.querySelector<HTMLInputElement>('input[type="time"]')!,'16:45');
 button('保存并导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'save',secret:null,auth_session:'login',source:expect.objectContaining({name,resources:[id],schedule:'daily',daily_time:'16:45',connection:expect.objectContaining({platform,drive,user_access:true,address:''})})}}));
});

it.each([
 {drive:true,id:'drive:folder',title:'Team folder',browse:'浏览云盘文件夹',unselected:'未选择文件夹'},
 {drive:false,id:'wiki:space',title:'Team wiki',browse:'浏览知识库',unselected:'未选择知识库或页面'},
])('saves authorized Feishu $id before selecting an import scope',async({drive,id,title,browse,unselected})=>{
 let saved:OnlineSource|undefined;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='open_external_link')return;
  const {action}=args;
  if(action.action==='list')return {sources:saved?[{source:saved,next_at:null,last:null,running:false}]:[],documents:'/Documents'};
  if(action.action==='account_login')return {session:'login',url:'https://accounts.feishu.cn/'};
  if(action.action==='account_login_status')return {ready:true};
  if(action.action==='browse'){
   if(!saved)throw drive?'飞书暂时限制了请求频率，请稍后重试。':'飞书本月的 API 调用额度已用完。';
   return {items:[{id,title,expandable:false}],cursor:null};
  }
  if(action.action==='save'){saved={...action.source,id:'saved-connection'};return saved;}
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 飞书').click();
 await vi.waitFor(()=>expect(input('App ID')).toBeDefined());
 expect(input('连接名称').value).toBe('飞书');
 await fill(input('连接名称'),'');
 const format=[...document.querySelectorAll('select')].find(element=>element.options[1]?.text==='云盘')!;
 format.value=drive?'drive':'wiki';format.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 await fill(input('App ID'),'test-app');
 await fill(input('App Secret'),'fixture-secret');
 button('使用飞书账号授权').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain(drive?'飞书暂时限制了请求频率':'飞书本月的 API 调用额度已用完。'));
 await vi.waitFor(()=>expect(button('保存连接')?.disabled).toBe(false));
 button('保存连接').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain(unselected));
 expect(button('立即同步')).toBeUndefined();
 expect(invoke).toHaveBeenCalledWith('online_import',{action:{action:'save',source:expect.objectContaining({name:'飞书',resources:[]}),secret:null,auth_session:'login'}});

 const configure=[...document.querySelectorAll<HTMLButtonElement>('.connection-card button')].find(button=>button.textContent?.trim()==='配置')!;
 configure.click();await tick();
 await vi.waitFor(()=>expect(button(browse)).toBeDefined());
 button(browse).click();
 await vi.waitFor(()=>expect(input(title)).toBeDefined());
 input(title).click();await tick();
 button('保存并导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:{
  action:'save',source:expect.objectContaining({id:'saved-connection',resources:[id]}),secret:null,
 }}));
});

it('keeps Notion ZIP import accessible from the same row',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_share_settings')return {directory:'/Documents/share',clipboard:true};
  if(command==='online_import')return {sources:[],documents:'/Documents'};
  return {session:1,destination:'/Documents/notes/notion',folders:[]};
 });
 mounted=mount(DataImport,{target:document.body});
 await tick();
 expect([...document.querySelectorAll('h3')].map(heading=>heading.textContent)).toEqual(['Agent','在线数据源','第三方应用','分享']);
 const groups=[...document.querySelectorAll('.data-import-sources')];
 const names=(group:Element)=>[...group.querySelectorAll('.data-import-name strong')].map(item=>item.textContent);
 expect(names(groups[0])).toEqual(['Claude Code','Claude Desktop','Codex','WorkBuddy']);
 expect(names(groups[2])).toEqual(['Evernote','Google Keep','Apple Notes']);
 button('配置 Notion').click();
 await vi.waitFor(()=>expect(button('从导出文件导入')).toBeDefined());
 button('从导出文件导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('prepare_note_import',expect.objectContaining({source:'notion'})));
});

it('configures a POST API request and saves only selected response fields',async()=>{
 invoke.mockImplementation(async(command,{action})=>{
  if(command==='online_import'&&action.action==='api_sample')return {data:{items:[{id:7,title:'Example',description:'Do not import'}]}};
  if(command==='online_import'&&action.action==='api_preview')return '**title**\n\nExample';
  return {sources:[],documents:'/Documents'};
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 API 请求').click();
 await vi.waitFor(()=>expect(input('连接名称')).toBeDefined());
 await fill(input('连接名称'),'测试 API');
 await fill(input('API 地址'),'https://example.com/api');
 const method=[...document.querySelectorAll('select')].find(select=>select.value==='GET')!;
 expect(method.options.length).toBe(2);
 method.selectedIndex=1;
 method.dispatchEvent(new Event('change',{bubbles:true}));
 await tick();
 await vi.waitFor(()=>expect(document.querySelector('textarea')).not.toBeNull());
 const body=document.querySelector<HTMLTextAreaElement>('textarea')!;
 body.value='{"scope":"public"}';
 body.dispatchEvent(new Event('input',{bubbles:true}));
 await tick();
 button('测试请求').click();
 await vi.waitFor(()=>expect(input('title')).toBeDefined());
 input('title').click();
 await tick();
 button('预览 Markdown').click();
 await vi.waitFor(()=>expect([...document.querySelectorAll('.online-body pre')].at(-1)?.textContent).toBe('**title**\n\nExample'));
 button('保存并导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('online_import',{action:expect.objectContaining({
  action:'save',
  source:expect.objectContaining({
   connection:expect.objectContaining({platform:'api'}),
   api:expect.objectContaining({
    request:expect.objectContaining({method:'POST',url:'https://example.com/api',body:'{"scope":"public"}'}),
    records_path:'/data/items',
    fields:['/title'],
   })
  })
 })}));
});

it('keeps saved API parameters collapsed until they are opened',async()=>{
 const source:OnlineSource={
  id:'saved-api',name:'USGS',connection:{platform:'api',drive:false,app_id:'',address:''},
  resources:['api'],directory:'/Documents/USGS',schedule:'manual',daily_time:'09:00',paused:false,
  api:{
   request:{method:'GET',url:'https://earthquake.usgs.gov/fdsnws/event/1/query',query:[['limit','3']],headers:[],secret_header:'',body:null,allow_lan:false},
   records_path:'/features',fields:['/properties/mag'],dedup_field:null,cleanup:null,
  }
 };
 invoke.mockResolvedValue({sources:[{source,next_at:null,last:null,running:false}],documents:'/Documents'});
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('配置 API 请求').click();
 const configButton=()=>[...document.querySelectorAll<HTMLButtonElement>('.online-import button')].find(node=>node.textContent?.trim()==='配置');
 await vi.waitFor(()=>expect(configButton()).toBeDefined());
 configButton()!.click();
 await tick();
 expect([...document.querySelectorAll('.api-form legend')].map(legend=>legend.textContent?.trim())).toEqual(['请求','保留字段','AI 清洗','保存位置']);
 const details=document.querySelector<HTMLDetailsElement>('details.advanced')!;
 expect(details.open).toBe(false);
 details.querySelector('summary')!.click();
 expect(details.open).toBe(true);
 expect(details.querySelector<HTMLInputElement>('input')?.value).toBe('limit');
});
