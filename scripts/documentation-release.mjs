import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,realpathSync,existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const root=resolve(import.meta.dirname,'..');
const hash=text=>createHash('sha256').update(text).digest('hex');
export function generatePages(source){
 const normalized=source.replace(/\r\n/g,'\n');
 assert.doesNotMatch(normalized,/^## (?!\d+\.\d+\.\d+(?:\s|$))/m,'Use explicit version headings');
 const entries=[...normalized.matchAll(/^## (\d+\.\d+\.\d+)(?:[^\n]*)\n([\s\S]*?)(?=^## |$(?![\s\S]))/gm)];
 assert.ok(entries.length,'No versioned changelog entries');
 const versions=entries.map(e=>e[1]);assert.equal(new Set(versions).size,versions.length,'Duplicate changelog version');
 const bodies=[[],[]];
 for(const [,version,body] of entries){const translations=body.split(/^### English[ \t]*$/m);assert.equal(translations.length,2,`Missing/duplicate English translation: ${version}`);for(let i=0;i<2;i++){assert.match(translations[i],/^- \S/m,`Empty translation: ${version}`);bodies[i].push(`## ${version}\n\n${translations[i].trim()}`);}}
 const metadata=[['更新日志','按版本查看主要功能与改进。','按版本记录主要变化，详细操作见对应功能文档。安装包见[下载安装](/zh/docs/installation)。'],['Changelog','Major features and improvements by version.','Major changes by version. See the feature guides for instructions and the [Installation page](/en/docs/installation) for downloads.']];
 const pages=bodies.map((sections,i)=>{const [title,description,intro]=metadata[i],body=sections.join('\n\n');return `---\ntitle: "${title}"\ndescription: "${description}"\n---\n\n{/* Generated from CHANGELOG.md. Run npm --prefix docs-site run changelog:generate. */}\n\n${intro}\n\n<span data-docs-changelog-sha="${hash(body)}" aria-hidden="true" />\n\n${body}\n`;});
 return {versions,pages};
}
export function manifest(source,sourceCommit){assert.match(sourceCommit,/^[a-f0-9]{40}$/);const {versions,pages}=generatePages(source);return {schema:1,sourceDirty:false,sourceCommit,changelogSha256:hash(source.replace(/\r\n/g,'\n')),versions,pages:Object.fromEntries(['zh','en'].map((lang,i)=>[lang,{sha256:hash(pages[i]),contentSha256:/data-docs-changelog-sha="([a-f0-9]+)"/.exec(pages[i])[1]}]))};}
export async function verifyDeployment({url,expected,release,fetcher=fetch}){
 const base=new URL(url);assert.ok(base.protocol==='https:'||(base.protocol==='http:'&&['127.0.0.1','localhost','[::1]'].includes(base.hostname)),'Use HTTPS or loopback HTTP');
 if(release)assert.ok(expected.versions.includes(release.replace(/^v/,'')),'Expected release is not documented');
 async function get(path){const endpoint=new URL(path,base);endpoint.searchParams.set('docs-check',String(Date.now()));const response=await fetcher(endpoint,{cache:'no-store',redirect:'error',signal:AbortSignal.timeout(15000)});assert.equal(response.status,200,`Deployment response: ${path}`);return response;}
 const actual=await (await get('/docs-version.json')).json();assert.deepEqual(actual,expected,'Deployed documentation version/source/content differs from expected build');
 for(const lang of ['zh','en']){const html=await (await get(`/${lang}/docs/updates`)).text();assert.ok(html.includes(`data-docs-changelog-sha="${expected.pages[lang].contentSha256}"`),`Stale ${lang} changelog page`);}
}
async function main(){
 const command=process.argv[2],source=readFileSync(resolve(root,'CHANGELOG.md'),'utf8'),generated=generatePages(source);
 for(let i=0;i<2;i++){const path=resolve(root,`docs-site/content/docs/updates${i?'.en':''}.mdx`);if(command==='generate')writeFileSync(path,generated.pages[i]);else assert.equal(readFileSync(path,'utf8').replace(/\r\n/g,'\n'),generated.pages[i],`Stale generated file ${path}; run changelog:generate`);}
 if(command==='generate'||command==='lint')return;
 const option=name=>process.argv[process.argv.indexOf(name)+1];
 const commit=execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();
 const expected=manifest(source,commit);
 const dirty=execFileSync('git',['status','--porcelain','--','CHANGELOG.md','docs-site','scripts/documentation-release.mjs'],{cwd:root,encoding:'utf8'}).trim();
 if(command==='verify')assert.equal(dirty,'','Deployment verification requires a clean checkout of the expected source');
 expected.sourceDirty=Boolean(dirty);
 if(command==='build'){writeFileSync(resolve(root,'docs-site/public/docs-version.json'),JSON.stringify(expected,null,2)+'\n');return;}
 assert.equal(command,'verify','Use generate, lint, build or verify');assert.ok(process.argv.includes('--url'),'Missing --url');assert.ok(process.argv.includes('--expected-sha'),'Missing --expected-sha');assert.equal(option('--expected-sha'),commit,'Check out the expected deployed commit before verification');
 await verifyDeployment({url:option('--url'),expected,release:process.argv.includes('--release')?option('--release'):undefined});console.log('Deployed bilingual documentation matches the expected commit and changelog');
}
if(process.argv[1]&&existsSync(resolve(process.argv[1]))&&import.meta.url===pathToFileURL(realpathSync(resolve(process.argv[1]))).href)main().catch(error=>{console.error(error.message);process.exitCode=1;});
