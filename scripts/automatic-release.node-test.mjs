import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {dirname, join, resolve} from 'node:path';
import {tmpdir} from 'node:os';
import test from 'node:test';
import {changedPaths, compareVersions, createPreparedCommit, gitAt, hasProductChanges, isProductPath, latestPublished,
  nextVersion, hasNewerPreparedSource, preparedContents, retryAction, updateVersionFiles, validateReleaseSource, validateReplaceableDraft, validatePublicationAssets, versionFiles} from './automatic-release.mjs';
import {assertNoPendingReleaseDeployments, releaseApiPath} from './migrate-release-environment.mjs';
import {validateVersions} from './release.mjs';

const root = resolve(import.meta.dirname, '..');
function fixture(documentationPath = 'docs/zh-CN/中文 更新 说明.md') {
  const directory = mkdtempSync(join(tmpdir(), 'sag-automatic-release-'));
  for (const name of [...versionFiles, 'CHANGELOG.md']) {
    mkdirSync(dirname(join(directory, name)), {recursive: true});
    copyFileSync(join(root, name), join(directory, name));
  }
  const git = gitAt(directory);
  git('init', '--quiet');
  git('config', 'user.name', 'Release fixture');
  git('config', 'user.email', 'release@example.invalid');
  git('config', 'commit.gpgsign', 'false');
  git('config', 'tag.gpgsign', 'false');
  git('config', 'core.autocrlf', 'false');
  const save = (name, text, message) => {
    mkdirSync(dirname(join(directory, name)), {recursive: true});
    writeFileSync(join(directory, name), text);
    git('add', '--', name); git('commit', '--quiet', '-m', message);
    return git('rev-parse', 'HEAD');
  };
  git('add', '.'); git('commit', '--quiet', '-m', 'Published baseline');
  const baseline = git('rev-parse', 'HEAD');
  const originalVersion = validateVersions(directory);
  git('tag', `v${originalVersion}`);
  const product = save('src/product.txt', 'A real product change\n', 'fix: restore document indexing');
  const source = save(documentationPath, 'Documentation\n', 'docs: explain indexing');
  git('update-ref', 'refs/remotes/origin/main', source);
  const version = nextVersion([originalVersion], [`v${originalVersion}`]);
  const provenance = {schema: 1, source, baseline, version};
  const annotate = (tag, commit, data = provenance) => git('tag', '-a', '-f', tag, commit, '-m', JSON.stringify(data));
  return {directory, git, save, baseline, product, source, version, provenance, annotate,
    close: () => rmSync(directory, {recursive: true, force: true})};
}

test('classifies docs/CI-only pushes independently of cumulative unreleased product changes', () => {
  for (const path of ['docs/zh-CN/release.md', 'docs-site/package-lock.json', '.github/workflows/automatic-release.yml',
    'CHANGELOG.md', 'AGENTS.md', 'scripts/automatic-release.mjs', 'scripts/automatic-release.node-test.mjs', 'scripts/release.mjs',
    'scripts/release-build-cache.node-test.mjs', 'scripts/configure-release.mjs', 'scripts/migrate-release-environment.mjs']) assert.equal(isProductPath(path), false, path);
  for (const path of ['src/App.svelte', 'core/src/lib.rs', 'package-lock.json', 'src-tauri/tauri.conf.json',
    'scripts/build-cli.mjs', 'scripts/sign-macos-release.mjs', 'scripts/package-updates.mjs', 'unknown/new-file']) assert.equal(isProductPath(path), true, path);
  const f = fixture();
  try {
    const paths = (a, b) => changedPaths(f.git, a, b);
    assert.deepEqual(paths(f.product, f.source), ['docs/zh-CN/中文 更新 说明.md']);
    assert.equal(hasProductChanges(paths(f.product, f.source)), false, 'Docs push must not release even with earlier product backlog');
    assert.equal(hasProductChanges(paths(f.baseline, f.source)), true, 'Next product release retains older failed/skipped product changes');
    const mixed = f.save('src/another.txt', 'New feature\n', 'feat: add document previews');
    assert.equal(hasProductChanges(paths(f.source, mixed)), true);
    const notes = preparedContents(f.git, {...f.provenance, source: mixed});
    assert.match(notes['CHANGELOG.md'], /restore document indexing/);
    assert.match(notes['CHANGELOG.md'], /add document previews/);
    assert.doesNotMatch(notes['CHANGELOG.md'].split('## ')[1], /docs: explain indexing/);
  } finally { f.close(); }
});

