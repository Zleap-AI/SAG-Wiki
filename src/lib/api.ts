import {newId} from './identity';
import { basePath, sdk } from './platform';
import { pathLabels } from './paths';
import { language } from './i18n';
import { get } from 'svelte/store';

const session = newId();
type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let ready: Promise<void> | undefined;

function connectEvents() {
  if (ready) return ready;
  ready = new Promise<void>((resolve, reject) => {
    const events = new EventSource(`${basePath}api/events?session=${session}`);
    events.addEventListener('ready', () => resolve());
    events.onmessage = ({ data }) => {
      const event = JSON.parse(data) as { event: string; payload?: unknown };
      for (const handler of handlers.get(event.event) ?? []) handler({ payload: event.payload });
    };
    events.onerror = () => {
      reject(new Error('无法连接 SAG 服务，请确认飞牛应用正在运行。'));
      for (const handler of handlers.get('app-error') ?? []) handler({ payload: '服务连接中断，正在重新连接。' });
    };
  });
  return ready;
}

export async function listen<T>(event: string, handler: (event: { payload: T }) => void) {
  const registered = handlers.get(event) ?? new Set<Handler>();
  registered.add(handler as Handler);
  handlers.set(event, registered);
  await connectEvents();
  return () => registered.delete(handler as Handler);
}

export async function api<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  switch (command) {
    case 'open_external_link': {
      const url = new URL(String(args.url));
      if (!['http:', 'https:', 'mailto:'].includes(url.protocol)) throw new Error('不支持的外部链接');
      if (sdk.isStandaloneWeb) { window.open(url.href, '_blank', 'noopener,noreferrer'); return undefined as T; }
      return await sdk.openURL(url.href, '_blank') as T;
    }
    case 'open_about_link': {
      const links: Record<string, string> = { project: 'https://github.com/Zleap-AI/SAG-Wiki', company: 'https://zleap.com', docling: 'https://docling.ai/', '302-api-keys': 'https://302.ai/api-keys/list' };
      if (!links[String(args.target)]) throw new Error('未知链接');
      return await api('open_external_link', { url: links[String(args.target)] }) as T;
    }
    case 'reveal_path':
      await api('authorize_path', args);
      if (sdk.isStandaloneWeb) throw new Error('请从飞牛桌面打开应用，以使用系统文件管理器。');
      return await sdk.openFileManager(String(args.path)) as T;
    case 'open_index_history_file':
      await api('authorize_path', { path: args.path });
      if (sdk.isStandaloneWeb) throw new Error('请从飞牛桌面打开应用，以使用系统文件打开方式。');
      return await sdk.openFile(String(args.path)) as T;
    case 'open_data_directory':
      if (sdk.isStandaloneWeb) {
        window.open(new URL('/', window.location.href).href, '_blank', 'noopener,noreferrer');
        return undefined as T;
      }
      return await sdk.openAppSetting() as T;
    case 'reveal_main_window':
      window.focus();
      return undefined as T;
    case 'quit_application':
      return await sdk.close() as T;
  }
  const response = await fetch(`${basePath}api/rpc`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'X-SAG-Request': '1' },
    body: JSON.stringify({ command, args, session }),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error ?? `请求失败（${response.status}）`);
  if (command === 'list_roots') {
    const paths = (result as {path:string}[]).map(root => root.path);
    if (!paths.length) pathLabels.set({});
    else {
      const converted = await api<{status:number;result:{path:string;semanticPath:string}[]}>('fnos_convert_paths', {paths,language:get(language)==='en'?'en-US':'zh-CN'});
      if (converted.status !== 0) throw new Error('飞牛路径转换失败');
      pathLabels.set(Object.fromEntries(converted.result.map(item => [item.path,item.semanticPath])));
    }
  }
  return result as T;
}
