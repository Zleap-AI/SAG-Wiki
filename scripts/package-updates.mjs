import {parseArgs} from 'node:util';
import {readFileSync,writeFileSync,mkdirSync,copyFileSync,existsSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {spawnSync} from 'node:child_process';

const {values}=parseArgs({options:{version:{type:'string'},macos:{type:'string'},windows:{type:'string'},notes:{type:'string'},output:{type:'string'}}});
const {version,macos,windows}=values;
if(!version||!/^\d+\.\d+\.\d+$/.test(version)||!macos||!windows)throw new Error('Usage: npm run package:updates -- --version X.Y.Z --macos app.tar.gz --windows setup.exe [--notes notes.md] [--output directory]');
if(!macos.endsWith('.app.tar.gz')||!windows.endsWith('.exe'))throw new Error('Use the macOS .app.tar.gz update bundle and the Windows NSIS .exe installer.');
const root=resolve(import.meta.dirname,'..');
const output=resolve(values.output??join(root,'releases',version,'updates'));
const keySource=process.env.TAURI_SIGNING_PRIVATE_KEY??join(root,'releases/updater-signing/sag-wiki.key');
const key=existsSync(keySource)?readFileSync(keySource,'utf8'):keySource;
const publicKey=JSON.parse(readFileSync(join(root,'src-tauri/tauri.conf.json'),'utf8')).plugins.updater.pubkey;
const publicId=Buffer.from(Buffer.from(publicKey,'base64').toString('utf8').trim().split('\n').at(-1),'base64').subarray(2,10);
const cli=join(root,'node_modules/@tauri-apps/cli/tauri.js');
const artifacts=[['darwin-aarch64',macos,`SAG-Wiki-${version}-macOS-arm64.app.tar.gz`],['windows-x86_64',windows,`SAG-Wiki-${version}-Windows-x64-setup.exe`]];
mkdirSync(output,{recursive:true});
const platforms={};
for(const [platform,input,name] of artifacts){
 const target=join(output,name);
 copyFileSync(resolve(input),target);
 const result=spawnSync(process.execPath,[cli,'signer','sign','--app-version',version,target],{cwd:root,env:{...process.env,TAURI_SIGNING_PRIVATE_KEY:key,TAURI_SIGNING_PRIVATE_KEY_PASSWORD:process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD??''},encoding:'utf8'});
 if(result.status!==0)throw new Error(`Update signing failed for ${platform}: ${result.stderr}`);
 const signature=readFileSync(target+'.sig','utf8').trim();
 const signatureId=Buffer.from(Buffer.from(signature,'base64').toString('utf8').split('\n')[1],'base64').subarray(2,10);
 if(!publicId.equals(signatureId))throw new Error('The signing key does not match the public key in tauri.conf.json.');
 platforms[platform]={url:`https://github.com/Zleap-AI/SAG-Wiki/releases/download/v${version}/${name}`,signature};
}
const manifest={version,notes:values.notes?readFileSync(resolve(values.notes),'utf8'):'',pub_date:new Date().toISOString(),platforms};
writeFileSync(join(output,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
console.log(`Signed update files and latest.json: ${output}`);