// Newlines are legal Unix filenames; keep NUL-delimited Git path coverage
// separate from the shared fixture, which must also be creatable on Windows.
test('classifies a newline-containing Git path as one documentation change', {skip: process.platform === 'win32'}, () => {
  const f = fixture('docs/zh-CN/中文 更新\n说明.md');
  try {
    const paths = changedPaths(f.git, f.product, f.source);
    assert.deepEqual(paths, ['docs/zh-CN/中文 更新\n说明.md']);
    assert.equal(hasProductChanges(paths), false);
  } finally { f.close(); }
});

test('allocates above every manifest and occupied remote tag without prerelease confusion', () => {
  assert.equal(nextVersion(['0.2.13', '0.2.14'], ['v0.2.16', 'v0.2.99-beta.1', 'v1.0.0-rc.1']), '0.2.17');
  assert.equal(nextVersion(['0.2.13'], ['v0.10.9', 'v0.9.100']), '0.10.10');
  assert.equal(nextVersion(['0.2.13'], ['v1.0.0']), '1.0.1');
  assert.equal(compareVersions('v0.10.0', '0.9.100'), 1);
  assert.throws(() => nextVersion(['invalid'], []));
});

for (const newline of ['\n', '\r\n']) {
  test(`synchronizes all seven manifests and lock entries while preserving dependencies (${JSON.stringify(newline)})`, () => {
    const files = Object.fromEntries(versionFiles.map(name => [name, readFileSync(join(root, name), 'utf8').replace(/\r?\n/g, newline)]));
    const updated = updateVersionFiles(files, '9.8.7');
    const directory = mkdtempSync(join(tmpdir(), 'sag-version-sync-'));
    try {
      for (const [name, contents] of Object.entries(updated)) {
        mkdirSync(dirname(join(directory, name)), {recursive: true});
        writeFileSync(join(directory, name), contents);
      }
      assert.equal(validateVersions(directory), '9.8.7');
      for (const name of versionFiles) {
        const before = files[name].replace(/\r\n/g, '\n').split('\n');
        const after = updated[name].replace(/\r\n/g, '\n').split('\n');
        assert.equal(before.length, after.length);
        for (let i = 0; i < before.length; i++) if (before[i] !== after[i]) assert.match(after[i], /version.*9\.8\.7/);
      }
    } finally { rmSync(directory, {recursive: true, force: true}); }
  });
}

test('creates a deterministic detached child without touching main/worktree and accepts only its exact tree', () => {
  const f = fixture();
  try {
    const before = f.git('status', '--porcelain');
    const commit = createPreparedCommit(f.directory, f.provenance);
    assert.equal(createPreparedCommit(f.directory, f.provenance), commit, 'Same source/version must yield the same prepared commit');
    assert.equal(f.git('rev-parse', 'HEAD'), f.source);
    assert.equal(f.git('status', '--porcelain'), before);
    assert.equal(f.git('rev-parse', 'origin/main'), f.source);
    f.annotate(`v${f.version}`, commit);
    assert.deepEqual(validateReleaseSource(f.git, commit, `v${f.version}`), f.provenance);
    assert.deepEqual(validateReleaseSource(f.git, f.baseline, `v${validateVersions(f.directory)}`), {source: f.baseline});
    // A later published/main source must not invalidate the older prepared tag.
    const later = f.save('src/later.txt', 'later change\n', 'feat: later product');
    f.git('update-ref', 'refs/remotes/origin/main', later);
    assert.deepEqual(validateReleaseSource(f.git, commit, `v${f.version}`), f.provenance);
    f.git('checkout', '--quiet', '--detach', commit);
    assert.equal(validateVersions(f.directory), f.version);
    const attack = f.save('src/evil.txt', 'unexpected executable content\n', 'tamper');
    f.annotate(`v${f.version}`, attack);
    assert.throws(() => validateReleaseSource(f.git, attack, `v${f.version}`), /exactly one recorded parent/);
  } finally { f.close(); }
});

