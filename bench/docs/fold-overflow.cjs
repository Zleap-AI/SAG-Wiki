const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 try{
  for(const width of [1280,960]){
   const page=await browser.newPage({viewport:{width,height:900}});
   await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?start=files&lang=en&fold-long-token`);
   await page.locator('.simple-file-row').first().click();await page.getByRole('button',{name:'Release guide.md',exact:true}).click();
   await page.locator('.document-fold-toggle').click();await page.locator('.fold-reader.ready').waitFor();
   const frame=page.frames().find(f=>f.url().includes('/textfold/reader.html'));
   await frame.locator('#zoombar [data-l="0"]').click();await page.waitForTimeout(1200);
   const bounds=await frame.locator('#u0').evaluate(el=>{
    const card=el.closest('.chunk').getBoundingClientRect(),chip=el.getBoundingClientRect();
    return {cardRight:card.right,chipRight:chip.right,client:el.clientWidth,scroll:el.scrollWidth,text:el.textContent};
   });
   assert(bounds.text.includes('enterprise'.repeat(35)),'source text stays intact behind the visual ellipsis');
   assert(bounds.chipRight<=bounds.cardRight && bounds.scroll<=bounds.client+1,`long URL escapes card at ${width}px: ${JSON.stringify(bounds)}`);
   await page.close();console.log(`PASS: long URL is elided within card at ${width}px`);
  }
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
