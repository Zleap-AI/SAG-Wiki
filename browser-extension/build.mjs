import {build} from 'esbuild';
import {zipSync} from 'fflate';
import {cp, mkdir, readdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
const root=path.dirname(fileURLToPath(import.meta.url));
const output=path.join(root,'build');
const packages=path.join(root,'../src-tauri/clipper');
await mkdir(packages,{recursive:true});
const version=JSON.parse(await readFile(path.join(root,'package.json'),'utf8')).version;
const icons=Object.fromEntries([16,32,48,128].map(size=>[size,`icon-${size}.png`]));
const manifest={manifest_version:3,name:'__MSG_extensionName__',description:'__MSG_extensionDescription__',version,default_locale:'en',permissions:['activeTab','scripting','contextMenus','storage'],icons,action:{default_title:'SAG Wiki',default_popup:'popup.html',default_icon:icons},options_ui:{page:'options.html',open_in_tab:true}};
for(const browser of ['chrome','safari']) {
 const dir=path.join(output,browser);
 await rm(dir,{recursive:true,force:true});await mkdir(dir,{recursive:true});
 await cp(path.join(root,'static'),dir,{recursive:true});
 await build({entryPoints:['options','popup'].map(name=>path.join(root,`src/${name}.js`)),outdir:dir,bundle:true,minify:true,format:'esm',target:'es2022',define:{__SAFARI__:String(browser==='safari')}});
 await build({entryPoints:[path.join(root,'src/background.js')],outfile:path.join(dir,'background.js'),bundle:true,minify:true,format:'iife',target:'es2022',define:{__SAFARI__:String(browser==='safari')}});
 await build({entryPoints:[path.join(root,'src/extract.js')],outfile:path.join(dir,'extract.js'),bundle:true,minify:true,format:'iife',target:'es2022',define:{__SAFARI__:String(browser==='safari')},footer:{js:';globalThis.__sagWebClip;'}});
 await writeFile(path.join(dir,'manifest.json'),JSON.stringify({...manifest,permissions:[...manifest.permissions,browser==='safari'?'nativeMessaging':'downloads'],options_ui:browser==='chrome'?manifest.options_ui:{page:'options.html'},background:browser==='chrome'?{service_worker:'background.js'}:{scripts:['background.js']}},null,2));
 await cp(path.join(root,'node_modules/defuddle/LICENSE'),path.join(dir,'DEFUDDLE-LICENSE.txt'));
}
async function files(dir,prefix='') {
 const result={};
 for(const entry of await readdir(dir,{withFileTypes:true})) {
  const name=prefix+entry.name;
  if(entry.isDirectory())Object.assign(result,await files(path.join(dir,entry.name),name+'/'));
  else result[name]=new Uint8Array(await readFile(path.join(dir,entry.name)));
 }
 return result;
}
await writeFile(path.join(packages,'sag-web-clipper-chrome.zip'),zipSync(await files(path.join(output,'chrome'))));
if(process.argv.includes('--safari') || (process.argv.includes('--desktop') && process.platform==='darwin')) {
 if(process.platform!=='darwin')throw new Error('Safari packaging requires macOS and Xcode.');
 const project=path.join(output,'safari-project');
 execFileSync('xcrun',['safari-web-extension-converter',path.join(output,'safari'),'--project-location',project,'--app-name','SAG Web Clipper','--bundle-identifier','com.sag.webclipper','--macos-only','--swift','--copy-resources','--no-open','--no-prompt','--force'],{stdio:'inherit'});
 const projectFile=path.join(project,'SAG Web Clipper/SAG Web Clipper.xcodeproj/project.pbxproj');
 await writeFile(projectFile,(await readFile(projectFile,'utf8')).replaceAll('com.sag.SAG-Web-Clipper','com.sag.webclipper').replaceAll('CURRENT_PROJECT_VERSION = 1;',`CURRENT_PROJECT_VERSION = ${version.split('.').reduce((value,part)=>value*1000+Number(part),0)};`).replaceAll('MARKETING_VERSION = 1.0;',`MARKETING_VERSION = ${version};`));
 const appIcon=path.join(project,'SAG Web Clipper/SAG Web Clipper/Assets.xcassets/AppIcon.appiconset');
 await cp(path.join(root,'assets/macos'),appIcon,{recursive:true});
 await writeFile(path.join(appIcon,'Contents.json'),JSON.stringify({images:[16,32,128,256,512].flatMap(size=>[1,2].map(scale=>({idiom:'mac',size:`${size}x${size}`,scale:`${scale}x`,filename:`icon-${size*scale}.png`}))),info:{version:1,author:'xcode'}},null,2));
 const nativeCode = await readFile(path.join(root,'safari/ClipFileStore.swift'),'utf8') + '\n' + await readFile(path.join(root,'safari/SafariWebExtensionHandler.swift'),'utf8');
 await writeFile(path.join(project,'SAG Web Clipper/SAG Web Clipper Extension/SafariWebExtensionHandler.swift'),nativeCode);
 const appResources=path.join(project,'SAG Web Clipper/SAG Web Clipper/Resources');
 await cp(path.join(root,'safari/Resources'),appResources,{recursive:true});
 await writeFile(path.join(appResources,'Style.css'),await readFile(path.join(root,'static/style.css'),'utf8')+'\n'+await readFile(path.join(root,'safari/Resources/Style.css'),'utf8'));
 const entitlements = path.join(project,'SAG Web Clipper/Clipper.entitlements');
 await writeFile(entitlements,'<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.app-sandbox</key><true/><key>com.apple.security.files.downloads.read-write</key><true/><key>com.apple.security.files.user-selected.read-write</key><true/><key>com.apple.security.files.bookmarks.app-scope</key><true/></dict></plist>');
 const derived=path.join(output,'safari-derived');
 // Local test package. Release distribution must use the team's signing/notarization process.
 const log=execFileSync('xcodebuild',['-project',path.join(project,'SAG Web Clipper/SAG Web Clipper.xcodeproj'),'-scheme','SAG Web Clipper','-configuration','Release','-derivedDataPath',derived,`CODE_SIGN_ENTITLEMENTS=${entitlements}`,'CODE_SIGN_IDENTITY=-','CODE_SIGNING_ALLOWED=YES','MACOSX_DEPLOYMENT_TARGET=13.0','build'],{maxBuffer:20*1024*1024});
 await writeFile(path.join(output,'safari-build.log'),log);
 const app=path.join(derived,'Build/Products/Release/SAG Web Clipper.app');
 execFileSync('ditto',['-c','-k','--sequesterRsrc','--keepParent',app,path.join(packages,'sag-web-clipper-safari.zip')]);
}
console.log(`Extension packages: ${packages}`);
