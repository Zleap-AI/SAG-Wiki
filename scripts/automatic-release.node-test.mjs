import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {dirname, join, resolve} from 'node:path';
import {tmpdir} from 'node:os';
import test from 'node:test';
import {changedPaths, compareVersions, createPreparedCommit, validateReleaseDocumentation, releaseNotes, releaseBody, validatePullRequestNotes, gitAt, hasProductChanges, isProductPath, latestPublished,
  shouldSkipRelease, nextVersion, releaseVersion, hasNewerPreparedSource, preparedContents, publishDraftById, releaseForTag, retryAction, updateVersionFiles, validateReleaseSource, validateReplaceableDraft, validatePublicationAssets, versionFiles} from './automatic-release.mjs';
import {assertNoPendingReleaseDeployments, releaseApiPath} from './migrate-release-environment.mjs';
import {validateVersions} from './release.mjs';

const root = resolve(import.meta.dirname, '..');
test('every release body exposes Windows and both Mac installers without changing updater notes', () => {
  const notes = '## 0.3.10\n\n- 中文\n\n### English\n\n- English\n';
  const body = releaseBody('0.3.10', notes);
  assert.ok(body.startsWith(notes));
  for (const name of ['macOS-arm64.dmg','macOS-x86_64.dmg','Windows-x64-setup.exe']) assert.ok(body.includes(`https://github.com/Zleap-AI/SAG-Wiki/releases/download/v0.3.10/SAG-Wiki-0.3.10-${name}`));
  assert.throws(()=>releaseBody('../unsafe',notes));
});
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
  mkdirSync(join(directory, 'scripts'), {recursive: true});
  writeFileSync(join(directory, 'scripts/automatic-release.mjs'), '// Historical release preparation before reviewed-notes schema.\n');
  git('add', '.'); git('commit', '--quiet', '-m', 'Published baseline');
  const baseline = git('rev-parse', 'HEAD');
  const originalVersion = validateVersions(directory);
  git('tag', `v${originalVersion}`);
  writeFileSync(join(directory, 'CHANGELOG.md'), '# Changelog\n\n## Unreleased\n\n- 改善文档索引。\n\n### English\n\n- Improve document indexing.\n');
  git('add', 'CHANGELOG.md');
  const product = save('src/product.txt', 'A real product change\n', 'fix: restore document indexing');
  const source = save(documentationPath, 'Documentation\n', 'docs: explain indexing');
  git('update-ref', 'refs/remotes/origin/main', source);
  const version = nextVersion([originalVersion], [`v${originalVersion}`]);
  const provenance = {schema: 2, source, baseline, version};
  const annotate = (tag, commit, data = provenance) => git('tag', '-a', '-f', tag, commit, '-m', JSON.stringify(data));
  return {directory, git, save, baseline, product, source, version, provenance, annotate,
    close: () => rmSync(directory, {recursive: true, force: true})};
}

test('requires reviewed bilingual notes before preparing a release, never commit subjects', () => {
  const f = fixture();
  try {
    for (const notes of ['# Changelog\n',
      '# Changelog\n\n## Unreleased\n\n- 修复公式。\n',
      '# Changelog\n\n## Unreleased\n\n自动发布：包含产品改动。\n\n### English\n\n- Fix formulas.\n']) {
      const source = f.save('CHANGELOG.md', notes, 'feat: internal commit must never become release notes');
      assert.throws(() => createPreparedCommit(f.directory, {...f.provenance, source}), /release notes|bilingual|changelog/i);
      assert.deepEqual(f.git('tag', '--list').split('\n'), [`v${validateVersions(f.directory)}`]);
    }
  } finally { f.close(); }
});

test('renames reviewed Unreleased notes to the allocated version without commit boilerplate', () => {
  const f = fixture();
  try {
    const notes = '# 更新日志 / Changelog\n\n## Unreleased\n\n- 修复公式显示。\n\n### English\n\n- Fix formula rendering.\n\n## 0.1.0\n\nOld release.\n';
    const source = f.save('CHANGELOG.md', notes, 'fix(chat): should not appear (abcdef1)');
    const updated = preparedContents(f.git, {...f.provenance, source});
    assert.equal(updated['CHANGELOG.md'], notes.replace('## Unreleased', `## ${f.version}`));
  } finally { f.close(); }
});

