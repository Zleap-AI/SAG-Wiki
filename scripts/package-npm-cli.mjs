import {spawnSync} from 'node:child_process';
import {chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync} from 'node:fs';
import {delimiter, dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const run = (command, args, cwd = root) => {
  const result = spawnSync(command, args, {cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit']});
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed (${result.status ?? result.signal})`);
  return result.stdout.trim();
};
const targets = {
  'aarch64-apple-darwin': {platform: 'darwin', arch: 'arm64', file: 'sag'},
  'x86_64-apple-darwin': {platform: 'darwin', arch: 'x64', file: 'sag'},
  'aarch64-unknown-linux-gnu': {platform: 'linux', arch: 'arm64', file: 'sag'},
  'x86_64-unknown-linux-gnu': {platform: 'linux', arch: 'x64', file: 'sag'},
  'aarch64-pc-windows-msvc': {platform: 'win32', arch: 'arm64', file: 'sag.exe'},
  'x86_64-pc-windows-msvc': {platform: 'win32', arch: 'x64', file: 'sag.exe'},
};
const target = process.env.SAG_TARGET || run('rustc', ['-vV']).match(/^host: (.+)$/m)?.[1];
if (!targets[target]) throw new Error(`Unsupported Rust target: ${target}`);
const version = readFileSync(join(root, 'cli/Cargo.toml'), 'utf8').match(/^version = "(.+)"/m)[1];
run('cargo', ['build', '--release', '--locked', '--manifest-path', 'cli/Cargo.toml', ...(process.env.SAG_TARGET ? ['--target', target] : [])]);
const current = targets[target];
const nativeCache = join(root, 'releases/cli-native', version);
const currentDir = join(nativeCache, `${current.platform}-${current.arch}`);
mkdirSync(currentDir, {recursive: true});
copyFileSync(join(root, 'cli/target', ...(process.env.SAG_TARGET ? [target] : []), 'release', current.file), join(currentDir, current.file));

// Only this generated package directory is replaced. The versioned binary cache
// can be combined from separate platform builds before producing the final tarball.
const output = join(root, 'releases/npm');
const packageDir = join(output, 'sag');
rmSync(packageDir, {recursive: true, force: true});
mkdirSync(join(packageDir, 'bin'), {recursive: true});
copyFileSync(join(root, 'npm/sag/bin/sag.cjs'), join(packageDir, 'bin/sag.cjs'));
chmodSync(join(packageDir, 'bin/sag.cjs'), 0o755);
const supported = Object.values(targets).filter(item => existsSync(join(nativeCache, `${item.platform}-${item.arch}`, item.file)));
for (const item of supported) {
  const destination = join(packageDir, 'native', `${item.platform}-${item.arch}`);
  mkdirSync(destination, {recursive: true});
  copyFileSync(join(nativeCache, `${item.platform}-${item.arch}`, item.file), join(destination, item.file));
  if (item.platform !== 'win32') chmodSync(join(destination, item.file), 0o755);
}
const manifest = JSON.parse(readFileSync(join(root, 'npm/sag/package.json'), 'utf8'));
manifest.version = version;
delete manifest.private;
manifest.os = [...new Set(supported.map(item => item.platform))];
manifest.cpu = [...new Set(supported.map(item => item.arch))];
writeFileSync(join(packageDir, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`);
writeFileSync(join(packageDir, 'README.md'), readFileSync(join(root, 'npm/sag/README.md'), 'utf8').replace('{{SUPPORTED_PLATFORMS}}', supported.map(item => `${item.platform}-${item.arch}`).join(', ')));

// Invoke npm's JavaScript entry point so Windows does not need a shell for npm.cmd.
const npmEntry = process.env.npm_execpath || (process.platform === 'win32'
  ? join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')
  : process.env.PATH.split(delimiter).map(dir => join(dir, 'npm')).find(existsSync));
if (!npmEntry) throw new Error('npm was not found on PATH.');
const packed = JSON.parse(run(process.execPath, [realpathSync(npmEntry), 'pack', '--json', '--pack-destination', output], packageDir))[0];
console.log(`Platforms: ${supported.map(item => `${item.platform}-${item.arch}`).join(', ')}`);
console.log(join(output, packed.filename));
