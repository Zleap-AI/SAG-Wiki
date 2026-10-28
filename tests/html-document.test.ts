// @vitest-environment happy-dom
import {loadDocumentBridge} from './helpers/document-bridge';
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import {JSDOM} from 'jsdom';
import HtmlDocument from '../src/components/HtmlDocument.svelte';
let app:ReturnType<typeof mount>|null=null;
afterEach(async()=>{if(app)await unmount(app);app=null;document.body.innerHTML='';});
it.each([
 {label:'白底黑字',style:'background-color:#fff;color:#000',color:'rgb(0, 0, 0)',background:'rgb(255, 255, 255)'},
 {label:'透明背景黑字',style:'color:#000',color:'rgb(0, 0, 0)',background:'rgba(0, 0, 0, 0)'},
 {label:'深底白字',style:'background-color:#111;color:#fff',color:'rgb(255, 255, 255)',background:'rgb(17, 17, 17)'},
])('选中 HTML 段落时保留原文对比度：$label',async({style,color,background})=>{
 app=mount(HtmlDocument,{target:document.body,props:{body:`<p style="background-color:#fff;color:#000">needle <span style="${style}">原文内容</span></p>`,terms:['needle'],paragraphHighlight:true}});
 await tick();
 const frame=document.querySelector('iframe')!;
 const doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
 loadDocumentBridge(frame,doc);await tick();
 expect(doc.querySelector('.search-active-paragraph')?.tagName).toBe('P');
 // Evaluate the real generated styles after the real bridge selects a match.
 const rendered=new JSDOM(doc.documentElement.outerHTML);
 try{
  const paragraph=rendered.window.document.querySelector('p')!;
  const content=rendered.window.document.querySelector('span')!;
  expect(rendered.window.getComputedStyle(content).color).toBe(color);
  expect(rendered.window.getComputedStyle(content).backgroundColor).toBe(background);
  expect(rendered.window.getComputedStyle(paragraph).backgroundColor).toBe('rgb(255, 255, 255)');
 }finally{rendered.window.close()}
});
it('搜索原文按完整段落选中，嵌套列表不会连带选中父项，切换时清除旧段落',async()=>{
 app=mount(HtmlDocument,{target:document.body,props:{body:'<ul><li>父项<ul><li><p>段首 <strong>needle</strong> 段尾。</p></li></ul></li></ul><p>下一段 needle。</p>',terms:['needle'],paragraphHighlight:true}});
 await tick();
 const frame=document.querySelector('iframe')!;
 const doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
 loadDocumentBridge(frame,doc);await tick();
 expect(doc.querySelectorAll('.search-active-paragraph')).toHaveLength(1);
 expect(doc.querySelector('.search-active-paragraph')?.textContent).toBe('段首 needle 段尾。');
 expect(doc.querySelector('li.search-active-paragraph')).toBeNull();
 document.querySelector<HTMLButtonElement>('[aria-label="下一处匹配"]')!.click();await tick();
 expect(doc.querySelectorAll('.search-active-paragraph')).toHaveLength(1);
 expect(doc.querySelector('.search-active-paragraph')?.textContent).toBe('下一段 needle。');
});
it('保留 HTML 页面样式和 SVG 图形，高亮正文并切换匹配',async()=>{
 const body='<style>p{color:red}</style><p>早先 Token</p><h2>目标 Token</h2><svg><text x="10" y="20">Token</text></svg><script>Token()</script>';
 app=mount(HtmlDocument,{target:document.body,props:{body,terms:['Token'],initialMatch:1}});
 await tick();
 const frame=document.querySelector('iframe')!;
 const doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
 expect(doc.querySelectorAll('mark.search-hit')).toHaveLength(2);
 expect(doc.querySelectorAll('svg text tspan.search-hit')).toHaveLength(0);
 expect(doc.querySelector('svg text')?.textContent).toBe('Token');
 expect([...doc.querySelectorAll('style')].some(style=>style.textContent==='p{color:red}')).toBe(true);
 expect(doc.querySelector('script .search-hit')).toBeNull();
 expect(frame.getAttribute('sandbox')).toBe('allow-scripts');
 expect(doc.querySelector('meta[http-equiv="Content-Security-Policy"]')?.getAttribute('content')).toContain("script-src 'nonce-");
 const scroll=vi.fn();doc.querySelectorAll('.search-hit').forEach(mark=>Object.defineProperty(mark,'scrollIntoView',{value:scroll}));
 loadDocumentBridge(frame,doc);await tick();
 const contextmenu=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2});
 doc.querySelector('p')!.dispatchEvent(contextmenu);
 expect(contextmenu.defaultPrevented).toBe(true);
 expect(doc.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
 expect(document.querySelector('[aria-label="匹配位置"]')?.textContent).toContain('2 / 2');
 document.querySelector<HTMLButtonElement>('[aria-label="下一处匹配"]')!.click();await tick();
 expect(doc.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('0');
 expect(scroll).toHaveBeenCalledWith({block:'center'});
});
it('过滤文档脚本、事件处理和跳转代码，保留样式与普通链接',async()=>{
 app=mount(HtmlDocument,{target:document.body,props:{body:'<meta http-equiv="refresh" content="0;url=https://example.com"><base href="https://example.com"><script>parent.location="https://example.com"</script><p onclick="alert(1)">正文</p><a href="javascript:alert(1)">危险链接</a><iframe srcdoc="<script>alert(1)</script>"></iframe><a href="https://example.org">参考资料</a><style>p{color:red}</style>'}});
 await tick();const doc=new DOMParser().parseFromString(document.querySelector('iframe')!.getAttribute('srcdoc')!,'text/html');
 expect(doc.querySelector('script:not([nonce]),base,iframe,object,embed,[onclick]')).toBeNull();
 expect(doc.querySelector('[http-equiv="refresh"]')).toBeNull();
 expect(doc.querySelector('a[href^="javascript:"]')).toBeNull();
 expect(doc.querySelector('a[href="https://example.org"]')).not.toBeNull();
 expect([...doc.querySelectorAll('style')].some(style=>style.textContent==='p{color:red}')).toBe(true);
});