test('successive off-main releases include only new Unreleased bilingual bullet pairs', () => {
  const f = fixture();
  try {
    const first = createPreparedCommit(f.directory, f.provenance);
    const firstNotes = f.git.blob(first, 'CHANGELOG.md');
    assert.match(firstNotes, /Improve document indexing/);
    const pending = f.git.blob(f.source, 'CHANGELOG.md').replace('- 改善文档索引。', '- 改善文档索引。\n- 保留公式上下标。')
      .replace('- Improve document indexing.', '- Improve document indexing.\n- Preserve formula scripts.');
    const source = f.save('CHANGELOG.md', pending, 'docs: append the next product notes');
    const provenance = {...f.provenance, baseline: f.source, source, version: nextVersion([f.version], [])};
    const second = createPreparedCommit(f.directory, provenance);
    const notes = f.git.blob(second, 'CHANGELOG.md');
    assert.match(notes, /保留公式上下标。/);
    assert.match(notes, /Preserve formula scripts./);
    assert.doesNotMatch(notes, /改善文档索引|Improve document indexing/);
    assert.equal(f.git.blob(source, 'CHANGELOG.md'), pending, 'Main source notes remain intact');
    assert.equal(createPreparedCommit(f.directory, provenance), second);
    assert.throws(() => createPreparedCommit(f.directory, {...provenance, baseline: source}), /unpublished|new.*notes/i);
  } finally { f.close(); }
});

test('preserves issue and PR links after functional descriptions but rejects link-only entries', () => {
  const issue = '[Issue #123](https://github.com/Zleap-AI/SAG-Wiki/issues/123)';
  const pr = '[PR #142](https://github.com/Zleap-AI/SAG-Wiki/pull/142)';
  const wrap = (zh, en) => `# Changelog\n\n## 9.8.7\n\n- ${zh}\n\n### English\n\n- ${en}\n`;
  const notes = wrap(`修复公式显示。（${issue}，${pr}）`, `Fix formula rendering. (${issue}, ${pr})`);
  assert.equal(releaseNotes(notes, '9.8.7'), notes.slice(notes.indexOf('## 9.8.7')));
  for (const [zh, en] of [[`修复公式显示。 ${pr}`, pr], ['[问题 #123](https://github.com/Zleap-AI/SAG-Wiki/issues/123)', 'Fix formula rendering.']]) {
    assert.throws(() => releaseNotes(wrap(zh, en), '9.8.7'), /change descriptions/i);
  }
});

test('rejects boilerplate, commit subjects, hashes, migration prose and malformed bilingual sections', () => {
  const wrap = (zh, en = '- Fix formula rendering.') => `# Changelog\n\n## 9.8.7\n\n${zh}\n\n### English\n\n${en}\n`;
  assert.equal(releaseNotes(wrap('- 修复公式显示。'), '9.8.7'), '## 9.8.7\n\n- 修复公式显示。\n\n### English\n\n- Fix formula rendering.\n');
  for (const notes of [wrap('- 自动发布：包含产品改动。'), wrap('- 修复公式。', '- Automatic release: product changes since the previous published release.'),
    wrap('- 修复公式。', '- fix(chat): fix formulas'), wrap('- 修复公式。', '- Fix formulas (abcdef1)'),
    wrap('- 修复公式。', '### Update signing migration\n\n- Install old versions.'), wrap('- 修复公式。', ''),
    wrap('Missing Chinese bullets'), wrap('- 修复公式。', '- 修复公式。')]) {
    assert.throws(() => releaseNotes(notes, '9.8.7'), /release notes|bilingual/i);
  }
  assert.throws(() => releaseNotes(wrap('- 修复公式。') + wrap('- 修复公式。'), '9.8.7'), /duplicate/i);
  assert.doesNotThrow(() => releaseNotes(wrap('- 支持 1000000 条记录。', '- Support 1000000 records.'), '9.8.7'));
});

