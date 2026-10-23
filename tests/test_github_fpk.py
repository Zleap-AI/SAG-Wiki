"""GitHub release contract: real packages, git refs and HTTP requests."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / 'scripts/ci'))
from test_fpk_ci import package

class SourceTests(unittest.TestCase):
    def test_github_publisher_exists(self):
        self.assertTrue((Path(__file__).parents[1] / 'scripts/ci/github_fpk.py').is_file(),
                        'GitHub x86 publisher is missing')

    def setUp(self):
        path = Path(__file__).parents[1] / 'scripts/ci/github_fpk.py'
        if not path.exists():
            if self._testMethodName != 'test_github_publisher_exists':
                self.skipTest('publisher not implemented')
            return
        import github_fpk
        self.ci = github_fpk
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'fixture@example.test')
        self.git('config', 'user.name', 'Fixture')
        (self.root / 'package.json').write_text('{"version":"0.1.13"}')
        (self.root / 'package-lock.json').write_text('{"version":"0.1.13","packages":{"":{"version":"0.1.13"}}}')
        for name in ['backend', 'cli']:
            (self.root / name).mkdir()
            (self.root / name / 'Cargo.toml').write_text('[package]\nversion = "0.1.13"\n')
        (self.root / 'packaging/sag-desktop-fnos').mkdir(parents=True)
        (self.root / 'packaging/sag-desktop-fnos/manifest').write_text('version=0.1.13\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')
        self.sha = self.git('rev-parse', 'HEAD')
        self.git('update-ref', 'refs/remotes/origin/fnos/develop', self.sha)
        self.env = dict(GITHUB_REPOSITORY='Zleap-AI/SAG-Wiki', GITHUB_SHA=self.sha,
                        GITHUB_RUN_ID='123', GITHUB_REF='refs/heads/fnos/develop', GITHUB_EVENT_NAME='push')

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True).strip()

    def test_x86_only_assets_and_source(self):
        release = self.root / 'release'
        release.mkdir()
        path = release / 'sag-desktop-fnos-0.1.13-x86.fpk'
        package(path)
        self.ci.verify(self.root, self.env, None)
        info, assets = self.ci.prepare(self.root, self.env)
        self.assertEqual(info['tag_name'], f'fnos-dev-{self.sha}-123')
        self.assertTrue(info['prerelease'])
        self.assertEqual(info['make_latest'], 'false')
        self.assertEqual(len(assets), 4)
        self.assertFalse(any('arm' in p.name for p in assets))
        audit = release / 'audit-x86.json'
        data = json.loads(audit.read_text())
        data['commit'] = 'b' * 40
        audit.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, 'audit'):
            self.ci.prepare(self.root, self.env)

    def test_wrong_ref_event_repository_and_sha_rejected(self):
        for changes in [dict(GITHUB_REF='refs/heads/main'), dict(GITHUB_EVENT_NAME='pull_request'),
                        dict(GITHUB_REPOSITORY='other/repo'), dict(GITHUB_SHA='b'*40),
                        dict(GITHUB_REF='refs/tags/v0.1.13'), dict(GITHUB_REF='refs/heads/feat/test')]:
            with self.subTest(changes=changes), self.assertRaises((ValueError, subprocess.CalledProcessError)):
                self.ci.source(self.root, dict(self.env, **changes), publishing=True)

    def test_reviewed_formal_tag_must_match_version_and_ancestry(self):
        self.git('tag', 'fnos-v0.1.13')
        env = dict(self.env, GITHUB_REF='refs/tags/fnos-v0.1.13', GITHUB_EVENT_NAME='workflow_dispatch')
        result = self.ci.source(self.root, env, publishing=True)
        self.assertFalse(result['prerelease'])
        self.git('update-ref', '-d', 'refs/remotes/origin/fnos/develop')
        with self.assertRaises(subprocess.CalledProcessError):
            self.ci.source(self.root, env, publishing=True)

    def test_package_lock_version_must_match(self):
        (self.root / 'package-lock.json').write_text('{"version":"0.0.0","packages":{"":{"version":"0.0.0"}}}')
        with self.assertRaisesRegex(ValueError, 'version'):
            self.ci.source(self.root, self.env, publishing=True)

    def test_arm_or_missing_x86_rejected(self):
        release = self.root / 'release'
        release.mkdir()
        package(release / 'sag-desktop-fnos-0.1.13-arm.fpk', arch='arm')
        with self.assertRaises(FileNotFoundError):
            self.ci.prepare(self.root, self.env)
        package(release / 'sag-desktop-fnos-0.1.13-x86.fpk', machine=183)
        with self.assertRaisesRegex(ValueError, 'ELF'):
            self.ci.verify(self.root, self.env, None)

if __name__ == '__main__':
    unittest.main()

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import threading
import urllib.parse

class GitHubServer:
    def __init__(self, sha):
        self.sha, self.release, self.files = sha, None, {}
        self.mutations, self.fail_upload, self.redirect = [], False, None
        state = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def handle_request(self):
                if self.headers.get('Authorization') != 'Bearer synthetic-test-token':
                    self.send_response(401); self.end_headers(); return
                if state.redirect:
                    self.send_response(302); self.send_header('Location', state.redirect); self.end_headers(); return
                path = urllib.parse.urlsplit(self.path)
                data = self.rfile.read(int(self.headers.get('Content-Length', '0')))
                status, result = 200, {}
                if self.command != 'GET': state.mutations.append((self.command, path.path))
                if '/commits/' in path.path:
                    result = {'sha': state.sha}
                elif self.command == 'GET' and '/releases/tags/' in path.path:
                    status, result = (200, state.release) if state.release and not state.release['draft'] else (404, {})
                elif self.command == 'GET' and path.path.endswith('/releases'):
                    result = [state.release] if state.release else []
                elif self.command == 'POST' and path.path.endswith('/releases'):
                    state.release = dict(json.loads(data), id=1, assets=[])
                    result = state.release
                elif self.command == 'POST' and path.path.endswith('/assets'):
                    if state.fail_upload:
                        status = 502
                        name = urllib.parse.parse_qs(path.query)['name'][0]
                        state.release['assets'].append(dict(id=99, name=name, size=0, digest=None, state='starter'))
                    else:
                        name = urllib.parse.parse_qs(path.query)['name'][0]
                        state.files[name] = data
                        result = dict(name=name, size=len(data), digest='sha256:'+hashlib.sha256(data).hexdigest())
                        state.release['assets'].append(result)
                elif self.command == 'DELETE' and path.path.endswith('/releases/assets/99'):
                    state.release['assets'] = [a for a in state.release['assets'] if a.get('id') != 99]
                    self.send_response(204); self.end_headers(); return
                elif self.command == 'PATCH':
                    state.release.update(json.loads(data)); result = state.release
                elif path.path.endswith('/releases/1'):
                    result = state.release
                else: status = 404
                self.send_response(status); self.end_headers()
                self.wfile.write(json.dumps(result).encode())
            do_GET = do_POST = do_PATCH = do_DELETE = handle_request
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    def __enter__(self): self.thread.start(); return self
    def __exit__(self, *args): self.server.shutdown(); self.server.server_close(); self.thread.join()
    def client(self, ci):
        url = f'http://127.0.0.1:{self.server.server_port}'
        return ci.GitHub('synthetic-test-token', api=url, uploads=url)

class PublicationTests(SourceTests):
    def fixture_assets(self):
        (self.root / 'release').mkdir()
        package(self.root / 'release/sag-desktop-fnos-0.1.13-x86.fpk')
        self.ci.verify(self.root, self.env, None)

    def test_publish_x86_and_retry_without_duplicate_or_latest(self):
        self.fixture_assets()
        with GitHubServer(self.sha) as server:
            self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertFalse(server.release['draft'])
            self.assertTrue(server.release['prerelease'])
            self.assertEqual(server.release['make_latest'], 'false')
            self.assertEqual(len(server.files), 4)
            before = len(server.mutations)
            self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertEqual(len(server.mutations), before)
            server.release['assets'][0]['digest'] = 'sha256:wrong'
            with self.assertRaisesRegex(ValueError, 'differ'):
                self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertEqual(len(server.mutations), before)

    def test_failed_upload_keeps_draft_and_can_resume(self):
        self.fixture_assets()
        with GitHubServer(self.sha) as server:
            server.fail_upload = True
            with self.assertRaises(RuntimeError):
                self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertTrue(server.release['draft'])
            server.fail_upload = False
            self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertFalse(server.release['draft'])
            self.assertEqual(sum(verb == 'DELETE' for verb, _ in server.mutations), 1)
            self.assertEqual(sum(verb == 'POST' and path.endswith('/releases') for verb, path in server.mutations), 1)

    def test_redirect_and_wrong_source_do_not_publish(self):
        self.fixture_assets()
        with GitHubServer('b'*40) as server:
            with self.assertRaisesRegex(ValueError, 'tag'):
                self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertEqual(server.mutations, [])
            server.redirect = 'http://127.0.0.1:1/credentials'
            with self.assertRaisesRegex(ValueError, 'redirect'):
                self.ci.publish(self.root, self.env, server.client(self.ci))
            self.assertEqual(server.mutations, [])