test('rejects off-main code changes, settings/dependency changes, whitespace, modes and fake provenance', () => {
  const f = fixture();
  try {
    const expected = preparedContents(f.git, f.provenance);
    const tag = `v${f.version}`;
    for (const [name, alter] of [
      ['src/product.txt', () => 'malicious replacement\n'],
      ['src-tauri/tauri.conf.json', text => text.replace('"requireSignedVersion": true', '"requireSignedVersion": false')],
      ['cli/Cargo.toml', text => text + '\n# unexpected dependency/configuration\n'],
      ['CHANGELOG.md', text => text + '\n'],
    ]) {
      f.git('checkout', '--quiet', '--detach', f.source);
      for (const [path, contents] of Object.entries(expected)) writeFileSync(join(f.directory, path), contents);
      const contents = expected[name] ?? readFileSync(join(f.directory, name), 'utf8');
      writeFileSync(join(f.directory, name), alter(contents));
      f.git('add', '.'); f.git('commit', '--quiet', '-m', 'Unexpected release tree');
      const commit = f.git('rev-parse', 'HEAD');
      f.annotate(tag, commit);
      assert.throws(() => validateReleaseSource(f.git, commit, tag), /Unexpected release (?:file delta|contents)/, name);
    }
    f.git('checkout', '--quiet', '--detach', f.source);
    for (const [path, contents] of Object.entries(expected)) writeFileSync(join(f.directory, path), contents);
    f.git('add', '.'); f.git('update-index', '--chmod=+x', 'CHANGELOG.md'); f.git('commit', '--quiet', '-m', 'Unexpected mode');
    const modeCommit = f.git('rev-parse', 'HEAD'); f.annotate(tag, modeCommit);
    assert.throws(() => validateReleaseSource(f.git, modeCommit, tag), /modes changed/);
    const valid = createPreparedCommit(f.directory, f.provenance);
    f.annotate(tag, valid, {...f.provenance, source: f.baseline});
    assert.throws(() => validateReleaseSource(f.git, valid, tag), /recorded parent/);
    f.git('tag', '-d', tag); f.git('tag', tag, valid);
    assert.throws(() => validateReleaseSource(f.git, valid, tag), /JSON/);
  } finally { f.close(); }
});

test('an older queued push cannot get a newer patch after a newer main source was prepared', () => {
  const f = fixture();
  try {
    assert.equal(hasNewerPreparedSource(f.git, f.product, [f.source]), true);
    assert.equal(hasNewerPreparedSource(f.git, f.source, [f.product]), false);
    assert.equal(hasNewerPreparedSource(f.git, f.source, [f.source]), false);
  } finally { f.close(); }
});

test('resumes failed runs instead of duplicating tags, active builds or existing successful drafts', () => {
  const commit = 'a'.repeat(40);
  const tag = 'v0.2.14';
  const run = {id: 123, head_sha: commit, head_branch: tag, event: 'workflow_dispatch', status: 'completed', conclusion: 'failure'};
  assert.deepEqual(retryAction([], commit, tag), {command: 'dispatch'});
  assert.deepEqual(retryAction([{...run, head_branch: 'main', conclusion: 'success'}], commit, tag), {command: 'dispatch'});
  assert.deepEqual(retryAction([{...run, head_branch: 'v0.2.99'}], commit, tag), {command: 'dispatch'});
  assert.deepEqual(retryAction([run], commit, tag), {command: 'rerun', run: 123});
  assert.deepEqual(retryAction([{...run, status: 'in_progress'}], commit, tag), {command: 'none'});
  assert.deepEqual(retryAction([{...run, conclusion: 'success'}], commit, tag), {command: 'none'});
  assert.deepEqual(retryAction([run, {...run, id: 124, status: 'queued'}], commit, tag), {command: 'none'});
});

