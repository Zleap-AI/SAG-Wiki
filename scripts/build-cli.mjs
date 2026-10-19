import {spawnSync} from 'node:child_process';
import {copyFileSync, mkdirSync, writeFileSync} from 'node:fs';
const debug = process.argv.includes('--debug');
const target = debug ? undefined : process.env.TAURI_ENV_TARGET_TRIPLE;
const args=['build',...(debug?[]:['--release']),'--manifest-path','cli/Cargo.toml',...(target?['--target',target]:[])];
const build=spawnSync('cargo',args,{stdio:'inherit'});
if(build.status!==0)process.exit(build.status??1);
const windows=target?target.includes('windows'):process.platform==='win32';
const name=windows?'sag.exe':'sag';
const destination=debug?'src-tauri/bin/development':'src-tauri/bin';
mkdirSync(destination,{recursive:true});
copyFileSync(`cli/target/${target?target+'/':''}${debug?'debug':'release'}/${name}`,`${destination}/${name}`);
if(process.platform==='darwin'&&!windows){
 const sign=spawnSync('codesign',['--force','--sign',process.env.APPLE_SIGNING_IDENTITY||'-',`${destination}/${name}`],{stdio:'inherit'});
 if(sign.status!==0)process.exit(sign.status??1);
}

if(!target) {
const identity=spawnSync(destination+'/'+name,['--build-info'],{encoding:'utf8'});
if(identity.status!==0)throw new Error('Unable to verify SAG build identity');
const metadata=JSON.parse(identity.stdout);
if(metadata.channel!==(debug?'development':'release'))throw new Error('SAG artifact channel does not match the build');
writeFileSync(destination+'/build-identity.json',JSON.stringify(metadata,null,2)+'\n');
}
