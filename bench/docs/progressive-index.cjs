// Synthetic UI state only: not a backend or performance measurement.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const path=require('path');
(async()=>{const browser=await chromium.launch({channel:'chrome',headless:true});for(const lang of ['cn','en']){
 const page=await browser.newPage({viewport:{width:1280,height:900}}),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?lang=${lang}&start=files&progressive=1`);
 await page.locator('.simple-file-row').first().click();
 await page.locator('.file-index-state').first().waitFor();
 if(await page.locator('.file-index-state').count()!==4)throw new Error('Expected all progressive states');
 if(errors.length)throw new Error(errors.join('\n'));
 await page.mouse.move(1279,899);await page.screenshot({path:path.join(process.cwd(),`docs-site/public/images/product-progressive-index-${lang}.png`),animations:'disabled'});
 await page.close();
 }await browser.close()})().catch(error=>{console.error(error);process.exit(1)});
