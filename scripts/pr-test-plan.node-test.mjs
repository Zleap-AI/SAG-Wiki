import assert from 'node:assert/strict';
import {test} from 'node:test';
import {selectPrTests} from './pr-test-plan.mjs';

function suites(...paths) {
  return Object.entries(selectPrTests(paths)).filter(([name, enabled]) =>
    enabled && name !== 'native').map(([name]) => name);
}

test('documentation-only changes need no application fixtures', () => {
  assert.deepEqual(suites('README.md', 'docs-site/content/docs/architecture.mdx'), []);
  assert.deepEqual(suites('docs-site/content/docs/faq.mdx'), ['cli', 'jev', 'http', 'web']);
});

test('frontend, embedded Rust input, and shared fixture changes are distinguished', () => {
  assert.deepEqual(suites('src/components/Search.svelte'), ['web', 'frontend']);
  assert.deepEqual(suites('src/lib/model-capabilities.json'), Object.keys(selectPrTests(['Cargo.lock'])).filter(name => name !== 'native'));
  assert.deepEqual(suites('tests/local-jev-e2e.py'), ['jev', 'http', 'web']);
  assert.deepEqual(suites('tests/http-api-e2e.py'), ['http']);
});

test('rust changes reach dependent integrations and mixed changes select their union', () => {
  assert.deepEqual(suites('core/src/lib.rs'), ['cli', 'jev', 'http', 'web']);
  assert.deepEqual(suites('client/src/lib.rs', 'src/App.svelte'), ['client', 'cli', 'jev', 'http', 'web', 'frontend']);
  assert.deepEqual(suites('protocol/src/lib.rs'), ['client', 'cli', 'jev', 'http', 'web']);
});
