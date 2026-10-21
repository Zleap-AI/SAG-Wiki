// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest';
const host = vi.hoisted(() => ({ standalone: false, result: { code: 0, msg: '', data: ['/vol1/user/docs'] }, open: vi.fn(), settings: vi.fn(), callback: {} as Record<string, unknown> }));
vi.mock('@trimjs/web-app', () => ({ TrimApp: class {
  get isStandaloneWeb() { return host.standalone; }
  ready() { return Promise.resolve(); }
  pickUserFile() { return Promise.resolve(host.result); }
  openAppAuth(...args: unknown[]) { host.open(...args); return Promise.resolve(''); }
  openAppSetting() { return host.settings(); }
  parseAppAuthCallback() { return host.callback; }
} }));
import { appName, handleAuthCallback, pickDirectory } from '../src/lib/platform';
import { initializeStorage, newId, userStorage } from '../src/lib/identity';
import { api } from '../src/lib/api';

beforeEach(() => { host.standalone = false; host.open.mockClear(); host.settings.mockReset(); vi.restoreAllMocks(); sessionStorage.clear(); });
describe('fnOS browser integration', () => {
  it('opens the fnOS desktop without calling a host-only API in standalone pages', async () => {
    host.standalone = true;
    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    await api('open_data_directory');
    expect(open).toHaveBeenCalledWith(new URL('/', window.location.href).href, '_blank', 'noopener,noreferrer');
    expect(host.settings).not.toHaveBeenCalled();
  });
  it('opens native app settings inside fnOS and reports host failures', async () => {
    await api('open_data_directory');
    expect(host.settings).toHaveBeenCalledOnce();
    host.settings.mockRejectedValueOnce(new Error('native settings failed'));
    await expect(api('open_data_directory')).rejects.toThrow('native settings failed');
  });
  it('uses the native picker and returns its authorized directories', async () => {
    host.result = { code: 0, msg: '', data: ['/vol1/user/docs'] };
    expect(await pickDirectory()).toEqual(['/vol1/user/docs']);
    host.result = { code: 403, msg: '用户拒绝授权', data: [] };
    await expect(pickDirectory()).rejects.toThrow('用户拒绝授权');
  });
  it('uses a same-origin callback and one-time state in standalone browsers', async () => {
    host.standalone = true;
    expect(await pickDirectory()).toEqual([]);
    const [, params, options] = host.open.mock.calls[0];
    expect(params.appName).toBe(appName);
    expect(params.redirectUri).toBe('/app/sag-desktop-fnos/auth-callback.html');
    expect(params.state).toBe(sessionStorage.getItem('fnos.auth-state'));
    expect(options.target).toBe('_self');
  });
  it('rejects forged and replayed authorization callbacks', () => {
    host.callback = { appName, method: 'pickUserFile', state: 'forged', status: 'success' };
    sessionStorage.setItem('fnos.auth-state', 'expected');
    expect(handleAuthCallback).toThrow('授权回调校验失败');
    expect(sessionStorage.getItem('fnos.auth-state')).toBeNull();
    expect(handleAuthCallback).toThrow('授权回调校验失败');
  });
  it('isolates browser history when a different NAS user logs in', () => {
    initializeStorage(1000); userStorage.setItem('history', 'Alice');
    initializeStorage(1001); expect(userStorage.getItem('history')).toBeNull();
    userStorage.setItem('history', 'Bob');
    initializeStorage(1000); expect(userStorage.getItem('history')).toBe('Alice');
  });
  it('generates valid random UUIDs without requiring crypto.randomUUID', () => {
    const id = newId();
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(newId()).not.toBe(id);
  });
});
