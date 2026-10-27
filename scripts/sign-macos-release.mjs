import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createHash, randomBytes} from 'node:crypto';
import {chmodSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, readdirSync, renameSync, rmSync, symlinkSync, writeFileSync} from 'node:fs';
import {basename, dirname, join, relative, resolve} from 'node:path';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
import {validateStagedArtifacts, validateVersions} from './release.mjs';

const root = resolve(import.meta.dirname, '..');
const appName = 'SAG Wiki.app';
const credentialNames = ['APPLE_CERTIFICATE_BASE64', 'APPLE_CERTIFICATE_PASSWORD', 'APPLE_ID', 'APPLE_APP_SPECIFIC_PASSWORD', 'APPLE_TEAM_ID'];
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const external = (command, args, options = {}) => {
  const result = spawnSync(command, args, {cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, ...options});
  // Arguments and command output may contain credentials. Never include them in errors.
  assert.equal(result.status, 0, `${basename(command)} failed (exit ${result.status ?? 'unavailable'})`);
  return {stdout: result.stdout ?? '', stderr: result.stderr ?? ''};
};

export function validateAppleCredentials(environment) {
  for (const name of credentialNames) assert.ok(typeof environment[name] === 'string' && environment[name].trim(), `Missing Apple release credential: ${name}`);
  assert.match(environment.APPLE_TEAM_ID, /^[A-Z0-9]{10}$/, 'Invalid Apple team ID');
  const certificate = environment.APPLE_CERTIFICATE_BASE64.replace(/\s/g, '');
  assert.ok(certificate.length > 0 && Buffer.from(certificate, 'base64').toString('base64') === certificate, 'Invalid APPLE_CERTIFICATE_BASE64');
  return Object.fromEntries(credentialNames.map(name => [name, environment[name]]));
}

export function selectSigningIdentity(output, team) {
  const identities = [...output.matchAll(/^\s*\d+\) ([A-Fa-f0-9]{40}) "Developer ID Application: [^"\n]+ \(([A-Z0-9]{10})\)"\s*$/gm)]
    .filter(match => match[2] === team);
  assert.equal(identities.length, 1, 'Expected exactly one valid Developer ID Application identity for the Apple team');
  return identities[0][1];
}

export function validateCodeSignature(details, team, {runtime = true} = {}) {
  assert.equal(details.match(/^TeamIdentifier=(.+)$/m)?.[1]?.trim(), team, 'Code signature Apple team mismatch');
  assert.match(details, new RegExp(`^Authority=Developer ID Application: .+ \\(${team}\\)$`, 'm'), 'Developer ID Application authority is required');
  assert.match(details, /^Authority=Developer ID Certification Authority$/m, 'Developer ID certificate chain is required');
  assert.match(details, /^Authority=Apple Root CA$/m, 'Apple root authority is required');
  assert.doesNotMatch(details, /(?:Signature=adhoc|flags=[^\n]*\badhoc\b)/i, 'Ad hoc signatures cannot be released');
  assert.match(details, /^Timestamp=\S[^\n]*$/m, 'Trusted timestamp is required');
  assert.doesNotMatch(details, /^Timestamp=(?:none|not set|0)\s*$/mi, 'Trusted timestamp is required');
  if (runtime) assert.match(details, /flags=0x[\da-f]+\([^\n)]*\bruntime\b[^\n)]*\)/i, 'Hardened runtime is required');
}

export function validateNotarization(output) {
  const result = JSON.parse(output);
  assert.equal(result.status, 'Accepted', 'Apple notarization was not Accepted');
  assert.match(result.id ?? '', /^[\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12}$/i, 'Invalid Apple notarization submission ID');
  return result.id;
}

