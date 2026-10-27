// Apple services are exercised by the macOS release job, never simulated as acceptance here.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join} from 'node:path';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {targets, validateVersions} from './release.mjs';

const moduleUrl = new URL('./sign-macos-release.mjs', import.meta.url);
const implementation = () => import(moduleUrl.href);
const team = 'ABCDE12345';
const uuid = '12345678-1234-1234-1234-123456789abc';
const environment = {APPLE_CERTIFICATE_BASE64: Buffer.from('private p12 fixture').toString('base64'), APPLE_CERTIFICATE_PASSWORD: 'p12-secret', APPLE_ID: 'release@example.test', APPLE_APP_SPECIFIC_PASSWORD: 'apple-secret', APPLE_TEAM_ID: team};
const details = `Executable=/SAG Wiki.app\nCodeDirectory v=20500 size=123 flags=0x10000(runtime) hashes=3+7 location=embedded\nAuthority=Developer ID Application: Example (${team})\nAuthority=Developer ID Certification Authority\nAuthority=Apple Root CA\nTimestamp=Oct 27, 2026 at 10:00:00 AM\nTeamIdentifier=${team}\n`;
const digest = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const python = [process.env.SAG_TEST_PYTHON, process.platform === 'win32' ? 'python' : 'python3', 'python3.12', 'python3.13', 'python3.14']
  .filter(Boolean).find(command => spawnSync(command, ['-c', 'import tarfile; assert hasattr(tarfile, "data_filter")'], {stdio: 'ignore'}).status === 0);
assert.ok(python, 'Signing tests require Python 3.12+');
const portable = path => path.replaceAll('\\', '/');
function execute(command, args, options = {}) {
  const executable = command === 'python3' ? python : command === 'tar' && process.platform === 'win32' ? join(process.env.SystemRoot, 'System32', 'tar.exe') : command;
  const result = spawnSync(executable, args, {encoding: 'utf8', ...options, env: {...process.env, ...options.env, COPYFILE_DISABLE: '1'}});
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  return {stdout: result.stdout ?? '', stderr: result.stderr ?? ''};
}

test('requires every Apple credential without disclosing any secret value', async () => {
  const {validateAppleCredentials} = await implementation();
  for (const name of Object.keys(environment)) {
    assert.throws(() => validateAppleCredentials({...environment, [name]: ''}), error => error.message.includes(name) && !error.message.includes('p12-secret') && !error.message.includes('apple-secret'));
  }
  assert.equal(validateAppleCredentials(environment).APPLE_TEAM_ID, team);
  assert.throws(() => validateAppleCredentials({...environment, APPLE_TEAM_ID: 'wrong'}), /team/i);
});

test('selects exactly one valid Developer ID Application identity belonging to the expected team', async () => {
  const {selectSigningIdentity} = await implementation();
  const valid = `  1) ${'A'.repeat(40)} "Developer ID Application: Example (${team})"\n     1 valid identities found\n`;
  assert.equal(selectSigningIdentity(valid, team), 'A'.repeat(40));
  assert.throws(() => selectSigningIdentity(valid.replace(team, 'OTHER12345'), team), /exactly one/i);
  assert.throws(() => selectSigningIdentity(valid.replace('Developer ID Application:', 'Apple Development:'), team), /exactly one/i);
  assert.throws(() => selectSigningIdentity(valid + valid.replaceAll('A'.repeat(40), 'B'.repeat(40)), team), /exactly one/i);
});

test('rejects ad hoc, wrong-team and Developer ID signatures missing hardened runtime or trusted timestamp', async () => {
  const {validateCodeSignature} = await implementation();
  assert.doesNotThrow(() => validateCodeSignature(details, team));
  for (const invalid of [details.replace('flags=0x10000(runtime)', 'flags=0x2(adhoc)'), details.replaceAll(team, 'OTHER12345'), details.replace('runtime', ''), details.replace(/^Timestamp=.*\n/m, ''), details.replace(/^Timestamp=.*$/m, 'Timestamp=none'), details.replace(/^Authority=Developer ID Application:.*\n/m, '')]) {
    assert.throws(() => validateCodeSignature(invalid, team));
  }
});

