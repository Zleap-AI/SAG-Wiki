import {spawnSync} from 'node:child_process';
import {copyFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

// Cargo resolves an explicit target directory relative to the invocation cwd.
// Keep the existing standalone CLI location when no shared directory is set.
export function resolveCliTargetDir(root = process.cwd(), env = process.env) {
  return resolve(root, env.CARGO_TARGET_DIR || 'cli/target');
}

export function cliBuildPlan({debug = false, env = process.env, platform = process.platform, cwd = process.cwd()} = {}) {
  const target = debug ? undefined : env.TAURI_ENV_TARGET_TRIPLE;
  const windows = target ? target.includes('windows') : platform === 'win32';
  const name = windows ? 'sag.exe' : 'sag';
  const destination = resolve(cwd, debug ? 'src-tauri/bin/development' : 'src-tauri/bin');
  return {
    debug, target, windows, name, destination,
    source: join(resolveCliTargetDir(cwd, env), ...(target ? [target] : []), debug ? 'debug' : 'release', name),
    args: ['build', '--locked', ...(debug ? [] : ['--release']), '--manifest-path', 'cli/Cargo.toml', ...(target ? ['--target', target] : [])],
  };
}

export function buildCli({debug = false, env = process.env, platform = process.platform, cwd = process.cwd(), run = spawnSync} = {}) {
  const plan = cliBuildPlan({debug, env, platform, cwd});
  const build = run('cargo', plan.args, {stdio: 'inherit', cwd, env});
  if (build.status !== 0) return build.status ?? 1;
  mkdirSync(plan.destination, {recursive: true});
  const binary = join(plan.destination, plan.name);
  copyFileSync(plan.source, binary);
  if (platform === 'darwin' && !plan.windows) {
    const sign = run('codesign', ['--force', '--sign', env.APPLE_SIGNING_IDENTITY || '-', binary], {stdio: 'inherit', cwd, env});
    if (sign.status !== 0) return sign.status ?? 1;
  }
  if (!plan.target) {
    const identity = run(binary, ['--build-info'], {encoding: 'utf8', cwd, env});
    if (identity.status !== 0) throw new Error('Unable to verify SAG build identity');
    const metadata = JSON.parse(identity.stdout);
    if (metadata.channel !== (debug ? 'development' : 'release')) throw new Error('SAG artifact channel does not match the build');
    writeFileSync(join(plan.destination, 'build-identity.json'), JSON.stringify(metadata, null, 2) + '\n');
  }
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = buildCli({debug: process.argv.includes('--debug')});
}
