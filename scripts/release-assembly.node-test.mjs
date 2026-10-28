// Requires npm ci: exercise the real Tauri signer without release credentials.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import test from 'node:test';
import {targets} from './release.mjs';
import {verifyUpdate} from './verify-update.mjs';

const root = resolve(import.meta.dirname, '..');
const inputs = ['scripts/release.mjs', 'scripts/package-updates.mjs', 'scripts/verify-update.mjs',
  'package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'cli/Cargo.toml', 'cli/Cargo.lock',
  'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'CHANGELOG.md', 'config/updates/legacy-latest.json'];
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

for (const [label, newline] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`real release assembly selects signed macOS bytes and preserves Windows with ${label}`, () => {
    const temporary = realpathSync(mkdtempSync(join(tmpdir(), 'sag release assembly ')));
    const checkout = join(temporary, 'mini checkout');
    mkdirSync(checkout);
    const run = (command, args, env = process.env) => spawnSync(command, args, {cwd: checkout, env, encoding: 'utf8'});
    const git = (...args) => {
      const result = run('git', args);
      assert.equal(result.status, 0, `Fixture git command failed: ${result.stderr || result.error?.message}`);
      return result.stdout.trim();
    };
    try {
      for (const name of inputs) {
        const destination = join(checkout, name);
        mkdirSync(dirname(destination), {recursive: true});
        if (name === 'config/updates/legacy-latest.json') copyFileSync(join(root, name), destination);
        else writeFileSync(destination, readFileSync(join(root, name), 'utf8').replace(/\r?\n/g, newline));
      }
      symlinkSync(join(root, 'node_modules'), join(checkout, 'node_modules'), process.platform === 'win32' ? 'junction' : 'dir');
      const privateKey = join(temporary, 'disposable signing key');
      const env = {...process.env, TAURI_SIGNING_PRIVATE_KEY: privateKey, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ''};
      const generated = run(process.execPath, [join(checkout, 'node_modules/@tauri-apps/cli/tauri.js'),
        'signer', 'generate', '--ci', '--password', '', '--write-keys', privateKey], env);
      // Signer output contains private key material: never attach it to failures.
      assert.equal(generated.status, 0, 'Disposable updater key generation failed');
      const publicKey = readFileSync(privateKey + '.pub', 'utf8').trim();
      const configPath = join(checkout, 'src-tauri/tauri.conf.json');
      const config = json(configPath);
      config.plugins.updater.pubkey = publicKey;
      writeFileSync(configPath, JSON.stringify(config, null, 2).replace(/\n/g, newline) + newline);
      git('init', '--quiet');
      git('-c', 'core.autocrlf=false', 'add', '--', ...inputs);
      git('-c', 'user.name=Release assembly test', '-c', 'user.email=release-assembly@example.invalid',
        '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'Isolated release assembly fixture');
      const commit = git('rev-parse', 'HEAD');
      const version = json(join(checkout, 'package.json')).version;
      const raw = join(temporary, 'raw artifacts');
      const signed = join(temporary, 'signed artifacts', 'macos');
      const output = join(temporary, 'assembled release');
      const macNames = [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`];
      const windowsName = `SAG-Wiki-${version}-Windows-x64-setup.exe`;
      const rawMetadata = {};
      const bytes = {};
      for (const [platform, names] of [['macos', macNames], ['windows', [windowsName]]]) {
        const directory = join(raw, platform);
        mkdirSync(directory, {recursive: true});
        const files = {};
        for (const name of names) {
          bytes[name] = Buffer.from(`original tested ${platform} bytes: ${name}\0\xff`, 'latin1');
          writeFileSync(join(directory, name), bytes[name]);
          files[name] = hash(bytes[name]);
        }
        rawMetadata[platform] = {schema: 1, version, commit, platform, target: targets[platform],
          build_identity: {version, channel: 'release'}, files};
        writeFileSync(join(directory, 'metadata.json'), JSON.stringify(rawMetadata[platform]));
      }
      mkdirSync(signed, {recursive: true});
      const signedFiles = {};
      for (const name of macNames) {
        bytes[name] = Buffer.from(`Apple signed and stapled final bytes: ${name}\0\xfe`, 'latin1');
        writeFileSync(join(signed, name), bytes[name]);
        signedFiles[name] = hash(bytes[name]);
      }
      const signedMetadata = {...rawMetadata.macos, files: signedFiles, source_files: rawMetadata.macos.files,
        apple_signing: {team_id: 'ABCDEFGHIJ', app_notarization_id: '11111111-1111-4111-8111-111111111111',
          dmg_notarization_id: '22222222-2222-4222-8222-222222222222'}};
      const saveSignedMetadata = metadata => writeFileSync(join(signed, 'metadata.json'), JSON.stringify(metadata));
      saveSignedMetadata(signedMetadata);
      const assemble = destination => run(process.execPath, [join(checkout, 'scripts/release.mjs'), 'assemble',
        '--input', raw, '--signed-macos', signed, '--output', destination], env);
      const assembled = assemble(output);
      assert.equal(assembled.status, 0, `Release assembly failed: ${assembled.stderr || assembled.error?.message}`);
      for (const [name, expected] of Object.entries(bytes)) assert.deepEqual(readFileSync(join(output, name)), expected);
      const manifest = json(join(output, 'latest-v2.json'));
      assert.equal(manifest.version, version);
      for (const [platform, name] of [['darwin-aarch64', macNames[1]], ['windows-x86_64', windowsName]]) {
        const signature = readFileSync(join(output, name + '.sig'), 'utf8').trim();
        assert.equal(manifest.platforms[platform].signature, signature);
        assert.equal(verifyUpdate(bytes[name], signature, publicKey, version), version);
        assert.throws(() => verifyUpdate(bytes[name], signature, publicKey, '999.0.0'), /Signed version/);
      }
      assert.deepEqual(readFileSync(join(output, 'latest.json')), readFileSync(join(root, 'config/updates/legacy-latest.json')));
      assert.deepEqual(json(join(output, 'build-metadata.json')), {repository: 'Zleap-AI/SAG-Wiki', commit, version,
        platforms: {macos: signedMetadata, windows: rawMetadata.windows}});

      const {apple_signing, ...unproved} = signedMetadata;
      saveSignedMetadata(unproved);
      const rejectedOutput = join(temporary, 'must not be created');
      const rejected = assemble(rejectedOutput);
      assert.notEqual(rejected.status, 0);
      assert.match(rejected.stderr, /Missing Apple team/);
      assert.equal(existsSync(rejectedOutput), false, 'Missing Apple proof must fail before creating output');
    } finally {
      rmSync(temporary, {recursive: true, force: true});
    }
  });
}
