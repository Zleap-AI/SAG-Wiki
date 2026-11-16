import {afterEach,expect,it} from 'vitest';
import {mkdtemp,writeFile,rm,symlink} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createServer,get} from 'node:http';
import {documentFixtureMiddleware} from '../bench/document-fixture-server.mjs';

const cleanup:(()=>Promise<void>)[]=[];
afterEach(async()=>{for(const stop of cleanup.splice(0).reverse())await stop();});
async function serve(fixturePath:string){
 const server=createServer(documentFixtureMiddleware(fixturePath));
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 cleanup.push(()=>new Promise(resolve=>server.close(()=>resolve())));
 const address=server.address();if(!address||typeof address==='string')throw new Error('No loopback port');
 return `http://127.0.0.1:${address.port}`;
}
it('serves only the configured fixture, with an anonymous document identity and reproducible byte/hash metadata',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'sag-document-bench-'));cleanup.push(()=>rm(directory,{recursive:true,force:true}));
 const path=join(directory,'private-customer.md');await writeFile(path,'# 合成测试\n');
 const response=await fetch(await serve(path)+'/__document-fixture');
 const payload=await response.json();
 expect(response.headers.get('cache-control')).toBe('no-store');
 expect(payload.document).toMatchObject({path:'/benchmark/fixture.md',name:'fixture.md',body:'# 合成测试\n'});
 expect(payload.bytes).toBe(15);expect(payload.sha256).toMatch(/^[0-9a-f]{64}$/);
 expect(JSON.stringify(payload)).not.toContain('private-customer');
});
it('rejects cross-origin requests and arbitrary paths without disclosing the configured path',async()=>{
 const url=await serve('/private/missing-customer.md');
 const cross=await fetch(url+'/__document-fixture',{headers:{Origin:'https://external.example'}});
 expect(cross.status).toBe(403);expect(await cross.text()).not.toContain('customer');
 const rebound=await new Promise<number|undefined>((resolve,reject)=>{get(url+'/__document-fixture',{headers:{Host:'external.example',Origin:'http://external.example'}},response=>{response.resume();resolve(response.statusCode);}).on('error',reject);});
 expect(rebound).toBe(403);
 expect((await fetch(url+'/__document-fixture?path=/etc/passwd')).status).toBe(404);
 expect((await fetch(url+'/__document-fixture',{method:'POST'})).status).toBe(405);
 const missing=await fetch(url+'/__document-fixture');expect(missing.status).toBe(503);expect(await missing.text()).not.toContain('customer');
});
it('loads declared local image bytes through opaque ids and refuses traversal, outside files, symlinks and undeclared files',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'sag-document-images-'));cleanup.push(()=>rm(directory,{recursive:true,force:true}));
 const fixturePath=join(directory,'fixture.md');
 await writeFile(join(directory,'declared.png'),new Uint8Array([137,80,78,71]));
 await writeFile(join(directory,'undeclared.png'),'private');
 await symlink(join(directory,'undeclared.png'),join(directory,'link.png'));
 await writeFile(fixturePath,'![ok](declared.png)\n![bad](../outside.png)\n![bad](link.png)\n![bad](sub/../undeclared.png)\n');
 const url=await serve(fixturePath),payload=await (await fetch(url+'/__document-fixture')).json();
 expect(payload.images).toEqual([{path:'/benchmark/declared.png',id:0}]);
 const image=await (await fetch(url+'/__document-image/0')).json();
 expect(image.data).toBe('data:image/png;base64,iVBORw==');
 expect(JSON.stringify(image)).not.toContain(directory);
 expect((await fetch(url+'/__document-image/1')).status).toBe(404);
 expect((await fetch(url+'/__document-image/undeclared.png')).status).toBe(404);
 // Replacing an approved file with a symlink must not bypass the request-time check.
 await rm(join(directory,'declared.png'));await symlink(join(directory,'undeclared.png'),join(directory,'declared.png'));
 expect((await fetch(url+'/__document-image/0')).status).toBe(404);
});
