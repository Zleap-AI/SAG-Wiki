import assert from 'node:assert/strict';
import {chromium} from 'playwright';
const origin=process.env.DOCS_ORIGIN??'http://127.0.0.1:3033';
const browser=await chromium.launch({headless:true});
try {
 for(const lang of ['zh','en']) {
  const page=await browser.newPage();
  for(const slug of ['','installation','usage','development']) {
   const response=await page.goto(`${origin}/${lang}/docs/${slug}`);
   assert.equal(response.status(),200);
   await page.locator('h1').waitFor();
   if(slug==='usage') {
    await page.locator('h2').filter({hasText:lang==='zh'?'PDF 与 Office 自动转换':'PDF and Office conversion'}).waitFor();
    await page.locator('h2').filter({hasText:lang==='zh'?'本地 MCP':'Local MCP'}).waitFor();
    for(const image of ['fnos-docling-formats.png','fnos-mcp-stdio.png']) {
     const picture=page.locator(`img[src*="${image.replace(".png", ".")}"]`).first();
     await picture.scrollIntoViewIfNeeded();
     const loaded=await picture.evaluate(async node=>{await node.decode();return node.naturalWidth>0});
     assert(loaded);
    }
   }
  }
  const response=await fetch(`${origin}/api/search?query=Docling&locale=${lang}`);
  assert.equal(response.status,200);
  const result=await response.json();
  assert.match(JSON.stringify(result),/Docling/);
  console.log(`${lang}: four pages, conversion/MCP headings, images and search passed`);
  await page.close();
 }
} finally {await browser.close();}
