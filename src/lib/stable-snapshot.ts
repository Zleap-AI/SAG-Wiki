import type {IndexStatus} from './types';

// These protocol rows contain scalar fields. Reuse unchanged rows and the whole
// array when possible so unrelated progress does not invalidate history content.
export function stableRows<T extends object>(previous: T[], next: T[], key: (row: T) => string | number): T[] {
  const byKey = new Map(previous.map(row => [key(row), row]));
  let same = previous.length === next.length;
  const rows = next.map((row, index) => {
    const old = byKey.get(key(row));
    const fields = Object.keys(row) as (keyof T)[];
    const value = old && Object.keys(old).length === fields.length && fields.every(field => old[field] === row[field]) ? old : row;
    if (value !== previous[index]) same = false;
    return value;
  });
  return same ? previous : rows;
}

export function sameIndexStatus(previous: IndexStatus, next: IndexStatus): boolean {
  return previous.paused === next.paused && previous.tasks.length === next.tasks.length && next.tasks.every((task, index) => {
    const old = previous.tasks[index];
    return old.path === task.path && old.stage === task.stage && old.completed === task.completed && old.total === task.total &&
      old.files.length === task.files.length && task.files.every((file, fileIndex) => file === old.files[fileIndex]);
  });
}
