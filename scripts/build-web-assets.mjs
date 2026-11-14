import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';

export function buildWebAssets() {
  const directory=resolve('dist-web');
  const vite=resolve('node_modules/vite/bin/vite.js');
  const result=spawnSync(process.execPath,[vite,'build','--outDir',directory],{stdio:'inherit'});
  if(result.status!==0)throw new Error('Web build failed',{cause:result.error});
  return directory;
}
