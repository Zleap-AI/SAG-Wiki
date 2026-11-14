// @vitest-environment node
import {expect,it} from 'vitest';
import {createHash} from 'node:crypto';
import {mkdtempSync,readFileSync,readdirSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {basename,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

it('emits every KaTeX WOFF2 face as a readable local asset',()=>{
 const root=fileURLToPath(new URL('..',import.meta.url));
 const output=mkdtempSync(join(tmpdir(),'sag-math-assets-'));
 const hash=(bytes:Buffer)=>createHash('sha256').update(bytes).digest('hex');
 try{
  const build=spawnSync(process.execPath,[join(root,'node_modules/vite/bin/vite.js'),'build','--outDir',output],{cwd:root,encoding:'utf8',timeout:60_000});
  expect(build.status,build.stderr||build.stdout).toBe(0);
  const assets=join(output,'assets');
  const faces=readdirSync(assets).filter(name=>name.endsWith('.css')).flatMap(name=>[...readFileSync(join(assets,name),'utf8').matchAll(/@font-face\{[^}]+\}/g)].map(match=>match[0])).filter(face=>/font-family:["']?KaTeX_/.test(face));
  const originals=join(root,'node_modules/katex/dist/fonts');
  const expected=readdirSync(originals).filter(name=>name.endsWith('.woff2')).map(name=>hash(readFileSync(join(originals,name))));
  expect(faces).toHaveLength(expected.length);
  const emitted=faces.map(face=>{
   const urls=[...face.matchAll(/url\(([^)]+)\)/g)].map(match=>match[1].replace(/^["']|["']$/g,''));
   expect(urls,face).toHaveLength(1);
   const url=new URL(urls[0],'https://sag.test/assets/style.css');
   expect(url.origin,face.slice(0,120)).toBe('https://sag.test');
   expect(basename(url.pathname)).toMatch(/^KaTeX_.*\.woff2$/);
   return hash(readFileSync(resolve(output,'.'+decodeURIComponent(url.pathname))));
  });
  expect(new Set(emitted)).toEqual(new Set(expected));
 }finally{rmSync(output,{recursive:true,force:true})}
},60_000);
