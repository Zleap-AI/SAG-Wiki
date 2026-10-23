import assert from 'node:assert/strict';
import { createServer, request } from 'node:http';
import { spawn } from 'node:child_process';
import { cp, mkdir, mkdtemp, readFile, realpath, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium } from 'playwright';
import { zipSync, strToU8 } from 'fflate';

// Only this test fixture simulates fnOS. The application has no development bypass.
const base = await realpath(await mkdtemp('/tmp/fnos-web-'));
const app = resolve(base, 'app');
const files = resolve(base, '知识库');
await mkdir(resolve(app, 'www'), { recursive: true });
await mkdir('.local/qa', { recursive: true });
await mkdir(files);
await cp('dist', resolve(app, 'www'), { recursive: true });
await writeFile(resolve(files, '飞牛测试.md'), '# 飞牛文件测试\n\n原生接口检索标记 nativefnosmarker。\n');
await writeFile(resolve(files, '只读.md'), '# 只读文档\n\n不得覆盖。\n');
// Built-in conversion fixtures: a minimal Word document and a GBK-free CSV need no Docling component.
const docx = zipSync({
  '[Content_Types].xml': strToU8('<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'),
  '_rels/.rels': strToU8('<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'),
  'word/document.xml': strToU8('<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>飞牛内置转换验证 builtindocxmarker</w:t></w:r></w:p></w:body></w:document>'),
});
await writeFile(resolve(files, '验收报告.docx'), docx);
await writeFile(resolve(files, '型号清单.csv'), '型号,价格\nBCD-215,1999\nbuiltincsvmarker,1\n');
let authorized = false;
const nativeCalls = [];
const nativeSocket = resolve(base, 'native.sock');
const native = createServer(async (req, res) => {
  let body = ''; for await (const chunk of req) body += chunk;
  const payload = JSON.parse(body); nativeCalls.push(payload);
  assert.equal(req.headers.authorization, 'Bearer local-test-only');
  assert.equal(payload.appName, 'sag-desktop-fnos');
  const uid = payload.data.uid;
  let data;
  if (payload.req === 'trim.file.getUserAccessibleFolders') data = { paths: authorized && uid === 1000 ? [files] : [] };
  else if (payload.req === 'trim.file.checkUserACL') data = payload.data.path.map(path => ({path, readable: authorized && uid === 1000 && (path === files || path.startsWith(files + '/')), writable: !path.endsWith('只读.md'), deletable: false}));
  else if (payload.req === 'trim.file.convertPath') data = payload.data.path.map(path=>({path,semanticPath:path.replace(files,'我的文件/知识库')}));
  else throw new Error(`Unexpected native API: ${payload.req}`);
  res.setHeader('Content-Type', 'application/json');res.end(JSON.stringify({code:0,msg:'',data}));
});
await new Promise(resolve => native.listen(nativeSocket, resolve));
const server = spawn(resolve('target/debug/sag-desktop-fnos'), [], {
  env: {...process.env,TRIM_APPDEST:app,TRIM_PKGVAR:resolve(base,'data'),TRIM_API_TOKEN:'local-test-only',SAG_FNOS_API_SOCKET:nativeSocket},
  stdio: ['ignore','pipe','pipe'],
});
let serverLog=''; server.stderr.on('data',chunk=>serverLog+=chunk);
const proxy = createServer((req,res) => {
  const url = new URL(req.url, 'http://localhost');
  if(url.pathname==='/') {
    res.setHeader('Content-Type','text/html; charset=utf-8');
    res.end('<!doctype html><title>飞牛桌面测试</title><h1>飞牛桌面（测试）</h1><p>应用中心 → 已安装 → SAG Wiki → 设置</p>');return;
  }
  if(url.pathname==='/app-auth/pick-user-file') {
    const callback=new URL(url.searchParams.get('redirectUri'),'http://localhost');
    for(const key of ['appName','state'])callback.searchParams.set(key,url.searchParams.get(key));
    callback.searchParams.set('method','pickUserFile');callback.searchParams.set('status','success');callback.searchParams.set('path',JSON.stringify([files]));
    res.setHeader('Content-Type','text/html; charset=utf-8');
    res.end(`<h1>测试授权页面</h1><p>此页面仅模拟飞牛，验证回调流程。</p><button>确认测试授权</button><script>document.querySelector('button').onclick=async()=>{await fetch('/__authorize',{method:'POST'});location.assign(${JSON.stringify(callback.pathname+callback.search)})};</script>`);return;
  }
  if(url.pathname==='/__authorize'){authorized=true;res.end('ok');return;}
  const upstream=request({socketPath:resolve(app,'app.sock'),path:req.url,method:req.method,headers:{...req.headers,host:'localhost','x-trim-userid':req.headers['x-test-uid']??'1000'}},response=>{res.writeHead(response.statusCode,response.headers);response.pipe(res);});
  upstream.on('error',error=>{if(!res.headersSent)res.writeHead(502);res.end(error.message);});
  res.on('close',()=>upstream.destroy());req.pipe(upstream);
});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));
const origin=`http://127.0.0.1:${proxy.address().port}`;
const root=`${origin}/app/sag-desktop-fnos/`;
let browser;
try{
  for(let i=0;i<100;i++){const res=await fetch(root+'api/health');if(res.ok)break;await new Promise(resolve=>setTimeout(resolve,100));}
  const health=await fetch(root+'api/health');
  assert.equal(health.status,200,`${await health.text()} ${serverLog}`);
  if (process.argv.includes('--serve')) {
    console.log(`Local fnOS test fixture: ${root}`);
    await new Promise(resolve => process.once('SIGINT', resolve));
  } else {
  browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE});
  const context=await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce'});
  await context.addInitScript(()=>localStorage.setItem('fnos.1000.sag.onboarding.seen','1'));
  const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
  page.setDefaultTimeout(10000);
  const loaded=await page.goto(root);
  assert.equal(loaded.status(),200,await loaded.text());
  console.log('Page loaded');
  await page.getByRole('button',{name:'添加文件夹',exact:true}).click();
  const picker = page.getByRole('dialog',{name:'添加文件夹',exact:true});
  const layout = await picker.evaluate(el => {
    const heading = el.querySelector('.folder-picker-heading');
    const title = heading.querySelector('h2').getBoundingClientRect();
    const close = heading.querySelector('button').getBoundingClientRect();
    return { headingDisplay: getComputedStyle(heading).display,
      contentPadding: parseFloat(getComputedStyle(el.querySelector('.folder-picker-content')).paddingLeft),
      aligned: Math.abs(title.top - close.top) < 20 };
  });
  assert.equal(layout.headingDisplay,'flex');
  assert.ok(layout.contentPadding >= 16);
  assert.ok(layout.aligned);
  await page.screenshot({path:'.local/qa/fnos-folder-picker.png',fullPage:true,animations:'disabled'});
  await picker.getByRole('button',{name:'索引选项',exact:true}).click();
  const footerBottom = await picker.locator('.dialog-footer').evaluate(el => el.getBoundingClientRect().bottom);
  assert.ok(footerBottom <= 1000, 'index options must keep the footer inside the viewport');
  await picker.getByRole('button',{name:'索引选项',exact:true}).click();
  await page.getByRole('button',{name:'选择并授权文件夹',exact:true}).click();
  console.log('Authorization route opened');
  await page.getByRole('button',{name:'确认测试授权'}).click();
  await page.getByRole('dialog',{name:'添加文件夹'}).waitFor();
  await page.getByRole('dialog').getByRole('button',{name:'添加文件夹',exact:true}).click();
  await page.getByRole('dialog',{name:'添加文件夹'}).waitFor({state:'hidden'});
  console.log('Folder added');
  const rpc=async(command,args={},uid='1000')=>{
    const response=await fetch(root+'api/rpc',{method:'POST',headers:{'Content-Type':'application/json','x-test-uid':uid},body:JSON.stringify({command,args,session:'12345678-1234-4234-8234-123456789abc'})});
    return {status:response.status,body:await response.json()};
  };
  for(let i=0;i<200;i++){const result=await rpc('list_files');if(result.body.length===4)break;await page.waitForTimeout(100);}
  const indexedFiles=(await rpc('list_files')).body.map(file=>file.path);
  assert.equal(indexedFiles.length,4,JSON.stringify(indexedFiles));
  assert(indexedFiles.some(path=>path.endsWith('/.sag-converted/验收报告.docx.md')),'built-in Word conversion must be indexed without Docling');
  assert(indexedFiles.some(path=>path.endsWith('/.sag-converted/型号清单.csv.md')),'built-in CSV conversion must be indexed without Docling');
  assert.match(await readFile(resolve(files,'.sag-converted/验收报告.docx.md'),'utf8'),/builtindocxmarker/);
  assert.equal((await rpc('get_docling_status')).body.installed,false,'built-in conversion must not require the Docling component');
  const missingLog=await rpc('get_docling_install_log');
  assert.equal(missingLog.status,400);assert.match(missingLog.body.error,/尚未生成安装日志/);
  assert.equal((await rpc('list_roots')).body[0].status,'ready');
  const converted=await rpc('run_query',{query:{id:'12345678-1234-4234-8234-1234567890ab',kind:'search',question:'builtindocxmarker',scope:[],references:[],model:'',conversation_id:null,replace_from:null,page:1}});
  assert.equal(converted.status,200,JSON.stringify(converted.body));
  assert.equal(converted.body.results.length,1);
  assert.match(converted.body.results[0].path,/验收报告\.docx\.md$/);
  console.log('Built-in document conversion indexed');
  const settings=(await rpc('get_settings')).body;
  assert.equal((await rpc('save_settings',{settings:{...settings,search_summary_enabled:false},answerKey:'',jevKey:'',mcpCredentials:null})).status,200);
  // A local Jev without authentication is accepted, and the answer model may not use that mode.
  const localJev={...settings,search_summary_enabled:false,jev:{...settings.jev,url:'http://127.0.0.1:8000/v1',model:'local-jev',auth_mode:'none'}};
  assert.equal((await rpc('save_settings',{settings:localJev,answerKey:'',jevKey:'',mcpCredentials:null})).status,200);
  const savedJev=(await rpc('get_settings')).body;
  assert.equal(savedJev.jev.auth_mode,'none');assert.equal(savedJev.jev.has_key,false);
  const rejected=await rpc('save_settings',{settings:{...savedJev,answer:{...savedJev.answer,auth_mode:'none'}},answerKey:'',jevKey:'',mcpCredentials:null});
  assert.equal(rejected.status,400);assert.match(rejected.body.error,/问答模型/);
  // A source changed after indexing is left out of search instead of failing it.
  await writeFile(resolve(files,'只读.md'),'# 只读文档\n\n不得覆盖。nativefnosmarker stale\n');
  const staleSearch=await rpc('run_query',{query:{id:'12345678-1234-4234-8234-1234567890ac',kind:'search',question:'不得覆盖',scope:[],references:[],model:'',conversation_id:null,replace_from:null,page:1}});
  assert.equal(staleSearch.status,200,JSON.stringify(staleSearch.body));
  assert.equal(staleSearch.body.results.length,0);assert.equal(staleSearch.body.pagination.total,0);
  await writeFile(resolve(files,'只读.md'),'# 只读文档\n\n不得覆盖。\n');
  for(let i=0;i<100;i++){const result=await rpc('run_query',{query:{id:'12345678-1234-4234-8234-1234567890ad',kind:'search',question:'不得覆盖',scope:[],references:[],model:'',conversation_id:null,replace_from:null,page:1}});if(result.body.results?.length===1)break;await page.waitForTimeout(100);}
  console.log('Stale source skipped and Jev auth mode saved');
  await page.locator('.workspace-switch').getByRole('button',{name:'搜索',exact:true}).click();
  await page.getByLabel('搜索模式',{exact:true}).click();
  await page.getByRole('option',{name:'快速',exact:true}).click();
  await page.getByRole('textbox',{name:'搜索关键词'}).fill('nativefnosmarker');
  await page.getByRole('button',{name:'搜索文件',exact:true}).click();
  await page.locator('.search-result').first().waitFor();
  await page.waitForFunction(()=>document.querySelector('.search-preview')?.textContent?.includes('nativefnosmarker'));
  assert.match(await page.locator('.search-preview').innerText(),/nativefnosmarker/);
  await page.screenshot({path:'.local/qa/fnos-search.png',fullPage:true,animations:'disabled'});
  await page.getByRole('button',{name:'打开文档',exact:true}).click();
  const editor=page.getByRole('textbox',{name:'文档正文'});
  await editor.fill('飞牛保存验证 savedthroughui');
  await page.getByRole('button',{name:'保存',exact:true}).click();
  await page.getByText('已保存',{exact:true}).waitFor();
  assert.match(await readFile(resolve(files,'飞牛测试.md'),'utf8'),/savedthroughui/);
  const readonly=await rpc('read_document',{path:resolve(files,'只读.md')});
  const denied=await rpc('save_document',{path:resolve(files,'只读.md'),body:'overwrite',expectedHash:readonly.body.hash});
  assert.equal(denied.status,400);assert.match(await readFile(resolve(files,'只读.md'),'utf8'),/不得覆盖/);
  assert.deepEqual((await rpc('list_roots',{},'1001')).body,[]);
  assert.equal((await rpc('read_document',{path:resolve(files,'飞牛测试.md')},'1001')).status,400);
  // The header entry is now 接入 Agent; fnOS settings live in the settings navigation.
  await page.locator('.sidebar-tools').getByRole('button',{name:'设置',exact:true}).click();
  await page.locator('.settings-nav').getByRole('button',{name:'飞牛设置',exact:true}).click();
  await page.getByText('我的文件/知识库',{exact:true}).waitFor();
  const folderRow=page.getByRole('list',{name:'已授权目录'}).getByRole('listitem');
  assert.equal(await folderRow.count(),1);
  assert.match(await folderRow.innerText(),/知识库/);
  const folderLayout=await folderRow.evaluate(node=>({padding:parseFloat(getComputedStyle(node).paddingLeft),fits:node.scrollWidth<=node.clientWidth}));
  assert(folderLayout.padding>=12&&folderLayout.fits);
  const desktopPagePromise=context.waitForEvent('page');
  await page.getByRole('button',{name:'打开飞牛桌面',exact:true}).click();
  const desktopPage=await desktopPagePromise;
  await desktopPage.getByRole('heading',{name:'飞牛桌面（测试）'}).waitFor();
  await desktopPage.close();
  assert.equal(await page.getByRole('alert').filter({hasText:'Host bridge'}).count(),0);
  await page.screenshot({path:'.local/qa/fnos-settings.png',fullPage:true,animations:'disabled'});
  await page.locator('.settings-nav').getByRole('button',{name:'文件设置',exact:true}).click();
  await page.getByRole('button',{name:'选择其他格式',exact:true}).click();
  const formatDialog=page.getByRole('dialog',{name:'选择其他格式',exact:true});
  await formatDialog.waitFor();
  const builtin=await formatDialog.getByRole('region',{name:'内置转换格式'}).innerText();
  assert.match(builtin,/无需下载/);assert.match(builtin,/\.pdf/);assert.match(builtin,/\.docx/);
  assert.equal(await page.getByRole('checkbox',{name:'转换 .pdf',exact:true}).count(),0,'built-in formats are not offered as component downloads');
  // Picking a component format asks to install Docling first instead of applying silently.
  await page.getByRole('checkbox',{name:'转换 .pages',exact:true}).check();
  await page.getByRole('button',{name:'安装并应用',exact:true}).click();
  await page.getByRole('dialog',{name:'下载文档解析组件'}).waitFor();
  await page.screenshot({path:'.local/qa/fnos-docling-download.png',fullPage:true,animations:'disabled'});
  await page.getByRole('dialog',{name:'下载文档解析组件'}).getByRole('button',{name:'取消',exact:true}).click();
  await page.getByRole('dialog',{name:'下载文档解析组件'}).waitFor({state:'hidden'});
  await formatDialog.getByRole('button',{name:'取消',exact:true}).click();
  // UI fixture only: real installation and conversion run separately in Linux.
  const component=resolve(base,'data/users/1000/docling');
  await mkdir(resolve(component,'bin'),{recursive:true});
  await writeFile(resolve(component,'ready'),'2.140.0');
  await writeFile(resolve(component,'bin/docling'),'#!/bin/sh\nexit 0\n');
  await page.getByRole('button',{name:'选择其他格式',exact:true}).click();
  await page.getByRole('checkbox',{name:'转换 .pages',exact:true}).check();
  await page.getByRole('spinbutton',{name:'转换并行数',exact:true}).fill('2');
  await page.screenshot({path:'.local/qa/fnos-docling-formats.png',fullPage:true,animations:'disabled'});
  await page.getByRole('button',{name:'应用',exact:true}).click();
  await formatDialog.waitFor({state:'hidden'});
  await page.locator('.settings-nav').getByRole('button',{name:'问答设置',exact:true}).click();
  await page.getByRole('button',{name:'导入 JSON',exact:true}).click();
  await page.getByRole('textbox',{name:'MCP JSON 配置',exact:true}).fill(JSON.stringify({mcpServers:{'本地工具示例':{command:'python3',args:['/path/to/server.py']}}}));
  await page.getByRole('button',{name:'导入配置',exact:true}).click();
  await page.getByRole('textbox',{name:'MCP 启动命令',exact:true}).waitFor();
  assert.equal(await page.getByRole('textbox',{name:'MCP 启动命令',exact:true}).inputValue(),'python3');
  await page.locator('.mcp-connection').scrollIntoViewIfNeeded();
  await page.screenshot({path:'.local/qa/fnos-mcp-stdio.png',fullPage:true,animations:'disabled'});
    assert.deepEqual(errors,[]);
  assert(nativeCalls.some(call=>call.req==='trim.file.checkUserACL'));
  assert(nativeCalls.some(call=>call.req==='trim.file.convertPath'));
  console.log(JSON.stringify({result:'passed',checks:['browser authorization callback','native ACL','indexing','built-in Word/CSV conversion without Docling','stale source skipped in search','local Jev without authentication','search preview','editor save','read-only denial','user isolation','Docling install prompt, built-in format list and component selection','stdio MCP import and command fields'],nativeCalls:nativeCalls.length,screenshots:['.local/qa/fnos-search.png','.local/qa/fnos-settings.png']},null,2));
  }
}catch(error){console.error(error,serverLog);throw error;}
finally{
  await browser?.close();
  if(server.exitCode===null)await new Promise(resolve=>{server.once('exit',resolve);server.kill('SIGTERM');});
  proxy.closeAllConnections();await new Promise(resolve=>proxy.close(resolve));await new Promise(resolve=>native.close(resolve));
}