test('premerge product checks require changed reviewed notes while infra changes need none', () => {
  const f = fixture();
  try {
    assert.equal(validatePullRequestNotes(f.git, f.product, f.source), false);
    const source = f.save('CHANGELOG.md', '# Changelog\n', 'docs: omit pending notes');
    assert.throws(() => validatePullRequestNotes(f.git, f.baseline, source), /release notes|bilingual/i);
    assert.throws(() => validatePullRequestNotes(f.git, f.baseline, f.source), /numbered|Unreleased/i);
    const missingDocs = f.save('CHANGELOG.md', f.git.blob(f.source, 'CHANGELOG.md').replace('## Unreleased', `## ${f.version}`), 'docs: use numbered release notes');
    assert.throws(() => validatePullRequestNotes(f.git, f.baseline, missingDocs), /updates.mdx/);
    f.save('docs-site/content/docs/updates.mdx', `## ${f.version}\n\n- 改善文档索引。\n`, 'docs: Chinese notes');
    const numbered = f.save('docs-site/content/docs/updates.en.mdx', `## ${f.version}\n\n- Improve document indexing.\n`, 'docs: English notes');
    assert.equal(validatePullRequestNotes(f.git, f.baseline, numbered), true);
    f.save('CHANGELOG.md', f.git.blob(numbered, 'CHANGELOG.md') + '\n', 'docs: whitespace only');
    const changed = f.save('src/extra.txt', 'Extra product', 'feat: extra');
    assert.throws(() => validatePullRequestNotes(f.git, numbered, changed), /unpublished|new.*notes/i);
    const product = f.save('src/new.txt', 'Unexplained change', 'feat: new change');
    assert.throws(() => validatePullRequestNotes(f.git, changed, product), /notes|CHANGELOG/i);
  } finally { f.close(); }
});

test('premerge checks reject whitespace-only changes to existing version-specific notes', () => {
  const f = fixture();
  try {
    const notes = `# Changelog\n\n## ${f.version}\n\n- 修复公式。\n\n### English\n\n- Fix formulas.\n`;
    const base = f.save('CHANGELOG.md', notes, 'docs: reviewed upcoming release');
    f.save('src/new-product.txt', 'New functionality', 'feat: next functionality');
    const source = f.save('CHANGELOG.md', notes + '\n', 'docs: whitespace only');
    assert.throws(() => validatePullRequestNotes(f.git, base, source), /unpublished|new.*notes/i);
  } finally { f.close(); }
});

test('validates historical schema-1 trees but refuses new schema-1 preparation and downgrade', () => {
  const f = fixture();
  try {
    const legacy = {...f.provenance, schema: 1};
    const prepareLegacy = provenance => {
      f.git('checkout', '--quiet', '--detach', provenance.source);
      const files = Object.fromEntries(versionFiles.map(name => [name, f.git.blob(provenance.source, name)]));
      for (const [name, contents] of Object.entries(updateVersionFiles(files, provenance.version))) writeFileSync(join(f.directory, name), contents);
      const original = f.git.blob(provenance.source, 'CHANGELOG.md');
      const heading = original.indexOf('\n');
      // Historical schema-1 output is an exact contract, including boilerplate
      // and the product commit's subject/hash. Later policy must still verify it.
      const section = `\n## ${provenance.version}\n\n自动发布：包含自上次正式发布以来的产品改动。\n\nAutomatic release: product changes since the previous published release.\n\n- fix: restore document indexing (${f.product.slice(0, 7)})\n`;
      writeFileSync(join(f.directory, 'CHANGELOG.md'), original.slice(0, heading + 1) + section + original.slice(heading + 1));
      f.git('add', '.'); f.git('commit', '--quiet', '-m', 'Historical prepared release');
      return f.git('rev-parse', 'HEAD');
    };
    const commit = prepareLegacy(legacy);
    f.annotate(`v${f.version}`, commit, legacy);
    assert.deepEqual(validateReleaseSource(f.git, commit, `v${f.version}`), legacy);
    assert.throws(() => createPreparedCommit(f.directory, legacy), /reviewed bilingual notes/);
    f.git('checkout', '--quiet', '--detach', f.source);
    const source = f.save('scripts/automatic-release.mjs', readFileSync(join(root, 'scripts/automatic-release.mjs'), 'utf8'), 'ci: reviewed-notes policy');
    f.git('update-ref', 'refs/remotes/origin/main', source);
    const downgrade = {...legacy, source};
    const downgraded = prepareLegacy(downgrade);
    f.annotate(`v${f.version}`, downgraded, downgrade);
    assert.throws(() => validateReleaseSource(f.git, downgraded, `v${f.version}`), /reviewed-notes schema/);
  } finally { f.close(); }
});

