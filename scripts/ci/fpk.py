"""Inspect FPK artifacts and publish verified main builds with CI_JOB_TOKEN."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import tarfile
import urllib.error
import urllib.parse
import urllib.request

BINARIES = ('sag-desktop-fnos', 'sag-fnos-worker', 'sag')
MACHINES = {'x86': 62, 'arm': 183}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def member(archive, name):
    matches = [item for item in archive.getmembers() if item.name.removeprefix('./') == name]
    if len(matches) != 1 or not matches[0].isfile():
        raise ValueError(f'Expected one regular archive member: {name}')
    return matches[0]


def inspect_package(path, arch, version, binary_directory=None):
    path = Path(path)
    result = {'file': path.name, 'version': version, 'arch': arch, 'sha256': digest(path.read_bytes()), 'binaries': {}}
    with tarfile.open(path) as outer:
        manifest = outer.extractfile(member(outer, 'manifest')).read().decode()
        values = dict(re.findall(r'^(\w+)\s*=\s*(.*?)\s*$', manifest, re.MULTILINE))
        for key, expected in [('appname', 'sag-desktop-fnos'), ('version', version), ('platform', arch)]:
            if values.get(key) != expected:
                raise ValueError(f'Manifest {key} must be {expected}')
        with tarfile.open(fileobj=io.BytesIO(outer.extractfile(member(outer, 'app.tgz')).read())) as app:
            for name in BINARIES:
                item = member(app, 'bin/' + name)
                data = app.extractfile(item).read()
                if len(data) < 20 or data[:6] != b'\x7fELF\x02\x01' or int.from_bytes(data[18:20], 'little') != MACHINES[arch]:
                    raise ValueError(f'Wrong ELF architecture for {arch}: {name}')
                if item.mode & 0o7777 != 0o755:
                    raise ValueError(f'Expected executable mode 0755: {name}')
                if binary_directory and data != (Path(binary_directory) / name).read_bytes():
                    raise ValueError(f'FPK binary differs from build output: {name}')
                result['binaries'][name] = digest(data)
    return result


def version(root):
    value = json.loads((root / 'package.json').read_text())['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-\d+)?', value):
        raise ValueError('Invalid numeric fnOS package version')
    return value


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        raise ValueError('Refusing to redirect an authenticated GitLab request')


class GitLab:
    def __init__(self, env):
        self.base = f"{env['CI_API_V4_URL'].rstrip('/')}/projects/{env['CI_PROJECT_ID']}"
        self.token = env['CI_JOB_TOKEN']
        self.opener = urllib.request.build_opener(NoRedirect)

    def request(self, method, path, data=None, json_body=False):
        headers = {'JOB-TOKEN': self.token}
        if json_body:
            data = json.dumps(data).encode()
            headers['Content-Type'] = 'application/json'
        request = urllib.request.Request(self.base + path, data=data, method=method, headers=headers)
        try:
            with self.opener.open(request, timeout=180) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code == 404 and method == 'GET':
                return None
            raise RuntimeError(f'GitLab {method} {path}: HTTP {error.code}') from None


def publish(root, env):
    if env.get('CI_COMMIT_BRANCH') != 'main' or env.get('CI_PIPELINE_SOURCE') not in {'push', 'web'}:
        raise ValueError('Only push/manual main pipelines may publish')
    commit, pipeline = env['CI_COMMIT_SHA'], env['CI_PIPELINE_ID']
    if not re.fullmatch(r'[0-9a-f]{40}', commit) or not pipeline.isdigit():
        raise ValueError('Invalid commit or pipeline identifier')
    root, assets = Path(root), []
    release_version = version(root)
    # Validate the complete set before sending the first byte to GitLab.
    audits = []
    for arch in MACHINES:
        audit_path = root / 'release' / f'audit-{arch}.json'
        audit = json.loads(audit_path.read_text())
        package_path = root / 'release' / f'sag-desktop-fnos-{release_version}-{arch}.fpk'
        actual = inspect_package(package_path, arch, release_version)
        if audit != dict(actual, commit=commit, pipeline_id=pipeline):
            raise ValueError(f'Artifact audit does not match this source/pipeline: {arch}')
        checksum = package_path.with_suffix('.fpk.sha256')
        if checksum.read_text().strip() != f'{actual["sha256"]}  {package_path.name}':
            raise ValueError(f'Wrong checksum sidecar: {arch}')
        assets.extend([package_path, checksum, audit_path])
        audits.append(audit)
    package_version = f'{release_version}-main.{commit[:8]}.{pipeline}'
    tag = f'v{package_version}'
    metadata = root / 'release' / 'build-info.json'
    metadata.write_text(json.dumps({'version': release_version, 'commit': commit, 'pipeline_id': pipeline,
                                   'pipeline_url': env['CI_PIPELINE_URL'], 'artifacts': audits}, sort_keys=True, indent=2) + '\n')
    assets.append(metadata)
    client = GitLab(env)
    # Runners may use an internal HTTP GitLab address; downloads should use the
    # browser-facing HTTPS endpoint so normal session authentication still works.
    public_base = f"{env.get('FPK_PUBLIC_API_V4_URL', env['CI_API_V4_URL']).rstrip('/')}/projects/{env['CI_PROJECT_ID']}"
    prefix = f'/packages/generic/sag-wiki-fnos/{package_version}/'
    links = []
    for asset in assets:
        path = prefix + asset.name
        data = asset.read_bytes()
        existing = client.request('GET', path)
        if existing is not None:
            if existing != data:
                raise ValueError(f'Refusing to overwrite existing package: {asset.name}')
        else:
            client.request('PUT', path, data)
        # Verify storage before advertising any download link in a Release.
        if client.request('GET', path) != data:
            raise ValueError(f'Published package verification failed: {asset.name}')
        links.append({'name': asset.name, 'url': public_base + path, 'link_type': 'package',
                      'direct_asset_path': '/' + asset.name.replace(f'-{release_version}-', '-')})
    release_path = '/releases/' + urllib.parse.quote(tag, safe='')
    existing = client.request('GET', release_path)
    if existing:
        release = json.loads(existing)
        actual_links = {(item['name'], item['url']) for item in release['assets']['links']}
        if release['commit']['id'] != commit or actual_links != {(item['name'], item['url']) for item in links}:
            raise ValueError('Existing release differs from this build; refusing to replace it')
    else:
        client.request('POST', '/releases', {
            'name': f'SAG Wiki fnOS {release_version} · {commit[:8]} · pipeline {pipeline}',
            'tag_name': tag, 'ref': commit, 'released_at': env['CI_PIPELINE_CREATED_AT'],
            'description': f'Automated main build.\n\nSource: `{commit}`\n\nPipeline: {env["CI_PIPELINE_URL"]}\n\n'
                           'Assets include x86-64 and ARM64 FPKs, SHA-256 sidecars and build audits. '
                           'CI checks do not replace fnOS device acceptance.',
            'assets': {'links': links},
        }, json_body=True)
    print(f'Published {tag} from {commit}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    verify = commands.add_parser('verify')
    verify.add_argument('--arch', choices=MACHINES, required=True)
    verify.add_argument('--binary-directory', type=Path, required=True)
    commands.add_parser('publish')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    if args.command == 'publish':
        publish(root, os.environ)
    else:
        release_version = version(root)
        for manifest in ['backend/Cargo.toml', 'cli/Cargo.toml']:
            if not re.search(r'^version\s*=\s*"' + re.escape(release_version) + r'"\s*$', (root / manifest).read_text(), re.MULTILINE):
                raise ValueError(f'Version mismatch: {manifest}')
        result = inspect_package(root / 'release' / f'sag-desktop-fnos-{release_version}-{args.arch}.fpk',
                                 args.arch, release_version, args.binary_directory)
        result.update(commit=os.environ['CI_COMMIT_SHA'], pipeline_id=os.environ['CI_PIPELINE_ID'])
        (root / 'release' / f'audit-{args.arch}.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
        print(f'Verified {result["file"]}: {result["sha256"]}')


if __name__ == '__main__':
    main()
