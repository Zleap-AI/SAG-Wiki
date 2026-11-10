#!/usr/bin/env node
// Stage the fixed parser bootstrap used by desktop installs. Keeping uv and
// Python inside the signed application removes two fragile GitHub/Astral
// downloads from the first Docling installation. Dependencies and models are
// still downloaded on demand, so the application stays lightweight.
import {createHash} from 'node:crypto';
import {mkdir, readFile, readdir, rename, rm, writeFile} from 'node:fs/promises';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

export const UV_VERSION = '0.12.1';
export const PYTHON_VERSION = '3.12.13';
const PYTHON_RELEASE = '20260728';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const plans = {
  'aarch64-apple-darwin': {
    platform: 'macos-aarch64',
    artifacts: [
      {
        name: 'uv-aarch64-apple-darwin.tar.gz',
        url: `https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-aarch64-apple-darwin.tar.gz`,
        sha256: '77d2906988e8074fd43f2f329ec452ebbf9b0c257ba1c66451c71de70a6baf42',
      },
      {
        name: `cpython-${PYTHON_VERSION}+${PYTHON_RELEASE}-aarch64-apple-darwin-install_only_stripped.tar.gz`,
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_RELEASE}/cpython-${PYTHON_VERSION}%2B${PYTHON_RELEASE}-aarch64-apple-darwin-install_only_stripped.tar.gz`,
        sha256: '2f18cdef4125ca1440dd1ba00ebcb267526efb532138c0860438f755ea4eebac',
      },
    ],
  },
  'x86_64-pc-windows-msvc': {
    platform: 'windows-x86_64',
    artifacts: [
      {
        name: 'uv-x86_64-pc-windows-msvc.zip',
        url: `https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-pc-windows-msvc.zip`,
        sha256: '8fcb0cb46e1229065e344758980924e569bef5882ef45f46fada8fb24e06b74a',
      },
      {
        name: `cpython-${PYTHON_VERSION}+${PYTHON_RELEASE}-x86_64-pc-windows-msvc-install_only_stripped.tar.gz`,
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_RELEASE}/cpython-${PYTHON_VERSION}%2B${PYTHON_RELEASE}-x86_64-pc-windows-msvc-install_only_stripped.tar.gz`,
        sha256: '242b94b37682ac55f9bf9eb624348dc8d17c64f74f56028104545ea3ffe35e26',
      },
    ],
  },
};

export function planForTarget(target) {
  const plan = plans[target];
  if (!plan) throw new Error(`No pinned Docling bootstrap for target ${target}`);
  return {target, ...plan};
}

function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function download(artifact, destination) {
  try {
    const existing = await readFile(destination);
    if (digest(existing) === artifact.sha256) return 'cached';
  } catch {}
  const response = await fetch(artifact.url, {redirect: 'follow'});
  if (!response.ok) throw new Error(`Download failed (${response.status}) for ${artifact.url}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (digest(bytes) !== artifact.sha256) {
    throw new Error(`SHA-256 mismatch for ${artifact.name}`);
  }
  const temporary = `${destination}.download`;
  await writeFile(temporary, bytes);
  await rename(temporary, destination);
  return 'downloaded';
}

export async function prepare({target = process.env.TARGET_TRIPLE, output = join(ROOT, 'src-tauri/bin/docling-bootstrap')} = {}) {
  if (!target) {
    const arch = process.arch === 'arm64' ? 'aarch64' : process.arch === 'x64' ? 'x86_64' : process.arch;
    target = `${arch}-${process.platform === 'darwin' ? 'apple-darwin' : process.platform === 'win32' ? 'pc-windows-msvc' : process.platform}`;
  }
  const plan = planForTarget(target);
  await mkdir(output, {recursive: true});
  const artifacts = [];
  for (const artifact of plan.artifacts) {
    const destination = join(output, artifact.name);
    const status = await download(artifact, destination);
    artifacts.push({...artifact, status});
  }
  const manifest = {
    schema: 1,
    uv_version: UV_VERSION,
    python_version: PYTHON_VERSION,
    target,
    platform: plan.platform,
    artifacts: artifacts.map(({name, sha256}) => ({name, sha256})),
  };
  await writeFile(join(output, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  const expected = new Set([...plan.artifacts.map(artifact => artifact.name), 'manifest.json']);
  for (const name of await readdir(output)) {
    if (!expected.has(name)) await rm(join(output, name), {recursive: true, force: true});
  }
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const manifest = await prepare();
  console.log(`Prepared Docling bootstrap ${manifest.platform} in src-tauri/bin/docling-bootstrap`);
}
