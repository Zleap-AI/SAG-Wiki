const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const path = require('path');
(async () => {
 const browser = await chromium.launch({channel:'chrome', headless:true});
 try {
  for (const lang of ['cn','en']) {
   const page = await browser.newPage({viewport:{width:1280,height:900},deviceScaleFactor:1});
   const errors=[]; page.on('pageerror', e=>errors.push(e.message));
   await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT || 1437}/bench/docs.html?lang=${lang}`);
   await page.locator('.sidebar-settings').click();
   await page.locator('.settings-nav').getByRole('button',{name:lang==='cn'?'文件设置':'File settings',exact:true}).click();
   const toggle=page.getByRole('switch',{name:lang==='cn'?'Office 仅提取文字':'Office text only',exact:true});
   await toggle.scrollIntoViewIfNeeded();
   if (await toggle.getAttribute('aria-checked') !== 'false') throw new Error('Must default off');
   await page.locator('.settings-dialog').screenshot({path:path.resolve(`docs-site/public/images/product-office-text-${lang}.png`),animations:'disabled'});
   if (errors.length) throw new Error(errors.join('\n'));
   await page.close();
  }
 } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exit(1)});
