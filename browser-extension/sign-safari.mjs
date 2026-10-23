import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtemp, writeFile, rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const [mode, appPath, identity] = process.argv.slice(2);
assert(['sign','verify'].includes(mode) && appPath && (mode!=='sign' || identity),
 'Usage: node sign-safari.mjs sign <app-path> <identity> | verify <app-path>');
const app = path.resolve(appPath);
const extension = path.join(app,'Contents/PlugIns/SAG Web Clipper Extension.appex');
const bundles = [extension,app].map(bundle=>{
 const plist = execFileSync('codesign',['--display','--entitlements',':-',bundle],{stdio:['pipe','pipe','pipe']});
 const entitlements = JSON.parse(execFileSync('plutil',['-convert','json','-o','-','--','-'],{input:plist,encoding:'utf8'}));
 assert.equal(entitlements['com.apple.security.app-sandbox'],true,`${bundle}: App Sandbox is required`);
 assert.deepEqual(entitlements['com.apple.security.temporary-exception.files.home-relative-path.read-write'],['/Documents/sag-wiki/'],
  `${bundle}: default saves require access to Documents/sag-wiki only`);
 if(bundle===app) assert.equal(entitlements['com.apple.security.network.client'],true,
  'Safari setup requires com.apple.security.network.client; omitting it causes a blank window');
 return {bundle,entitlements};
});
if(mode==='sign') {
 const temporary = await mkdtemp(path.join(os.tmpdir(),'sag-safari-sign-'));
 try {
  for(const [index,{bundle,entitlements}] of bundles.entries()) {
   // Preserve each target's effective Xcode entitlements, excluding debugger access.
   delete entitlements['com.apple.security.get-task-allow'];
   const file=path.join(temporary,`${index}.plist`);
   await writeFile(file,JSON.stringify(entitlements));
   execFileSync('plutil',['-convert','xml1',file]);
   execFileSync('codesign',['--force','--sign',identity,'--options','runtime','--timestamp','--entitlements',file,bundle],{stdio:'inherit'});
  }
 } finally {await rm(temporary,{recursive:true,force:true});}
}
for(const {bundle} of bundles) {
 const plist=execFileSync('codesign',['--display','--entitlements',':-',bundle],{stdio:['pipe','pipe','pipe']});
 const entitlements=JSON.parse(execFileSync('plutil',['-convert','json','-o','-','--','-'],{input:plist,encoding:'utf8'}));
 assert.notEqual(entitlements['com.apple.security.get-task-allow'],true,`${bundle}: release must not allow debugger access`);
 if(bundle===app) assert.equal(entitlements['com.apple.security.network.client'],true);
}
execFileSync('codesign',['--verify','--deep','--strict',app],{stdio:'inherit'});
console.log('Safari signing verified: sandbox, setup WebKit permission, and no debugger access.');