const extractPython = `import posixpath, sys, tarfile
from pathlib import PurePosixPath
archive, destination = sys.argv[1:]
assert hasattr(tarfile, 'data_filter'), 'Python 3.12+ with tarfile data filter is required'
def inside(name):
    path = PurePosixPath(name)
    return not path.is_absolute() and bool(path.parts) and path.parts[0] == 'SAG Wiki.app' and '..' not in path.parts and chr(92) not in name
with tarfile.open(archive, 'r:gz') as source:
    members = source.getmembers()
    assert members, 'Empty app archive'
    names = set()
    for member in members:
        assert inside(member.name), 'Archive path escapes SAG Wiki.app'
        normalized = str(PurePosixPath(member.name))
        assert normalized not in names, 'Duplicate archive path'
        names.add(normalized)
        assert member.isfile() or member.isdir() or member.issym() or member.islnk(), 'Special archive entry is forbidden'
        if member.issym() or member.islnk():
            assert not PurePosixPath(member.linkname).is_absolute() and chr(92) not in member.linkname, 'Absolute archive link is forbidden'
            target = posixpath.normpath(posixpath.join(posixpath.dirname(member.name), member.linkname) if member.issym() else member.linkname)
            assert inside(target), 'Archive link escapes SAG Wiki.app'
    source.extractall(destination, members=members, filter='data')
`;

export function extractAppArchive(archive, destination, run = external) {
  run('python3', ['-I', '-c', extractPython, archive, destination]);
}

function walk(directory) {
  return readdirSync(directory).sort().flatMap(name => {
    const path = join(directory, name);
    return lstatSync(path).isDirectory() ? [path, ...walk(path)] : [path];
  });
}

function appContents(app) {
  return Object.fromEntries(walk(app).map(path => {
    const stat = lstatSync(path);
    return [relative(app, path), stat.isSymbolicLink() ? {link: readlinkSync(path)} : stat.isDirectory() ? {directory: true} : {sha256: digest(path), mode: stat.mode & 0o777}];
  }));
}

const isMachO = path => lstatSync(path).isFile() && ['feedface', 'cefaedfe', 'feedfacf', 'cffaedfe', 'cafebabe', 'bebafeca', 'cafebabf', 'bfbafeca'].includes(readFileSync(path).subarray(0, 4).toString('hex'));

