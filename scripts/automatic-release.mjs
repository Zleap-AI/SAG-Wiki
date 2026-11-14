import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {appendFileSync, mkdtempSync, readFileSync, readdirSync, rmSync} from 'node:fs';
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

// Unknown paths are product changes. Release infrastructure is intentionally
// narrow: build-cli, packaging, updater and macOS signing code affect the product.
export function isProductPath(path) {
  return !(/^(?:docs\/|docs-site\/|\.github\/)/.test(path) ||
    /^(?:[^/]+\.md|LICENSE(?:\.[^/]+)?|\.gitignore|\.gitattributes)$/.test(path) ||
    /^scripts\/(?:automatic-release|release(?:-build-cache|-downloads|-assembly)?|configure-release|migrate-release-environment)(?:\.node-test)?\.mjs$/.test(path));
}
export const hasProductChanges = paths => paths.some(isProductPath);
export const changedPaths = (git, base, source) => git.raw('diff', '--name-only', '-z', '--no-renames', base, source, '--').split('\0').filter(Boolean);

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
  assert.equal(data.schema, 1);
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
  assert.ok(compareVersions(version, JSON.parse(files['package.json']).version) > 0, 'Prepared version must increase');
  const updates = updateVersionFiles(files, version);
  const subjects = git('log', '--no-merges', '--format=%H', `${baseline}..${source}`).split('\n').filter(Boolean)
    .filter(commit => hasProductChanges(git.raw('diff-tree', '--root', '--no-commit-id', '--name-only', '-z', '--no-renames', '-r', commit).split('\0').filter(Boolean)))
    .map(commit => `- ${git('show', '-s', '--format=%s', commit).replace(/[\r\n]/g, ' ')} (${commit.slice(0, 7)})`);
  assert.ok(subjects.length, 'No product commits to release');
  const changelog = blob(git, source, 'CHANGELOG.md');
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
  const expected = preparedContents(git, provenance);
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
  return {source, eligible: hasProductChanges(changedPaths(git, event.before, source))};
}

function classify() {
  const {eligible} = classifyPush();
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `eligible=${eligible}\n`);
  summary(eligible ? 'Product changes: prepare a checked release.' : 'Documentation/CI-only push: no desktop release.');
}

export function hasNewerPreparedSource(git, source, sources) {
  return sources.some(candidate => {
    if (candidate === source) return false;
    try { git('merge-base', '--is-ancestor', source, candidate); return true; } catch { return false; }
  });
}

function prepare() {
  const {source, eligible} = classifyPush();
  if (!eligible) { summary('Documentation/CI-only push: no desktop release.'); return; }
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
    const version = nextVersion(versions, tags);
    tag = `v${version}`;
    const provenance = {schema: 1, source, baseline, version};
    commit = createPreparedCommit(root, provenance);
    git('-c', 'user.name=github-actions[bot]', '-c', 'user.email=41898282+github-actions[bot]@users.noreply.github.com',
      '-c', 'tag.gpgsign=false', 'tag', '-a', tag, commit, '-m', JSON.stringify(provenance));
    validateReleaseSource(git, commit, tag);
    // Only this new tag is pushed. Protected main is never changed or bypassed.
    // Checkout credentials are scoped to this preparation job; no PAT is needed.
    git('push', 'origin', `refs/tags/${tag}:refs/tags/${tag}`);
  }
  if (releases.some(release => release.tag_name === tag && !release.draft)) { summary(`${tag} already published for ${source}.`); return; }
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
  gh('release', 'create', tag, '--repo', repository, '--verify-tag', '--target', commit, '--draft',
    '--title', `SAG Wiki ${version}`, '--notes-file', join(directory, 'release-notes.md'),
    ...readdirSync(directory).sort().map(name => join(directory, name)));
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
    `SAG-Wiki-${version}-macOS-arm64.app.tar.gz.sig`, `SAG-Wiki-${version}-Windows-x64-setup.exe`,
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

function publish() {
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  const tag = process.env.RELEASE_TAG;
  const version = process.env.RELEASE_VERSION;
  const commit = process.env.RELEASE_COMMIT;
  assert.equal(tag, `v${version}`);
  assert.equal(git('rev-parse', 'HEAD'), commit);
  validateReleaseSource(git, commit, tag);
  const release = api(`releases/tags/${tag}`);
  assert.equal(release.prerelease, false);
  assert.equal(release.tag_name, tag);
  // The preceding assembly job owns the draft and its immutable assets. A rerun
  // after a successful edit is harmless; never overwrite or upload assets here.
  if (!release.draft) { summary(`${tag} is already published.`); return; }
  validateReplaceableDraft(release, {tag, commit});
  const checksums = execute('gh', ['release', 'download', tag, '--repo', repository, '--pattern', `${tag}-SHA256SUMS.txt`, '--output', '-'], {trim: false});
  validatePublicationAssets(release, checksums, process.env.RELEASE_CHECKSUMS, version);
  const latest = latestPublished(pages('releases'));
  const makeLatest = !latest || compareVersions(version, latest.tag_name) > 0;
  gh('release', 'edit', tag, '--repo', repository, '--draft=false', `--latest=${makeLatest}`);
  summary(`Published ${tag}; latest=${makeLatest}. All platform, documentation, signature and provenance checks passed.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  assert.ok(['classify', 'prepare', 'request', 'check-draft', 'create-draft', 'publish'].includes(process.argv[2]), 'Use classify, prepare, request, check-draft, create-draft or publish');
  if (process.argv[2] === 'classify') classify();
  else if (process.argv[2] === 'prepare') prepare();
  else if (process.argv[2] === 'request') request();
  else if (process.argv[2] === 'check-draft') checkDraft();
  else if (process.argv[2] === 'create-draft') createDraft(); else publish();
}
