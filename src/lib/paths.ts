import { derived, writable } from 'svelte/store';

export const pathLabels = writable<Record<string, string>>({});
export const displayPath = derived(pathLabels, labels => (path: string) => {
  const root = Object.keys(labels).sort((a,b) => b.length-a.length)
    .find(root => path === root || path.startsWith(root + '/'));
  return root ? labels[root] + path.slice(root.length) : path;
});
