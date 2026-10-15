"""Local JSON-RPC fixture for MCP transport and tool-routing tests."""
import json
import os
import sys
import signal

signal.signal(signal.SIGPIPE, signal.SIG_DFL)
import time

for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    method = request['method']
    if method == 'initialize':
        result = {'protocolVersion': request['params']['protocolVersion'], 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'test-mcp', 'version': '1'}}
    elif method == 'tools/list':
        result = {'tools': [{'name': name, 'description': 'Return the provided text', 'inputSchema': {'type': 'object', 'properties': {'text': {'type': 'string'}, 'delay': {'type': 'number'}, 'fail': {'type': 'boolean'}}, 'required': ['text']}} for name in ['echo', 'reverse']]}
    elif method == 'tools/call':
        params = request['params']
        arguments = params['arguments']
        if arguments.get('disconnect'):
            sys.exit(2)
        time.sleep(arguments.get('delay', 0))
        result = {'content': [{'type': 'text', 'text': (arguments['text'][::-1] if params['name'] == 'reverse' else arguments['text']) + os.getenv('MCP_TEST_SUFFIX', '')}], 'isError': arguments.get('fail', False)}
    elif method == 'ping':
        result = {}
    else:
        print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32601, 'message': 'Unknown method'}}), flush=True)
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
