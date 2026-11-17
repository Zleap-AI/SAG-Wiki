import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
import {existsSync} from 'node:fs';

export function buildWebAssets({cwd=process.cwd(),env=process.env,run=spawnSync} = {}) {
  if (env.SAG_WEB_DIST) {
    const directory=resolve(cwd,env.SAG_WEB_DIST);
    if (!existsSync(resolve(directory,'index.html'))) throw new Error('Supplied release web assets are missing');
    return directory;
  }
  const directory=resolve(cwd,'dist-web');
  const vite=resolve(cwd,'node_modules/vite/bin/vite.js');
  const result=run(process.execPath,[vite,'build','--outDir',directory],{stdio:'inherit',cwd,env});
  if(result.status!==0)throw new Error('Web build failed',{cause:result.error});
  return directory;
}