export function signMacosRelease({input = 'release-artifacts', output = 'signed-release-artifacts'} = {}, {run: commandRunner = external, environment = process.env, platform = process.platform, arch = process.arch} = {}) {
  const credentials = validateAppleCredentials(environment);
  assert.equal(platform, 'darwin', 'Apple release signing requires macOS');
  assert.equal(arch, 'arm64', 'Apple release signing requires native arm64 Node.js');
  const childEnvironment = {...process.env};
  for (const name of credentialNames) delete childEnvironment[name];
  const run = (command, args, options = {}) => commandRunner(command, args, {env: childEnvironment, ...options});
  assert.equal(run('uname', ['-m']).stdout.trim(), 'arm64', 'Apple release signing requires an arm64 host');
  const version = validateVersions();
  const commit = run('git', ['rev-parse', 'HEAD']).stdout.trim();
  if (environment.GITHUB_SHA) assert.equal(commit, environment.GITHUB_SHA, 'Checkout must match the triggering commit');
  const source = resolve(input);
  const destination = resolve(output);
  const original = validateStagedArtifacts(source, version, commit).macos;
  assert.ok(!existsSync(destination), 'Signed release output must be fresh');
  assert.ok(destination !== source && !destination.startsWith(source + '/'), 'Signed output must be outside the source staging directory');
  mkdirSync(dirname(destination), {recursive: true});
  const temporary = mkdtempSync(join(dirname(destination), '.sag-apple-signing-'));
  chmodSync(temporary, 0o700);
  const prepared = join(temporary, 'prepared');
  const signed = join(prepared, 'macos');
  const extraction = join(temporary, 'extracted');
  const app = join(extraction, appName);
  const cli = join(app, 'Contents/Resources/bin/sag');
  const keychain = join(temporary, 'release.keychain-db');
  const password = randomBytes(32).toString('hex');
  let keychainCreated = false;
  let searchListChanged = false;
  let originalSearchList = [];
  const filenames = Object.keys(original.files);
  const dmgName = filenames.find(name => name.endsWith('.dmg'));
  const archiveName = filenames.find(name => name.endsWith('.app.tar.gz'));
  try {
    mkdirSync(signed, {recursive: true});
    mkdirSync(extraction);
    extractAppArchive(join(source, 'macos', archiveName), extraction, run);
    assert.ok(isMachO(cli), 'Bundled resource CLI must be a regular Mach-O executable');
    assert.ok(isMachO(join(app, 'Contents/MacOS/sag-desktop')), 'Desktop must be a regular Mach-O executable');
    try {
      originalSearchList = run('security', ['list-keychains', '-d', 'user']).stdout.trim().split('\n').filter(Boolean).map(line => JSON.parse(line.trim()));
      const certificate = join(temporary, 'certificate.p12');
      writeFileSync(certificate, Buffer.from(credentials.APPLE_CERTIFICATE_BASE64, 'base64'), {mode: 0o600});
      searchListChanged = true;
      keychainCreated = true;
      run('security', ['create-keychain', '-p', password, keychain]);
      run('security', ['set-keychain-settings', '-lut', '21600', keychain]);
      run('security', ['unlock-keychain', '-p', password, keychain]);
      run('security', ['list-keychains', '-d', 'user', '-s', keychain, ...originalSearchList]);
      run('security', ['import', certificate, '-k', keychain, '-P', credentials.APPLE_CERTIFICATE_PASSWORD, '-T', '/usr/bin/codesign', '-T', '/usr/bin/security']);
      run('security', ['set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', password, keychain]);
      const identity = selectSigningIdentity(run('security', ['find-identity', '-v', '-p', 'codesigning', keychain]).stdout, credentials.APPLE_TEAM_ID);
      const sign = (path, {runtime = true} = {}) => run('codesign', ['--force', '--sign', identity, '--keychain', keychain, ...(runtime ? ['--options', 'runtime'] : []), '--timestamp', path]);
      const verify = (path, options) => {
        run('codesign', ['--verify', ...(path.endsWith('.app') ? ['--deep'] : []), '--strict', '--verbose=2', path]);
        const details = run('codesign', ['--display', '--verbose=4', path]);
        validateCodeSignature(details.stdout + '\n' + details.stderr, credentials.APPLE_TEAM_ID, options);
      };
      // Resources/bin/sag is not a standard nested-code location: enumerate Mach-O files explicitly.
      for (const path of walk(app).filter(isMachO).sort((a, b) => b.split('/').length - a.split('/').length)) {
        run('lipo', ['-verify_arch', 'arm64', path]);
        sign(path);
      }
      for (const path of walk(app).filter(path => lstatSync(path).isDirectory() && /\.(?:app|framework|xpc|appex)$/.test(path)).sort((a, b) => b.length - a.length)) sign(path);
      sign(app);
      verify(cli);
      verify(app);
      const notarize = path => validateNotarization(run('xcrun', ['notarytool', 'submit', path, '--apple-id', credentials.APPLE_ID, '--password', credentials.APPLE_APP_SPECIFIC_PASSWORD, '--team-id', credentials.APPLE_TEAM_ID, '--wait', '--timeout', '30m', '--output-format', 'json'], {timeout: 31 * 60 * 1000}).stdout);
      const staple = path => {
        run('xcrun', ['stapler', 'staple', path]);
        run('xcrun', ['stapler', 'validate', path]);
      };
      const zip = join(temporary, 'notarize-app.zip');
      run('ditto', ['-c', '-k', '--keepParent', '--rsrc', '--extattr', '--acl', app, zip]);
      const appNotarization = notarize(zip);
      staple(app);
      verify(cli);
      verify(app);
      run('spctl', ['--assess', '--type', 'execute', '--verbose=4', app]);
      const imageContents = join(temporary, 'image-contents');
      mkdirSync(imageContents);
      run('ditto', ['--rsrc', '--extattr', '--acl', app, join(imageContents, appName)]);
      symlinkSync('/Applications', join(imageContents, 'Applications'));
      const dmg = join(signed, dmgName);
      run('hdiutil', ['create', '-volname', 'SAG Wiki', '-srcfolder', imageContents, '-format', 'UDZO', dmg]);
      sign(dmg, {runtime: false});
      verify(dmg, {runtime: false});
      const dmgNotarization = notarize(dmg);
      staple(dmg);
      verify(dmg, {runtime: false});
      run('spctl', ['--assess', '--type', 'open', '--context', 'context:primary-signature', '--verbose=4', dmg]);
      // macOS tar carries the stapled app's metadata. Exercise the archive consumers receive.
      const archiveEnvironment = {...childEnvironment};
      delete archiveEnvironment.COPYFILE_DISABLE;
      delete archiveEnvironment.COPY_EXTENDED_ATTRIBUTES_DISABLE;
      const archive = join(signed, archiveName);
      run('tar', ['-czf', archive, '-C', extraction, appName], {env: archiveEnvironment});
      const updaterExtraction = join(temporary, 'updater-extracted');
      mkdirSync(updaterExtraction);
      run('tar', ['-xzf', archive, '-C', updaterExtraction], {env: archiveEnvironment});
      const updaterApp = join(updaterExtraction, appName);
      const acceptApp = acceptedApp => {
        const acceptedCli = join(acceptedApp, 'Contents/Resources/bin/sag');
        run('xcrun', ['stapler', 'validate', acceptedApp]);
        verify(acceptedCli);
        verify(acceptedApp);
        run('spctl', ['--assess', '--type', 'execute', '--verbose=4', acceptedApp]);
        const identity = JSON.parse(run(acceptedCli, ['--build-info']).stdout);
        assert.deepEqual(identity, original.build_identity, 'Packaged CLI build identity differs from the staged release');
        run('python3', [join(root, 'tests/desktop_bundle_test.py')], {env: {...childEnvironment, SAG_DESKTOP_BUNDLE: acceptedApp}});
        run('python3', [join(root, 'tests/local-jev-e2e.py'), '--binary', acceptedCli]);
      };
      assert.deepEqual(appContents(updaterApp), appContents(app), 'Updater archive differs from the stapled app, including code signatures');
      acceptApp(updaterApp);
      const mount = join(temporary, 'mounted');
      mkdirSync(mount);
      let attachAttempted = false;
      try {
        attachAttempted = true;
        run('hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mount, dmg]);
        const mountedApp = join(mount, appName);
        assert.deepEqual(appContents(mountedApp), appContents(updaterApp), 'DMG app differs from the extracted updater app, including code signatures');
        acceptApp(mountedApp);
      } finally {
        if (attachAttempted) run('hdiutil', ['detach', mount]);
      }
      const metadata = {...original, source_files: original.files, files: Object.fromEntries(filenames.map(name => [name, digest(join(signed, name))])), apple_signing: {team_id: credentials.APPLE_TEAM_ID, app_notarization_id: appNotarization, dmg_notarization_id: dmgNotarization}};
      writeFileSync(join(signed, 'metadata.json'), JSON.stringify(metadata, null, 2) + '\n');
    } finally {
      const cleanupFailures = [];
      if (searchListChanged) {
        try { run('security', ['list-keychains', '-d', 'user', '-s', ...originalSearchList]); }
        catch (error) { cleanupFailures.push(error); }
      }
      if (keychainCreated) {
        try { run('security', ['delete-keychain', keychain]); }
        catch (error) { cleanupFailures.push(error); }
      }
      if (cleanupFailures.length) throw new AggregateError(cleanupFailures, 'Apple signing keychain cleanup failed');
    }
    assert.ok(!existsSync(destination), 'Signed release output must be fresh');
    renameSync(prepared, destination);
    return JSON.parse(readFileSync(join(destination, 'macos/metadata.json'), 'utf8'));
  } finally {
    rmSync(temporary, {recursive: true, force: true});
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const {values} = parseArgs({options: {input: {type: 'string'}, output: {type: 'string'}}});
    signMacosRelease(values);
    console.log(`Signed and notarized macOS release verified at ${resolve(values.output ?? 'signed-release-artifacts')}`);
  } catch (error) {
    // AssertionError renderings can include secret values; print only the deliberate message.
    console.error(error.message);
    process.exitCode = 1;
  }
}
