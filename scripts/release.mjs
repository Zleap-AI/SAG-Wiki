import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync} from 'node:fs';
import {basename, join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
import {packageUpdates} from './package-updates.mjs';

const root = resolve(import.meta.dirname, '..');
const repository = 'Zleap-AI/SAG-Wiki';
export const targets = {macos: 'aarch64-apple-darwin', windows: 'x86_64-pc-windows-msvc'};
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {cwd: root, encoding: 'utf8', ...options});
  assert.equal(result.status, 0, `${command} failed: ${result.stderr?.trim() || result.error?.message || result.status}`);
  return result.stdout.trim();
};
const git = (...args) => run('git', args);

export function validateVersions(directory = root) {
  const version = json(join(directory, 'package.json')).version;
  assert.match(version, /^\d+\.\d+\.\d+$/, 'Stable version required');
  const lock = json(join(directory, 'package-lock.json'));
  for (const other of [lock.version, lock.packages[''].version, json(join(directory, 'src-tauri/tauri.conf.json')).version]) {
    assert.equal(other, version, 'Desktop versions must agree');
  }
  for (const [directoryName, packageName] of [['cli', 'sag-cli'], ['src-tauri', 'sag-desktop']]) {
    const manifest = readFileSync(join(directory, directoryName, 'Cargo.toml'), 'utf8');
    assert.equal(manifest.match(/^version = "([^"]+)"/m)?.[1], version, 'Cargo manifest version mismatch');
    const cargoLock = readFileSync(join(directory, directoryName, 'Cargo.lock'), 'utf8').replace(/\r\n/g, '\n');
    assert.equal(cargoLock.match(new RegExp(`\\[\\[package\\]\\]\\nname = "${packageName}"\\nversion = "([^"]+)"`))?.[1], version, 'Cargo lock version mismatch');
  }
  assert.equal(json(join(directory, 'src-tauri/tauri.conf.json')).plugins.updater.requireSignedVersion, true, 'Signed versions must be enforced');
  return version;
}

function validate() {
  const version = validateVersions();
  const commit = git('rev-parse', 'HEAD');
  if (process.env.GITHUB_SHA) assert.equal(commit, process.env.GITHUB_SHA, 'Checkout must match the triggering commit');
  if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
    assert.equal(process.env.GITHUB_REF, `refs/tags/v${version}`, 'Tag must exactly match the release version');
    git('merge-base', '--is-ancestor', commit, 'origin/main');
    assert.equal(git('rev-parse', `refs/tags/v${version}^{commit}`), commit, 'Release tag moved');
  }
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\ncommit=${commit}\n`);
  console.log(JSON.stringify({version, commit}));
}

function expectedFiles(platform, version) {
  return platform === 'macos'
    ? [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`]
    : [`SAG-Wiki-${version}-Windows-x64-setup.exe`];
}

function stage({platform, target}) {
  assert.equal(target, targets[platform], 'Unexpected release target');
  const version = validateVersions();
  const commit = git('rev-parse', 'HEAD');
  const bundle = join(root, 'src-tauri/target', target, 'release/bundle');
  const output = join(root, 'release-artifacts', platform);
  assert.ok(!existsSync(output), 'Staging directory must be fresh');
  mkdirSync(output, {recursive: true});
  const names = expectedFiles(platform, version);
  if (platform === 'macos') {
    const dmgs = readdirSync(join(bundle, 'dmg')).filter(name => name.endsWith('.dmg'));
    assert.equal(dmgs.length, 1, 'Expected exactly one DMG');
    copyFileSync(join(bundle, 'dmg', dmgs[0]), join(output, names[0]));
    // This is an ad-hoc build input. The protected finalizer adds Apple tickets
    // before creating the public updater archive with its metadata preserved.
    run('tar', ['-czf', join(output, names[1]), '-C', join(bundle, 'macos'), 'SAG Wiki.app'], {env:{...process.env, COPYFILE_DISABLE:'1'}});
  } else {
    const installers = readdirSync(join(bundle, 'nsis')).filter(name => name.endsWith('.exe'));
    assert.equal(installers.length, 1, 'Expected exactly one NSIS installer');
    copyFileSync(join(bundle, 'nsis', installers[0]), join(output, names[0]));
  }
  const cli = join(root, 'cli/target', target, 'release', platform === 'windows' ? 'sag.exe' : 'sag');
  const identity = JSON.parse(run(cli, ['--build-info']));
  assert.equal(identity.version, version, 'Bundled CLI version mismatch');
  assert.equal(identity.channel, 'release', 'Development executable cannot be released');
  const metadata = {schema: 1, version, commit, platform, target, build_identity: identity, files: Object.fromEntries(names.map(name => [name, digest(join(output, name))]))};
  writeFileSync(join(output, 'metadata.json'), JSON.stringify(metadata, null, 2) + '\n');
  console.log(`Staged ${platform} ${version} from ${commit}`);
}

export function validateStagedArtifacts(input, version, commit) {
  const manifests = {};
  for (const platform of Object.keys(targets)) {
    const directory = join(input, platform);
    const metadata = json(join(directory, 'metadata.json'));
    assert.equal(metadata.schema, 1, 'Unsupported artifact metadata');
    assert.equal(metadata.version, version, 'Artifact version mismatch');
    assert.equal(metadata.commit, commit, 'Artifacts must come from the same release commit');
    assert.equal(metadata.platform, platform, 'Artifact platform mismatch');
    assert.equal(metadata.target, targets[platform], 'Artifact target mismatch');
    assert.equal(metadata.build_identity.version, version, 'CLI identity version mismatch');
    assert.equal(metadata.build_identity.channel, 'release', 'Development CLI cannot be released');
    assert.deepEqual(Object.keys(metadata.files).sort(), expectedFiles(platform, version).sort(), 'Unexpected or missing release artifacts');
    assert.deepEqual(readdirSync(directory).sort(), [...Object.keys(metadata.files), 'metadata.json'].sort(), 'Untracked files in release staging');
    for (const [name, sha256] of Object.entries(metadata.files)) {
      assert.equal(basename(name), name, 'Invalid artifact path');
      assert.equal(digest(join(directory, name)), sha256, 'Artifact checksum mismatch');
    }
    manifests[platform] = metadata;
  }
  return manifests;
}

