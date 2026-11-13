#!/usr/bin/env python3
"""Exercise CLI-managed HTTP API using a private, temporary SAG data directory."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    fixture_spec = importlib.util.spec_from_file_location('local_jev', Path(__file__).with_name('local-jev-e2e.py'))
    fixture_module = importlib.util.module_from_spec(fixture_spec)
    fixture_spec.loader.exec_module(fixture_module)
    with tempfile.TemporaryDirectory(prefix='sag-http-e2e-') as temporary:
        root = Path(temporary).resolve()
        data = root / 'data'
        a, b = root / '项目 A', root / '项目 B'
        a.mkdir(); b.mkdir(); (a / 'sub').mkdir()
        for name, body in fixture_module.DOCUMENTS.items():
            (a / name).write_text(body, encoding='utf-8')
        (b / 'other.md').write_text('# Vega release\nVega release documentation is archived.\n', encoding='utf-8')
        env = dict(os.environ, SAG_DATA_DIR=str(data))
        def cli(*arguments, json_output=False):
            p = subprocess.run([binary, *arguments], env=env, capture_output=True, text=True, encoding='utf-8', timeout=90)
            if p.returncode:
                raise AssertionError(f'CLI failed: {arguments}: {p.stderr}')
            return json.loads(p.stdout) if json_output else p.stdout
        with socket.socket() as available:
            available.bind(('127.0.0.1', 0)); port = available.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        def http(path, key='', payload=None, expected=200):
            headers = {'Authorization': f'Bearer {key}'} if key else {}
            if payload is not None: headers['Content-Type'] = 'application/json'
            request = urllib.request.Request(base + path, headers=headers, data=json.dumps(payload).encode() if payload is not None else None)
            try: response = urllib.request.urlopen(request, timeout=65)
            except urllib.error.HTTPError as error: response = error
            with response:
                assert response.status == expected, (path, response.status, response.read().decode())
                assert response.headers.get('Access-Control-Allow-Origin') is None
                value = json.load(response)
                if expected != 200: assert value['request_id'] and value['error']['code']
                return value
        model = fixture_module.JevFixture().__enter__()
        try:
            status = cli('api','status','--json',json_output=True)
            assert not status['enabled'] and not status['running'] and not status['autostart']
            assert not (data / 'sag-service.json').exists(), 'status unexpectedly started the backend'
            cli('api','start','--host','0.0.0.0','--port',str(port))
            for path in ('/v1/status','/v1/folders','/v1/search'):
                http(path, expected=401)
            key = cli('api','key','create','--name','Application A','--json',json_output=True)
            other = cli('api','key','create','--name','Application B','--json',json_output=True)
            listed = cli('api','key','list','--json',json_output=True)
            assert len(listed)==2 and all('key' not in item for item in listed)
            assert key['key'] not in (data / 'http-api.json').read_text()
            cli('add',str(a),'--types','md','--json',json_output=True)
            cli('add',str(b),'--types','md','--json',json_output=True)
            roots = http('/v1/folders',key['key'])['folders']; assert len(roots)==2
            root_paths = {item['name']: Path(item['path']) for item in roots}
            children = http('/v1/folders?'+urllib.parse.urlencode({'path':str(a)}),key['key'])['folders']
            assert [item['name'] for item in children]==['sub']
            result = http('/v1/search',key['key'],{'query':'Vega release','mode':'fast','paths':[str(a),str(a/'sub'),str(b)],'limit':100})
            assert len(result['results']) >= 4, result
            assert any(Path(item['path']).is_relative_to(root_paths[a.name]) for item in result['results'])
            assert any(Path(item['path']).is_relative_to(root_paths[b.name]) for item in result['results'])
            for item in result['results']: assert Path(item['path']).is_absolute() and item['content'] and item['start_line']>0
            assert http('/v1/search',key['key'],{'query':'absentmarker','mode':'fast'})['results']==[]
            assert http('/v1/search',key['key'],{'query':'Vega release'},expected=409)['error']['code']=='decision_not_configured'
            assert http('/v1/search',key['key'],{'query':'Vega','mode':'fast','paths':[str(a),str(root)]},expected=403)['error']['code']=='path_not_allowed'
            # Only the model is simulated; configuration, scoring and HTTP service are real.
            cli('config','--url',model.origin + '/v1','--model',fixture_module.MODEL,'--auth','none')
            result = http('/v1/search',key['key'],{'query':fixture_module.QUESTION,'paths':[str(a)],'limit':2})
            assert result['mode']=='deep' and result['results'], result
            assert model.calls and not model.violations
            # Changing a source after scoring starts is an internal search
            # failure, not an upstream decision-model failure.
            source = a / 'checklist.md'
            original_text = source.read_text(encoding='utf-8')
            original_calls = model.calls
            class MutatingCalls(list):
                def append(self, item):
                    super().append(item)
                    source.write_text(source.read_text(encoding='utf-8') + '\nchanged during search\n', encoding='utf-8')
            model.calls = MutatingCalls(original_calls)
            try:
                failure = http('/v1/search',key['key'],{'query':fixture_module.QUESTION,'paths':[str(a)]},expected=500)
                assert failure['error']['code']=='internal_error'
            finally:
                model.calls = original_calls
                source.write_text(original_text, encoding='utf-8')
            cli('add',str(a),'--json',json_output=True)
            model.behavior='unauthorized'
            assert http('/v1/search',key['key'],{'query':fixture_module.QUESTION},expected=502)['error']['code']=='decision_request_failed'
            model.behavior='valid'
            cli('service','restart')
            assert cli('api','status','--json',json_output=True)['running']
            http('/v1/status',key['key'])
            cli('api','key','revoke',key['id']);http('/v1/status',key['key'],expected=401);http('/v1/status',other['key'])
            cli('api','key','revoke',other['id'])
            assert cli('api','status','--json',json_output=True)['running']
            http('/v1/status',other['key'],expected=401)
            cli('api','stop');cli('service','restart')
            status=cli('api','status','--json',json_output=True);assert not status['enabled'] and not status['running']
            assert len(cli('list','--json',json_output=True))==2
            cli('service','stop')
            assert not cli('api','status','--json',json_output=True)['running']
            assert not (data/'sag-service.json').exists()
            print('HTTP API E2E passed: CLI lifecycle, persisted state, keys, folders, scopes, fast/deep search, model failures, restart and stop isolation')
        finally:
            try:
                subprocess.run([binary,'service','stop'],env=env,capture_output=True,timeout=90)
            finally:
                model.__exit__()


if __name__ == '__main__': main()
