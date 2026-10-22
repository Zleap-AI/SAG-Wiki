import assert from 'node:assert/strict';
import {createHash, createPublicKey, verify} from 'node:crypto';

function decode(value) {
  assert.equal(typeof value, 'string', 'Expected a base64 signature or public key');
  const text = value.trim();
  assert.match(text, /^[A-Za-z0-9+/]+={0,2}$/, 'Invalid base64');
  const bytes = Buffer.from(text, 'base64');
  assert.equal(bytes.toString('base64'), text, 'Noncanonical base64');
  return bytes;
}

// Minisign ED uses BLAKE2b-512 prehashing. Its second Ed25519 signature
// authenticates the trusted comment, including Tauri's signed app version.
// This follows minisign-verify, used by tauri-plugin-updater, and is tested
// against the same signed package exercised by the native updater tests.
export function verifyUpdate(payload, signature, publicKey, expectedVersion) {
  const publicLines = decode(publicKey).toString('utf8').trim().split('\n');
  assert.equal(publicLines.length, 2, 'Invalid minisign public key');
  assert.ok(publicLines[0].startsWith('untrusted comment: '), 'Invalid public key comment');
  const record = decode(publicLines[1]);
  assert.equal(record.length, 42, 'Invalid public key length');
  assert.equal(record.subarray(0, 2).toString(), 'Ed', 'Unsupported public key algorithm');
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), record.subarray(10)]),
    type: 'spki', format: 'der',
  });

  const lines = decode(signature).toString('utf8').trim().split('\n');
  assert.equal(lines.length, 4, 'Invalid minisign signature');
  assert.ok(lines[0].startsWith('untrusted comment: '), 'Invalid signature comment');
  assert.ok(lines[2].startsWith('trusted comment: '), 'Missing trusted signature comment');
  const signed = decode(lines[1]);
  assert.equal(signed.length, 74, 'Invalid signature length');
  assert.deepEqual(signed.subarray(2, 10), record.subarray(2, 10), 'Signature key does not match');
  const algorithm = signed.subarray(0, 2).toString();
  assert.ok(algorithm === 'ED' || algorithm === 'Ed', 'Unsupported signature algorithm');
  const message = algorithm === 'ED' ? createHash('blake2b512').update(payload).digest() : payload;
  const detached = signed.subarray(10);
  assert.ok(verify(null, message, key, detached), 'Package signature verification failed');
  const comment = lines[2].slice('trusted comment: '.length);
  const global = decode(lines[3]);
  assert.equal(global.length, 64, 'Invalid global signature length');
  assert.ok(verify(null, Buffer.concat([detached, Buffer.from(comment)]), key, global), 'Trusted comment signature verification failed');
  const versions = comment.split('\t').filter(part => part.startsWith('version:'));
  assert.equal(versions.length, 1, 'Missing or ambiguous signed version');
  const version = versions[0].slice('version:'.length);
  assert.equal(version, expectedVersion, 'Signed version does not match release version');
  return version;
}
