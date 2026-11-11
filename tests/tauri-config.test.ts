import {readFileSync} from 'node:fs';
import {describe,expect,it} from 'vitest';
// Tauri layers tauri.<platform>.conf.json over tauri.conf.json with RFC 7396 JSON Merge Patch,
// where arrays such as app.windows replace the base value instead of being merged.
function mergePatch(doc:unknown,patch:unknown):unknown{
 if(patch===null||typeof patch!=='object'||Array.isArray(patch))return patch;
 const out:Record<string,unknown>=doc!==null&&typeof doc==='object'&&!Array.isArray(doc)?{...(doc as Record<string,unknown>)}:{};
 for(const [key,value] of Object.entries(patch as Record<string,unknown>)){
  if(value===null)delete out[key];else out[key]=mergePatch(out[key],value);
 }
 return out;
}
type Config={app:{windows:{label:string}[]}};
const base=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url),'utf8')) as Config;
const windows=mergePatch(base,JSON.parse(readFileSync(new URL('../src-tauri/tauri.windows.conf.json',import.meta.url),'utf8'))) as Config;
const labels=(config:Config)=>config.app.windows.map(window=>window.label);
describe('tauri window configuration',()=>{
 it('defines the main and quick-search windows',()=>{
  expect(labels(base)).toEqual(['main','quick-search']);
 });
 it('keeps every window when the Windows override is applied',()=>{
  expect(labels(windows)).toEqual(labels(base));
  // main intentionally differs on Windows (custom title bar); quick-search must stay identical.
  const quick=(config:Config)=>config.app.windows.find(window=>window.label==='quick-search');
  expect(quick(windows)).toEqual(quick(base));
 });
 it('keeps the quick-search window hidden until the shortcut opens it',()=>{
  for(const config of [base,windows]){
   const quick=config.app.windows.find(window=>window.label==='quick-search') as Record<string,unknown>|undefined;
   expect(quick).toMatchObject({visible:false,decorations:false,alwaysOnTop:true,skipTaskbar:true,url:'index.html?quick-search'});
  }
 });
});
