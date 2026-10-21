// @vitest-environment happy-dom
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import App from '../src/App.svelte';
import IndexDetails from '../src/components/IndexDetails.svelte';
import type {IndexHistory, IndexStatus, Root} from '../src/lib/types';

const {invoke, listeners} = vi.hoisted(() => ({invoke: vi.fn(), listeners: new Map<string, (event: {payload: unknown}) => void>()}));
vi.mock('@tauri-apps/api/core', () => ({invoke}));
vi.mock('@tauri-apps/api/event', () => ({listen: vi.fn(async (name, handler) => {
  listeners.set(name, handler);
  return () => {if (listeners.get(name) === handler) listeners.delete(name)};
})}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow: () => ({onCloseRequested: async () => () => {}})}));

let app: ReturnType<typeof mount> | undefined;
const root: Root = {path: '/notes', name: 'Notes', status: 'ready', count: 1, error: null};
const record: IndexHistory = {id: 1, path: '/notes', name: 'First run', started_at: 1700000000000, finished_at: 1700000001000, completed: 1, total: 1, status: 'ready', error: null};
const idle: IndexStatus = {paused: false, tasks: []};
let status: IndexStatus, records: IndexHistory[];
let statusReads: number, historyReads: number, logReads: number;
const flush = async () => {for (let index = 0; index < 16; index++) await tick()};
const changed = () => {
  expect(listeners.get('data-changed')).toBeTypeOf('function');
  listeners.get('data-changed')!({payload: null});
};
const mountDetails = async (roots: Root[] = [root]) => {
  app = mount(IndexDetails, {target: document.body, props: {roots, onClose: vi.fn(), onPauseChange: vi.fn()}});
  await flush();
};

beforeEach(() => {
  vi.useFakeTimers({toFake: ['setTimeout', 'clearTimeout', 'Date']});
  status = structuredClone(idle); records = [structuredClone(record)];
  statusReads = historyReads = logReads = 0;
  invoke.mockImplementation(async command => {
    if (command === 'get_index_status') {statusReads++; return structuredClone(status)}
    if (command === 'get_index_history') {historyReads++; return structuredClone(records)}
    if (command === 'get_index_history_log') {logReads++; return {raw: JSON.stringify({time: 1700000000000, path: '/notes/a.pdf', stage: 'converting', status: 'completed', error: null})}}
    if (command === 'clear_index_history') {records = []; return}
    throw Error(`Unexpected command: ${command}`);
  });
});
afterEach(async () => {
  if (app) await unmount(app);
  app = undefined; document.body.innerHTML = ''; listeners.clear(); vi.clearAllMocks(); vi.useRealTimers();
});

it('completed history stops fast polling and discovers a missed same-root completion with the idle fallback', async () => {
  await mountDetails();
  expect(document.querySelector('.index-history')?.textContent).toContain('First run');
  await vi.advanceTimersByTimeAsync(3000);
  expect(statusReads).toBe(1);
  records = [{...record, id: 2, name: 'Second run'}, record];
  await vi.advanceTimersByTimeAsync(2000); await flush();
  expect(statusReads).toBe(2);
  expect(document.querySelector('.index-history')?.textContent).toContain('Second run');
});

it('a data event refreshes a new same-root completion even when task paths stay empty', async () => {
  await mountDetails();
  records = [{...record, id: 2, name: 'Second run'}, record];
  changed(); await flush();
  expect(document.querySelector('.index-history')?.textContent).toContain('Second run');
  expect(historyReads).toBe(2);
});

it('active progress keeps 600ms updates then slows down after completion', async () => {
  status = {paused: false, tasks: [{path: root.path, stage: 'indexing_native', completed: 1, total: 2, files: []}]};
  await mountDetails();
  expect(document.querySelector('.index-active')?.textContent).toContain('1 / 2');
  status.tasks[0].completed = 2;
  await vi.advanceTimersByTimeAsync(600); await flush();
  expect(document.querySelector('.index-active')?.textContent).toContain('2 / 2');
  status = structuredClone(idle);
  await vi.advanceTimersByTimeAsync(600); await flush();
  expect(document.querySelector('.index-active')).toBeNull();
  const completedReads = statusReads;
  await vi.advanceTimersByTimeAsync(3000);
  expect(statusReads).toBe(completedReads);
});

