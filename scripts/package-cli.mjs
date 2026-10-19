import {spawnSync} from 'node:child_process';
import {copyFileSync, mkdirSync, readFileSync, writeFileSync, mkdtempSync, rmSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
const run=(command,args)=>{const result=spawnSync(command,args,{stdio:'inherit'});if(result.status!==0)process.exit(result.status??1);};
const target=process.env.SAG_TARGET||spawnSync('rustc',['-vV'],{encoding:'utf8'}).stdout.match(/^host: (.+)$/m)?.[1];
if(!target)throw Error('Cannot determine the Rust target.');
const version=readFileSync('cli/Cargo.toml','utf8').match(/^version = "(.+)"/m)[1];
run('cargo',['build','--release','--locked','--manifest-path','cli/Cargo.toml',...(process.env.SAG_TARGET?['--target',target]:[])]);
const windows=target.includes('windows');
const staging=mkdtempSync(join(tmpdir(),'sag-release-'));
const output=resolve('releases/cli');mkdirSync(output,{recursive:true});
const name=`sag-${version}-${target}.${windows?'zip':'tar.gz'}`;
try{
 copyFileSync(`cli/target/${process.env.SAG_TARGET?target+'/':''}release/sag${windows?'.exe':''}`,join(staging,`sag${windows?'.exe':''}`));
 copyFileSync(`scripts/install.${windows?'ps1':'sh'}`,join(staging,`install.${windows?'ps1':'sh'}`));
 if(windows)run('powershell',['-NoProfile','-Command',`Compress-Archive -Path '${staging.replaceAll("'","''")}/*' -DestinationPath '${join(output,name).replaceAll("'","''")}' -Force`]);
 else run('tar',['-czf',join(output,name),'-C',staging,'sag','install.sh']);
 const hash=createHash('sha256').update(readFileSync(join(output,name))).digest('hex');
 writeFileSync(join(output,name+'.sha256'),`${hash}  ${name}\n`);
 console.log(join(output,name));
}finally{rmSync(staging,{recursive:true,force:true});}