test('requires an Accepted notarization result with a UUID submission ID', async () => {
  const {validateNotarization} = await implementation();
  assert.equal(validateNotarization(JSON.stringify({id: uuid, status: 'Accepted'})), uuid);
  for (const result of [{id: uuid, status: 'Invalid'}, {id: uuid, status: 'In Progress'}, {id: 'bad', status: 'Accepted'}]) {
    assert.throws(() => validateNotarization(JSON.stringify(result)), /notari/i);
  }
});

test('extracts an app with safe framework links and rejects traversal, escaping links and special files', async () => {
  const {extractAppArchive} = await implementation();
  const directory = mkdtempSync(join(tmpdir(), 'sag-safe-app-'));
  try {
    const archive = join(directory, 'app.tar.gz');
    for (const [name, link, type] of [['SAG Wiki.app/Contents/data', '', 'file'], ['../escaped', '', 'file'], ['SAG Wiki.app/Contents/link', '../../../escaped', 'symlink'], ['SAG Wiki.app/Contents/pipe', '', 'fifo']]) {
      execute('python3', ['-c', 'import io,sys,tarfile\np,n,l,k=sys.argv[1:]\nwith tarfile.open(p,"w:gz") as t:\n i=tarfile.TarInfo(n); i.type={"file":tarfile.REGTYPE,"symlink":tarfile.SYMTYPE,"fifo":tarfile.FIFOTYPE}[k]; i.linkname=l; i.size=1 if k=="file" else 0; t.addfile(i,io.BytesIO(b"x") if k=="file" else None)', archive, name, link, type]);
      const destination = join(directory, 'extracted');
      mkdirSync(destination);
      if (name.endsWith('/data')) {
        extractAppArchive(archive, destination, execute);
        assert.equal(readFileSync(join(destination, name), 'utf8'), 'x');
      } else assert.throws(() => extractAppArchive(archive, destination, execute));
      assert.equal(existsSync(join(directory, 'escaped')), false);
      rmSync(destination, {recursive: true});
    }
    execute('python3', ['-c', 'import io,sys,tarfile\nwith tarfile.open(sys.argv[1],"w:gz") as t:\n i=tarfile.TarInfo("SAG Wiki.app/Contents/Frameworks/F.framework/Versions/A/F"); i.size=1; t.addfile(i,io.BytesIO(b"x")); i=tarfile.TarInfo("SAG Wiki.app/Contents/Frameworks/F.framework/Versions/Current"); i.type=tarfile.SYMTYPE; i.linkname="A"; t.addfile(i)', archive]);
    const destination = join(directory, 'framework');
    mkdirSync(destination);
    extractAppArchive(archive, destination, execute);
    assert.equal(readFileSync(join(destination, 'SAG Wiki.app/Contents/Frameworks/F.framework/Versions/Current/F'), 'utf8'), 'x');
  } finally { rmSync(directory, {recursive: true, force: true}); }
});

function releaseFixture() {
  const directory = mkdtempSync(join(tmpdir(), 'sag-finalizer-test-'));
  const input = join(directory, 'input');
  const output = join(directory, 'output');
  const app = join(directory, 'payload/SAG Wiki.app');
  const version = validateVersions();
  const commit = 'a'.repeat(40);
  for (const name of ['Contents/MacOS/sag-desktop', 'Contents/Resources/bin/sag']) {
    const file = join(app, name);
    mkdirSync(dirname(file), {recursive: true});
    writeFileSync(file, Buffer.concat([Buffer.from('cffaedfe', 'hex'), Buffer.from('fixture')]));
  }
  for (const platform of Object.keys(targets)) {
    const destination = join(input, platform);
    mkdirSync(destination, {recursive: true});
    const names = platform === 'macos' ? [`SAG-Wiki-${version}-macOS-arm64.dmg`, `SAG-Wiki-${version}-macOS-arm64.app.tar.gz`] : [`SAG-Wiki-${version}-Windows-x64-setup.exe`];
    for (const name of names) {
      if (name.endsWith('.tar.gz')) execute('tar', ['-czf', join(destination, name), '-C', dirname(app), 'SAG Wiki.app']);
      else writeFileSync(join(destination, name), 'unsigned artifact');
    }
    writeFileSync(join(destination, 'metadata.json'), JSON.stringify({schema: 1, version, commit, platform, target: targets[platform], build_identity: {version, channel: 'release'}, files: Object.fromEntries(names.map(name => [name, digest(join(destination, name))]))}));
  }
  return {directory, input, output, version, commit};
}

