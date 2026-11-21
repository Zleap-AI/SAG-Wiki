const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const path=require('node:path');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 try{
  for(const lang of ['cn','en']){
   const en=lang==='en', page=await browser.newPage({viewport:{width:1280,height:900}});
   const errors=[];page.on('pageerror',e=>errors.push(e.message));
   await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?lang=${lang}&start=files`);
   if(!process.env.SCHEDULE_ONLY){
    await page.getByRole('button',{name:en?'Project notes':'项目资料',exact:true}).first().click();
    await page.getByRole('button',{name:en?'New document':'新建文档',exact:true}).waitFor();
    await page.screenshot({path:path.resolve(`docs-site/public/images/product-files-${lang}.png`),animations:'disabled'});
   }
   await page.getByRole('button',{name:en?'Settings':'设置',exact:true}).click();
   await page.getByRole('button',{name:en?'Data sources':'数据源',exact:true}).click();
   if(process.env.SCHEDULE_ONLY){
    const dialogName=en?'Import Codex chats':'导入 Codex 聊天记录';
    await page.getByRole('button',{name:dialogName,exact:true}).click();
    const dialog=page.getByRole('dialog',{name:dialogName});
    const schedule=dialog.locator('.chat-import-schedule');
    const toggle=schedule.getByRole('switch',{name:en?'Scheduled sync':'定时同步'});
    if(await toggle.getAttribute('aria-checked')!=='false'||await schedule.locator('select').count())throw Error('Scheduled sync must default to manual');
    await toggle.click();
    const frequency=schedule.locator('select:enabled');
    await frequency.waitFor();
    await frequency.selectOption('30');
    await frequency.waitFor();
    if(await frequency.inputValue()!=='30'||await schedule.locator('input[type="time"]').count())throw Error('Interval schedules should not display a time input');
    await frequency.selectOption('daily');
    await schedule.locator('input[type="time"]:enabled').waitFor();
    await schedule.locator('input[type="time"]').fill('09:30');
    await dialog.getByRole('button',{name:en?'Close':'关闭',exact:true}).waitFor();
    if(await schedule.locator('button').count()!==1)throw Error('Only the scheduling switch should be a button');
    await dialog.getByRole('button',{name:en?'Close':'关闭',exact:true}).click();
    await page.getByRole('button',{name:dialogName,exact:true}).click();
    await dialog.locator('.chat-preview-document h1').waitFor();
    if(await schedule.locator('input[type="time"]').inputValue()!=='09:30')throw Error('Saved time did not persist');
    await page.screenshot({path:path.resolve(`docs-site/public/images/product-chat-import-schedule-${lang}.png`),animations:'disabled'});
    if(errors.length)throw Error(errors.join('\n'));
    await page.close();
    continue;
   }
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