test('draft recovery never deletes a public, foreign or differently targeted release', () => {
  const context = {tag: 'v0.2.14', commit: 'a'.repeat(40)};
  const draft = {tag_name: context.tag, draft: true, prerelease: false, target_commitish: context.commit,
    author: {login: 'github-actions[bot]', type: 'Bot'}};
  assert.doesNotThrow(() => validateReplaceableDraft(undefined, context));
  assert.doesNotThrow(() => validateReplaceableDraft(draft, context));
  for (const patch of [{draft: false}, {prerelease: true}, {author: {login: 'maintainer', type: 'User'}},
    {target_commitish: 'main'}, {target_commitish: 'b'.repeat(40)}, {tag_name: 'v0.2.15'}]) {
    assert.throws(() => validateReplaceableDraft({...draft, ...patch}, context));
  }
});

test('publication verifies every uploaded asset against the exact successful assembly job checksum output', () => {
  const version = '0.2.14';
  const names = [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`,
    `SAG-Wiki-${version}-macOS-arm64.app.tar.gz.sig`, `SAG-Wiki-${version}-Windows-x64-setup.exe`,
    `SAG-Wiki-${version}-Windows-x64-setup.exe.sig`, 'latest-v2.json', 'latest.json', 'build-metadata.json', 'release-notes.md'];
  const hash = 'a'.repeat(64);
  const checksums = names.map(name => `${hash}  ${name}`).join('\n') + '\n';
  const digest = createHash('sha256').update(checksums).digest('hex');
  const release = {assets: [...names.map(name => ({name, digest: `sha256:${hash}`, size: 42, state: 'uploaded'})),
    {name: `v${version}-SHA256SUMS.txt`, digest: `sha256:${digest}`, size: checksums.length, state: 'uploaded'}]};
  assert.doesNotThrow(() => validatePublicationAssets(release, checksums, digest, version));
  assert.throws(() => validatePublicationAssets(release, checksums, 'b'.repeat(64), version), /assembly/);
  assert.throws(() => validatePublicationAssets({...release, assets: release.assets.slice(1)}, checksums, digest, version), /draft assets/);
  const corrupted = structuredClone(release); corrupted.assets[0].digest = `sha256:${'b'.repeat(64)}`;
  assert.throws(() => validatePublicationAssets(corrupted, checksums, digest, version), /checksum mismatch/);
  const incomplete = structuredClone(release); incomplete.assets[0].state = 'new';
  assert.throws(() => validatePublicationAssets(incomplete, checksums, digest, version), /incomplete/);
});

test('stable latest selection never rolls back or uses drafts and prereleases', () => {
  const releases = [
    {tag_name: 'v0.2.15', draft: false, prerelease: false}, {tag_name: 'v0.2.14', draft: false, prerelease: false},
    {tag_name: 'v0.2.99', draft: true, prerelease: false}, {tag_name: 'v0.3.0-beta.1', draft: false, prerelease: true},
  ];
  assert.equal(latestPublished(releases).tag_name, 'v0.2.15');
  assert.equal(compareVersions('0.2.14', latestPublished(releases).tag_name) > 0, false);
  assert.equal(compareVersions('0.2.16', latestPublished(releases).tag_name) > 0, true);
});

test('migration refuses pending release deployments and imports without contacting GitHub', () => {
  assert.doesNotThrow(() => assertNoPendingReleaseDeployments([]));
  assert.doesNotThrow(() => assertNoPendingReleaseDeployments([{environment: {name: 'preview'}}]));
  assert.throws(() => assertNoPendingReleaseDeployments([{environment: {name: 'release'}}]), /pending release deployments/);
  const imported = spawnSync(process.execPath, ['--input-type=module', '-e', `import ${JSON.stringify(new URL('./migrate-release-environment.mjs', import.meta.url).href)}`],
    {encoding: 'utf8', env: {...process.env, PATH: ''}});
  assert.equal(imported.status, 0, imported.stderr);
  assert.equal(imported.stdout, '');
});

for (const [label, newline] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`workflows dispatch the exact tag and publish only after all final checks with ${label}`, () => {
    const automatic = readFileSync(join(root, '.github/workflows/automatic-release.yml'), 'utf8').replace(/\r?\n/g, newline);
    const desktop = readFileSync(join(root, '.github/workflows/desktop-release.yml'), 'utf8').replace(/\r?\n/g, newline);
    const helper = readFileSync(join(root, 'scripts/automatic-release.mjs'), 'utf8');
    assert.match(automatic, /push:\r?\n\s+branches: \[main\]/);
    assert.doesNotMatch(automatic, /pull_request|workflow_dispatch/);
    assert.match(automatic, /needs: classify\r?\n\s+if: needs\.classify\.outputs\.eligible == 'true'/);
    assert.match(automatic, /group: automatic-desktop-release-preparation\r?\n\s+cancel-in-progress: false\r?\n\s+queue: max/);
    assert.match(desktop, /on:\r?\n  workflow_dispatch:/);
    assert.doesNotMatch(desktop.slice(0, desktop.indexOf('permissions:')), /push:/);
    assert.match(helper, /gh\('workflow', 'run', 'desktop-release.yml', '--repo', repository, '--ref', tag\)/);
    assert.doesNotMatch(helper, /git\('push'.*(?:main|--force)/);
    assert.ok(desktop.indexOf('release-build-cache.mjs wait all') < desktop.indexOf('release.mjs assemble'));
    assert.ok(desktop.indexOf('release.mjs assemble') < desktop.indexOf('actions/attest-build-provenance'));
    assert.ok(desktop.indexOf('actions/attest-build-provenance') < desktop.indexOf('automatic-release.mjs create-draft'));
    assert.match(desktop, /publish:\r?\n\s+name: Publish verified desktop release\r?\n\s+needs: \[validate, release\]/);
    assert.match(desktop, /group: desktop-release-publication\r?\n\s+cancel-in-progress: false\r?\n\s+queue: max/);
    assert.match(desktop, /run: node scripts\/automatic-release.mjs publish/);
  });
}


test('migration uses the canonical repository endpoint without a trailing slash', () => {
  assert.equal(releaseApiPath(''), 'repos/Zleap-AI/SAG-Wiki');
  assert.equal(releaseApiPath('environments/release'), 'repos/Zleap-AI/SAG-Wiki/environments/release');
});


test('migration dry-run handles large API responses without writes', {skip: process.platform === 'win32'}, () => {
  const directory = mkdtempSync(join(tmpdir(), 'sag-migration-api-'));
  try {
    const shim = `#!${process.execPath}\nconst args=process.argv.slice(2), path=args.at(-1); if(args.includes('--method') || path.endsWith('/')) process.exit(9); let data;if(path==='repos/Zleap-AI/SAG-Wiki') data={permissions:{admin:true},description:'x'.repeat(2*1024*1024)};else if(path.endsWith('/deployment-branch-policies')) data={branch_policies:[{name:'v*',type:'tag'}]};else if(path.endsWith('/environments/release')) data={can_admins_bypass:false,protection_rules:[]};else process.exit(8);process.stdout.write(JSON.stringify(data));\n`;
    writeFileSync(join(directory, 'gh'), shim); chmodSync(join(directory, 'gh'), 0o755);
    const result = spawnSync(process.execPath, [join(root, 'scripts/migrate-release-environment.mjs')], {encoding:'utf8',env:{...process.env,PATH:directory+':'+process.env.PATH}});
    assert.equal(result.status, 0, result.stderr); assert.match(result.stdout, /dry-run/);
  } finally { rmSync(directory, {recursive:true,force:true}); }
});
