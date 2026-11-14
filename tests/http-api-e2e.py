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
        def rejected_config(*arguments, message):
            p = subprocess.run([binary, 'api', 'mcp', 'config', *arguments], env=env,
                               capture_output=True, text=True, encoding='utf-8', timeout=30)
            assert p.returncode != 0 and not p.stdout and message in p.stderr
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
            assert status['mcp_local_url'] == status['local_url'] + '/mcp'
            rejected_config(message='sag api start')
            assert not (data / 'sag-service.json').exists(), 'status unexpectedly started the backend'
            cli('api','start','--host','0.0.0.0','--port',str(port))
            rejected_config(message='请先创建 API Key')
            for path in ('/v1/status','/v1/folders','/v1/search'):
                http(path, expected=401)
            key = cli('api','key','create','--name','Application A','--json',json_output=True)
            other = cli('api','key','create','--name','Application B','--json',json_output=True)
            listed = cli('api','key','list','--json',json_output=True)
            assert len(listed)==2 and all('key' not in item for item in listed)
            assert listed[0]['copyable'] and listed[1]['copyable']
            status_json = cli('api','status','--json')
            status = json.loads(status_json)
            status_text = cli('api','status')
            assert status['mcp_local_url'] == base + '/mcp'
            assert status['mcp_lan_urls'] == [url + '/mcp' for url in status['lan_urls']]
            assert f"本机 MCP：{status['mcp_local_url']}" in status_text
            assert all(f'局域网 MCP：{url}' in status_text for url in status['mcp_lan_urls'])
            for entry in (key, other):
                assert entry['key'] not in status_text and entry['key'] not in status_json
            default_config = cli('api','mcp','config',json_output=True)
            assert default_config == {'mcpServers': {'sag-wiki': {'type': 'http',
                'url': base + '/mcp', 'headers': {'Authorization': 'Bearer ' + other['key']}}}}
            server = default_config['mcpServers']['sag-wiki']
            request = urllib.request.Request(server['url'], headers={**server['headers'],
                'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream'},
                data=json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
                    'protocolVersion': '2025-11-25', 'capabilities': {},
                    'clientInfo': {'name': 'cli-export-test', 'version': '1'},
                }}).encode())
            with urllib.request.urlopen(request, timeout=10) as response:
                assert json.load(response)['result']['serverInfo']['name'] == 'sag-wiki'
            addresses = status['mcp_lan_urls'] or [status['mcp_local_url']]
            for address in addresses:
                config = cli('api','mcp','config','--address',address,'--key-id',key['id'],json_output=True)
                server = config['mcpServers']['sag-wiki']
                assert server['url'] == address and server['headers']['Authorization'] == 'Bearer ' + key['key']
            rejected_config('--address',base + '/v1/search',message='sag api status')
            rejected_config('--key-id','unknown',message='API Key 不存在')
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
            # Upgrade an existing digest-only key alongside a newly copyable key.
            cli('service','stop')
            key_file = data / 'http-api.json'
            saved = json.loads(key_file.read_text(encoding='utf-8'))
            next(item for item in saved['keys'] if item['id'] == key['id']).pop('secret')
            key_file.write_text(json.dumps(saved), encoding='utf-8')
            cli('service','restart')
            assert cli('api','status','--json',json_output=True)['running']
            http('/v1/status',key['key'])
            metadata = {item['id']: item for item in cli('api','key','list','--json',json_output=True)}
            assert not metadata[key['id']]['copyable'] and metadata[other['id']]['copyable']
            rejected_config('--key-id',key['id'],message='请先创建 API Key')
            assert cli('api','mcp','config',json_output=True)['mcpServers']['sag-wiki']['headers']['Authorization'] == 'Bearer ' + other['key']
            cli('api','key','revoke',key['id']);http('/v1/status',key['key'],expected=401);http('/v1/status',other['key'])
            rejected_config('--key-id',key['id'],message='API Key 不存在')
            cli('api','key','revoke',other['id'])
            assert cli('api','status','--json',json_output=True)['running']
            http('/v1/status',other['key'],expected=401)
            cli('api','stop');cli('service','restart')
            status=cli('api','status','--json',json_output=True);assert not status['enabled'] and not status['running']
            assert len(cli('list','--json',json_output=True))==2
            cli('service','stop')
            assert not cli('api','status','--json',json_output=True)['running']
            assert not (data/'sag-service.json').exists()
            print('HTTP API E2E passed: MCP configuration export and connection, CLI lifecycle, persisted state, keys, folders, scopes, fast/deep search, model failures, restart and stop isolation')
        finally:
            try:
                subprocess.run([binary,'service','stop'],env=env,capture_output=True,timeout=90)
            finally:
                model.__exit__()


if __name__ == '__main__': main()