it('an unavailable history query does not hide or slow down active progress', async () => {
  status = {paused: false, tasks: [{path: root.path, stage: 'indexing_native', completed: 0, total: 2, files: []}]};
  const oldImplementation = invoke.getMockImplementation()!;
  invoke.mockImplementation(async command => {
    if (command === 'get_index_history') throw Error('History temporarily unavailable');
    return oldImplementation(command);
  });
  await mountDetails();
  expect(document.querySelector('.index-active')?.textContent).toContain('0 / 2');
  expect(document.querySelector('[role="alert"]')?.textContent).toContain('History temporarily unavailable');
  status.tasks[0].completed = 1;
  await vi.advanceTimersByTimeAsync(600); await flush();
  expect(document.querySelector('.index-active')?.textContent).toContain('1 / 2');
});

it('authoritative empty tasks stop fast polling even when a missed event leaves the parent root status stale', async () => {
  status = {paused: false, tasks: [{path: root.path, stage: 'indexing_native', completed: 1, total: 1, files: []}]};
  await mountDetails([{...root, status: 'indexing'}]);
  status = structuredClone(idle);
  await vi.advanceTimersByTimeAsync(600); await flush();
  expect(document.querySelector('.index-summary-state')?.textContent).toContain('索引已完成');
  expect(document.querySelector('.index-active')).toBeNull();
  const completedReads = statusReads;
  await vi.advanceTimersByTimeAsync(3000);
  expect(statusReads).toBe(completedReads);
});

it('equal history preserves expanded DOM, loaded logs, focus and scroll when events refresh it', async () => {
  await mountDetails();
  document.querySelector<HTMLButtonElement>('.index-history-toggle')!.click(); await flush();
  const row = document.querySelector('.index-history-toggle');
  const preview = document.querySelector('.index-file-preview');
  const file = document.querySelector<HTMLButtonElement>('.index-history-file')!;
  const body = document.querySelector<HTMLElement>('.index-detail-body')!;
  file.focus(); body.scrollTop = 64;
  const mutations: MutationRecord[] = [];
  const observer = new MutationObserver(changes => mutations.push(...changes));
  observer.observe(document.querySelector('.index-history')!, {subtree: true, childList: true, characterData: true});
  changed(); await flush();
  observer.disconnect();
  expect(historyReads).toBe(2);
  expect(document.querySelector('.index-history-toggle')).toBe(row);
  expect(document.querySelector('.index-file-preview')).toBe(preview);
  expect(document.activeElement).toBe(file);
  expect(body.scrollTop).toBe(64);
  expect(logReads).toBe(1);
  expect(mutations).toHaveLength(0);
});

it('an event during a history read discards that response and queues a fresh snapshot', async () => {
  await mountDetails();
  let release!: (rows: IndexHistory[]) => void;
  const oldImplementation = invoke.getMockImplementation()!;
  let held = true;
  invoke.mockImplementation(async command => {
    if (command === 'get_index_history' && held) {held = false; return new Promise<IndexHistory[]>(resolve => {release = resolve})}
    return oldImplementation(command);
  });
  changed(); await flush();
  expect(release).toBeTypeOf('function');
  records = [{...record, id: 3, name: 'Newest run'}];
  changed(); await flush();
  release([{...record, id: 2, name: 'Stale run'}]); await flush();
  expect(document.querySelector('.index-history')?.textContent).toContain('Newest run');
  expect(document.querySelector('.index-history')?.textContent).not.toContain('Stale run');
});

it('resuming during an event-driven history read retains the pending history refresh', async () => {
  status.paused = true;
  await mountDetails();
  let release!: (rows: IndexHistory[]) => void;
  const oldImplementation = invoke.getMockImplementation()!;
  let held = true;
  invoke.mockImplementation(async (command, args) => {
    if (command === 'get_index_history' && held) {held = false; return new Promise<IndexHistory[]>(resolve => {release = resolve})}
    if (command === 'set_index_paused') {status.paused = args.paused; return structuredClone(status)}
    return oldImplementation(command);
  });
  changed(); await flush();
  records = [{...record, id: 2, name: 'Completion during resume'}];
  document.querySelector<HTMLButtonElement>('[aria-label="继续索引"]')!.click(); await flush();
  release([record]); await flush();
  expect(document.querySelector('.index-history')?.textContent).toContain('Completion during resume');
});

