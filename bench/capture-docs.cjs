// Run from the repository root against `vite preview -c bench/docs.config.ts`.
const {spawnSync}=require('node:child_process');
for(const name of ['core','files','details']){
 const result=spawnSync(process.execPath,[`bench/docs/${name}.cjs`],{stdio:'inherit',env:process.env});
 if(result.status!==0)process.exit(result.status||1);
}
