export const latestReleaseUrl = 'https://github.com/Zleap-AI/SAG-Wiki/releases/latest';
export const latestReleaseApi = 'https://api.github.com/repos/Zleap-AI/SAG-Wiki/releases/latest';
export type DesktopDownloads = {
  version: string;
  macos: {url: string; size: string};
  windows: {url: string; size: string};
};

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid release data');
  return value as Record<string, unknown>;
}

export function parseDesktopDownloads(value: unknown): DesktopDownloads {
  const release = record(value);
  if (release.draft !== false || release.prerelease !== false || typeof release.published_at !== 'string' || !release.published_at) throw new Error('Expected a published stable release');
  if (typeof release.tag_name !== 'string' || !/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(release.tag_name)) throw new Error('Invalid desktop release tag');
  if (!Array.isArray(release.assets)) throw new Error('Missing release assets');
  const version = release.tag_name.slice(1);
  const assets = release.assets.map(record);
  function installer(suffix: string) {
    const name = `SAG-Wiki-${version}-${suffix}`;
    const matches = assets.filter(asset => asset.name === name);
    if (matches.length !== 1) throw new Error('Missing or duplicate installer');
    const asset = matches[0];
    const url = `https://github.com/Zleap-AI/SAG-Wiki/releases/download/v${version}/${name}`;
    if (asset.state !== 'uploaded' || asset.browser_download_url !== url || typeof asset.size !== 'number' || !Number.isSafeInteger(asset.size) || asset.size <= 0) throw new Error('Invalid installer metadata');
    return {url, size: `${(asset.size / 1_000_000).toFixed(1)}MB`};
  }
  return {version, macos: installer('macOS-arm64.dmg'), windows: installer('Windows-x64-setup.exe')};
}
