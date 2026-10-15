import {spawnSync} from 'node:child_process';
import {copyFileSync, mkdirSync} from 'node:fs';
const target = process.env.TAURI_ENV_TARGET_TRIPLE;
const args=['build','--release','--manifest-path','cli/Cargo.toml',...(target?['--target',target]:[])];
const build=spawnSync('cargo',args,{stdio:'inherit'});
if(build.status!==0)process.exit(build.status??1);
const windows=target?target.includes('windows'):process.platform==='win32';
const name=windows?'sag.exe':'sag';
mkdirSync('src-tauri/bin',{recursive:true});
copyFileSync(`cli/target/${target?target+'/':''}release/${name}`,`src-tauri/bin/${name}`);
if(process.platform==='darwin'&&!windows){
 const sign=spawnSync('codesign',['--force','--sign',process.env.APPLE_SIGNING_IDENTITY||'-',`src-tauri/bin/${name}`],{stdio:'inherit'});
 if(sign.status!==0)process.exit(sign.status??1);
}
