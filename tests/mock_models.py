"""Local-only protocol fixture. Never contacts an external model service."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if self.path.endswith('/systemone'):
            result = {'model': 'local-protocol-test', 'answers': {key: {'type': 'noul', 'noul': 0.85} for key in payload['questions']}, 'usage': {'input_tokens': 100, 'output_tokens': 10}}
            body = json.dumps(result).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
        elif payload.get('stream'):
            answer = '这是本地协议测试回答。客户端使用 SQLite 保存索引，并通过 Jev 进行多跳检索。[1]'
            chunks = ['data: ' + json.dumps({'choices': [{'delta': {'content': part}}]}, ensure_ascii=False) + '\n\n' for part in [answer[:12], answer[12:40], answer[40:]]]
            body = (''.join(chunks) + 'data: [DONE]\n\n').encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
        else:
            body = json.dumps({'model': 'local-protocol-test', 'choices': [{'message': {'content': 'OK'}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

if __name__ == '__main__':
    print('Local model protocol fixture: http://127.0.0.1:18080', flush=True)
    ThreadingHTTPServer(('127.0.0.1', 18080), Handler).serve_forever()
