'use client';

import {useEffect, useState} from 'react';
import {latestReleaseApi, latestReleaseUrl, parseDesktopDownloads, type DesktopDownloads} from '../lib/release-downloads';

const copy = {
  en: {
    unavailable: 'Download details are temporarily unavailable. ',
    loading: 'Loading the latest release… ',
    latest: 'Open the latest release',
    intro: (version: string) => <>Installers for <strong>{version}</strong> are available below.</>,
    system: 'System', architecture: 'Architecture', download: 'Download', size: 'Size',
    dmg: 'Download DMG', installer: 'Download installer',
  },
  zh: {
    unavailable: '暂时无法读取下载信息。',
    loading: '正在读取最新版本… ',
    latest: '前往最新正式版下载',
    intro: (version: string) => <>当前提供 <strong>{version}</strong> 安装包。</>,
    system: '系统', architecture: '架构', download: '下载', size: '大小',
    dmg: '下载 DMG', installer: '下载安装器',
  },
  ja: {
    unavailable: 'ダウンロード情報を取得できませんでした。',
    loading: '最新のリリースを取得しています… ',
    latest: '最新の安定版を開く',
    intro: (version: string) => <><strong>{version}</strong> のインストーラーをダウンロードできます。</>,
    system: 'OS', architecture: 'アーキテクチャ', download: 'ダウンロード', size: 'サイズ',
    dmg: 'DMG をダウンロード', installer: 'インストーラーをダウンロード',
  },
};

export function ReleaseDownloads({language}: {language: keyof typeof copy}) {
  const [downloads, setDownloads] = useState<DesktopDownloads | null>(null);
  const [failed, setFailed] = useState(false);
  const labels = copy[language];
  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 8000);
    async function load() {
      try {
        const response = await fetch(latestReleaseApi, {signal: controller.signal, cache: 'no-store', credentials: 'omit', referrerPolicy: 'no-referrer'});
        if (!response.ok) throw new Error('Release request failed');
        const result = parseDesktopDownloads(await response.json());
        if (active) setDownloads(result);
      } catch {
        if (active) setFailed(true);
      } finally {
        clearTimeout(timeout);
      }
    }
    void load();
    return () => {active = false; clearTimeout(timeout); controller.abort();};
  }, []);

  if (!downloads) return <p role="status">{failed ? labels.unavailable : labels.loading}
    <a href={latestReleaseUrl}>{labels.latest}</a>
  </p>;
  return <>
    <p>{labels.intro(downloads.version)}</p>
    <table>
      <thead><tr><th>{labels.system}</th><th>{labels.architecture}</th><th>{labels.download}</th><th>{labels.size}</th></tr></thead>
      <tbody>
        <tr><td>macOS</td><td>Apple Silicon</td><td><a href={downloads.macos.url}>{labels.dmg}</a></td><td>{downloads.macos.size}</td></tr>
        {downloads.macosIntel && <tr><td>macOS</td><td>Intel (x86_64)</td><td><a href={downloads.macosIntel.url}>{labels.dmg}</a></td><td>{downloads.macosIntel.size}</td></tr>}
        <tr><td>Windows</td><td>x64</td><td><a href={downloads.windows.url}>{labels.installer}</a></td><td>{downloads.windows.size}</td></tr>
      </tbody>
    </table>
  </>;
}
