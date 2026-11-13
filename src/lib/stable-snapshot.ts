import type {IndexStatus,IndexTask} from './types';

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

export function indexTaskKey(task:IndexTask):string{return task.id||`${task.path}:${task.kind||'primary'}`;}

export function sameIndexStatus(previous: IndexStatus, next: IndexStatus): boolean {
  const oldStates=previous.conversion_states??[],nextStates=next.conversion_states??[];
  return oldStates.length===nextStates.length && nextStates.every((state,index)=>{const old=oldStates[index];return old.source===state.source&&old.root===state.root&&old.status===state.status&&old.searchable===state.searchable&&old.error===state.error;}) && previous.paused === next.paused && previous.tasks.length === next.tasks.length && next.tasks.every((task, index) => {
    const old = previous.tasks[index];
    return indexTaskKey(old) === indexTaskKey(task) && old.kind === task.kind && old.path === task.path && old.stage === task.stage && old.completed === task.completed && old.total === task.total &&
      (old.conversions?.length??0) === (task.conversions?.length??0) &&
      (task.conversions??[]).every((item, conversionIndex) => item.path === old.conversions?.[conversionIndex].path && item.engine === old.conversions?.[conversionIndex].engine) &&
      old.files.length === task.files.length && task.files.every((file, fileIndex) => file === old.files[fileIndex]);
  });
}
