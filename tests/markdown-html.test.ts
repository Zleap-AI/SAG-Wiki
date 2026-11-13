// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {hydrateImages,renderMarkdown,protectHtml,restoreHtml} from '../src/lib/markdown';
import {invoke} from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(async()=> 'data:image/svg+xml;base64,PHN2Zy8+')}));
afterEach(()=>vi.clearAllMocks());
it('loads MinerU images from Windows canonical document paths',async()=>{
 const host=document.createElement('div');
 host.innerHTML=renderMarkdown('![figure](.sag-mineru-assets/hash/images/a.jpg)');
 await hydrateImages(host,String.raw`\\?\F:\资料\.sag-converted\report.pdf.md`);
 expect(invoke).toHaveBeenCalledExactlyOnceWith('load_image',{path:'F:/资料/.sag-converted/.sag-mineru-assets/hash/images/a.jpg'});
});
const header='<p align="center">\n  <img src="assets/logo.svg" alt="Zleap AI" width="180" />\n</p>\n\n<h1 align="center">SAG2</h1>\n\n<p align="center">\n  <a href="README.md">English</a> · <strong>简体中文</strong> · <a href="../README-CN.md">项目首页</a>\n</p>';

it('渲染 README 的居中标题、带尺寸的本地图片和链接，并保留编辑源码',()=>{
 const doc=new DOMParser().parseFromString(renderMarkdown(header),'text/html');
 expect(doc.querySelector('h1')?.textContent).toBe('SAG2');
 expect(doc.querySelector('h1')?.style.textAlign).toBe('center');
 const image=doc.querySelector('p img');
 expect(image?.parentElement?.style.textAlign).toBe('center');
 expect(image?.getAttribute('width')).toBe('180');
 expect(image?.getAttribute('data-source')).toBe('assets/logo.svg');
 expect(doc.querySelector('a[href="README.md"]')?.textContent).toBe('English');
 expect(doc.querySelector('strong')?.textContent).toBe('简体中文');
 expect(doc.querySelector('pre')).toBeNull();
 expect(restoreHtml(protectHtml(header))).toBe(header);
});

it('本地 HTML 图片通过现有文件权限加载，外部图片不自动请求',async()=>{
 const host=document.createElement('div');
 host.innerHTML=renderMarkdown(header+'\n\n<p><img src="https://example.com/badge.svg" alt="badge"></p>');
 await hydrateImages(host,'/notes/README-CN.md');
 expect(invoke).toHaveBeenCalledExactlyOnceWith('load_image',{path:'/notes/assets/logo.svg'});
 expect(host.querySelector('img')?.getAttribute('src')).toBe('data:image/svg+xml;base64,PHN2Zy8+');
 expect(host.querySelectorAll('img')[1].getAttribute('src')).toBe('');
});

it('代码块里的 HTML 仍显示源码，并清除正文中的脚本和事件',()=>{
 const html=renderMarkdown('```html\n'+header+'\n```\n\n<p onclick="alert(1)"><a href="javascript:alert(1)">正文</a><script>alert(1)</script></p>');
 const doc=new DOMParser().parseFromString(html,'text/html');
 expect(doc.querySelector('pre code')?.textContent).toBe(header);
 expect(doc.querySelector('script,[onclick],[href^="javascript:"]')).toBeNull();
 expect(doc.querySelector('h1')).toBeNull();
});
