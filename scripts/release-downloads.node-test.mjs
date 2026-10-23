import assert from 'node:assert/strict';
import test from 'node:test';
import {parseDesktopDownloads} from '../docs-site/lib/release-downloads.ts';
const fixture = (version = '0.2.0') => ({tag_name: `v${version}`, draft: false, prerelease: false,
  published_at: '2026-10-23T00:00:00Z', assets: [
    ['macOS-arm64.dmg', 15_432_100], ['Windows-x64-setup.exe', 11_678_900],
  ].map(([suffix, size]) => ({name: `SAG-Wiki-${version}-${suffix}`, state: 'uploaded', size,
    browser_download_url: `https://github.com/Zleap-AI/SAG-Wiki/releases/download/v${version}/SAG-Wiki-${version}-${suffix}`}))});
test('derives version, canonical URLs and decimal sizes from published installers', () => {
  const release = fixture(); release.assets.push({name: 'update.tar.gz', size: 123});
  const result = parseDesktopDownloads(release);
  assert.equal(result.version, '0.2.0');
  assert.equal(result.macos.size, '15.4MB');
  assert.equal(result.windows.size, '11.7MB');
  assert.equal(result.macos.url, release.assets[0].browser_download_url);
  assert.equal(parseDesktopDownloads(fixture('1.10.0')).version, '1.10.0');
});
test('rejects drafts, prereleases, unpublished and malformed data', () => {
  for (const value of [null, [], {}, {...fixture(), draft:true}, {...fixture(), prerelease:true}, {...fixture(), published_at:null}, {...fixture(), tag_name:'v0.2.0-beta'}, {...fixture(), tag_name:'v00.2.0'}, {...fixture(), assets:null}]) {
    assert.throws(() => parseDesktopDownloads(value));
  }
});
test('requires both complete installers and rejects duplicate or unsafe assets', () => {
  const missing = fixture(); missing.assets.pop(); assert.throws(() => parseDesktopDownloads(missing));
  const duplicate = fixture(); duplicate.assets.push(duplicate.assets[0]); assert.throws(() => parseDesktopDownloads(duplicate));
  for (const change of [{size:0}, {size:-1}, {size:1.5}, {state:'new'}, {browser_download_url:'https://example.com/file'}]) {
    const release = fixture(); Object.assign(release.assets[0], change); assert.throws(() => parseDesktopDownloads(release));
  }
});
