import assert from 'node:assert/strict';
import {parseArgs} from 'node:util';
import {readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {verifyUpdate} from './verify-update.mjs';

const root = resolve(import.meta.dirname, '..');
export const legacyDigest = 'c4999619677baaa384c1b35d198f73517c5fc63801a99e4f8aee26b1aa5efde8';

export function packageUpdates({version, macos, windows, notes = '', output}) {
  assert.match(version, /^\d+\.\d+\.\d+$/, 'A stable release version is required');
  assert.ok(macos?.endsWith('.app.tar.gz') && windows?.endsWith('.exe'), 'Both macOS .app.tar.gz and Windows NSIS .exe are required');
  const keySource = process.env.TAURI_SIGNING_PRIVATE_KEY;
  assert.ok(keySource?.trim(), 'TAURI_SIGNING_PRIVATE_KEY must be provided by the release environment');
  const key = existsSync(keySource) ? readFileSync(keySource, 'utf8') : keySource;
  const publicKey = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).plugins.updater.pubkey;
  const destination = resolve(output ?? join(root, 'releases', version, 'updates'));
  const legacy = readFileSync(join(root, 'config/updates/legacy-latest.json'));
  assert.equal(createHash('sha256').update(legacy).digest('hex'), legacyDigest, 'Legacy update manifest must remain unchanged');
  mkdirSync(destination, {recursive: true});
  const platforms = {};
  for (const [platform, input, name] of [
    ['darwin-aarch64', macos, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`],
    ['windows-x86_64', windows, `SAG-Wiki-${version}-Windows-x64-setup.exe`],
  ]) {
    const target = join(destination, name);
    if (resolve(input) !== target) copyFileSync(resolve(input), target);
    const result = spawnSync(process.execPath, [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'sign', '--app-version', version, target], {
      cwd: root, encoding: 'utf8',
      env: {...process.env, TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? ''},
    });
    // Never include the signer's output in logs: it runs with production secrets.
    assert.equal(result.status, 0, `Signing failed for ${platform}; check the configured key and password`);
    const signature = readFileSync(target + '.sig', 'utf8').trim();
    verifyUpdate(readFileSync(target), signature, publicKey, version);
    platforms[platform] = {url: `https://github.com/Zleap-AI/SAG-Wiki/releases/download/v${version}/${name}`, signature};
  }
  const manifest = {version, notes, pub_date: new Date().toISOString(), platforms};
  writeFileSync(join(destination, 'latest-v2.json'), JSON.stringify(manifest, null, 2) + '\n');
  writeFileSync(join(destination, 'latest.json'), legacy);
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const {values} = parseArgs({options: Object.fromEntries(['version', 'macos', 'windows', 'notes', 'output'].map(name => [name, {type: 'string'}]))});
  packageUpdates({...values, notes: values.notes ? readFileSync(resolve(values.notes), 'utf8') : ''});
  console.log('Both update signatures and signed versions verified; latest-v2.json and legacy latest.json generated.');
}
