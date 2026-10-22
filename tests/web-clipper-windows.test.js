import {readFileSync} from 'node:fs';
import {expect,it} from 'vitest';
import {makeClipping,validateFolder} from '../browser-extension/src/clipping.js';

it.each(['CON','NUL','AUX','COM1','LPT9','中文笔记 with spaces','title <>:"/\\|?*','长文😀'.repeat(100)])('creates a Windows-compatible Markdown filename for %s',title=>{
 const {filename,content}=makeClipping({title,url:'https://example.org',content:'网页正文'});
 expect(filename).toMatch(/\.md$/);
 expect(filename).not.toMatch(/[<>:"/\\|?*\u0000-\u001f]/);
 expect(new TextEncoder().encode(filename).length).toBeLessThan(240);
 expect(filename.split('.')[0]).not.toMatch(/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i);
 expect(content).toContain('网页正文');
 expect(content).toContain(JSON.stringify(title));
});

it('uses relative download subfolders and rejects pasted Windows absolute paths',()=>{
 expect(validateFolder('网页剪辑/项目 A')).toBe('网页剪辑/项目 A');
 expect(()=>validateFolder('C:\\Users\\User\\Documents')).toThrow('invalidFolder');
 expect(()=>validateFolder('C:/Users/User/Documents')).toThrow('invalidFolder');
});

it('includes the Chrome/Edge extension in the Windows desktop package',()=>{
 const config=JSON.parse(readFileSync('src-tauri/tauri.windows.conf.json','utf8'));
 expect(config.bundle.resources['clipper/sag-web-clipper-chrome.zip']).toBe('clipper/sag-web-clipper-chrome.zip');
});