it('隔离 HTML 正文可隐藏导航并恢复滚动，新目标和其它窗口消息不能覆盖位置',async()=>{
 const body='<p>needle first</p><p>needle second</p>',memoryKey='html-return:file:h:query-1';
 const open=async(props:Record<string,unknown>)=>{
  app=mount(HtmlDocument,{target:document.body,props:{body,terms:['needle'],memoryKey,...props}});await tick();
  const frame=document.querySelector('iframe')!,doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
  const scroll=vi.fn();doc.querySelectorAll('.search-hit').forEach(mark=>Object.defineProperty(mark,'scrollIntoView',{value:scroll}));
  loadDocumentBridge(frame,doc);await tick();return {frame,doc,scroll};
 };
 const first=await open({showNavigation:false});
 expect(document.querySelector('.match-navigation')).toBeNull();
 expect(first.frame.getAttribute('sandbox')).toBe('allow-scripts');
 first.doc.documentElement.scrollTop=350;first.doc.documentElement.scrollLeft=9;
 first.doc.dispatchEvent(new Event('scroll'));await tick();
 window.dispatchEvent(new MessageEvent('message',{source:window,data:{type:'sag-reader-scroll',top:999,left:999}}));
 await unmount(app!);app=null;
 const restored=await open({restoreScroll:true});
 expect(restored.doc.documentElement.scrollTop).toBe(350);
 expect(restored.doc.documentElement.scrollLeft).toBe(9);
 expect(restored.scroll).not.toHaveBeenCalled();
 await unmount(app!);app=null;
 const targeted=await open({restoreScroll:false,initialMatch:1});
 expect(targeted.doc.querySelector('.search-hit.active')?.getAttribute('data-search-match')).toBe('1');
 expect(targeted.scroll).toHaveBeenCalledWith({block:'center'});
});
