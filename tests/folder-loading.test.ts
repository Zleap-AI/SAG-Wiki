// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import App from '../src/App.svelte';
import type {Entry} from '../src/lib/types';

const {invoke} = vi.hoisted(() => ({invoke: vi.fn()}));
vi.mock('@tauri-apps/api/core', () => ({invoke}));
vi.mock('@tauri-apps/api/event', () => ({listen: vi.fn(async () => () => {})}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow: () => ({onCloseRequested: async () => () => {}})}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));

let app: ReturnType<typeof mount>;
afterEach(async () => {
  await unmount(app);
  document.body.innerHTML = '';
  localStorage.removeItem('sag.sidebar.expanded');
  vi.clearAllMocks();
});

describe('文件夹读取状态', () => {
  it.each(['/notes', '/notes/subfolder'])('移除 %s 前确认，取消保留文件夹，确认后只提交一次', async path => {
    let removed = false;
    let finish!: () => void;
    invoke.mockImplementation(async (command, args) => {
      if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
      if (command === 'list_roots') return removed && path === '/notes' ? [] : [{path: '/notes', name: 'Notes', status: 'ready', count: 0, error: null}];
      if (command === 'list_entries') return removed ? [] : [{path: '/notes/subfolder', name: 'subfolder', directory: true, symlink: false}];
      if (command === 'remove_root') return new Promise<void>(resolve => {finish = () => {removed = true; resolve();};});
      return [];
    });
    app = mount(App, {target: document.body});
    await vi.waitFor(() => expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
    if (path !== '/notes') {
      document.querySelector<HTMLButtonElement>('.tree-row .tree-toggle')!.click();
      await vi.waitFor(() => expect(document.querySelector('.tree-name[title="/notes/subfolder"]')).not.toBeNull());
    }
    const row = document.querySelector(`.tree-name[title="${path}"]`)!.closest('.tree-row')!;
    row.querySelector<HTMLButtonElement>('.tree-more')!.click();
    const label = path === '/notes' ? '移除' : '从知识库移除';
    await vi.waitFor(() => expect([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent === label)).toBe(true));
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item => item.textContent === label)!.click();
    await vi.waitFor(() => expect(document.querySelector('[aria-label="确认移除文件夹"]')).not.toBeNull());
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain(path);
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('本地原文件和会话记录会保留');
    expect(invoke.mock.calls.some(([command]) => command === 'remove_root')).toBe(false);
    document.querySelector<HTMLButtonElement>('[aria-label="取消移除文件夹"]')!.click();
    await vi.waitFor(() => expect(document.querySelector('[aria-label="确认移除文件夹"]')).toBeNull());
    expect(document.querySelector(`.tree-name[title="${path}"]`)).not.toBeNull();
    expect(invoke.mock.calls.some(([command]) => command === 'remove_root')).toBe(false);
    row.querySelector<HTMLButtonElement>('.tree-more')!.click();
    await vi.waitFor(() => expect([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent === label)).toBe(true));
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item => item.textContent === label)!.click();
    await vi.waitFor(() => expect(document.querySelector('[aria-label="确认移除文件夹"]')).not.toBeNull());
    const confirm = document.querySelector<HTMLButtonElement>('[aria-label="确认移除文件夹"]')!;
    confirm.click();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith('remove_root', {path}));
    expect(confirm.disabled).toBe(true);
    confirm.click();
    expect(invoke.mock.calls.filter(([command]) => command === 'remove_root')).toHaveLength(1);
    finish();
    await vi.waitFor(() => expect(document.querySelector('[aria-label="确认移除文件夹"]')).toBeNull());
    expect(document.querySelector('#notice')?.textContent).toContain('原文件保留');
  });

  it('移除失败时保留确认框并显示原因', async () => {
    invoke.mockImplementation(async command => {
      if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
      if (command === 'list_roots') return [{path: '/notes', name: 'Notes', status: 'ready', count: 0, error: null}];
      if (command === 'remove_root') throw new Error('移除失败');
      return [];
    });
    app = mount(App, {target: document.body});
    await vi.waitFor(() => expect(document.querySelector('.tree-more')).not.toBeNull());
    document.querySelector<HTMLButtonElement>('.tree-more')!.click();
    await vi.waitFor(() => expect([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent === '移除')).toBe(true));
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item => item.textContent === '移除')!.click();
    await vi.waitFor(() => expect(document.querySelector('[aria-label="确认移除文件夹"]')).not.toBeNull());
    document.querySelector<HTMLButtonElement>('[aria-label="确认移除文件夹"]')!.click();
    await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('移除失败'));
    expect(document.querySelector<HTMLButtonElement>('[aria-label="确认移除文件夹"]')!.disabled).toBe(false);
    expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull();
  });

  it.each(['button', 'contextmenu'])('最外层文件夹通过 %s 菜单打开系统文件管理器', async trigger => {
    invoke.mockImplementation(async command => {
      if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
      if (command === 'list_roots') return [{path: '/notes', name: 'Notes', status: 'ready', count: 0, error: null}];
      return [];
    });
    app = mount(App, {target: document.body});
    await vi.waitFor(() => expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());
    const row = document.querySelector('.tree-name[title="/notes"]')!.closest('.tree-row')!;
    if (trigger === 'button') row.querySelector<HTMLButtonElement>('.tree-more')!.click();
    else row.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    const label = /Mac/i.test(navigator.userAgent) ? '在 Finder 中打开' : '在文件资源管理器中打开';
    await vi.waitFor(() => expect([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent?.includes(label))).toBe(true));
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item => item.textContent?.includes(label))!;
    item.click();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith('reveal_path', {path: '/notes'}));
  });

  it('目录响应前显示读取状态，响应后显示文件且不误报为空', async () => {
    let resolveEntries!: (entries: Entry[]) => void;
    const entries = new Promise<Entry[]>(resolve => {resolveEntries = resolve});
    invoke.mockImplementation(async command => {
      if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
      if (command === 'list_roots') return [{path: '/notes', name: 'Notes', status: 'ready', count: 1, error: null}];
      if (command === 'list_files') return [{id:1,path:'/notes/README.md',name:'README.md'}];
      if (command === 'list_entries') return entries;
      return [];
    });
    app = mount(App, {target: document.body});
    await vi.waitFor(() => expect(document.querySelector('.tree-name[title="/notes"]')).not.toBeNull());

    document.querySelector<HTMLButtonElement>('.tree-name[title="/notes"]')!.click();
    await tick();
    await vi.waitFor(() => expect(document.querySelector('.folder-browser')?.textContent).toContain('正在读取文件夹…'));
    expect(document.querySelector('.folder-browser')?.textContent).not.toContain('文件夹是空的');
    expect(invoke.mock.calls.filter(([command]) => command === 'list_entries')).toHaveLength(1);

    resolveEntries([{path: '/notes/README.md', name: 'README.md', directory: false, symlink: false}]);
    await vi.waitFor(() => expect(document.querySelector('.simple-file-row[title="/notes/README.md"]')?.textContent).toContain('README.md'));
    await vi.waitFor(() => expect(document.querySelector('.folder-browser')?.textContent).not.toContain('正在读取文件夹…'));
    expect(document.querySelector('.folder-browser')?.textContent).not.toContain('文件夹是空的');
  });
});
