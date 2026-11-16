import test from 'node:test';
import assert from 'node:assert/strict';
import {generatePages,manifest,verifyDeployment} from './documentation-release.mjs';
import {isProductPath} from './automatic-release.mjs';
const source='## 1.2.3\n\n- 修复。\n\n### English\n\n- Fix.\n';
const sha='a'.repeat(40);
test('generation is deterministic and preserves both translations and release links',()=>{
 const result=generatePages(source);assert.deepEqual(result.versions,['1.2.3']);assert.match(result.pages[0],/修复。/);assert.match(result.pages[1],/Fix\./);assert.deepEqual(generatePages(source.replaceAll('\n','\r\n')),result);
 assert.throws(()=>generatePages(source+source),/Duplicate/);assert.throws(()=>generatePages(source.replace('### English','### Other')),/translation/);assert.throws(()=>generatePages(source.replace('- Fix.','')),/Empty/);
 const linked=source.replace('Fix.','Fix. ([PR #1](https://github.com/Zleap-AI/SAG-Wiki/pull/1))');assert.match(generatePages(linked).pages[1],/https:\/\/github.com/);
});
test('deployment verification rejects stale manifests, mismatched HTML and missing releases',async()=>{
 const expected=manifest(source,sha);
 const run=(data=expected,staleLanguage=null)=>verifyDeployment({url:'https://docs.example',expected,release:'v1.2.3',fetcher:async url=>url.pathname==='/docs-version.json'?{status:200,json:async()=>data}:{status:200,text:async()=>`<span data-docs-changelog-sha="${url.pathname.includes(staleLanguage)?'old':expected.pages[url.pathname.split('/')[1]].contentSha256}" />`}});
 await run();await assert.rejects(run({...expected,sourceCommit:'b'.repeat(40)}),/differs/);await assert.rejects(run({...expected,changelogSha256:'old'}),/differs/);for(const lang of ['zh','en'])await assert.rejects(run(expected,lang),/Stale/);
 await assert.rejects(verifyDeployment({url:'https://docs.example',expected,release:'v9.9.9'}),/not documented/);
 await assert.rejects(verifyDeployment({url:'http://docs.example',expected}),/HTTPS/);
 await assert.rejects(verifyDeployment({url:'https://docs.example',expected,fetcher:async()=>({status:404})}),/response/);
});
test('documentation automation cannot trigger a desktop release',()=>{
 for(const path of ['scripts/documentation-release.mjs','scripts/documentation-release.node-test.mjs','docs-site/package.json','.github/workflows/pr-e2e.yml','CHANGELOG.md'])assert.equal(isProductPath(path),false,path);
});

test('CLI lint rejects manual drift in either generated page',async()=>{
 const {mkdtempSync,mkdirSync,writeFileSync,copyFileSync,rmSync}=await import('node:fs');
 const {tmpdir}=await import('node:os');const {join}=await import('node:path');const {spawnSync}=await import('node:child_process');
 const folder=mkdtempSync(join(tmpdir(),'sag-docs-lint-'));
 try{
  mkdirSync(join(folder,'scripts'));mkdirSync(join(folder,'docs-site/content/docs'),{recursive:true});
  copyFileSync(new URL('./documentation-release.mjs',import.meta.url),join(folder,'scripts/documentation-release.mjs'));
  writeFileSync(join(folder,'CHANGELOG.md'),source);
  const run=command=>spawnSync(process.execPath,[join(folder,'scripts/documentation-release.mjs'),command],{encoding:'utf8'});
  assert.equal(run('generate').status,0);assert.equal(run('lint').status,0);
  for(const name of ['updates.mdx','updates.en.mdx']){writeFileSync(join(folder,'docs-site/content/docs',name),'Manual stale content');const failed=run('lint');assert.notEqual(failed.status,0);assert.match(failed.stderr,/Stale generated/);assert.equal(run('generate').status,0);}
 }finally{rmSync(folder,{recursive:true,force:true});}
});
