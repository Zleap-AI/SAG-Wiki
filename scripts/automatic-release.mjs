import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';

const root = resolve(import.meta.dirname, '..');
const repository = 'Zleap-AI/SAG-Wiki';
export const versionFiles = ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json',
  'cli/Cargo.toml', 'cli/Cargo.lock', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock'];
export const preparedFiles = [...versionFiles, 'CHANGELOG.md'];
const stable = /^(?:v)?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const sha = /^[a-f0-9]{40}$/;
const execute = (command, args, options = {}) => {
  const {trim = true, ...spawnOptions} = options;
  const result = spawnSync(command, args, {cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, ...spawnOptions});
  assert.equal(result.status, 0, `${command} failed: ${result.stderr?.trim() || result.error?.message || result.status}`);
  return trim ? result.stdout.trim() : result.stdout;
};
export const gitAt = directory => Object.assign((...args) => execute('git', args, {cwd: directory}), {
  blob: (ref, name) => execute('git', ['show', `${ref}:${name}`], {cwd: directory, trim: false}),
  raw: (...args) => execute('git', args, {cwd: directory, trim: false}),
});
const git = gitAt(root);
const gh = (...args) => execute('gh', args);
const api = path => JSON.parse(gh('api', `repos/${repository}/${path}`));
const pages = (path, field) => JSON.parse(gh('api', '--paginate', '--slurp', `repos/${repository}/${path}${path.includes('?') ? '&' : '?'}per_page=100`))
  .flatMap(page => field ? page[field] : page);
const blob = (git, ref, name) => git.blob(ref, name);

export function compareVersions(a, b) {
  const left = a.match(stable), right = b.match(stable);
  assert.ok(left && right, 'Stable semantic versions required');
  for (let i = 1; i <= 3; i++) {
    const x = BigInt(left[i]), y = BigInt(right[i]);
    if (x !== y) return x > y ? 1 : -1;
  }
  return 0;
}
export function nextVersion(manifests, tags) {
  const versions = [...manifests, ...tags.filter(tag => /^v/.test(tag) && stable.test(tag)).map(tag => tag.slice(1))];
  assert.ok(versions.length, 'A version baseline is required');
  const maximum = versions.reduce((a, b) => compareVersions(a, b) >= 0 ? a : b);
  const [major, minor, patch] = maximum.split('.');
  return `${major}.${minor}.${BigInt(patch) + 1n}`;
}

// Keep this marker stable: schema-1 tags predate the reviewed-notes policy.
export const releaseNotesSchema = 2;

function changelogSection(changelog, title) {
  const text = changelog.replace(/\r\n/g, '\n');
  const headings = [...text.matchAll(/^## ([^\n]+)(?:\n|$)/gm)];
  const matches = headings.filter(match => match[1] === title || match[1].startsWith(`${title} `));
  assert.ok(matches.length <= 1, `Duplicate changelog release notes: ${title}`);
  if (!matches.length) return;
  const heading = matches[0], next = headings[headings.indexOf(heading) + 1];
  return {text, start: heading.index, end: next?.index ?? text.length, heading: heading[1],
    body: text.slice(heading.index + heading[0].length, next?.index ?? text.length).trim()};
}

const hasReleaseNotes = (changelog, version) => Boolean(changelogSection(changelog, version));

// Release notes are reviewed product descriptions, never synthesized from Git.
// Only Chinese bullets followed by English bullets are accepted; arbitrary
// prose, extra headings and known generated/commit metadata fail before tagging.
export function releaseNotes(changelog, version) {
  const section = changelogSection(changelog, version);
  assert.ok(section, `Missing bilingual release notes for ${version}; add a numbered CHANGELOG.md section before merging.`);
  const parts = section.body.split(/^### English[ \t]*$/m);
  assert.equal(parts.length, 2, 'Bilingual release notes require Chinese bullets followed by ### English and English bullets');
  const groups = parts.map(part => part.split('\n').filter(line => line.trim()));
  assert.equal(groups[0].length, groups[1].length, 'Bilingual release notes require matching change bullet counts');
  for (const [index, part] of parts.entries()) {
    const lines = part.split('\n').filter(line => line.trim());
    assert.ok(lines.length && lines.every(line => /^- \S/.test(line)), 'Release notes must contain only bilingual change bullets');
    // Issue/PR links may follow a change, but their labels/URLs are not the
    // functional description. Preserve the original Markdown in the output.
    assert.ok(lines.every(line => (index === 0 ? /[\p{Script=Han}]/u : /[a-zA-Z]/)
      .test(line.replace(/\[[^\]]*\]\(https?:\/\/[^)\s]+\)|https?:\/\/\S+/g, ''))), 'Release notes require Chinese and English change descriptions');
  }
  assert.doesNotMatch(section.body, /自动发布[：:]|Automatic release:|更新签名迁移|Update signing migration/i, 'Release notes cannot contain generated boilerplate or signing migration prose');
  assert.doesNotMatch(section.body, /^- (?:\*\*)?(?:feat|fix|chore|docs|refactor|perf|test|build|ci|style|revert)(?:\([^\n)]*\))?!?:/im, 'Release notes cannot contain commit subjects');
  assert.doesNotMatch(section.body, /\([a-f0-9]{7,40}\)|\b(?=[a-f0-9]*[a-f])[a-f0-9]{40}\b|https?:\/\/[^\s)]+\/commit\/[a-f0-9]+/i, 'Release notes cannot contain commit hashes');
  return `## ${section.heading}\n\n${section.body}\n`;
}

