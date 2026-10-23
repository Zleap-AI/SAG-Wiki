import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, cp, mkdir, writeFile, symlink, rm, readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
test('desktop Safari installer policy', {skip: process.platform !== 'darwin'}, async t => {
 const temp = await mkdtemp(path.join(tmpdir(), 'sag-clipper-policy-'));
 t.after(() => rm(temp, {recursive:true, force:true}));
 const extension = path.join(temp, 'browser-extension');
 await mkdir(extension);
 for (const file of ['build.mjs','package.json','static','src']) await cp(path.join(root,'browser-extension',file),path.join(extension,file),{recursive:true});
 await symlink(path.join(root,'browser-extension/node_modules'),path.join(extension,'node_modules'));
 const bin=path.join(temp,'bin'); await mkdir(bin);
 const log=path.join(temp,'assessment.log');
 await writeFile(path.join(bin,'spctl'),'#!/bin/sh\nprintf "%s\\n" "$@" >> "$ASSESS_LOG"\nexit "$ASSESS_EXIT"\n',{mode:0o755});
 const run=(extra={})=>spawnSync(process.execPath,[path.join(extension,'build.mjs'),'--desktop'],{encoding:'utf8',env:{...process.env,PATH:`${bin}:${process.env.PATH}`,SAG_CLIPPER_SAFARI_DMG:'',SAG_REQUIRE_SAFARI_CLIPPER:'',ASSESS_LOG:log,ASSESS_EXIT:'0',...extra}});
 await t.test('missing optional installer keeps Chrome package available',async()=>{
  const result=run(); assert.equal(result.status,0,result.stderr); assert.match(result.stdout,/Safari export is disabled/);
  assert.ok((await readFile(path.join(temp,'src-tauri/clipper/sag-web-clipper-chrome.zip'))).length>0);
 });
 await t.test('required missing installer fails',()=>{assert.notEqual(run({SAG_REQUIRE_SAFARI_CLIPPER:'1'}).status,0);});
 const supplied=path.join(temp,'supplied.dmg'); await writeFile(supplied,'fixture installer');
 await t.test('supplied installer must pass assessment',async()=>{
  assert.notEqual(run({SAG_CLIPPER_SAFARI_DMG:supplied,ASSESS_EXIT:'1'}).status,0);
  assert.match(await readFile(log,'utf8'),/context:primary-signature/);
 });
 await t.test('existing installer is reassessed on the next build',()=>{
  assert.notEqual(run({ASSESS_EXIT:'1'}).status,0);
  const result=run(); assert.equal(result.status,0,result.stderr);
 });
});
