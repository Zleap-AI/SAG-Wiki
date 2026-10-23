'use client';

import {useEffect, useState} from 'react';
import {latestReleaseApi, latestReleaseUrl, parseDesktopDownloads, type DesktopDownloads} from '../lib/release-downloads';

export function ReleaseDownloads({language}: {language: 'zh' | 'en'}) {
  const [downloads, setDownloads] = useState<DesktopDownloads | null>(null);
  const [failed, setFailed] = useState(false);
  const english = language === 'en';
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

  if (!downloads) return <p role="status">{failed
    ? (english ? 'Download details are temporarily unavailable. ' : '暂时无法读取下载信息。')
    : (english ? 'Loading the latest release… ' : '正在读取最新版本… ')}
    <a href={latestReleaseUrl}>{english ? 'Open the latest release' : '前往最新正式版下载'}</a>
  </p>;
  return <>
    <p>{english ? <>Installers for <strong>{downloads.version}</strong> are available below.</> : <>当前提供 <strong>{downloads.version}</strong> 安装包。</>}</p>
    <table>
      <thead><tr><th>{english ? 'System' : '系统'}</th><th>{english ? 'Architecture' : '架构'}</th><th>{english ? 'Download' : '下载'}</th><th>{english ? 'Size' : '大小'}</th></tr></thead>
      <tbody>
        <tr><td>macOS</td><td>Apple Silicon</td><td><a href={downloads.macos.url}>{english ? 'Download DMG' : '下载 DMG'}</a></td><td>{downloads.macos.size}</td></tr>
        <tr><td>Windows</td><td>x64</td><td><a href={downloads.windows.url}>{english ? 'Download installer' : '下载安装器'}</a></td><td>{downloads.windows.size}</td></tr>
      </tbody>
    </table>
  </>;
}
