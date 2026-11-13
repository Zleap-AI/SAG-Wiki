import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,existsSync,readdirSync,cpSync,chmodSync,rmSync,statSync,copyFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {spawnSync} from 'node:child_process';
const version='12.4.2';
export async function prepareQpdf(destination, target=process.env.TAURI_ENV_TARGET_TRIPLE||`${process.arch}-${process.platform}`){
 const windows=target.includes('windows')||target.includes('win32');
 const arm=target.includes('aarch64')||target.includes('arm64');
 const mac=target.includes('darwin');
 const variant=windows?'msvc64':mac?`bin-macos-${arm?'arm64':'x86_64'}`:`bin-linux-${arm?'aarch64':'x86_64'}`;
 const hashes={msvc64:'db87077e683630c1217e0e8f9a20a9749d952ab676e881c3689187763a5de25d','bin-macos-arm64':'62e46987a30ea167cbc530ccb22690aec3d8c812ed09979a941bcee92e504b79','bin-macos-x86_64':'dd3b01f4414d198529bb0f13bba59c2f016a328bb3506695087a96fb80fc1481','bin-linux-x86_64':'db367d897829f22c4198ce1094143c9d467bd6ee7dfabc44ba6f02056b24f8b1','bin-linux-aarch64':'8fd9d009eb0838398180a603f2cf76531e5c68a4ecc8847598e0a1ccb1d3b600'};
 const cache=resolve('node_modules/.cache/qpdf',version,variant);mkdirSync(cache,{recursive:true});
 const archive=join(cache,'package.zip');
 if(!existsSync(archive)){const response=await fetch(`https://github.com/qpdf/qpdf/releases/download/v${version}/qpdf-${version}-${variant}.zip`);if(!response.ok)throw Error(`QPDF download: ${response.status}`);writeFileSync(archive,Buffer.from(await response.arrayBuffer()));}
 if(createHash('sha256').update(readFileSync(archive)).digest('hex')!==hashes[variant])throw Error('QPDF checksum mismatch');
 const extracted=join(cache,'extracted');mkdirSync(extracted,{recursive:true});
 const result=process.platform==='win32'?spawnSync('powershell',['-NoProfile','-Command',`Expand-Archive -LiteralPath '${archive.replaceAll("'","''")}' -DestinationPath '${extracted.replaceAll("'","''")}' -Force`],{stdio:'inherit'}):spawnSync('unzip',['-qo',archive,'-d',extracted],{stdio:'inherit'});
 if(result.status!==0)throw Error('QPDF extraction failed');
 const locate=dir=>{if(existsSync(join(dir,'bin',windows?'qpdf.exe':'qpdf')))return dir;for(const entry of readdirSync(dir,{withFileTypes:true})){if(entry.isDirectory()){const found=locate(join(dir,entry.name));if(found)return found;}}};
 const root=locate(extracted);if(!root)throw Error('QPDF executable missing');
 // npm excludes symlinks; real library files also make tar/desktop payloads
 // independent of the build cache. Replace stale absolute links from old builds.
 rmSync(destination,{recursive:true,force:true});
 const copyFiles=(source,target)=>{
  if(statSync(source).isDirectory()){
   mkdirSync(target,{recursive:true});
   for(const entry of readdirSync(source))copyFiles(join(source,entry),join(target,entry));
  }else copyFileSync(source,target);
 };
 copyFiles(root,destination);
 // SAG invokes only qpdf. Keep the original runtime libraries and notices,
 // but omit standalone tools that are never called by conversion.
 for(const name of ['fix-qdf','fix-qdf.exe','zlib-flate','zlib-flate.exe'])
  rmSync(join(destination,'bin',name),{force:true});
 // Official POSIX archives expose the same shared library under its ABI name and its
 // fully versioned name. qpdf loads the ABI name; npm needs it as a real file.
 // Remove the extra copy only after verifying the pinned archive's bytes match.
 if(!windows){
  const abi=join(destination,mac?'lib/libqpdf.30.dylib':'lib/libqpdf.so.30');
  const full=join(destination,mac?'lib/libqpdf.30.4.2.dylib':'lib/libqpdf.so.30.4.2');
  if(!existsSync(abi)||!existsSync(full)||!readFileSync(abi).equals(readFileSync(full)))
   throw Error('Unexpected QPDF runtime library layout');
  rmSync(full);
 }
 // Windows loads DLLs from bin; lib and include contain SDK/link-time files only.
 // Keep the audited license records from the pinned 12.4.2 manual;
 // manual HTML, fonts, scripts, images and examples are not runtime inputs.
 if(windows){
  rmSync(join(destination,'lib'),{recursive:true,force:true});
  rmSync(join(destination,'include'),{recursive:true,force:true});
  const docs=join(destination,'share/doc/qpdf');
  const notices=new Set(['manual-html/license.html','manual-html/_sources/license.rst.txt']);
  for(const name of notices)if(!existsSync(join(docs,name))||!statSync(join(docs,name)).isFile())
   throw Error('Unexpected QPDF manual license layout');
  const prune=(directory,prefix='')=>{
   for(const entry of readdirSync(directory,{withFileTypes:true})){
    const name=prefix+entry.name,path=join(directory,entry.name);
    if(entry.isDirectory()){
     prune(path,name+'/');
     if(readdirSync(path).length===0)rmSync(path,{recursive:true});
    }else if(!notices.has(name))rmSync(path);
   }
  };
  prune(docs);
 }
 cpSync(resolve('third-party/qpdf'),join(destination,'licenses'),{recursive:true});
 if(!windows)chmodSync(join(destination,'bin/qpdf'),0o755);
}
if(process.argv[1]&&resolve(process.argv[1])===resolve('scripts/prepare-qpdf.mjs'))await prepareQpdf(resolve(process.argv[2]||'src-tauri/bin/qpdf'));
