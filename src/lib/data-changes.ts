import {listen} from '@tauri-apps/api/event';

// App and the history dialog share one native subscription; each owns its cleanup.
const subscribers = new Set<() => void>();
let subscription: Promise<() => void> | null = null;

export async function subscribeDataChanges(callback: () => void): Promise<() => void> {
  const subscriber = () => callback();
  subscribers.add(subscriber);
  const current = subscription ??= listen('data-changed', () => {
    for (const notify of [...subscribers]) notify();
  });
  let stop: () => void;
  try {
    stop = await current;
  } catch (error) {
    subscribers.delete(subscriber);
    if (subscription === current) subscription = null;
    throw error;
  }
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    subscribers.delete(subscriber);
    if (!subscribers.size && subscription === current) {
      subscription = null;
      stop();
    }
  };
}
