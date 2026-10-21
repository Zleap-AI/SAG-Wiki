import {newId} from './identity';
import { TrimApp } from '@trimjs/web-app';
import { writable } from 'svelte/store';

export const appName = 'sag-desktop-fnos';
export const basePath = `/app/${appName}/`;
export const sdk = new TrimApp();
export const hostTheme = writable<'light' | 'dark'>('light');

export async function initializeHost() {
  await sdk.ready();
  if (sdk.isStandaloneWeb) {
    const query = matchMedia('(prefers-color-scheme: dark)');
    hostTheme.set(query.matches ? 'dark' : 'light');
    query.addEventListener('change', event => hostTheme.set(event.matches ? 'dark' : 'light'));
    return;
  }
  const config = await sdk.getPlatformConfig();
  hostTheme.set(config.theme);
  await sdk.setTitle('SAG Wiki · 飞牛');
  if (sdk.isWeb && !sdk.isStandaloneWeb) {
    await sdk.$on('os/theme', (theme: 'light' | 'dark') => hostTheme.set(theme));
  }
  return config;
}

export async function pickDirectory(): Promise<string[]> {
  await sdk.ready();
  if (sdk.isStandaloneWeb) {
    const state = newId();
    sessionStorage.setItem('fnos.auth-state', state);
    sessionStorage.setItem('fnos.reopen-picker', '1');
    await sdk.openAppAuth('pickUserFile', {
      appName, directory: true, redirectUri: `${basePath}auth-callback.html`, state,
    }, { target: '_self' });
    return [];
  }
  const result = await sdk.pickUserFile({ directory: true, title: '选择授权目录', okText: '确认授权' });
  if (!result) return [];
  if (result.code !== 0) throw new Error(result.msg || `飞牛授权失败（${result.code}）`);
  return result.data;
}

export function handleAuthCallback() {
  const result = sdk.parseAppAuthCallback(window.location.href);
  const expected = sessionStorage.getItem('fnos.auth-state');
  sessionStorage.removeItem('fnos.auth-state');
  if (!expected || result.state !== expected || result.appName !== appName || result.method !== 'pickUserFile') {
    throw new Error('授权回调校验失败，请重新选择目录。');
  }
  sessionStorage.setItem('fnos.auth-result', JSON.stringify(result));
  window.location.replace(basePath);
}