test('classifies docs/CI-only pushes independently of cumulative unreleased product changes', () => {
  for (const path of ['docs/zh-CN/release.md', 'docs-site/package-lock.json', '.github/workflows/automatic-release.yml',
    'CHANGELOG.md', 'AGENTS.md', 'scripts/automatic-release.mjs', 'scripts/automatic-release.node-test.mjs', 'scripts/release.mjs',
    'scripts/release-build-cache.node-test.mjs', 'scripts/test-profile-ab.mjs', 'scripts/test-profile-ab.node-test.mjs',
    'scripts/parallel-cli.mjs', 'scripts/parallel-cli.node-test.mjs', 'scripts/rust-dependency-cache.mjs',
    'scripts/rust-dependency-cache.node-test.mjs', 'scripts/rust-dependency-archive.py', 'scripts/run-pr-e2e.py', 'tests/local-jev-e2e.py', 'scripts/pr-test-plan.mjs', 'scripts/pr-test-plan.node-test.mjs', 'scripts/configure-release.mjs', 'scripts/migrate-release-environment.mjs']) assert.equal(isProductPath(path), false, path);
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
    assert.match(notes['CHANGELOG.md'], /Improve document indexing/);
    assert.doesNotMatch(notes['CHANGELOG.md'], /restore document indexing|add document previews|自动发布|Automatic release:/);
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

test('keeps the reviewed bilingual changelog for the allocated release version', () => {
  const f = fixture();
  try {
    const changelog = `# 更新日志 / Changelog\n\n## ${f.version}\n\n- 改善文档索引。\n\n### English\n\n- Improve document indexing.\n`;
    const source = f.save('CHANGELOG.md', changelog, 'docs: write bilingual release notes');
    f.git('update-ref', 'refs/remotes/origin/main', source);
    const provenance = {...f.provenance, source};
    const commit = createPreparedCommit(f.directory, provenance);
    const tag = `v${f.version}`;
    f.annotate(tag, commit, provenance);
    assert.equal(f.git.blob(commit, 'CHANGELOG.md'), changelog);
    assert.deepEqual(validateReleaseSource(f.git, commit, tag), provenance);
  } finally { f.close(); }
});

test('releases an unused reviewed manifest version without skipping it', () => {
  const f = fixture();
  try {
    const tag = `v${f.version}`;
    const notes = `# 更新日志 / Changelog\n\n## ${f.version}\n\n- 改善文档索引。\n\n### English\n\n- Improve document indexing.\n`;
    const files = Object.fromEntries(versionFiles.map(name => [name, f.git.blob(f.source, name)]));
    for (const [name, content] of Object.entries(updateVersionFiles(files, f.version))) {
      writeFileSync(join(f.directory, name), content);
    }
    writeFileSync(join(f.directory, 'CHANGELOG.md'), notes);
    f.git('add', '.');
    f.git('commit', '-m', 'Prepare the reviewed version');
    const source = f.git('rev-parse', 'HEAD');
    f.git('update-ref', 'refs/remotes/origin/main', source);
    const manifests = versionFiles.map(() => validateVersions(f.directory));
    assert.equal(releaseVersion(manifests, f.git('tag').split('\n'), notes), f.version);
    assert.equal(releaseVersion(manifests, [tag], notes), nextVersion(manifests, [tag]));
    assert.equal(releaseVersion(manifests, [], '# Changelog\n'), nextVersion(manifests, []));
    const provenance = {...f.provenance, source};
    const commit = createPreparedCommit(f.directory, provenance);
    f.annotate(tag, commit, provenance);
    assert.deepEqual(changedPaths(f.git, source, commit), []);
    assert.deepEqual(validateReleaseSource(f.git, commit, tag), provenance);
  } finally { f.close(); }
});

test('keeps a reviewed minor version bump above all occupied tags', () => {
  const notes = '# 更新日志 / Changelog\n\n## 0.3.0\n\n- 新增数据导入。\n\n### English\n\n- Add data import.\n';
  const manifests = versionFiles.map(() => '0.3.0');
  assert.equal(releaseVersion(manifests, ['v0.2.18'], notes), '0.3.0');
  assert.equal(releaseVersion(manifests, ['v0.3.0'], notes), '0.3.1');
  assert.equal(releaseVersion(manifests, ['v0.3.1'], notes), '0.3.2');
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

test('publication finds a draft through its listed release ID when tag lookup is unavailable', () => {
  const draft = {id: 42, tag_name: 'v0.2.16', draft: true};
  const calls = [];
  const list = path => { calls.push(path); return [{id: 41, tag_name: 'v0.2.15'}, draft]; };
  const read = path => {
    calls.push(path);
    if (path !== 'releases/42') throw new Error('Not Found (HTTP 404)');
    return {...draft, assets: [{name: 'verified-package'}]};
  };
  assert.deepEqual(releaseForTag('v0.2.16', list, read), {...draft, assets: [{name: 'verified-package'}]});
  assert.deepEqual(calls, ['releases', 'releases/42']);
  assert.throws(() => releaseForTag('v0.2.99', list, read), /not found/i);
});

test('publication verifies every uploaded asset against the exact successful assembly job checksum output', () => {
  const version = '0.2.14';
  const names = [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`,
    `SAG-Wiki-${version}-macOS-arm64.app.tar.gz.sig`,
    `SAG-Wiki-${version}-macOS-x86_64.dmg`, `SAG-Wiki-${version}-macOS-x86_64.app.tar.gz`, `SAG-Wiki-${version}-macOS-x86_64.app.tar.gz.sig`, `SAG-Wiki-${version}-Windows-x64-setup.exe`,
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

test('publishing uses numeric asset and release IDs and reconciles an uncertain write', async () => {
  const release = {id: 42, assets: [{id: 730, name: 'v0.3.13-SHA256SUMS.txt'}]};
  const calls = [];
  let reads = 0;
  const download = id => {
    calls.push(['download', id]);
    if (++reads === 1) throw new Error('HTTP 404');
    return 'verified bytes';
  };
  const validate = bytes => assert.equal(bytes, 'verified bytes');
  const update = (id, latest) => {
    calls.push(['update', id, latest]);
    throw new Error('spawnSync gh ETIMEDOUT');
  };
  const read = id => { calls.push(['read', id]); return {draft: false, id}; };
  await publishDraftById(release, 'v0.3.13', validate, () => [{tag_name: 'v0.3.14', draft: false, prerelease: false}], {download, update, read, pause:async()=>{}});
  assert.deepEqual(calls, [['download',730],['download',730],['update',42,false],['read',42]]);
  let writes = 0;
  await assert.rejects(publishDraftById(release,'v0.3.13',validate,()=>[],{
    download: () => 'verified bytes', update: () => { writes++; throw new Error('spawnSync gh ETIMEDOUT'); },
    read: () => { throw new Error('HTTP 503'); }, pause:async()=>{},
  }), /503/);
  assert.equal(writes,1,'a failed state check must not result in another PATCH');
  await assert.rejects(publishDraftById(release,'v0.3.13',()=>{throw new Error('bad SHA256');}, ()=>[], {
    download: () => 'different bytes', update: () => assert.fail('cannot publish unverified assets'), read,
  }), /bad SHA256/);
  await assert.rejects(publishDraftById({...release,assets:[...release.assets,...release.assets]},'v0.3.13',validate,()=>[],
    {download, update, read}), /unique checksum/);
});

test('does not rewrite a published release when the by-ID read still reports draft', async () => {
  const tag = 'v0.3.13';
  const release = {id: 42, draft: true, tag_name: tag, assets: [{id: 730, name: `${tag}-SHA256SUMS.txt`}]};
  let lists = 0;
  const writes = [];
  await publishDraftById(release, tag, bytes => assert.equal(bytes, 'verified bytes'),
    () => ++lists === 1 ? [] : [{...release, draft: false, prerelease: false}], {
      download: () => 'verified bytes',
      update: (id, makeLatest) => {
        writes.push([id, makeLatest]);
        throw new Error('spawnSync gh ETIMEDOUT');
      },
      read: () => release,
      pause: async () => {},
    });
  assert.equal(lists, 2);
  assert.deepEqual(writes, [[42, true]]);
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


test('release skip is scoped to all product commits in the captured push', () => {
  const f = fixture();
  try {
    const pr = (merge, labels = ['release:skip']) => ({state:'closed', merged_at:'2026-11-15T00:00:00Z', merge_commit_sha:merge,
      base:{ref:'main',repo:{full_name:'Zleap-AI/SAG-Wiki'}},labels:labels.map(name=>({name}))});
    const check = (before, source, lookup) => shouldSkipRelease(f.git, before, source, lookup);
    assert.equal(check(f.baseline,f.source,()=>[pr(f.product)]),true);
    assert.throws(()=>check(f.baseline,f.source,()=>[]),/Cannot uniquely verify/);
    assert.throws(()=>check(f.baseline,f.source,()=>[pr(f.product),pr(f.product)]),/Cannot uniquely verify/);
    assert.equal(check(f.baseline,f.source,()=>[pr(f.product,[])]),false);
    for (const patch of [{state:'open'}, {merged_at:null}, {base:{ref:'dev',repo:{full_name:'Zleap-AI/SAG-Wiki'}}},
      {base:{ref:'main',repo:{full_name:'fork/SAG-Wiki'}}}, {merge_commit_sha:f.baseline}]) {
      assert.throws(()=>check(f.baseline,f.source,()=>[{...pr(f.product),...patch}]),/Cannot uniquely verify/);
    }
    assert.throws(()=>check(f.baseline,f.source,()=>{throw Error('API unavailable');}),/API unavailable/);
    const next=f.save('src/next.txt','next feature','feat: next feature');
    const lookup=commit=>[pr(commit===f.product?f.product:next,commit===f.product?['release:skip']:[])];
    assert.equal(check(f.baseline,next,lookup),false,'mixed push retains ordinary feature release');
    assert.throws(()=>check(f.baseline,next,commit=>commit===f.product?[]:[pr(next,[])]),/Cannot uniquely verify/,'validate every product even after an ordinary feature');
    assert.equal(check(f.source,next,lookup),false,'old skip label cannot suppress a later feature');
    assert.equal(check(f.baseline,next,()=>[pr(next)]),true,'rebased product commits may share the final merged PR commit');
    assert.ok(changedPaths(f.git,f.baseline,next).includes('src/product.txt'),'next release includes skipped changes');
    assert.equal(check(f.product,f.source,()=>{throw Error('docs need no PR API');}),false);
    f.git('checkout','--quiet','-b','skip-topic');
    f.save('src/merged.txt','merged change','ci: parallel build input');
    f.git('checkout','--quiet','-');
    f.git('merge','--quiet','--no-ff','skip-topic','-m','Merge internal PR');
    const merged=f.git('rev-parse','HEAD');
    assert.equal(check(next,merged,()=>[pr(merged)]),true,'normal merge uses its main parent');
    assert.equal(check(next,merged,()=>[pr(merged,['release:skip-extra'])]),false,'label match is exact');
    assert.deepEqual(f.git('tag','--list').split('\n'),[`v${validateVersions(f.directory)}`],'classification creates no tags');
  } finally { f.close(); }
});


test('release documentation requires matching Chinese and English version entries', () => {
  const files = {
    'CHANGELOG.md': '## 9.8.7\n\n- 修复阅读。\n\n### English\n\n- Fix reading.\n',
    'docs-site/content/docs/updates.mdx': '## 9.8.7\n\n- 修复阅读。\n',
    'docs-site/content/docs/updates.en.mdx': '## 9.8.7\n\n- Fix reading.\n',
  };
  const git = {blob: (_, name) => { assert.ok(name in files, `Missing ${name}`); return files[name]; }};
  assert.doesNotThrow(() => validateReleaseDocumentation(git, 'HEAD', '9.8.7'));
  for (const name of ['updates.mdx', 'updates.en.mdx']) {
    const path = `docs-site/content/docs/${name}`, saved = files[path];
    files[path] = saved.replace('9.8.7', '9.8.6');
    assert.throws(() => validateReleaseDocumentation(git, 'HEAD', '9.8.7'), /must document release/);
    files[path] = saved.replace(/^- .+$/m, '- Stale content.');
    assert.throws(() => validateReleaseDocumentation(git, 'HEAD', '9.8.7'), /must match/);
    delete files[path];
    assert.throws(() => validateReleaseDocumentation(git, 'HEAD', '9.8.7'), /Missing/);
    files[path] = saved;
  }
  files['CHANGELOG.md'] = files['CHANGELOG.md'].replace('修复阅读。', '修复阅读。（[PR #149](https://github.com/Zleap-AI/SAG-Wiki/pull/149)）');
  assert.doesNotThrow(() => validateReleaseDocumentation(git, 'HEAD', '9.8.7'));
});
