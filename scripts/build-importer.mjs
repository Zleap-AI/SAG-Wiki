import {build} from 'esbuild';
import {mkdir,copyFile,writeFile,rm} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createRequire} from 'node:module';
import vm from 'node:vm';
const require=createRequire(import.meta.url);
const pbjs=require('protobufjs-cli/pbjs');
const schema=await build({entryPoints:['third-party/obsidian-importer/src/formats/apple-notes/descriptor.ts'],bundle:true,write:false,format:'cjs'});
const context={module:{exports:{}}};vm.runInNewContext(schema.outputFiles[0].text,context);
await writeFile('importer/schema.json',JSON.stringify(context.module.exports.descriptor));
try{
 const source=await new Promise((resolve,reject)=>pbjs.main(['-t','static-module','-w','es6','--no-create','--no-encode','--no-verify','--no-convert','--no-delimited','importer/schema.json'],(error,output)=>error?reject(error):resolve(output)));
 await writeFile('importer/generated-protobuf.js',source);
}finally{await rm('importer/schema.json');}
await mkdir('public/importer',{recursive:true});
await copyFile('importer/frame.html','public/importer/frame.html');
const result=await build({entryPoints:['importer/frame.ts'],outfile:'public/importer/frame.js',bundle:true,tsconfigRaw:{compilerOptions:{useDefineForClassFields:false}},format:'iife',platform:'browser',target:'es2022',minify:true,metafile:true,alias:{obsidian:resolve('importer/obsidian.ts')},plugins:[{name:'static-notes-protobuf',setup(b){b.onResolve({filter:/^protobufjs$/},()=>({path:resolve('importer/protobuf.ts')}));}}],external:['node:*'],logLevel:'info'});
if(process.argv.includes('--inputs'))console.log(Object.keys(result.metafile.inputs).filter(path=>path.startsWith('third-party/')).join('\n'));
