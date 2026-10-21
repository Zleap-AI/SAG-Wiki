import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync, chmodSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const arch = args.includes('--arch') ? args[args.indexOf('--arch') + 1] : 'x86';
if (!['x86', 'arm'].includes(arch)) throw new Error('--arch 必须为 x86 或 arm');
const target = arch === 'x86' ? 'x86_64-unknown-linux-gnu' : 'aarch64-unknown-linux-gnu';
const suppliedBinary = args.includes('--binary') ? resolve(args[args.indexOf('--binary') + 1]) : null;
function run(command, arguments_, cwd = root) {
  const result = spawnSync(command, arguments_, { cwd, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} 失败：${result.status}`);
}
run('npm', ['run', 'build']);
if (!suppliedBinary) run('rustup', ['run', 'stable', 'cargo', 'zigbuild', '-p', 'sag-desktop-fnos', '--release', '--target', `${target}.2.35`]);
const binary = suppliedBinary ?? resolve(root, 'target', target, 'release/sag-desktop-fnos');
const binaries = [binary, resolve(dirname(binary), 'sag-fnos-worker')];
for (const path of binaries) {
  const elf = readFileSync(path);
  if (elf.subarray(0, 4).toString('hex') !== '7f454c46' || elf[4] !== 2 || elf.readUInt16LE(18) !== (arch === 'x86' ? 62 : 183)) {
    throw new Error(`程序不是 ${arch} Linux ELF，不能打包：${path}`);
  }
}
const stage = resolve(root, 'release', `package-${arch}`);
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });
cpSync(resolve(root, 'packaging/sag-desktop-fnos'), stage, { recursive: true });
const manifest = resolve(stage, 'manifest');
writeFileSync(manifest, readFileSync(manifest, 'utf8').replace(/^platform=.*$/m, `platform=${arch}`));
mkdirSync(resolve(stage, 'app/bin'), { recursive: true });
for (const path of binaries) {
  const destination = resolve(stage, 'app/bin', path.split('/').at(-1));
  cpSync(path, destination);
  chmodSync(destination, 0o755);
}
cpSync(resolve(root, 'dist'), resolve(stage, 'app/www'), { recursive: true });
cpSync(resolve(root, 'licenses'), resolve(stage, 'app/licenses'), { recursive: true });
cpSync(resolve(root, 'LICENSE'), resolve(stage, 'app/LICENSE'));
const packer = process.env.FNPACK ?? resolve(root, 'tools/fnpack');
if (!existsSync(packer)) throw new Error('缺少 fnpack，请按 README 下载后设置 FNPACK');
run(packer, ['build', '--directory', stage], resolve(root, 'release'));
const generated = resolve(root, 'release/sag-desktop-fnos.fpk');
const version = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')).version;
const artifact = resolve(root, 'release', `sag-desktop-fnos-${version}-${arch}.fpk`);
cpSync(generated, artifact);
const digest = createHash('sha256').update(readFileSync(artifact)).digest('hex');
writeFileSync(`${artifact}.sha256`, `${digest}  ${artifact.split('/').at(-1)}\n`);
console.log(`安装包：${artifact}\nSHA-256：${digest}`);
