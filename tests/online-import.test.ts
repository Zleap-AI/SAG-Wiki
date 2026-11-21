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

it.each([
 ['zh-CN','配置 {name}','配置教程','定时同步',['飞书','Lark','Notion','语雀','RSS / Atom']],
 ['en','Configure {name}','Setup guide','Scheduled sync',['Feishu','Lark','Notion','Yuque','RSS / Atom']],
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

it.each([
 ['zh-CN','配置 RSS / Atom','订阅地址','https://zleap.com/blog/rss.xml'],
 ['zh-TW','Configure RSS / Atom','Feed URL','https://zleap.com/blog/rss.xml'],
 ['en','Configure RSS / Atom','Feed URL','https://zleap.com/en/blog/rss.xml'],
] as const)('prefills an editable RSS feed in %s',async(locale,configure,address,feed)=>{
 setLanguage(locale);
 invoke.mockResolvedValue({sources:[],documents:'/Documents'});
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button(configure).click();
 await vi.waitFor(()=>expect(input(address)?.value).toBe(feed));
 expect(invoke).not.toHaveBeenCalledWith('online_import',{action:expect.objectContaining({action:'browse'})});
 const custom='https://example.org/feed.xml';
 await fill(input(address),custom);
 expect(input(address).value).toBe(custom);
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
 expect(document.querySelectorAll('.data-import-clock')).toHaveLength(5);
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
 expect([...document.querySelectorAll('h3')].map(heading=>heading.textContent)).toEqual(['第三方应用','在线数据源','分享']);
 button('配置 Notion').click();
 await vi.waitFor(()=>expect(button('从导出文件导入')).toBeDefined());
 button('从导出文件导入').click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('prepare_note_import',expect.objectContaining({source:'notion'})));
});