export function validateSignedMacOSArtifacts(directory, source) {
  const metadata = json(join(directory, 'metadata.json'));
  for (const field of ['schema', 'version', 'commit', 'platform', 'target', 'build_identity']) {
    assert.deepEqual(metadata[field], source[field], `Signed macOS ${field} mismatch`);
  }
  assert.equal(source.platform, 'macos', 'Expected original macOS build');
  assert.deepEqual(metadata.source_files, source.files, 'Signed macOS artifacts must derive from the tested source');
  assert.deepEqual(Object.keys(metadata.files).sort(), expectedFiles('macos', source.version).sort(), 'Unexpected signed macOS artifacts');
  assert.deepEqual(readdirSync(directory).sort(), [...Object.keys(metadata.files), 'metadata.json'].sort(), 'Untracked signed macOS files');
  assert.match(metadata.apple_signing?.team_id ?? '', /^[A-Z0-9]{10}$/, 'Missing Apple team');
  for (const kind of ['app', 'dmg']) {
    assert.match(metadata.apple_signing?.[`${kind}_notarization_id`] ?? '', /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i, `Missing ${kind} notarization`);
  }
  for (const [name, sha256] of Object.entries(metadata.files)) {
    assert.equal(basename(name), name, 'Invalid signed artifact path');
    assert.equal(digest(join(directory, name)), sha256, 'Signed artifact checksum mismatch');
  }
  return metadata;
}

function releaseNotes(version) {
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8').replace(/\r\n/g, '\n');
  const section = changelog.split(/^## /m).find(text => text.startsWith(`${version}\n`) || text.startsWith(`${version} `));
  assert.ok(section, 'Changelog must describe this release');
  return `## ${section.trim()}\n\n### 更新签名迁移 / Update signing migration\n\n0.1.8 及更早版本的用户请手动下载安装一次 0.1.9。旧更新通道保留在 0.1.8，后续自动更新从 0.1.9 的新签名通道继续。\n\nUsers on 0.1.8 or earlier must manually install 0.1.9 once. The legacy update channel remains on 0.1.8; subsequent automatic updates use the new signing channel.\n`;
}

function assemble({input, output, 'signed-macos': signedMacOS}) {
  const version = validateVersions();
  const commit = git('rev-parse', 'HEAD');
  const source = resolve(input ?? 'release-artifacts');
  const metadata = validateStagedArtifacts(source, version, commit);
  const signed = resolve(signedMacOS ?? 'signed-release-artifacts/macos');
  metadata.macos = validateSignedMacOSArtifacts(signed, metadata.macos);
  const destination = resolve(output ?? join(root, 'releases', version, 'updates'));
  assert.ok(!existsSync(destination), 'Release output must be fresh');
  mkdirSync(destination, {recursive: true});
  const notes = releaseNotes(version);
  packageUpdates({version, notes, output: destination,
    macos: join(signed, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`),
    windows: join(source, 'windows', `SAG-Wiki-${version}-Windows-x64-setup.exe`),
  });
  copyFileSync(join(signed, `SAG-Wiki-${version}-macOS-arm64.dmg`), join(destination, `SAG-Wiki-${version}-macOS-arm64.dmg`));
  writeFileSync(join(destination, 'release-notes.md'), notes);
  writeFileSync(join(destination, 'build-metadata.json'), JSON.stringify({repository, commit, version, platforms: metadata}, null, 2) + '\n');
  const checksums = readdirSync(destination).sort().map(name => `${digest(join(destination, name))}  ${name}`).join('\n') + '\n';
  writeFileSync(join(destination, `v${version}-SHA256SUMS.txt`), checksums);
  console.log(`Verified dual-platform release assembled at ${destination}`);
}

export function validateEnvironment(environment, policies) {
  assert.equal(environment.can_admins_bypass, false, 'Administrators must not bypass release approval');
  const reviewers = environment.protection_rules?.find(rule => rule.type === 'required_reviewers');
  assert.ok(reviewers?.reviewers?.length > 0, 'release environment must require reviewers');
  assert.equal(reviewers.prevent_self_review, true, 'Self-approval must be disabled');
  assert.equal(environment.deployment_branch_policy?.custom_branch_policies, true, 'Custom tag restrictions required');
  assert.equal(environment.deployment_branch_policy?.protected_branches, false, 'Only explicit release tags may deploy');
  assert.ok(policies.length > 0 && policies.every(policy => policy.type === 'tag' && policy.name === 'v*'), 'Only v* tags may access release secrets');
}

export function verifyEnvironment() {
  const environment = JSON.parse(run('gh', ['api', `repos/${repository}/environments/release`]));
  const policies = JSON.parse(run('gh', ['api', `repos/${repository}/environments/release/deployment-branch-policies`]));
  validateEnvironment(environment, policies.branch_policies);
  console.log('Release environment protection and tag restrictions verified.');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const {positionals, values} = parseArgs({allowPositionals: true, options: Object.fromEntries(['platform', 'target', 'input', 'output', 'signed-macos'].map(name => [name, {type: 'string'}]))});
  const commands = {validate, stage, assemble, 'verify-environment': verifyEnvironment};
  assert.ok(commands[positionals[0]], 'Use validate, stage, assemble or verify-environment');
  commands[positionals[0]](values);
}
