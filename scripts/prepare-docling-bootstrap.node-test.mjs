import assert from 'node:assert/strict';
import test from 'node:test';
import {planForTarget, PYTHON_VERSION, UV_VERSION} from './prepare-docling-bootstrap.mjs';

test('desktop bootstrap pins every runtime archive by SHA-256', () => {
  for (const target of ['aarch64-apple-darwin', 'x86_64-pc-windows-msvc']) {
    const plan = planForTarget(target);
    assert.equal(plan.target, target);
    assert.equal(plan.artifacts.length, 2);
    for (const artifact of plan.artifacts) {
      assert.match(artifact.sha256, /^[0-9a-f]{64}$/);
      assert.match(artifact.url, new RegExp(target.replaceAll('-', '\\-')));
      if (artifact.name.startsWith('uv-')) assert.match(artifact.url, new RegExp(UV_VERSION.replaceAll('.', '\\.')));
      else assert.match(artifact.url, new RegExp(`${PYTHON_VERSION.replaceAll('.', '\\.')}%2B`));
    }
  }
});

test('unsupported desktop targets fail instead of producing an unpinned bundle', () => {
  assert.throws(() => planForTarget('x86_64-unknown-linux-gnu'), /No pinned Docling bootstrap/);
});