function appleRunner(fixture, {rejectNotarization = false, rejectDmgNotarization = false, failAcceptance = false, failMountedAcceptance = false, corruptMountedApp = false, loseArchiveTicket = false, failUnlock = false, hostArchitecture = 'arm64'} = {}) {
  const state = {restored: false, deleted: false, detached: false, cliSigned: false, submissions: 0, gates: []};
  let imageApp;
  const run = (command, args, options = {}) => {
    if (command === 'uname') return {stdout: hostArchitecture, stderr: ''};
    if (command === 'git') return {stdout: fixture.commit, stderr: ''};
    if (command === 'security') {
      if (args[0] === 'list-keychains' && !args.includes('-s')) return {stdout: '    "/existing/login.keychain-db"\n', stderr: ''};
      if (args[0] === 'find-identity') return {stdout: `  1) ${'A'.repeat(40)} "Developer ID Application: Example (${team})"\n 1 valid identities found`, stderr: ''};
      if (args[0] === 'list-keychains' && args.at(-1) === '/existing/login.keychain-db' && args.length === 5) state.restored = true;
      if (args[0] === 'delete-keychain') state.deleted = true;
      if (args[0] === 'unlock-keychain' && failUnlock) throw new Error('keychain unlock failed');
    } else if (command === 'codesign') {
      const target = args.at(-1);
      if (args.includes('--verify') && target.endsWith('.app')) assert.equal(args.includes('--deep'), true, 'enclosing app verification includes nested code');
      if (args.includes('--sign')) {
        if (portable(target).endsWith('/Contents/Resources/bin/sag')) state.cliSigned = true;
        if (target.endsWith('.app')) {
          assert.equal(state.cliSigned, true, 'resource CLI must be signed before its enclosing app');
          mkdirSync(join(target, 'Contents/_CodeSignature'), {recursive: true});
          writeFileSync(join(target, 'Contents/_CodeSignature/CodeResources'), 'real finalizer boundary fixture');
        }
        if (target.endsWith('.dmg')) assert.equal(args.includes('--options'), false, 'DMG has no Mach-O runtime flags');
      }
      if (args.includes('--display')) return {stdout: '', stderr: details};
    } else if (command === 'xcrun' && args[0] === 'notarytool') {
      state.submissions++;
      return {stdout: JSON.stringify({id: uuid, status: rejectNotarization || (rejectDmgNotarization && state.submissions === 2) ? 'Invalid' : 'Accepted'}), stderr: ''};
    } else if (command === 'xcrun' && args[0] === 'stapler' && args.at(-1).endsWith('.app')) {
      const ticket = join(args.at(-1), 'Contents/stapled-ticket');
      if (args[1] === 'staple') writeFileSync(ticket, 'Apple ticket boundary fixture');
      else assert.equal(existsSync(ticket), true, 'archive extraction must retain the stapled ticket');
    } else if (command === 'ditto') {
      if (args.includes('-c')) writeFileSync(args.at(-1), 'zip fixture');
      else cpSync(args.at(-2), args.at(-1), {recursive: true});
    } else if (command === 'hdiutil') {
      if (args[0] === 'create') {
        imageApp = join(args[args.indexOf('-srcfolder') + 1], 'SAG Wiki.app');
        writeFileSync(args.at(-1), 'Developer ID signed, stapled fixture DMG');
      }
      if (args[0] === 'attach') {
        const mount = args[args.indexOf('-mountpoint') + 1];
        cpSync(imageApp, join(mount, 'SAG Wiki.app'), {recursive: true});
        if (corruptMountedApp) writeFileSync(join(mount, 'SAG Wiki.app/Contents/_CodeSignature/CodeResources'), 'tampered DMG app');
      }
      if (args[0] === 'detach') state.detached = true;
    } else if (command === 'python3' && !args.includes('-c')) {
      state.gates.push({args, environment: options.env});
      if (failAcceptance || (failMountedAcceptance && state.gates.length >= 3)) throw new Error('packaged CLI regression failed');
    } else if (portable(command).endsWith('/Contents/Resources/bin/sag')) {
      return {stdout: JSON.stringify({version: fixture.version, channel: 'release'}), stderr: ''};
    } else if (command === 'python3' || command === 'tar') {
      const result = execute(command, args, options);
      if (loseArchiveTicket && command === 'tar' && args[0] === '-xzf') rmSync(join(args.at(-1), 'SAG Wiki.app/Contents/stapled-ticket'));
      return result;
    }
    return {stdout: '', stderr: ''};
  };
  return {run, state};
}

