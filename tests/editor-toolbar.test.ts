// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';
import RichEditor from '../src/components/RichEditor.svelte';

let app: ReturnType<typeof mount>;
afterEach(async () => {
  await unmount(app);
  document.body.innerHTML = '';
});

it('撤销和重做按钮反映文档编辑历史', async () => {
  const onChange = vi.fn();
  app = mount(RichEditor, {
    target: document.body,
    props: { body: '原文', path: '/notes.md', onChange },
  });
  await tick();
  const undo = document.querySelector<HTMLButtonElement>('[aria-label="撤销"]')!;
  const redo = document.querySelector<HTMLButtonElement>('[aria-label="重做"]')!;
  expect(undo.disabled).toBe(true);
  expect(redo.disabled).toBe(true);

  const paste = new Event('paste', { bubbles: true, cancelable: true });
  Object.defineProperty(paste, 'clipboardData', {
    value: { getData: (format: string) => format === 'text/plain' ? '新增文字' : '' },
  });
  document.querySelector<HTMLElement>('.tiptap')!.dispatchEvent(paste);
  await tick();
  expect(undo.disabled).toBe(false);
  expect(redo.disabled).toBe(true);

  undo.click();
  await tick();
  expect(undo.disabled).toBe(true);
  expect(redo.disabled).toBe(false);
  expect(document.querySelector('.tiptap')!.textContent).toBe('原文');

  redo.click();
  await tick();
  expect(undo.disabled).toBe(false);
  expect(redo.disabled).toBe(true);
  expect(document.querySelector('.tiptap')!.textContent).toContain('新增文字');
});
