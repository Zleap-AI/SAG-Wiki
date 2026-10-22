"""Release contract tests: real tar files and an isolated HTTP registry."""
import hashlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
from pathlib import Path
import tarfile
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SPEC = importlib.util.spec_from_file_location('fpk_ci', Path(__file__).parents[1] / 'scripts/ci/fpk.py')
ci = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ci)


def package(path, arch='x86', version='0.1.13', machine=None):
    binary = bytearray(64)
    binary[:5] = b'\x7fELF\x02'
    binary[5] = 1
    binary[18:20] = (machine or {'x86': 62, 'arm': 183}[arch]).to_bytes(2, 'little')
    app = io.BytesIO()
    with tarfile.open(fileobj=app, mode='w:gz') as archive:
        for name in ci.BINARIES:
            item = tarfile.TarInfo('./bin/' + name)
            item.mode, item.size = 0o755, len(binary)
            archive.addfile(item, io.BytesIO(binary))
    with tarfile.open(path, 'w:gz') as archive:
        for name, data in [('manifest', f'appname=sag-desktop-fnos\nversion={version}\nplatform={arch}\n'.encode()), ('app.tgz', app.getvalue())]:
            item = tarfile.TarInfo(name)
            item.size = len(data)
            archive.addfile(item, io.BytesIO(data))


