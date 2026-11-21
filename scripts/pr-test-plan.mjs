import {execFileSync} from 'node:child_process';
import {appendFileSync,readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

const groups = ['client', 'cli', 'jev', 'http', 'web', 'qpdf', 'frontend'];
const docs = /^(?:docs(?:-site)?\/|(?:README(?:\.[^/]*)?|CHANGELOG|CONTRIBUTING|AGENTS|LICENSE)\.md$)/;
const bundledDocs = new Set(readFileSync(new URL('../core/assistant-pages.txt',import.meta.url),'utf8').trim().split(/\r?\n/));
const web = /^(?:src\/|static\/|public\/|vite\.config\.|index\.html$|svelte\.config\.|tsconfig[^/]*\.json$)/;

export function selectPrTests(paths) {
  const selected = new Set();
  const add = (...names) => names.forEach(name => selected.add(name));
  for (const path of paths) {
    const bundledPage = /^docs-site\/content\/docs\/([^/.]+)(?:\.[^/.]+)?\.mdx$/.exec(path);
    if (bundledPage && bundledDocs.has(bundledPage[1])) {
      add('cli', 'jev', 'http', 'web');
      continue;
    }
    if (docs.test(path)) continue;
    // These two files are also compiled into sag-core.
    if (/^src\/lib\/(?:model-capabilities|sag-tools)\.json$/.test(path)) {
      add(...groups);
    } else if (web.test(path)) {
      add('web', 'frontend');
    } else if (/^tests\/web-service-(?:e2e\.py|browser\.cjs)$/.test(path)) {
      add('web');
    } else if (path === 'tests/http-api-e2e.py') {
      add('http');
    } else if (path === 'tests/local-jev-e2e.py') {
      // Both other fixtures import helpers from this file.
      add('jev', 'http', 'web');
    } else if (/^tests\/[^/]+\.test\.[cm]?[jt]s$/.test(path)) {
      add('frontend');
    } else if (/^(?:scripts\/prepare-(?:qpdf|pdfium)\.mjs|scripts\/qpdf-packaging\.node-test\.mjs)$/.test(path)) {
      add('qpdf', 'cli', 'jev', 'http', 'web');
    } else if (/^(?:core|cli|protocol)\//.test(path)) {
      add('cli', 'jev', 'http', 'web');
      if (path.startsWith('protocol/')) add('client');
    } else if (path.startsWith('client/')) {
      add('client', 'cli', 'jev', 'http', 'web');
    } else {
      // Shared build files, CI and unclassified inputs take the full path.
      add(...groups);
    }
  }
  const result = Object.fromEntries(groups.map(name => [name, selected.has(name)]));
  result.native = ['client', 'cli', 'jev', 'http', 'web', 'qpdf'].some(name => result[name]);
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const paths = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', process.env.BASE_SHA, 'HEAD', '--'],
    {encoding: 'utf8', maxBuffer: 16 * 1024 * 1024}).split('\0').filter(Boolean);
  const plan = selectPrTests(paths);
  console.log('Selected PR suites:', Object.keys(plan).filter(name => plan[name] && name !== 'native').join(', ') || 'none');
  appendFileSync(process.env.GITHUB_OUTPUT, Object.entries(plan).map(([name, enabled]) => `${name}=${enabled}\n`).join(''));
}
