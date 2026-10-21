import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {resolve, dirname} from 'node:path';
import {zipSync} from 'fflate';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const output=resolve(root,'public/skills');
mkdirSync(output,{recursive:true});
for(const [archive,directory,files] of [
 ['sag-cli-skill.zip','cli/skills/sag',['SKILL.md','references/setup.md','references/parameters.md','references/ignores.md']],
 ['sag-fnos-skill.zip','skills/sag-fnos',['SKILL.md']],
]){
 const name=directory.split('/').at(-1);
 const entries=Object.fromEntries(files.map(file=>[`${name}/${file}`,[readFileSync(resolve(root,directory,file)),{mtime:new Date('2020-01-01T00:00:00Z')}]]));
 writeFileSync(resolve(output,archive),zipSync(entries));
}
