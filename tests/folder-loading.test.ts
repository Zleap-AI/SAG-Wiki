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
  vi.clearAllMocks();
});

describe('文件夹读取状态', () => {
  it('目录响应前显示读取状态，响应后显示文件且不误报为空', async () => {
    let resolveEntries!: (entries: Entry[]) => void;
    const entries = new Promise<Entry[]>(resolve => {resolveEntries = resolve});
    invoke.mockImplementation(async command => {
      if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
      if (command === 'list_roots') return [{path: '/notes', name: 'Notes', status: 'ready', count: 1, error: null}];
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
    await vi.waitFor(() => expect(document.querySelector('.simple-file-row')?.textContent).toContain('README.md'));
    expect(document.querySelector('.folder-browser')?.textContent).not.toContain('正在读取文件夹…');
    expect(document.querySelector('.folder-browser')?.textContent).not.toContain('文件夹是空的');
  });
});
