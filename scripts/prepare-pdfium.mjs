import {createHash} from 'node:crypto';
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const version = '8125';
const hashes = {
  'mac-arm64': '4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d',
  'mac-x64': '8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0',
  'win-x64': '9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b',
  'win-arm64': 'd394ba61991fb53f4d87f70988035f337d9e73f2bd2c3c1b966df8c43fcabb05',
  'linux-arm64': '8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa',
  'linux-x64': 'e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659',
};

export async function preparePdfium(destination, target = process.env.TAURI_ENV_TARGET_TRIPLE || `${process.arch}-${process.platform}`) {
  const platform = target.includes('darwin') ? 'mac' : /windows|win32/.test(target) ? 'win' : target.includes('linux') ? 'linux' : '';
  const arch = /aarch64|arm64/.test(target) ? 'arm64' : /x86_64|x64/.test(target) ? 'x64' : '';
  const variant = `${platform}-${arch}`;
  if (!hashes[variant]) throw Error(`Unsupported PDFium target: ${target}`);
  const name = `pdfium-${variant}.tgz`;
  const archive = new URL(`../third-party/pdfium/${version}/${name}`, import.meta.url);
  const bytes = readFileSync(archive);
  if (createHash('sha256').update(bytes).digest('hex') !== hashes[variant]) throw Error(`PDFium checksum mismatch: ${name}`);
  mkdirSync(destination, {recursive: true});
  const entry = {mac: 'lib/libpdfium.dylib', win: 'bin/pdfium.dll', linux: 'lib/libpdfium.so'}[platform];
  // GNU tar treats a Windows drive letter in the archive path as a remote host.
  const library = spawnSync('tar', ['-xOzf', '-', entry], {input: bytes, maxBuffer: 64 * 1024 * 1024});
  if (library.status !== 0 || library.stdout.length === 0) throw Error(`PDFium library extraction failed: ${library.stderr?.toString().trim() || library.error?.message || 'empty output'}`);
  writeFileSync(resolve(destination, entry.split('/').at(-1)), library.stdout);
  const notices = spawnSync('tar', ['-xzf', '-', 'LICENSE', 'licenses'], {cwd: destination, input: bytes});
  if (notices.status !== 0) throw Error(`PDFium license extraction failed: ${notices.stderr?.toString().trim() || notices.error?.message || `exit ${notices.status}`}`);
  if (process.platform === 'darwin' && platform === 'mac') {
    const sign = spawnSync('codesign', ['--force', '--sign', '-', resolve(destination, entry.split('/').at(-1))], {stdio: 'inherit'});
    if (sign.status !== 0) throw Error('PDFium ad hoc signing failed', {cause: sign.error});
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await preparePdfium(resolve(process.argv[2] || 'src-tauri/bin/pdfium'));
}
