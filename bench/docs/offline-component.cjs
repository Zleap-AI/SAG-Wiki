// Real renderer with synthetic status; no filesystem, downloads or OCR calls.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 try{
  for(const lang of ['cn','en']){
   const page=await browser.newPage({viewport:{width:1280,height:900}}),errors=[];
   page.on('pageerror',error=>errors.push(error.message));
   await page.emulateMedia({reducedMotion:'reduce'});
   await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?offline-component=1&lang=${lang}`);
   const dialog=page.locator('.docling-dialog');
   const button=page.getByRole('button',{name:lang==='en'?'Import offline component':'导入离线组件',exact:true});
   await button.waitFor();assert(await button.isEnabled());
   assert((await dialog.innerText()).includes('manifest.json'));
   if(lang==='en')assert(!/[\u3400-\u9fff]/.test(await dialog.innerText()));
   for(const width of [1280,720]){
    await page.setViewportSize({width,height:900});
    assert(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth),'dialog must not overflow horizontally');
   }
   await page.setViewportSize({width:1280,height:900});
   await page.waitForTimeout(500);
   await page.screenshot({path:`docs-site/public/images/product-offline-component-${lang}.png`});
   assert.deepEqual(errors,[]);await page.close();
  }
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
