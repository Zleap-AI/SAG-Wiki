// @vitest-environment happy-dom
import {afterEach, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import App from '../src/App.svelte';

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

it('文件夹菜单的问答开启独立对话，并将新问题限定在该文件夹', async () => {
  const previous = {id: 'previous-chat', title: '原有对话', updated: 0, deleted: false, messages: [{
    question: '原有问题', answer: '原有回答', model: 'test', evidence: [], scope: [], references: [],
    status: 'complete', error: null, retrieval_question: '原有问题',
  }]};
  invoke.mockImplementation(async command => {
    if (command === 'get_settings') return {theme: 'light', start_page: 'ask', editor_size: 15, answer: {model: 'test', models: ['test']}};
    if (command === 'list_roots') return [{path: '/notes', name: 'Notes', status: 'ready', count: 1, error: null}];
    if (command === 'list_conversations') return [previous];
    if (command === 'run_query') return new Promise(() => {});
    return [];
  });
  app = mount(App, {target: document.body});
  await vi.waitFor(() => expect(document.querySelector('[title="原有对话"]')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[title="原有对话"]')!.click();
  await vi.waitFor(() => expect(document.querySelector('.answer-text')?.textContent).toBe('原有回答'));

  document.querySelector<HTMLButtonElement>('[aria-label="Notes 的文件夹选项"]')!.click();
  await vi.waitFor(() => expect(document.querySelector('[role="menuitem"]')).not.toBeNull());
  const ask = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(item => item.textContent?.trim() === '问答')!;
  ask.click();
  await vi.waitFor(() => expect(document.querySelector('.answer-text')).toBeNull());
  expect(document.querySelector('.composer .scope-button')?.textContent).toContain('notes');
  expect(document.querySelector('.history-row.active')).toBeNull();

  const input = document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
  input.value = '新文件夹问题';
  input.dispatchEvent(new Event('input', {bubbles: true}));
  await tick();
  input.closest('form')!.dispatchEvent(new Event('submit', {bubbles: true, cancelable: true}));
  await vi.waitFor(() => expect(invoke.mock.calls.some(([command]) => command === 'run_query')).toBe(true));
  const request = invoke.mock.calls.find(([command]) => command === 'run_query')![1].query;
  expect(request).toMatchObject({conversation_id: null, question: '新文件夹问题', scope: ['/notes'], references: [], replace_from: null});
  expect(invoke.mock.calls.some(([command]) => command === 'replace_messages' || command === 'edit_conversation')).toBe(false);
});
