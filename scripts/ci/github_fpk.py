"""Validate and release x86 FPKs from the isolated GitHub fnOS channel."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

from fpk import digest, inspect_package, version

REPOSITORY = 'Zleap-AI/SAG-Wiki'
BRANCH = 'refs/heads/fnos/develop'


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def source(root, env, publishing=False):
    root = Path(root)
    if env.get('GITHUB_REPOSITORY') != REPOSITORY:
        raise ValueError('Wrong repository')
    commit, run = env['GITHUB_SHA'], env['GITHUB_RUN_ID']
    if not re.fullmatch('[0-9a-f]{40}', commit) or not re.fullmatch('[0-9]+', run):
        raise ValueError('Invalid source/run identity')
    if git(root, 'rev-parse', 'HEAD') != commit:
        raise ValueError('Checkout does not match event SHA')
    number = version(root)
    lock = json.loads((root / 'package-lock.json').read_text())
    if lock['version'] != number or lock['packages']['']['version'] != number:
        raise ValueError('package-lock version mismatch')
    for path, pattern in [('backend/Cargo.toml', r'^version\s*=\s*"([^"]+)"'),
                          ('cli/Cargo.toml', r'^version\s*=\s*"([^"]+)"'),
                          ('packaging/sag-desktop-fnos/manifest', r'^version=(.+)$')]:
        match = re.search(pattern, (root / path).read_text(), re.MULTILINE)
        if not match or match[1] != number:
            raise ValueError(f'version mismatch: {path}')
    ref, event = env['GITHUB_REF'], env['GITHUB_EVENT_NAME']
    formal = ref == f'refs/tags/fnos-v{number}' and event == 'workflow_dispatch'
    branch = ref == BRANCH and event in {'push', 'workflow_dispatch'}
    pr = event == 'pull_request' and env.get('GITHUB_BASE_REF') == 'fnos/develop'
    if not (formal or branch or (pr and not publishing)):
        raise ValueError('Only fnOS branch/manual version tags may publish')
    if formal or branch:
        git(root, 'merge-base', '--is-ancestor', commit, 'refs/remotes/origin/fnos/develop')
    if formal and git(root, 'rev-parse', f'{ref}^{{commit}}') != commit:
        raise ValueError('Release tag moved')
    return {'version': number, 'commit': commit, 'run_id': run,
            'run_url': f'https://github.com/{REPOSITORY}/actions/runs/{run}',
            'tag_name': f'fnos-v{number}' if formal else f'fnos-dev-{commit}-{run}',
            'prerelease': not formal, 'make_latest': 'false'}


def verify(root, env, binary_directory):
    root = Path(root)
    info = source(root, env)
    path = root / 'release' / f'sag-desktop-fnos-{info["version"]}-x86.fpk'
    audit = inspect_package(path, 'x86', info['version'], binary_directory)
    audit.update(commit=info['commit'], run_id=info['run_id'])
    (path.parent / 'audit-x86.json').write_text(json.dumps(audit, sort_keys=True, indent=2) + '\n')
    path.with_suffix('.fpk.sha256').write_text(f'{audit["sha256"]}  {path.name}\n')
    return audit


def prepare(root, env):
    root = Path(root)
    info = source(root, env, publishing=True)
    directory = root / 'release'
    path = directory / f'sag-desktop-fnos-{info["version"]}-x86.fpk'
    actual = inspect_package(path, 'x86', info['version'])
    audit_path = directory / 'audit-x86.json'
    audit = json.loads(audit_path.read_text())
    if audit != dict(actual, commit=info['commit'], run_id=info['run_id']):
        raise ValueError('Artifact audit differs from this source/run')
    checksum = path.with_suffix('.fpk.sha256')
    if checksum.read_text().strip() != f'{actual["sha256"]}  {path.name}':
        raise ValueError('Artifact checksum differs')
    metadata = directory / 'build-info.json'
    metadata.write_text(json.dumps(dict(info, artifacts=[audit]), sort_keys=True, indent=2) + '\n')
    return info, [path, checksum, audit_path, metadata]


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        raise ValueError('Refusing authenticated redirect')


class GitHub:
    def __init__(self, token, api='https://api.github.com', uploads='https://uploads.github.com'):
        self.api, self.uploads, self.token = api, uploads, token
        self.opener = urllib.request.build_opener(NoRedirect)

    def request(self, method, path, data=None, upload=False):
        headers = {'Authorization': f'Bearer {self.token}', 'Accept': 'application/vnd.github+json',
                   'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'sag-wiki-fnos-release'}
        if isinstance(data, dict):
            data = json.dumps(data).encode()
            headers['Content-Type'] = 'application/json'
        elif data is not None:
            headers['Content-Type'] = 'application/octet-stream'
        url = (self.uploads if upload else self.api) + f'/repos/{REPOSITORY}' + path
        request = urllib.request.Request(url, data=data, method=method, headers=headers)
        try:
            with self.opener.open(request, timeout=180) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if method == 'GET' and error.code == 404:
                return None
            raise RuntimeError(f'GitHub {method} {path}: HTTP {error.code}') from None


def publish(root, env, client=None):
    info, paths = prepare(root, env)
    client = client or GitHub(env['GITHUB_TOKEN'])
    tag = info['tag_name']
    encoded = urllib.parse.quote(tag, safe='')
    existing_tag = client.request('GET', '/commits/' + encoded)
    if not existing_tag:
        if not info['prerelease']:
            raise ValueError('Formal tag must already exist')
        client.request('POST', '/git/refs', {'ref': 'refs/tags/' + tag, 'sha': info['commit']})
        existing_tag = client.request('GET', '/commits/' + encoded)
    if not existing_tag or existing_tag['sha'] != info['commit']:
        raise ValueError('Release tag differs from source')
    body = (f'Source: `{info["commit"]}`\n\nRun: {info["run_url"]}\n\n'
            'x86-64 FPK only. CI verification does not replace fnOS device acceptance.\n')
    expected = dict(tag_name=tag, target_commitish=info['commit'],
                    name=f'SAG Wiki fnOS {info["version"]}' + (' (development)' if info['prerelease'] else ''),
                    body=body, prerelease=info['prerelease'])
    release = client.request('GET', '/releases/tags/' + encoded)
    if release:
        if any(release.get(k) != v for k, v in expected.items()):
            raise ValueError('Existing release differs; refusing overwrite')
    else:
        release = client.request('POST', '/releases', dict(expected, draft=True, make_latest='false'))
    endpoint = f'/releases/{release["id"]}'
    files = {p.name: p.read_bytes() for p in paths}
    assets = release.get('assets', [])
    names = [a['name'] for a in assets]
    if len(names) != len(set(names)) or set(names) - files.keys():
        raise ValueError('Existing release assets differ')
    for asset in assets:
        data = files[asset['name']]
        if asset.get('digest') != 'sha256:' + digest(data) or asset['size'] != len(data):
            raise ValueError('Existing asset bytes differ; refusing overwrite')
    missing = files.keys() - set(names)
    if missing and not release['draft']:
        raise ValueError('Published release is incomplete; refusing mutation')
    for name in sorted(missing):
        client.request('POST', endpoint + '/assets?name=' + urllib.parse.quote(name, safe=''), files[name], upload=True)
    final = client.request('GET', endpoint)
    actual = {a['name']: (a.get('digest'), a['size']) for a in final['assets']}
    wanted = {name: ('sha256:' + digest(data), len(data)) for name, data in files.items()}
    if actual != wanted:
        raise ValueError('Uploaded assets differ; keeping release draft')
    if final['draft']:
        client.request('PATCH', endpoint, {'draft': False, 'make_latest': 'false'})
    print(f'Published {tag} from {info["commit"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['source', 'verify', 'publish'])
    parser.add_argument('--binary-directory', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    if args.command == 'source':
        print(json.dumps(source(root, os.environ)))
    elif args.command == 'verify':
        if not args.binary_directory:
            parser.error('verify requires --binary-directory')
        print(json.dumps(verify(root, os.environ, args.binary_directory)))
    else:
        publish(root, os.environ)


if __name__ == '__main__':
    main()
