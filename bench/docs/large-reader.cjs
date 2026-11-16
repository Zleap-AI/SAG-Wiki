const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const path=require('node:path');

(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 try{
  for(const lang of ['cn','en']){
   const en=lang==='en',page=await browser.newPage({viewport:{width:1280,height:900}});
   const errors=[];page.on('pageerror',error=>errors.push(error.message));
   try{
    await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?lang=${lang}&start=files&large-reader=1`);
    await page.locator('.simple-file-row').first().click();
    await page.locator('.simple-file-row').filter({hasText:en?'Release guide.md':'发布流程.md'}).click();
    const reader=page.locator('.large-document-reader');
    await reader.locator('.reader-piece[data-state="rendered"]').first().waitFor();
    await reader.locator('.outline-toggle').click();
    await reader.locator('.reader-outline .outline-heading').first().waitFor();
    await reader.locator('.reader-outline [role="status"]').waitFor({state:'detached'});
    await reader.locator('.reader-tools').getByRole('button',{name:en?'Find in document':'正文查找',exact:true}).click();
    await reader.locator('.large-find input').fill(en?'Preparation':'发布准备');
    await reader.locator('.large-find').getByRole('button',{name:en?'Find':'查找',exact:true}).click();
    await reader.locator('.search-hit.active').first().waitFor();
    await page.waitForFunction(()=>!/统计|Counting/.test(document.querySelector('.large-document-reader .match-navigation')?.textContent||''));
    if(await page.locator('[contenteditable="true"]').count())throw new Error('Large file unexpectedly mounted an editor');
    if(await reader.locator('.reader-piece').count()>12)throw new Error('Large reader mounted too many pieces');
    if(errors.length)throw new Error(errors.join('\n'));
    await page.mouse.move(1279,899);
    await page.screenshot({path:path.join(process.cwd(),'docs-site/public/images',`product-large-document-${lang}.png`),animations:'disabled'});
    console.log(`Captured large reader ${lang}`);
   }finally{await page.close();}
  }
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
