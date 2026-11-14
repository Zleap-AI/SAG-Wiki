#!/usr/bin/env python3
"""Verify the packaged web workspace using demo files and a local answer-model fixture."""
import argparse
import importlib.util,json,os,socket,subprocess,tempfile,threading,urllib.request,urllib.error,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
spec=importlib.util.spec_from_file_location('fixture',Path(__file__).with_name('local-jev-e2e.py'));helpers=importlib.util.module_from_spec(spec);spec.loader.exec_module(helpers)
class Answer(BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def do_POST(self):
  if self.path=='/rotate-web-code':
   with helpers.CoreClient(self.server.sag_data) as wire:
    wire.call('api',{'action':{'action':'web_rotate_code'}})
    result=wire.call('api',{'action':{'action':'web_code'}})
   self.send_response(200);self.end_headers();self.wfile.write(result['code'].encode())
   return
  payload=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  assert payload['model']=='demo-model'
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  chunk={'choices':[{'delta':{'content':'The checklist is VX-417.'},'finish_reason':None}]}
  self.wfile.write(('data: '+json.dumps(chunk)+'\n\ndata: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n').encode())
binding=os.environ.get('SAG_WEB_TEST_IP','127.0.0.1')
parser=argparse.ArgumentParser()
parser.add_argument('--binary',required=True)
binary=str(Path(parser.parse_args().binary).resolve())
with tempfile.TemporaryDirectory(prefix='sag-web-demo-') as td, ThreadingHTTPServer(('127.0.0.1',0),Answer) as model:
 root=Path(td).resolve();data=root/'data';docs=root/'项目资料';docs.mkdir()
 model.sag_data=data
 source=docs/'发布流程.md'
 source.write_text('# Release guide\n\nVega release checklist VX-417\n',encoding='utf-8')
 (docs/'会议纪要.md').write_text('# Meeting notes\n\nReview the release checklist before deployment.\n',encoding='utf-8')
 (docs/'项目笔记.md').write_text('# Project notes\n\nCollect trial feedback.\n',encoding='utf-8')
 env={**os.environ,'SAG_DATA_DIR':str(data),'SAG_DESKTOP_DATA_DIR':str(data)}
 model.daemon_threads=True
 threading.Thread(target=model.serve_forever,daemon=True).start()
 def cli(*args):
  r=subprocess.run([binary,*args],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=45);assert r.returncode==0,r.stderr
  return r.stdout.strip()
 with socket.socket() as free:
  free.bind(('0.0.0.0',0));port=free.getsockname()[1]
 url=f'http://{binding}:{port}'
 browser_opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
 def http(route,token='',body=None,expected=200,extra=None):
  headers={'X-Sag-Tab':str(uuid.uuid4()),**(extra or {})}
  if token:headers['Authorization']='Bearer '+token
  if body is not None:headers['Content-Type']='application/json'
  request=urllib.request.Request(url+route,data=json.dumps(body).encode() if body is not None else None,headers=headers)
  try:r=browser_opener.open(request,timeout=10)
  except urllib.error.HTTPError as e:r=e
  with r:
   raw=r.read()
   assert r.status==expected,(route,r.status,raw[:500])
   return raw
 try:
  cli('add',str(docs),'--types','md')
  with helpers.CoreClient(data) as wire:
   s=wire.call('get_settings')
   s['answer'].update(url=f'http://127.0.0.1:{model.server_port}/v1',model='demo-model',models=['demo-model'],auth_mode='bearer')
   s['model_profiles']['demo-model']={'sag2_enabled':False}
   wire.call('save_settings',{'settings':s,'answer_key':'local-fixture-only','jev_key':'','mcp_credentials':None})
  cli('api','web','start','--host','0.0.0.0','--port',str(port))
  code=cli('api','web','code')
  api_key=json.loads(cli('api','key','create','--name','fixture','--json'))['key']
  assert b'data-sag-web' in http('/')
  http('/web/operation',body={'operation':'list_roots'},expected=401)
  http('/web/operation',api_key,{'operation':'list_roots'},expected=401)
  http('/v1/status',api_key,expected=401)
  http('/web/operation',code,{'operation':'list_roots'},expected=403,extra={'Origin':'http://example.invalid'})
  http('/web/operation',code,{'operation':'shutdown'},expected=400)
  roots=json.loads(http('/web/operation',code,{'operation':'list_roots'}))
  assert len(roots)==1
  # Use the service's canonical path, including Windows verbatim prefixes.
  path=roots[0]['path']
  rejected=json.loads(http('/web/operation',code,{'operation':'remove_root','args':{'path':path}},expected=400))
  assert rejected['error']['message']=='此操作需在桌面端完成'
  assert [item['path'] for item in json.loads(http('/web/operation',code,{'operation':'list_roots'}))]==[path]
  outside=root/'outside.md';outside.write_text('outside indexed folders',encoding='utf-8')
  http('/web/operation',code,{'operation':'read_document','args':{'path':str(outside)}},expected=400)
  cli('api','start')
  http('/v1/status',api_key)
  http('/v1/status',code,expected=401)
  subprocess.run(['node',str(Path(__file__).with_name('web-service-browser.cjs'))],env={**env,'SAG_WEB_URL':url,'SAG_WEB_CODE':code,'SAG_WEB_TEST_ROTATE':f'http://127.0.0.1:{model.server_port}/rotate-web-code'},check=True,timeout=100)
  assert 'Browser edit marker' in source.read_text(encoding='utf-8')
  http('/web/session',code,expected=401)
  code=cli('api','web','code')
  http('/v1/status',api_key)
  cli('service','restart')
  http('/web/session',code)
  http('/v1/status',api_key)
  cli('api','web','stop')
  http('/',expected=404);http('/web/session',code,expected=404);http('/v1/status',api_key)
  cli('api','web','start')
  http('/web/session',code,expected=401)
  cli('api','stop')
  http('/',expected=200);http('/v1/status',api_key,expected=401)
  print('Web and API serve independent switches and credentials; edits persist to the same source')
 finally:
  cli('service','stop')
