// Isolated renderer acceptance check + bilingual screenshots. No model service.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const assert=require('node:assert/strict');
const path=require('node:path');
const config=require('../../src-tauri/tauri.conf.json');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 for(const lang of ['cn','en']){
  const page=await browser.newPage({viewport:{width:1280,height:900}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  // Observe actual animation keyframes, including the upstream closed shadow
  // root. document.getAnimations() alone cannot see its staged animations.
  await page.addInitScript(()=>{
   window.foldMotion=[];const original=Element.prototype.animate;
   Element.prototype.animate=function(frames,options){window.foldMotion.push(frames);return original.call(this,frames,options);};
   window.foldRoots=[];const attachShadow=Element.prototype.attachShadow;
   Element.prototype.attachShadow=function(options){const root=attachShadow.call(this,options);window.foldRoots.push(root);return root;};
  });
  await page.route('**/textfold/reader.html',async route=>{
   const response=await route.fetch();
   await route.fulfill({response,headers:{...response.headers(),'content-security-policy':config.app.security.csp}});
  });
  await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?start=files&lang=${lang}`);
  await page.locator('.simple-file-row').first().click();
  await page.getByRole('button',{name:lang==='en'?'Release guide.md':'发布流程.md',exact:true}).click();
  const editorFont=await page.locator('#rich-editor').evaluate(el=>getComputedStyle(el).fontFamily);
  await page.locator('.document-fold-toggle').click();
  await page.locator('.fold-reader.ready').waitFor();
  const frame=page.frames().find(f=>f.url().includes('/textfold/reader.html'));
  assert.equal(await frame.locator('#doc').getAttribute('data-level'),'3');
  const originalBar=await frame.locator('#zoombar').boundingBox();
  assert.equal(await frame.locator('.u').first().evaluate(el=>getComputedStyle(el).fontFamily),editorFont);
  assert.equal(await frame.locator('#hint').isVisible(),false);
  await frame.locator('.fold-help').click();
  assert.equal(await frame.locator('#hint').isVisible(),true);
  assert((await frame.locator('#hint').innerText()).length>0);
  await page.keyboard.press('Escape');
  assert.equal(await frame.locator('#hint').isVisible(),false);
  await frame.locator('.fold-help').click();await frame.locator('.paper-head h1').click();
  assert.equal(await frame.locator('#hint').isVisible(),false);
  await frame.locator('.zoombar button[data-l="4"]').click();
  await page.waitForTimeout(1200);
  assert(await frame.locator('table').isVisible());
  await frame.evaluate(()=>window.foldMotion=[]);
  await frame.locator('.zoombar button[data-l="2"]').click();
  await page.waitForTimeout(1000);
  assert(await frame.evaluate(()=>window.foldMotion.some(keys=>{
   const transforms=keys.map(k=>k.transform).filter(Boolean);return new Set(transforms).size>1;
  })),'the original reader must animate positions');
  await frame.locator('.zoombar button[data-l="4"]').click();
  await page.waitForTimeout(1200);
  const full=await frame.locator('.zoombar button[data-l="4"]').boundingBox();
  const key=await frame.locator('.zoombar button[data-l="2"]').boundingBox();
  const take=await frame.locator('.zoombar button[data-l="1"]').boundingBox();
  await page.mouse.move(full.x+full.width/2,full.y+full.height/2);await page.mouse.down();
  await page.mouse.move((key.x+key.width/2+take.x+take.width/2)/2,full.y+full.height/2,{steps:8});
  await page.waitForTimeout(160);
  const position=await frame.evaluate(()=>window.__reader.pos);
  assert(position>1&&position<2,'the slider must render fractional reading levels');
  const colors=await frame.evaluate(()=>({page:getComputedStyle(document.body).backgroundColor,
   animation:getComputedStyle(window.foldRoots.find(root=>root.host.classList.contains('zhost')).querySelector('.zstage')).backgroundColor}));
  assert.equal(colors.animation,colors.page,'the animation background must match the reader background');
  const animationFonts=await frame.evaluate(()=>Array.from(new Set([...window.foldRoots.find(root=>root.host.classList.contains('zhost')).querySelectorAll('.zl.live .zp')]
   .filter(el=>el.textContent.trim()).map(el=>getComputedStyle(el).fontFamily))));
  assert.deepEqual(animationFonts,[editorFont],'animation text must retain the editor font');
  await page.mouse.up();await page.waitForTimeout(1000);
  await frame.locator('.zoombar button[data-l="0"]').click();await page.waitForTimeout(1200);
  assert.equal(await frame.locator('#doc').getAttribute('data-level'),'0');
  assert.equal(await frame.locator('#doc .chunks').evaluate(el=>getComputedStyle(el).display),'grid');
  assert((await frame.locator('.u[data-l="0"] .tx').allTextContents()).some(t=>t.trim()));
  const bar=await frame.locator('#zoombar').boundingBox(),mapTab=await frame.locator('.sag-map-tab').boundingBox();
  assert.equal(await frame.locator('.tview').isVisible(),false,'no secondary Cards / Map row');
  assert(Math.abs(mapTab.y+mapTab.height/2-bar.y-bar.height/2)<2,'Map must share the primary tab row');
  assert.equal(await frame.locator('#zoombar [data-l="0"]').innerText(),lang==='en'?'Cards':'卡片');
  await frame.locator('.sag-map-tab').click();await page.waitForTimeout(1200);
  assert(await frame.locator('.mmap').isVisible(),'Map tab opens the mind map directly');
  assert.equal(await frame.locator('.sag-map-tab').getAttribute('aria-pressed'),'true');
  assert.equal(await frame.locator('#zoombar [data-l="0"]').getAttribute('aria-pressed'),'false');
  await frame.locator('#zoombar [data-l="0"]').click();await page.waitForTimeout(800);
  assert.equal(await frame.locator('.mmap').isVisible(),false,'Cards tab returns directly to cards');
  assert.equal(await frame.locator('.sag-map-tab').getAttribute('aria-pressed'),'false');
  assert(Math.abs(bar.y-originalBar.y)<2,'the primary row stays at the same height');
  await page.mouse.move(0,0);
  await page.screenshot({path:path.join(process.cwd(),`docs-site/public/images/product-document-fold-${lang}.png`)});
  await frame.locator('.zoombar button[data-l="3"]').click();await page.waitForTimeout(1200);
  const unit=frame.locator('.u').first();await unit.click();await page.waitForTimeout(900);
  assert.equal(await unit.getAttribute('data-l'),'4');
  await page.locator('.document-fold-toggle').click();assert(await page.locator('#rich-editor').isVisible());
  // Reopening uses the completed result; palette follows the host's dark mode.
  await page.evaluate(()=>document.body.dataset.theme='dark');await page.locator('.document-fold-toggle').click();
  await page.locator('.fold-reader.ready').waitFor();
  const reopened=page.frames().find(f=>f.url().includes('/textfold/reader.html'));
  assert.equal(await reopened.locator('html').getAttribute('data-theme'),'dark');
  await reopened.locator('.zoombar button[data-l="4"]').click();await page.waitForTimeout(1200);
  await reopened.evaluate(()=>window.__reader.scrubTo(2.5));await page.waitForTimeout(160);
  const darkColors=await reopened.evaluate(()=>({page:getComputedStyle(document.body).backgroundColor,
   animation:getComputedStyle(window.foldRoots.find(root=>root.host.classList.contains('zhost')).querySelector('.zstage')).backgroundColor}));
  assert.equal(darkColors.animation,darkColors.page,'dark animation background must match the reader background');
  await page.goto(`http://127.0.0.1:${process.env.DOCS_PORT||1437}/bench/docs.html?start=files&lang=${lang}&fold-progress`);
  await page.locator('.simple-file-row').first().click();
  await page.getByRole('button',{name:lang==='en'?'Release guide.md':'发布流程.md',exact:true}).click();
  await page.locator('.document-fold-toggle').click();
  await page.waitForFunction(()=>document.querySelector('progress')?.value===67);
  assert((await page.locator('.fold-progress').innerText()).includes('67%'));
  await page.waitForFunction(()=>document.querySelector('progress')?.value>67);
  assert((await page.locator('progress').evaluate(el=>el.value))<90,'simulated waiting progress should advance gradually');
  assert.equal(await page.locator('.fold-progress').evaluate(el=>getComputedStyle(el).opacity),'1');
  await page.mouse.move(0,0);
  await page.screenshot({path:path.join(process.cwd(),`docs-site/public/images/product-document-fold-progress-${lang}.png`)});
  await page.locator('.document-fold-toggle').click();
  assert.equal(await page.locator('.fold-progress').count(),0);
  await page.locator('.document-fold-toggle').click();
  assert((await page.locator('progress').evaluate(el=>el.value))>=67,'closing must preserve the in-flight task');
  await page.getByRole('button',{name:lang==='en'?'Stop folding':'停止折叠',exact:true}).click();
  await page.getByRole('alert').waitFor();
  assert.equal(await page.locator('progress').count(),0);
  assert.deepEqual(errors,[]);await page.close();
  console.log(`PASS ${lang}: motion, slider, cards, table, expansion, CSP, dark mode, progress and cancellation`);
 }
 await browser.close();
})().catch(error=>{console.error(error);process.exit(1);});
