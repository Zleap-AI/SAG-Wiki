// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';
import App from '../src/App.svelte';
import type { Candidate, Document } from '../src/lib/types';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onCloseRequested: async () => () => {} }) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

let app: ReturnType<typeof mount>;
afterEach(async () => {
  await unmount(app);
  document.body.innerHTML = '';
  vi.clearAllMocks();
});

const hits: Candidate[] = [1, 2].map(id => ({
  id, document_id: id, path: `/files/file-${id}.txt`, title: `file-${id}.txt`,
  heading: '', content: 'octx 文档', start_line: 1, end_line: 1,
  source_ranges: [[1, 1]], source_spans: [], bm25: -1, score: 0, order: id, hash: 'same',
}));

async function setup() {
  const reads = new Map<string, { resolve: (doc: Document) => void; reject: (error: Error) => void }>();
  invoke.mockImplementation((command, args) => {
    if (command === 'get_settings') return Promise.resolve({ theme: 'light', start_page: 'search', editor_size: 15, answer: { model: 'test', models: ['test'] } });
    if (command === 'run_query') return Promise.resolve({ results: hits, terms: ['octx'], elapsed_ms: 24, trace: [], conversation: null });
    if (command === 'read_document') return new Promise<Document>((resolve, reject) => reads.set(args.path, { resolve, reject }));
    return Promise.resolve([]);
  });
  app = mount(App, { target: document.body });
  await vi.waitFor(() => expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull());
  const input = document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;
  input.value = 'octx';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  input.closest('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(reads.has(hits[0].path)).toBe(true));
  return reads;
}

it('原文尚未读取完时已结束搜索，预览区显示读取状态', async () => {
  const reads = await setup();
  expect(document.querySelector('.search-page')!.getAttribute('aria-busy')).toBe('false');
  expect(document.querySelector('[aria-label="停止搜索"]')).toBeNull();
  expect(document.querySelector('[aria-label="搜索模式"]')?.textContent).toContain('深入');
  expect(document.querySelector('.search-summary')!.textContent).toContain('2 个文件');
  expect(document.querySelector('.search-preview')!.textContent).toContain('正在读取文件…');

  reads.get(hits[0].path)!.resolve({ id: 1, path: hits[0].path, name: hits[0].title, body: '第一份 octx 正文', hash: 'same' });
  await vi.waitFor(() => expect(document.querySelector('.search-preview-body')!.textContent).toContain('第一份 octx 正文'));
  expect(document.querySelector('.search-preview')!.textContent).not.toContain('正在读取文件…');
});

it('原文读取失败后结束预览等待并保留搜索结果', async () => {
  const reads = await setup();
  reads.get(hits[0].path)!.reject(new Error('文件暂时不可读'));
  await vi.waitFor(() => expect(document.querySelector('.search-preview')!.textContent).toContain('文件暂时不可读'));
  expect(document.querySelector('.search-preview')!.textContent).not.toContain('正在读取文件…');
  expect(document.querySelectorAll('.search-result')).toHaveLength(2);
  expect(document.querySelector('.search-page')!.getAttribute('aria-busy')).toBe('false');
});

it('前一个文件的迟到错误不会清除新文件的等待状态或显示错误', async () => {
  const reads = await setup();
  document.querySelectorAll<HTMLButtonElement>('.search-result')[1].click();
  await vi.waitFor(() => expect(reads.has(hits[1].path)).toBe(true));
  reads.get(hits[0].path)!.reject(new Error('旧文件的迟到错误'));
  await tick();
  await tick();
  expect(document.querySelector('.search-preview')!.textContent).toContain('正在读取文件…');
  expect(document.querySelector('.search-preview')!.textContent).not.toContain('旧文件的迟到错误');

  reads.get(hits[1].path)!.resolve({ id: 2, path: hits[1].path, name: hits[1].title, body: '第二份 octx 正文', hash: 'same' });
  await vi.waitFor(() => expect(document.querySelector('.search-preview-body')!.textContent).toContain('第二份 octx 正文'));
  expect(document.querySelector('.search-preview')!.textContent).not.toContain('正在读取文件…');
  expect(document.querySelector('.search-preview')!.textContent).not.toContain('旧文件的迟到错误');
});