class PackageTests(unittest.TestCase):
    def test_both_architectures_are_inspected(self):
        with tempfile.TemporaryDirectory() as directory:
            for arch in ['x86', 'arm']:
                path = Path(directory) / f'{arch}.fpk'
                package(path, arch)
                result = ci.inspect_package(path, arch, '0.1.13')
                self.assertEqual(result['sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertEqual(set(result['binaries']), {'sag-desktop-fnos', 'sag-fnos-worker', 'sag'})

    def test_wrong_architecture_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'bad.fpk'
            package(path, machine=183)
            with self.assertRaisesRegex(ValueError, 'ELF'):
                ci.inspect_package(path, 'x86', '0.1.13')

    def test_wrong_manifest_version_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'old.fpk'
            package(path, version='0.1.11')
            with self.assertRaisesRegex(ValueError, 'version'):
                ci.inspect_package(path, 'x86', '0.1.13')


class PackagingTests(unittest.TestCase):
    def test_packer_uses_explicit_ci_toolchain_and_preserves_local_default(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            shutil.copy(Path(__file__).parents[1] / 'scripts/package-fnos.mjs', root / 'scripts')
            (root / 'package.json').write_text('{"version":"0.1.13"}')
            (root / 'packaging/sag-desktop-fnos').mkdir(parents=True)
            (root / 'packaging/sag-desktop-fnos/manifest').write_text('platform=x86\n')
            for name in ['dist', 'licenses', 'bin']:
                (root / name).mkdir()
            (root / 'LICENSE').write_text('fixture')
            binaries = root / 'target/x86_64-unknown-linux-gnu/release'
            binaries.mkdir(parents=True)
            for name in ci.BINARIES:
                data = bytearray(64)
                data[:5], data[18:20] = b'\x7fELF\x02', (62).to_bytes(2, 'little')
                (binaries / name).write_bytes(data)
            for name, script in {
                'npm': '#!/bin/sh\nexit 0\n',
                'rustup': '#!/bin/sh\nprintf "%s\\n" "$@" > "$TOOLCHAIN_LOG"\n',
                'fnpack': '#!/bin/sh\nprintf fixture > sag-desktop-fnos.fpk\n',
            }.items():
                path = root / 'bin' / name
                path.write_text(script)
                path.chmod(0o755)
            env = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                       FNPACK=str(root / 'bin/fnpack'), TOOLCHAIN_LOG=str(root / 'toolchain.log'))
            for toolchain in [None, '1.99.1']:
                env.pop('RUSTUP_TOOLCHAIN', None)
                if toolchain:
                    env['RUSTUP_TOOLCHAIN'] = toolchain
                subprocess.run(['node', str(root / 'scripts/package-fnos.mjs')], env=env, check=True, capture_output=True)
                self.assertEqual((root / 'toolchain.log').read_text().splitlines()[1], toolchain or 'stable')


class Registry:
    def __init__(self):
        self.files, self.release, self.puts, self.posts = {}, None, [], []
        self.fail_upload = False
        self.redirect = None
        self.gets = []
        state = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def reply(self, status, body=b''):
                self.send_response(status)
                self.end_headers()
                self.wfile.write(body)

            def authenticated(self):
                if self.headers.get('JOB-TOKEN') != 'synthetic-job-token':
                    self.reply(401)
                    return False
                return True

            def do_GET(self):
                if not self.authenticated():
                    return
                state.gets.append(self.path)
                if state.redirect:
                    self.send_response(302)
                    self.send_header('Location', state.redirect)
                    self.end_headers()
                    return
                if '/packages/generic/' in self.path:
                    self.reply(200, state.files[self.path]) if self.path in state.files else self.reply(404)
                elif '/repository/tags/' in self.path:
                    self.reply(401)
                elif state.release:
                    self.reply(200, json.dumps(state.release).encode())
                else:
                    self.reply(404)

            def do_PUT(self):
                if not self.authenticated():
                    return
                if state.fail_upload:
                    self.reply(500)
                    return
                state.puts.append(self.path)
                state.files[self.path] = self.rfile.read(int(self.headers['Content-Length']))
                self.reply(201, b'{}')

            def do_POST(self):
                if not self.authenticated():
                    return
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                state.posts.append(body)
                state.release = dict(body, commit={'id': body['ref']})
                self.reply(201, json.dumps(body).encode())

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *args):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    @property
    def api(self):
        return f'http://127.0.0.1:{self.server.server_port}/api/v4'


class PublicationTests(unittest.TestCase):
    def fixture(self, directory, registry):
        root = Path(directory)
        (root / 'package.json').write_text('{"version":"0.1.13"}')
        release = root / 'release'
        release.mkdir()
        for arch in ['x86', 'arm']:
            path = release / f'sag-desktop-fnos-0.1.13-{arch}.fpk'
            package(path, arch)
            audit = ci.inspect_package(path, arch, '0.1.13')
            audit.update(commit='a' * 40, pipeline_id='123')
            (release / f'audit-{arch}.json').write_text(json.dumps(audit))
            path.with_suffix('.fpk.sha256').write_text(f'{audit["sha256"]}  {path.name}\n')
        env = {'CI_API_V4_URL': registry.api, 'CI_PROJECT_ID': '134', 'CI_JOB_TOKEN': 'synthetic-job-token',
               'CI_COMMIT_SHA': 'a' * 40, 'CI_PIPELINE_ID': '123', 'CI_COMMIT_BRANCH': 'main',
               'CI_PIPELINE_CREATED_AT': '2026-10-22T01:00:00Z', 'CI_PIPELINE_SOURCE': 'push', 'CI_PIPELINE_URL': 'https://gitlab.example/pipelines/123'}
        return root, env

    def test_publish_and_retry_do_not_duplicate_or_overwrite_assets(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            ci.publish(root, env)
            count = len(registry.puts)
            self.assertGreaterEqual(count, 5)
            self.assertEqual(len(registry.posts), 1)
            self.assertEqual(registry.posts[0]['ref'], 'a' * 40)
            self.assertEqual(registry.posts[0]['released_at'], env['CI_PIPELINE_CREATED_AT'])
            urls = [link['url'] for link in registry.posts[0]['assets']['links']]
            self.assertEqual(len(urls), len(set(urls)))
            self.assertTrue(any(url.endswith('-arm.fpk') for url in urls))
            self.assertTrue(any(url.endswith('-x86.fpk') for url in urls))
            self.assertIn('/sag-desktop-fnos-x86.fpk', [link.get('direct_asset_path') for link in registry.posts[0]['assets']['links']])
            ci.publish(root, env)
            self.assertEqual(len(registry.puts), count)
            self.assertEqual(len(registry.posts), 1)

    def test_public_asset_urls_do_not_change_authenticated_upload_destination(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            env['FPK_PUBLIC_API_V4_URL'] = 'https://gitlab.example/api/v4/'
            ci.publish(root, env)
            self.assertEqual(len(registry.puts), 7)
            links = registry.posts[0]['assets']['links']
            self.assertEqual(len(links), 7)
            self.assertTrue(all(link['url'].startswith('https://gitlab.example/api/v4/projects/134/packages/generic/') for link in links))
            ci.publish(root, env)
            self.assertEqual(len(registry.posts), 1)
            self.assertEqual(len(registry.puts), 7)

    def test_authenticated_redirect_is_rejected_without_contacting_destination(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry, Registry() as destination:
            root, env = self.fixture(directory, registry)
            registry.redirect = destination.api + '/unexpected'
            with self.assertRaisesRegex(ValueError, 'redirect'):
                ci.publish(root, env)
            self.assertEqual(destination.gets, [])
            self.assertEqual(registry.posts, [])

    def test_old_pipeline_keeps_original_release_time_when_finishing_late(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            newer_time = '2026-10-22T02:00:00Z'
            ci.publish(root, dict(env, CI_PIPELINE_CREATED_AT=newer_time))
            registry.release = None
            ci.publish(root, env)
            self.assertLess(registry.posts[-1]['released_at'], registry.posts[0]['released_at'])

    def test_existing_release_for_different_commit_is_not_accepted(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            ci.publish(root, env)
            registry.release['commit']['id'] = 'b' * 40
            with self.assertRaisesRegex(ValueError, 'differs'):
                ci.publish(root, env)
            self.assertEqual(len(registry.posts), 1)

    def test_different_bytes_at_same_release_path_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            ci.publish(root, env)
            first = registry.puts[0]
            registry.files[first] = b'existing incompatible artifact'
            with self.assertRaisesRegex(ValueError, 'overwrite'):
                ci.publish(root, env)
            self.assertEqual(registry.files[first], b'existing incompatible artifact')

    def test_upload_failure_does_not_create_release(self):
        with tempfile.TemporaryDirectory() as directory, Registry() as registry:
            root, env = self.fixture(directory, registry)
            registry.fail_upload = True
            with self.assertRaises(Exception):
                ci.publish(root, env)
            self.assertEqual(registry.posts, [])

    def test_missing_architecture_or_wrong_commit_prevents_upload(self):
        for failure in ['missing', 'wrong-commit', 'wrong-digest']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory, Registry() as registry:
                root, env = self.fixture(directory, registry)
                audit = root / 'release/audit-arm.json'
                if failure == 'missing':
                    audit.unlink()
                elif failure == 'wrong-commit':
                    value = json.loads(audit.read_text())
                    value['commit'] = 'b' * 40
                    audit.write_text(json.dumps(value))
                else:
                    (root / 'release/sag-desktop-fnos-0.1.13-arm.fpk.sha256').write_text('bad checksum')
                with self.assertRaises((ValueError, FileNotFoundError)):
                    ci.publish(root, env)
                self.assertEqual(registry.puts, [])

    def test_non_main_or_tag_pipeline_cannot_publish(self):
        for change in [{'CI_COMMIT_BRANCH': 'feat/test'}, {'CI_PIPELINE_SOURCE': 'merge_request_event'}]:
            with tempfile.TemporaryDirectory() as directory, Registry() as registry:
                root, env = self.fixture(directory, registry)
                env.update(change)
                with self.assertRaisesRegex(ValueError, 'main'):
                    ci.publish(root, env)
                self.assertEqual(registry.puts, [])


if __name__ == '__main__':
    unittest.main()
