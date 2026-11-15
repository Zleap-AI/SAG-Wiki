const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const path=require('node:path');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 try{
  for(const lang of ['cn','en']){
   const en=lang==='en', page=await browser.newPage({viewport:{width:1280,height:900}});
   const errors=[];page.on('pageerror',e=>errors.push(e.message));
   await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?lang=${lang}&start=files`);
   await page.locator('.library-folder-action').waitFor();
   await page.getByRole('button',{name:en?'Project notes':'项目资料',exact:true}).first().click();
   await page.locator('.library-folder-action').waitFor();
   await page.screenshot({path:path.resolve(`docs-site/public/images/product-files-${lang}.png`),animations:'disabled'});
   await page.getByRole('button',{name:en?'Settings':'设置',exact:true}).click();
   await page.getByRole('button',{name:en?'Data import':'数据导入',exact:true}).click();
   await page.screenshot({path:path.resolve(`docs-site/public/images/product-data-import-${lang}.png`),animations:'disabled'});
   await page.getByRole('button',{name:en?'Import Claude Code chats':'导入 Claude Code 聊天记录',exact:true}).click();
   const toggle=page.getByRole('switch',{name:en?'Show tool calls and results':'展示工具调用过程'});
   if(await toggle.getAttribute('aria-checked')!=='false')throw Error('Tool process must default off');
   await page.locator('.chat-session-open').first().click();
   await page.locator('.chat-preview-document h1').waitFor();
   await page.locator('.chat-session [role=checkbox]').first().click();
   await page.screenshot({path:path.resolve(`docs-site/public/images/product-chat-import-${lang}.png`),animations:'disabled'});
   await toggle.click();
   await page.waitForFunction(()=>document.querySelector('.chat-preview pre')?.textContent.includes('npm test'));
   await page.getByRole('button',{name:en?'Import':'导入',exact:true}).click();
   await page.locator('.chat-import-results').waitFor();
   if(errors.length)throw Error(errors.join('\n'));
   console.log(`${lang}: import entry, default-off option, preview and result passed`);
   await page.close();
  }
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