it('unmount removes the event subscription and leaves no active or idle timer', async () => {
  await mountDetails();
  await unmount(app!); app = undefined;
  expect(listeners.has('data-changed')).toBe(false);
  await vi.advanceTimersByTimeAsync(30000);
  expect(statusReads).toBe(1);
});

it('App serializes data refreshes and never paints an obsolete root snapshot', async () => {
  let rootReads = 0, concurrentReads = 0, maxConcurrentReads = 0;
  let release!: (roots: Root[]) => void;
  let releaseNewest!: (roots: Root[]) => void;
  invoke.mockImplementation(async command => {
    if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
    if (command === 'get_index_status') return idle;
    if (command === 'list_roots') {
      rootReads++; concurrentReads++; maxConcurrentReads = Math.max(maxConcurrentReads, concurrentReads);
      if (rootReads === 2) return new Promise<Root[]>(resolve => {release = rows => {concurrentReads--; resolve(rows)}});
      if (rootReads === 3) return new Promise<Root[]>(resolve => {releaseNewest = rows => {concurrentReads--; resolve(rows)}});
      concurrentReads--; return [{...root, name: rootReads === 1 ? 'Initial root' : 'Newest root'}];
    }
    return [];
  });
  app = mount(App, {target: document.body}); await flush();
  expect(document.querySelector('.tree-name')?.textContent).toContain('Initial root');
  changed(); await flush();
  changed(); await flush();
  expect(maxConcurrentReads).toBe(1);
  release([{...root, name: 'Obsolete root'}]); await flush();
  expect(document.querySelector('.tree-name')?.textContent).toContain('Initial root');
  releaseNewest([{...root, name: 'Newest root'}]); await flush();
  expect(document.querySelector('.tree-name')?.textContent).toContain('Newest root');
  expect(document.querySelector('.tree-name')?.textContent).not.toContain('Obsolete root');
});

it('App subscribes before its initial snapshot so a change during startup is not lost', async () => {
  let release!: (roots: Root[]) => void;
  let rootReads = 0;
  invoke.mockImplementation(async command => {
    if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
    if (command === 'get_index_status') return idle;
    if (command === 'list_roots') {if (++rootReads === 1) return new Promise<Root[]>(resolve => {release = resolve}); return [{...root, name: 'Startup update'}]}
    return [];
  });
  app = mount(App, {target: document.body}); await flush();
  changed();
  release([{...root, name: 'Stale startup'}]); await flush();
  expect(document.querySelector('.tree-name')?.textContent).toContain('Startup update');
});

it('an older App snapshot cannot undo a pause acknowledged by the index dialog', async () => {
  let fileReads = 0;
  let release!: (files: unknown[]) => void;
  status = {paused: false, tasks: [{path: root.path, stage: 'indexing_native', completed: 0, total: 2, files: []}]};
  invoke.mockImplementation(async (command, args) => {
    if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
    if (command === 'list_roots') return [{...root, status: 'indexing'}];
    if (command === 'list_files') {if (++fileReads === 2) return new Promise(resolve => {release = resolve}); return []}
    if (command === 'set_index_paused') {status.paused = args.paused; return structuredClone(status)}
    if (command === 'get_index_status') return structuredClone(status);
    return [];
  });
  app = mount(App, {target: document.body}); await flush();
  changed(); await flush();
  document.querySelector<HTMLButtonElement>('[aria-label="查看索引详情"]')!.click(); await flush();
  document.querySelector<HTMLButtonElement>('[aria-label="暂停索引"]')!.click(); await flush();
  expect(document.querySelector('.index-status')?.textContent).toContain('索引已暂停');
  release([]); await flush();
  expect(document.querySelector('.index-status')?.textContent).toContain('索引已暂停');
});