export function releaseBody(version, notes) {
  assert.ok(stable.test(version) && !version.startsWith('v'), 'Expected stable version');
  const base = `https://github.com/${repository}/releases/download/v${version}/SAG-Wiki-${version}-`;
  return `${notes}\n### 下载 / Downloads\n\n| 平台 / Platform | 安装包 / Installer |\n| --- | --- |\n| Windows x64 | [EXE](${base}Windows-x64-setup.exe) |\n| macOS Apple Silicon | [DMG](${base}macOS-arm64.dmg) |\n| macOS Intel | [DMG](${base}macOS-x86_64.dmg) |\n`;
}

function preparedChangelog(changelog, version, baselineChangelog = '') {
  const reviewed = changelogSection(changelog, version);
  if (reviewed) { releaseNotes(changelog, version); return changelog; }
  // Retain this legacy transform to verify already-published schema-2 tags.
  // New product PRs require numbered sections before merging.
  const pending = changelogSection(changelog, 'Unreleased');
  releaseNotes(changelog, 'Unreleased');
  const previous = changelogSection(baselineChangelog, 'Unreleased');
  let body = pending.body;
  if (previous) {
    // Prepared tags are off-main, so main retains Unreleased. Compare bilingual
    // pairs against the published source recorded in provenance, not HEAD or a
    // mutable latest tag, to avoid repeating notes in the next release.
    releaseNotes(baselineChangelog, 'Unreleased');
    const pairs = body => {
      const groups = body.split(/^### English[ \t]*$/m).map(part => part.split('\n').filter(line => line.trim()));
      assert.equal(groups[0].length, groups[1].length, 'Unreleased bilingual release notes must have matching change bullet counts');
      return groups[0].map((zh, index) => [zh, groups[1][index]]);
    };
    const published = new Set(pairs(previous.body).map(pair => JSON.stringify(pair)));
    const fresh = pairs(body).filter(pair => !published.has(JSON.stringify(pair)));
    assert.ok(fresh.length, 'No unpublished release notes; append new bilingual change bullets before merging');
    body = fresh.map(pair => pair[0]).join('\n') + '\n\n### English\n\n' + fresh.map(pair => pair[1]).join('\n');
  }
  if (body !== pending.body) {
    return pending.text.slice(0, pending.start) + `## ${version}\n\n${body}\n\n` + pending.text.slice(pending.end);
  }
  return pending.text.slice(0, pending.start) + pending.text.slice(pending.start, pending.end)
    .replace(/^## Unreleased(?=\s|$)/, `## ${version}`) + pending.text.slice(pending.end);
}

// Compare user-facing change bullets, allowing optional Issue/PR references.
export function validateReleaseDocumentation(git, source, version) {
  const section = changelogSection(blob(git, source, 'CHANGELOG.md'), version);
  assert.ok(section, `Missing release notes for ${version}`);
  releaseNotes(blob(git, source, 'CHANGELOG.md'), version);
  const groups = section.body.split(/^### English[ \t]*$/m);
  const bullets = text => text.split('\n').filter(line => /^- /.test(line)).map(line => line
    .replace(/\[([^\]]*)\]\(([^)]*)\)/g, (_, label, url) => /\/(?:pull|issues)\/\d+/.test(url) ? '' : label)
    .replace(/[（(]\s*[)）]/g, '').replace(/\s+/g, ' ').trim());
  for (const [index, name] of ['updates.mdx', 'updates.en.mdx'].entries()) {
    const path = `docs-site/content/docs/${name}`;
    const documented = changelogSection(blob(git, source, path), version);
    assert.ok(documented, `${path} must document release ${version}`);
    assert.deepEqual(bullets(documented.body), bullets(groups[index]), `${path} release ${version} must match CHANGELOG.md`);
  }
}

export function validatePullRequestNotes(git, base, source) {
  const paths = changedPaths(git, base, source);
  if (!hasProductChanges(paths)) return false;
  assert.ok(paths.includes('CHANGELOG.md'), 'Product PRs must update CHANGELOG.md with reviewed bilingual release notes before merging');
  const changelog = blob(git, source, 'CHANGELOG.md');
  assert.doesNotMatch(changelog, /^## Unreleased(?:\s|$)/im, 'Use numbered release notes instead of an Unreleased heading');
  const version = releaseVersion([JSON.parse(blob(git, source, 'package.json')).version], git('tag', '--list', 'v*').split('\n'), changelog);
  const baselineChangelog = blob(git, base, 'CHANGELOG.md');
  const reviewed = changelogSection(changelog, version);
  if (reviewed) assert.notEqual(reviewed.body, changelogSection(baselineChangelog, version)?.body,
    'No new release notes for this product PR; update the reviewed version section before merging');
  preparedChangelog(changelog, version, baselineChangelog);
  validateReleaseDocumentation(git, source, version);
  return true;
}

export function releaseVersion(manifests, tags, changelog) {
  const version = manifests[0];
  const occupied = tags.filter(tag => /^v/.test(tag) && stable.test(tag));
  if (version && manifests.every(value => value === version) && hasReleaseNotes(changelog, version) &&
      occupied.length && occupied.every(tag => compareVersions(version, tag) > 0)) {
    return version;
  }
  return nextVersion(manifests, tags);
}

// Unknown paths are product changes. Release infrastructure is intentionally
// narrow: build-cli, packaging, updater and macOS signing code affect the product.
export function isProductPath(path) {
  return !(/^(?:docs\/|docs-site\/|\.github\/)/.test(path) ||
    /^(?:[^/]+\.md|LICENSE(?:\.[^/]+)?|\.gitignore|\.gitattributes)$/.test(path) ||
    /^scripts\/(?:automatic-release|documentation-release|release(?:-build-cache|-downloads|-assembly|-test-plan)?|configure-release|migrate-release-environment|rust-dependency-cache|test-profile-ab|pr-test-plan|parallel-cli)(?:\.node-test)?\.mjs$/.test(path) ||
    ['scripts/rust-dependency-archive.py', 'scripts/run-pr-e2e.py', 'tests/local-jev-e2e.py'].includes(path) || path === 'scripts/fixtures/documentation-history.json');
}
export const hasProductChanges = paths => paths.some(isProductPath);
export const changedPaths = (git, base, source) => git.raw('diff', '--name-only', '-z', '--no-renames', base, source, '--').split('\0').filter(Boolean);

// Only merged PRs covering this push may suppress its automatic release.
// Walk main's first-parent integrations so merge, squash and rebase all work.
export function shouldSkipRelease(git, before, source, pullRequestsForCommit) {
  const commits = git('rev-list', '--first-parent', `${before}..${source}`).split('\n').filter(Boolean);
  const pushed = new Set(commits);
  const products = commits.filter(commit => hasProductChanges(changedPaths(git, `${commit}^1`, commit)));
  const decisions = products.map(commit => {
    const merged = pullRequestsForCommit(commit).filter(pr =>
      pr.state === 'closed' && pr.merged_at && pr.base?.ref === 'main' &&
      pr.base?.repo?.full_name === repository && pushed.has(pr.merge_commit_sha) &&
      (() => { try { git('merge-base', '--is-ancestor', commit, pr.merge_commit_sha); return true; } catch { return false; } })());
    assert.equal(merged.length, 1, `Cannot uniquely verify the merged PR for ${commit}; retry after GitHub indexes the merge. No tag was prepared.`);
    return merged[0].labels?.some(label => label.name === 'release:skip') === true;
  });
  return decisions.length > 0 && decisions.every(Boolean);
}

export function updateVersionFiles(files, version) {
  assert.ok(stable.test(version) && !version.startsWith('v'), 'Expected stable version');
  const updated = {};
  for (const name of versionFiles) {
    const original = files[name];
    assert.equal(typeof original, 'string', `Missing ${name}`);
    if (name.endsWith('.json')) {
      const data = JSON.parse(original);
      const previous = data.version;
      data.version = version;
      if (name === 'package-lock.json') {
        assert.equal(data.packages[''].version, previous, 'Root npm lock version mismatch');
        data.packages[''].version = version;
      }
      // Change the version tokens only, preserving every other byte/setting.
      let text = original.replace(/("version"\s*:\s*")[^"]+("(?:\s*[,}]))/, `$1${version}$2`);
      if (name === 'package-lock.json') {
        text = text.replace(/(""\s*:\s*\{[\s\S]*?"version"\s*:\s*")[^"]+("(?:\s*[,}]))/, `$1${version}$2`);
      }
      assert.deepEqual(JSON.parse(text), data, `Unexpected version transform: ${name}`);
      updated[name] = text;
    } else {
      const packageName = name.startsWith('cli/') ? 'sag-cli' : 'sag-desktop';
      const pattern = /^version = "[^"]+"/m;
      if (name.endsWith('.toml')) {
        assert.ok(pattern.test(original), `Missing Cargo package version: ${name}`);
        updated[name] = original.replace(pattern, `version = "${version}"`);
      } else {
        const normalized = original.replace(/\r\n/g, '\n');
        const lockPattern = new RegExp(`(\\[\\[package\\]\\]\\nname = "${packageName}"\\nversion = ")[^"]+(\")`);
        assert.ok(lockPattern.test(normalized), `Missing Cargo lock package: ${name}`);
        updated[name] = normalized.replace(lockPattern, `$1${version}$2`);
        if (original.includes('\r\n')) updated[name] = updated[name].replace(/\n/g, '\r\n');
      }
    }
  }
  return updated;
}

export function releaseProvenance(git, tag) {
  const data = JSON.parse(git('for-each-ref', '--format=%(contents)', `refs/tags/${tag}`));
  assert.deepEqual(Object.keys(data).sort(), ['baseline', 'schema', 'source', 'version']);
  assert.ok([1, releaseNotesSchema].includes(data.schema), 'Unsupported prepared release schema');
  assert.match(data.source, sha);
  assert.match(data.baseline, sha);
  assert.equal(tag, `v${data.version}`);
  assert.ok(stable.test(data.version));
  return data;
}

export function preparedContents(git, {source, baseline, version}) {
  assert.match(source, sha); assert.match(baseline, sha);
  git('merge-base', '--is-ancestor', baseline, source);
  const files = Object.fromEntries(versionFiles.map(name => [name, blob(git, source, name)]));
  const changelog = blob(git, source, 'CHANGELOG.md');
  const reviewed = hasReleaseNotes(changelog, version);
  const comparison = compareVersions(version, JSON.parse(files['package.json']).version);
  assert.ok(comparison > 0 || (comparison === 0 && reviewed), 'Prepared version must increase or have reviewed notes');
  const notes = preparedChangelog(changelog, version, blob(git, baseline, 'CHANGELOG.md'));
  return {...updateVersionFiles(files, version), 'CHANGELOG.md': notes};
}

// Read-only reconstruction for existing schema-1 release tags. New preparation
// never calls this legacy commit-subject generator. Keep its byte output stable.
function legacyPreparedContents(git, {source, baseline, version}) {
  assert.match(source, sha); assert.match(baseline, sha);
  git('merge-base', '--is-ancestor', baseline, source);
  const files = Object.fromEntries(versionFiles.map(name => [name, blob(git, source, name)]));
  const changelog = blob(git, source, 'CHANGELOG.md');
  const reviewed = changelog.replace(/\r\n/g, '\n').split(/^## /m).some(section => section.startsWith(`${version}\n`));
  const comparison = compareVersions(version, JSON.parse(files['package.json']).version);
  assert.ok(comparison > 0 || (comparison === 0 && reviewed), 'Prepared version must increase or have reviewed notes');
  const updates = updateVersionFiles(files, version);
  if (reviewed) {
    return {...updates, 'CHANGELOG.md': changelog};
  }
  const subjects = git('log', '--no-merges', '--format=%H', `${baseline}..${source}`).split('\n').filter(Boolean)
    .filter(commit => hasProductChanges(git.raw('diff-tree', '--root', '--no-commit-id', '--name-only', '-z', '--no-renames', '-r', commit).split('\0').filter(Boolean)))
    .map(commit => `- ${git('show', '-s', '--format=%s', commit).replace(/[\r\n]/g, ' ')} (${commit.slice(0, 7)})`);
  assert.ok(subjects.length, 'No product commits to release');
  const heading = changelog.indexOf('\n');
  assert.ok(heading >= 0, 'Changelog heading is required');
  updates['CHANGELOG.md'] = changelog.slice(0, heading + 1) +
    `\n## ${version}\n\n自动发布：包含自上次正式发布以来的产品改动。\n\nAutomatic release: product changes since the previous published release.\n\n${subjects.join('\n')}\n` + changelog.slice(heading + 1);
  return updates;
}

// An off-main release is trusted only when its complete tree is exactly the
// deterministic metadata-only child of the recorded, still-reachable main SHA.
export function validateReleaseSource(git, commit, tag) {
  assert.match(commit, sha);
  assert.match(tag, /^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/);
  assert.equal(git('rev-parse', `refs/tags/${tag}^{commit}`), commit, 'Release tag moved');
  try { git('merge-base', '--is-ancestor', commit, 'origin/main'); return {source: commit}; }
  catch { /* A prepared child is deliberately not added to protected main. */ }
  const provenance = releaseProvenance(git, tag);
  assert.equal(git('rev-list', '--parents', '-n', '1', commit), `${commit} ${provenance.source}`, 'Prepared release must have exactly one recorded parent');
  git('merge-base', '--is-ancestor', provenance.source, 'origin/main');
  if (provenance.schema === 1) {
    assert.ok(!blob(git, provenance.source, 'scripts/automatic-release.mjs').includes('export const releaseNotesSchema = 2;'),
      'New release sources require the reviewed-notes schema');
  }
  const expected = provenance.schema === 1 ? legacyPreparedContents(git, provenance) : preparedContents(git, provenance);
  const changed = changedPaths(git, provenance.source, commit);
  assert.deepEqual(changed.sort(), Object.keys(expected).filter(name => blob(git, provenance.source, name) !== expected[name]).sort(), 'Unexpected release file delta');
  for (const [name, contents] of Object.entries(expected)) {
    assert.equal(blob(git, commit, name), contents, `Unexpected release contents: ${name}`);
    assert.equal(git('ls-tree', commit, '--', name).split(' ')[0], git('ls-tree', provenance.source, '--', name).split(' ')[0], 'Release file modes changed');
  }
  return provenance;
}

export function createPreparedCommit(directory, provenance) {
  const git = gitAt(directory);
  assert.equal(provenance.schema, releaseNotesSchema, 'New prepared releases require reviewed bilingual notes');
  const updated = preparedContents(git, provenance);
  const temporary = mkdtempSync(join(tmpdir(), 'sag-release-index-'));
  const env = {...process.env, GIT_INDEX_FILE: join(temporary, 'index')};
  const isolatedGit = (args, input) => execute('git', args, {cwd: directory, env, input});
  try {
    isolatedGit(['read-tree', provenance.source]);
    for (const [name, contents] of Object.entries(updated)) {
      const hash = isolatedGit(['hash-object', '-w', '--stdin'], contents);
      const mode = git('ls-tree', provenance.source, '--', name).split(' ')[0];
      isolatedGit(['update-index', '--add', '--cacheinfo', `${mode},${hash},${name}`]);
    }
    const tree = isolatedGit(['write-tree']);
    const date = git('show', '-s', '--format=%aI', provenance.source);
    const commitEnv = {...env, GIT_AUTHOR_NAME: 'github-actions[bot]', GIT_AUTHOR_EMAIL: '41898282+github-actions[bot]@users.noreply.github.com',
      GIT_COMMITTER_NAME: 'github-actions[bot]', GIT_COMMITTER_EMAIL: '41898282+github-actions[bot]@users.noreply.github.com', GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date};
    return execute('git', ['-c', 'commit.gpgsign=false', 'commit-tree', tree, '-p', provenance.source], {
      cwd: directory, env: commitEnv, input: `chore: prepare desktop release ${provenance.version}\n\nSource: ${provenance.source}\n`,
    });
  } finally { rmSync(temporary, {recursive: true, force: true}); }
}

export function latestPublished(releases) {
  return releases.filter(release => !release.draft && !release.prerelease && /^v/.test(release.tag_name) && stable.test(release.tag_name))
    .sort((a, b) => compareVersions(b.tag_name, a.tag_name))[0];
}

function sourceOfTag(tag) {
  const commit = git('rev-parse', `refs/tags/${tag}^{commit}`);
  return validateReleaseSource(git, commit, tag).source;
}
const summary = text => { console.log(text); if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, text + '\n'); };


function classifyPush() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  assert.equal(process.env.GITHUB_REF, 'refs/heads/main');
  assert.equal(process.env.GITHUB_EVENT_NAME, 'push');
  const source = process.env.GITHUB_SHA;
  assert.match(source ?? '', sha);
  assert.equal(git('rev-parse', 'HEAD'), source, 'Use the captured push SHA');
  git('merge-base', '--is-ancestor', source, 'origin/main');
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  assert.match(event.before ?? '', sha);
  assert.notEqual(event.before, '0'.repeat(40), 'An existing main history is required');
  git('merge-base', '--is-ancestor', event.before, source);
  const product = hasProductChanges(changedPaths(git, event.before, source));
  const skipped = product && shouldSkipRelease(git, event.before, source, commit => pages(`commits/${commit}/pulls`));
  return {source, eligible: product && !skipped, skipped};
}

function classify() {
  const {eligible, skipped} = classifyPush();
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `eligible=${eligible}\n`);
  summary(skipped ? 'release:skip: no version tag or desktop release for this push.' :
    eligible ? 'Product changes: prepare a checked release.' : 'Documentation/CI-only push: no desktop release.');
}

export function hasNewerPreparedSource(git, source, sources) {
  return sources.some(candidate => {
    if (candidate === source) return false;
    try { git('merge-base', '--is-ancestor', source, candidate); return true; } catch { return false; }
  });
}

function prepare() {
  const {source, eligible, skipped} = classifyPush();
  if (!eligible) { summary(skipped ? 'release:skip: no version tag or desktop release for this push.' : 'Documentation/CI-only push: no desktop release.'); return; }
  const releases = pages('releases');
  const tags = git('tag', '--list', 'v*').split('\n').filter(Boolean);
  const existing = tags.filter(tag => {
    try { return releaseProvenance(git, tag).source === source; } catch { return false; }
  });
  assert.ok(existing.length <= 1, 'Multiple prepared tags for the same source');
  let tag, commit;
  if (existing.length) {
    tag = existing[0]; commit = git('rev-parse', `refs/tags/${tag}^{commit}`);
    validateReleaseSource(git, commit, tag);
  } else {
    const preparedSources = tags.flatMap(candidate => {
      try {
        const data = releaseProvenance(git, candidate);
        validateReleaseSource(git, git('rev-parse', `refs/tags/${candidate}^{commit}`), candidate);
        return [data.source];
      } catch { return []; }
    });
    if (hasNewerPreparedSource(git, source, preparedSources)) { summary('A newer main source is already prepared; skip this obsolete push.'); return; }
    const published = latestPublished(releases);
    assert.ok(published, 'A published stable release baseline is required');
    const baseline = sourceOfTag(published.tag_name);
    git('merge-base', '--is-ancestor', baseline, source);
    if (!hasProductChanges(changedPaths(git, baseline, source))) { summary('No unreleased product changes.'); return; }
    const files = Object.fromEntries(versionFiles.map(name => [name, blob(git, source, name)]));
    const versions = versionFiles.map(name => name.endsWith('.json') ? JSON.parse(files[name]).version :
      files[name].match(name.endsWith('.toml') ? /^version = "([^"]+)"/m : new RegExp(`\\[\\[package\\]\\]\\nname = "${name.startsWith('cli/') ? 'sag-cli' : 'sag-desktop'}"\\nversion = "([^"]+)"`))?.[1]);
    const pendingTag = `v${versions[0]}`;
    if (versions.every(version => version === versions[0]) && tags.includes(pendingTag) &&
        !releases.some(release => release.tag_name === pendingTag && !release.draft)) {
      summary(`${pendingTag} is not published yet; keep its version and request an explicit rebuild after fixing the release.`);
      return;
    }
    const version = releaseVersion(versions, tags, blob(git, source, 'CHANGELOG.md'));
    validateReleaseDocumentation(git, source, version);
    tag = `v${version}`;
    const provenance = {schema: releaseNotesSchema, source, baseline, version};
    commit = createPreparedCommit(root, provenance);
    git('-c', 'user.name=github-actions[bot]', '-c', 'user.email=41898282+github-actions[bot]@users.noreply.github.com',
      '-c', 'tag.gpgsign=false', 'tag', '-a', tag, commit, '-m', JSON.stringify(provenance));
    validateReleaseSource(git, commit, tag);
    // Only this new tag is pushed. Protected main is never changed or bypassed.
    // Checkout credentials are scoped to this preparation job; no PAT is needed.
    git('push', 'origin', `refs/tags/${tag}:refs/tags/${tag}`);
  }
  if (releases.some(release => release.tag_name === tag && !release.draft)) { summary(`${tag} already published for ${source}.`); return; }
  validateReleaseDocumentation(git, commit, tag.slice(1));
  requestTag(tag, commit);
  summary(`Prepared ${tag} (${commit}) from main ${source}; desktop checks, signing and publishing run on the exact tagged commit.`);
}

export function retryAction(runs, commit, tag) {
  const run = runs.filter(run => run.head_sha === commit && run.head_branch === tag && run.event === 'workflow_dispatch').sort((a, b) => b.id - a.id)[0];
  if (!run) return {command: 'dispatch'};
  if (run.status !== 'completed' || run.conclusion === 'success') return {command: 'none'};
  assert.ok(Number.isSafeInteger(run.id) && run.id > 0, 'Invalid failed run');
  return {command: 'rerun', run: run.id};
}

function requestTag(tag, commit) {
  const runs = pages(`actions/workflows/desktop-release.yml/runs?head_sha=${commit}`, 'workflow_runs');
  const action = retryAction(runs, commit, tag);
  if (action.command === 'dispatch') gh('workflow', 'run', 'desktop-release.yml', '--repo', repository, '--ref', tag);
  // Keep successful jobs and their receipts/draft. A fresh workflow would repeat
  // signing and conflict with an already assembled draft after a publish failure.
  if (action.command === 'rerun') gh('run', 'rerun', String(action.run), '--failed', '--repo', repository);
}

function request() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  assert.equal(process.env.GITHUB_REF, 'refs/heads/main');
  const tag = process.env.RELEASE_TAG;
  assert.match(tag ?? '', /^v\d+\.\d+\.\d+$/);
  const commit = git('rev-parse', `refs/tags/${tag}^{commit}`);
  validateReleaseSource(git, commit, tag);
  requestTag(tag, commit);
  summary(`Requested or resumed ${tag} (${commit}). Release checks and publication remain automatic.`);
}

export function validateReplaceableDraft(release, {tag, commit}) {
  if (!release) return;
  assert.equal(release.tag_name, tag, 'Unexpected release tag');
  assert.equal(release.draft, true, 'Published releases must never be replaced');
  assert.equal(release.prerelease, false, 'Unexpected prerelease draft');
  assert.equal(release.author?.login, 'github-actions[bot]', 'Only an automation-owned draft can be resumed');
  assert.equal(release.author?.type, 'Bot', 'Expected Actions bot author');
  assert.equal(release.target_commitish, commit, 'Draft must target the exact validated release commit');
}

function draftContext() {
  const tag = process.env.RELEASE_TAG, commit = process.env.RELEASE_COMMIT, version = process.env.RELEASE_VERSION;
  assert.equal(tag, `v${version}`);
  assert.equal(git('rev-parse', 'HEAD'), commit);
  validateReleaseSource(git, commit, tag);
  const existing = pages('releases').find(release => release.tag_name === tag);
  validateReplaceableDraft(existing, {tag, commit});
  return {tag, commit, version, existing};
}

function checkDraft() { draftContext(); }

function createDraft() {
  const {tag, commit, version, existing} = draftContext();
  const directory = join(root, 'releases', version, 'updates');
  const metadata = JSON.parse(readFileSync(join(directory, 'build-metadata.json'), 'utf8'));
  assert.equal(metadata.commit, commit); assert.equal(metadata.version, version);
  const checksumName = `${tag}-SHA256SUMS.txt`;
  const checksums = readFileSync(join(directory, checksumName), 'utf8').trim().split('\n').map(line => {
    const match = line.match(/^([a-f0-9]{64})  ([^/\\]+)$/);
    assert.ok(match, 'Invalid assembled checksum entry');
    return {hash: match[1], name: match[2]};
  });
  assert.deepEqual(checksums.map(entry => entry.name).sort(), readdirSync(directory).filter(name => name !== checksumName).sort(), 'Assembled files changed');
  for (const {hash, name} of checksums) assert.equal(createHash('sha256').update(readFileSync(join(directory, name))).digest('hex'), hash, 'Assembled checksum mismatch');
  // All platform checks, signed assembly and attestation preceded this command.
  // Recover a partial upload by rebuilding only this exact bot-owned draft.
  if (existing) gh('release', 'delete', tag, '--repo', repository, '--yes');
  const temporary = mkdtempSync(join(tmpdir(),'sag-release-body-'));
  try {
    const notesFile = join(temporary,'body.md');
    writeFileSync(notesFile,releaseBody(version,readFileSync(join(directory,'release-notes.md'),'utf8')));
    gh('release', 'create', tag, '--repo', repository, '--verify-tag', '--target', commit, '--draft',
      '--title', `SAG Wiki ${version}`, '--notes-file', notesFile,
      ...readdirSync(directory).sort().map(name => join(directory, name)));
  } finally { rmSync(temporary,{recursive:true,force:true}); }
  const checksumsDigest = createHash('sha256').update(readFileSync(join(directory, checksumName))).digest('hex');
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `checksums=${checksumsDigest}\n`);
  summary(`Verified draft ready for automatic publication: https://github.com/${repository}/releases/tag/${tag}`);
}

export function validatePublicationAssets(release, checksums, expectedDigest, version) {
  assert.match(expectedDigest ?? '', /^[a-f0-9]{64}$/, 'Missing assembly checksum provenance');
  assert.equal(createHash('sha256').update(checksums).digest('hex'), expectedDigest, 'Release checksums differ from checked assembly');
  const entries = checksums.trim().split('\n').map(line => {
    const match = line.match(/^([a-f0-9]{64})  ([^/\\]+)$/);
    assert.ok(match, 'Invalid release checksum entry');
    return {digest: `sha256:${match[1]}`, name: match[2]};
  });
  entries.push({name: `v${version}-SHA256SUMS.txt`, digest: `sha256:${expectedDigest}`});
  const expectedNames = [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`,
    `SAG-Wiki-${version}-macOS-arm64.app.tar.gz.sig`,
    `SAG-Wiki-${version}-macOS-x86_64.dmg`, `SAG-Wiki-${version}-macOS-x86_64.app.tar.gz`, `SAG-Wiki-${version}-macOS-x86_64.app.tar.gz.sig`, `SAG-Wiki-${version}-Windows-x64-setup.exe`,
    `SAG-Wiki-${version}-Windows-x64-setup.exe.sig`, 'latest-v2.json', 'latest.json', 'build-metadata.json',
    'release-notes.md', `v${version}-SHA256SUMS.txt`].sort();
  assert.deepEqual(entries.map(entry => entry.name).sort(), expectedNames, 'Unexpected assembled asset manifest');
  assert.deepEqual(release.assets?.map(asset => asset.name).sort(), expectedNames, 'Missing or unexpected draft assets');
  for (const entry of entries) {
    const asset = release.assets.find(asset => asset.name === entry.name);
    assert.equal(asset.state, 'uploaded', 'Release asset upload incomplete');
    assert.ok(asset.size > 0, 'Empty release asset');
    assert.equal(asset.digest, entry.digest, `Uploaded asset checksum mismatch: ${entry.name}`);
  }
}

export function releaseForTag(tag, list = pages, read = api) {
  // GitHub's tag endpoint can return 404 for an existing draft. Listing
  // authenticated releases includes drafts; fetch that exact ID for fresh assets.
  const release = list('releases').find(release => release.tag_name === tag);
  assert.ok(release, `Release ${tag} not found`);
  assert.ok(Number.isSafeInteger(release.id) && release.id > 0, 'Invalid release ID');
  return read(`releases/${release.id}`);
}

const transientReleaseError = error => /HTTP (?:404|408|429|5\d\d)\b|Release v\d+\.\d+\.\d+ not found|connection reset|connection refused|timed? out|timeout|unexpected EOF|ETIMEDOUT|ECONNRESET|EAI_AGAIN|TLS handshake/i.test(String(error));
const releaseRetryPause = milliseconds => new Promise(resolve => setTimeout(resolve,milliseconds));

async function retryReleaseRequest(request, pause) {
  for (let attempt=0; ; attempt++) {
    try { return await request(); }
    catch (error) {
      if (attempt === 2 || !transientReleaseError(error)) throw error;
      await pause(1000 * (2 ** attempt));
    }
  }
}

export async function publishDraftById(release, tag, validate, listReleases, {
  download, read, update, pause=releaseRetryPause,
}) {
  assert.ok(Number.isSafeInteger(release.id) && release.id > 0, 'Invalid release ID');
  const assets = release.assets?.filter(asset => asset.name === `${tag}-SHA256SUMS.txt`) ?? [];
  assert.ok(assets.length === 1 && Number.isSafeInteger(assets[0].id) && assets[0].id > 0,
    'Expected a unique checksum asset ID');
  const checksums = await retryReleaseRequest(() => download(assets[0].id), pause);
  validate(checksums);
  for (let attempt=0; ; attempt++) {
    const releases = await retryReleaseRequest(listReleases, pause);
    if (releases.some(current => current.id === release.id && current.draft === false)) return;
    const latest = latestPublished(releases);
    const makeLatest = !latest || compareVersions(tag, latest.tag_name) > 0;
    try { return await update(release.id, makeLatest); }
    catch (error) {
      if (!transientReleaseError(error)) throw error;
      const current = await retryReleaseRequest(() => read(release.id), pause);
      assert.equal(current.id, release.id, 'Unexpected release during retry');
      if (current.draft === false) return;
      assert.equal(current.draft, true, 'Unknown release state');
      if (attempt === 2) throw error;
      await pause(1000 * (2 ** attempt));
    }
  }
}

async function publish() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  const tag = process.env.RELEASE_TAG;
  const version = process.env.RELEASE_VERSION;
  const commit = process.env.RELEASE_COMMIT;
  assert.equal(tag, `v${version}`);
  assert.equal(git('rev-parse', 'HEAD'), commit);
  validateReleaseSource(git, commit, tag);
  const release = await retryReleaseRequest(() => releaseForTag(tag), releaseRetryPause);
  assert.equal(release.prerelease, false);
  assert.equal(release.tag_name, tag);
  // The preceding assembly job owns the draft and its immutable assets. A rerun
  // after a successful edit is harmless; never overwrite or upload assets here.
  if (!release.draft) { summary(`${tag} is already published.`); return; }
  validateReleaseDocumentation(git, commit, version);
  validateReplaceableDraft(release, {tag, commit});
  const startedAt = Date.now();
  await publishDraftById(release, tag,
    checksums => validatePublicationAssets(release, checksums, process.env.RELEASE_CHECKSUMS, version),
    () => pages('releases'), {
      download: id => execute('gh', ['api', '-H', 'Accept: application/octet-stream', `repos/${repository}/releases/assets/${id}`], {trim:false,timeout:20000}),
      read: id => JSON.parse(execute('gh', ['api', `repos/${repository}/releases/${id}`], {timeout:20000})),
      update: (id, makeLatest) => {
        const updated = JSON.parse(execute('gh', ['api','--method','PATCH',`repos/${repository}/releases/${id}`,'--input','-'],
          {input:JSON.stringify({draft:false,make_latest:String(makeLatest)}),timeout:20000}));
        assert.equal(updated.id,id);
        assert.equal(updated.draft,false);
      },
    });
  summary(`Published ${tag} by Release ID ${release.id} in ${((Date.now()-startedAt)/1000).toFixed(1)}s after draft validation. All platform, documentation, signature and provenance checks passed.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  assert.ok(['check-notes', 'classify', 'prepare', 'request', 'check-draft', 'create-draft', 'publish'].includes(process.argv[2]), 'Use classify, prepare, request, check-draft, create-draft or publish');
  if (process.argv[2] === 'check-notes') {
    validatePullRequestNotes(git, process.env.BASE_SHA, 'HEAD');
    summary('Reviewed bilingual release notes checked for product changes.');
  }
  else if (process.argv[2] === 'classify') classify();
  else if (process.argv[2] === 'prepare') prepare();
  else if (process.argv[2] === 'request') request();
  else if (process.argv[2] === 'check-draft') checkDraft();
  else if (process.argv[2] === 'create-draft') createDraft(); else await publish();
}