test('publishes only final hashes and provenance after both notarizations and mounted-app gates', async () => {
  const {signMacosRelease} = await implementation();
  const fixture = releaseFixture();
  const {run, state} = appleRunner(fixture);
  try {
    signMacosRelease(fixture, {run, environment, platform: 'darwin', arch: 'arm64'});
    const metadata = JSON.parse(readFileSync(join(fixture.output, 'macos/metadata.json')));
    assert.deepEqual(metadata.source_files, JSON.parse(readFileSync(join(fixture.input, 'macos/metadata.json'))).files);
    assert.deepEqual(metadata.apple_signing, {team_id: team, app_notarization_id: uuid, dmg_notarization_id: uuid});
    for (const [name, hash] of Object.entries(metadata.files)) {
      assert.equal(hash, digest(join(fixture.output, 'macos', name)));
      assert.notEqual(hash, metadata.source_files[name]);
    }
    assert.deepEqual(readdirSync(fixture.output), ['macos']);
    assert.equal(state.submissions, 2);
    assert.equal(state.gates.length, 4);
    assert.match(portable(state.gates[0].environment.SAG_DESKTOP_BUNDLE), /updater-extracted\/SAG Wiki\.app$/);
    assert.match(portable(state.gates[1].args.at(-1)), /updater-extracted\/SAG Wiki\.app\/Contents\/Resources\/bin\/sag$/);
    assert.match(portable(state.gates[2].environment.SAG_DESKTOP_BUNDLE), /mounted\/SAG Wiki\.app$/);
    assert.match(portable(state.gates[3].args.at(-1)), /mounted\/SAG Wiki\.app\/Contents\/Resources\/bin\/sag$/);
    assert.equal(state.restored && state.deleted && state.detached, true);
    assert.deepEqual(readdirSync(fixture.directory).sort(), ['input', 'output', 'payload']);
  } finally { rmSync(fixture.directory, {recursive: true, force: true}); }
});

const failures = {rejectNotarization: /not Accepted/, rejectDmgNotarization: /not Accepted/, failAcceptance: /regression failed/, failMountedAcceptance: /regression failed/, corruptMountedApp: /DMG app differs/, loseArchiveTicket: /Updater archive differs/, failUnlock: /unlock failed/};
for (const [failure, expectedError] of Object.entries(failures)) {
  test(`${failure} leaves no published artifacts and cleans up private signing state`, async () => {
    const {signMacosRelease} = await implementation();
    const fixture = releaseFixture();
    const {run, state} = appleRunner(fixture, {[failure]: true});
    try {
      assert.throws(() => signMacosRelease(fixture, {run, environment, platform: 'darwin', arch: 'arm64'}), expectedError);
      assert.equal(existsSync(fixture.output), false);
      assert.equal(state.restored && state.deleted, true);
      if (failure === 'corruptMountedApp' || failure === 'failMountedAcceptance') assert.equal(state.detached, true);
      assert.deepEqual(readdirSync(fixture.directory).sort(), ['input', 'payload']);
    } finally { rmSync(fixture.directory, {recursive: true, force: true}); }
  });
}

test('refuses non-Apple-Silicon hosts and missing credentials before starting commands', async () => {
  const {signMacosRelease} = await implementation();
  const run = () => assert.fail('must not reach an external command');
  assert.throws(() => signMacosRelease({}, {run, environment: {}, platform: 'darwin', arch: 'arm64'}), /APPLE_/);
  assert.throws(() => signMacosRelease({}, {run, environment, platform: 'linux', arch: 'arm64'}), /macOS/);
  assert.throws(() => signMacosRelease({}, {run, environment, platform: 'darwin', arch: 'x64'}), /arm64/);
});

test('refuses an empty host architecture instead of falling back to the Node.js architecture', async () => {
  const {signMacosRelease} = await implementation();
  const fixture = releaseFixture();
  const {run} = appleRunner(fixture, {hostArchitecture: ''});
  try {
    assert.throws(() => signMacosRelease(fixture, {run, environment, platform: 'darwin', arch: 'arm64'}), /arm64 host/);
    assert.equal(existsSync(fixture.output), false);
  } finally { rmSync(fixture.directory, {recursive: true, force: true}); }
});
