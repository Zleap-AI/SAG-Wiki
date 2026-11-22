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
    const reader=page.locator('.wysiwyg-editor');
    await reader.locator('#rich-editor .tiptap[contenteditable="false"]').waitFor();
    await reader.locator('.outline-toggle[aria-expanded="false"]').click();
    await reader.locator('.outline-heading').first().waitFor();
    await reader.getByRole('button',{name:en?'Edit':'编辑',exact:true}).waitFor();
    if(await reader.locator('.editor-toolbar, [contenteditable="true"]').count())throw new Error('View mode exposed editing controls');
    const fold=await reader.locator('.document-fold-toggle').boundingBox();
    const question=await reader.locator('.document-question-toggle').boundingBox();
    if(fold.y+fold.height>question.y)throw new Error('Fold and Chat controls overlap');
    if(errors.length)throw new Error(errors.join('\n'));
    await page.mouse.move(1279,899);
    await page.screenshot({path:path.join(process.cwd(),'docs-site/public/images',`product-large-document-${lang}.png`),animations:'disabled'});
    console.log(`Captured large reader ${lang}`);
   }finally{await page.close();}
  }
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
