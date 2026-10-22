import assert from 'node:assert/strict';
import {randomBytes} from 'node:crypto';
import {existsSync, mkdirSync, writeFileSync, chmodSync, realpathSync} from 'node:fs';
import {resolve, join, sep, dirname, basename} from 'node:path';
import {parseArgs} from 'node:util';
import {createRequire} from 'node:module';

const {values} = parseArgs({options: {output: {type: 'string'}}});
assert.ok(values.output, 'Use --output /secure/directory outside the repository');
let output = resolve(values.output);
const root = realpathSync(resolve(import.meta.dirname, '..'));
assert.ok(output !== root && !output.startsWith(root + sep), 'Signing secrets must stay outside the repository');
assert.ok(!existsSync(output), 'Refusing to overwrite existing signing material');
mkdirSync(dirname(output), {recursive: true, mode: 0o700});
output = join(realpathSync(dirname(output)), basename(output));
assert.ok(output !== root && !output.startsWith(root + sep), 'Signing directory resolves inside the repository');
mkdirSync(output, {recursive: true, mode: 0o700});
chmodSync(output, 0o700);
const password = randomBytes(32).toString('base64');
writeFileSync(join(output, 'updater.password'), password, {mode: 0o600, flag: 'wx'});
// Call Tauri in this process so the generated password never appears in OS
// process arguments. Its private key is encrypted before it is written.
const {run} = createRequire(import.meta.url)('@tauri-apps/cli');
await run(['signer', 'generate', '--ci', '--password', password, '--write-keys', join(output, 'updater.key')]);
chmodSync(join(output, 'updater.key'), 0o600);
chmodSync(join(output, 'updater.key.pub'), 0o600);
console.log(`Encrypted updater key and separate password file saved in ${output}. Transfer both to an approved recovery vault after configuring Environment Secrets.`);
