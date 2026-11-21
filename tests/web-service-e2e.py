#!/usr/bin/env python3
"""Verify the packaged web workspace using demo files and a local answer-model fixture."""
import argparse
import importlib.util,json,os,socket,subprocess,sys,tempfile,threading,time,urllib.request,urllib.error,urllib.parse,uuid
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
  access_before=json.loads(cli('api','status','--json'))['web_access_code_required']
  external='http://203.0.113.10:8080'
  cli('api','web','external-url',external)
  status=json.loads(cli('api','status','--json'))
  assert not status['web_enabled'] and not status['enabled']
  assert status['web_access_code_required'] is access_before
  assert status['web_external_url']==external
  cli('api','web','start','--host','0.0.0.0','--port',str(port))
  if sys.platform.startswith('linux'):
   assert json.loads(http('/web/session'))['access_code_required'] is False
   cli('api','web','access-code','on')
  else:
   http('/web/session',expected=401)
  code=cli('api','web','code')
  external_headers={'Host':'203.0.113.10:8080','Origin':external}
  http('/web/operation',code,{'operation':'list_roots'},extra=external_headers)
  http('/web/session',expected=401,extra=external_headers)
  http('/web/session',code,expected=403,extra={**external_headers,'Origin':'https://203.0.113.10:8080'})
  http('/web/session',code,expected=403,extra={**external_headers,'Host':'203.0.113.10:8081'})
  http('/web/operation',code,{'operation':'api','args':{'action':{'action':'web_set_external_url','url':None}}},expected=400,extra=external_headers)
  cli('api','web','external-url','https://wiki.example.test')
  http('/web/operation',code,{'operation':'list_roots'},extra={'Host':'wiki.example.test','Origin':'https://wiki.example.test'})
  cli('api','web','external-url','--clear')
  assert json.loads(cli('api','status','--json'))['web_access_code_required'] is True
  http('/web/session',code,expected=403,extra={'Host':'wiki.example.test'})
  http('/web/session',code)
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
  assert rejected['error']['message']=='此操作需在主机上完成'
  assert [item['path'] for item in json.loads(http('/web/operation',code,{'operation':'list_roots'}))]==[path]
  outside=root/'outside.md';outside.write_text('outside indexed folders',encoding='utf-8')
  http('/web/operation',code,{'operation':'read_document','args':{'path':str(outside)}},expected=400)
  cli('api','start')
  http('/v1/status',api_key)
  http('/v1/status',code,expected=401)
  def operation(name,args=None,expected=200):
   body={'operation':name}
   if args is not None:body['args']=args
   return json.loads(http('/web/operation',code,body,expected=expected))
  http('/web/operation',body={'operation':'get_settings'},expected=401)
  web_code_request={'action':{'action':'web_code'}}
  http('/web/operation',body={'operation':'api','args':web_code_request},expected=401)
  assert operation('api',web_code_request)['code']==code
  assert operation('get_settings')['answer']['has_key'] is True
  assert 'local-fixture-only' not in json.dumps(operation('get_settings'))
  operation('api',{'action':{'action':'web_stop'}},400)
  operation('api',{'action':{'action':'start','host':'127.0.0.1','port':port}},400)
  read_access=operation('get_access')
  operation('set_access',{'access':{'enabled':not read_access['enabled']}})
  assert operation('get_access')['enabled'] is not read_access['enabled']
  operation('set_access',{'access':read_access})
  subprocess.run(['node',str(Path(__file__).with_name('web-service-browser.cjs'))],env={**env,'SAG_WEB_URL':url,'SAG_WEB_CODE':code,'SAG_WEB_TEST_ROTATE':f'http://127.0.0.1:{model.server_port}/rotate-web-code'},check=True,timeout=100)
  assert 'Browser edit marker' in source.read_text(encoding='utf-8')
  http('/web/session',code,expected=401)
  code=cli('api','web','code')
  http('/v1/status',api_key)
  cli('service','restart')
  http('/web/session',code)
  http('/v1/status',api_key)
  cli('api','web','access-code','off')
  assert json.loads(cli('api','status','--json'))['web_access_code_required'] is False
  assert json.loads(http('/web/session'))['access_code_required'] is False
  http('/web/operation',body={'operation':'api','args':web_code_request},expected=400)
  assert len(json.loads(http('/web/operation',body={'operation':'list_roots'})))==1
  http('/web/operation',body={'operation':'shutdown'},expected=400)
  http('/v1/status',expected=401);http('/mcp',expected=401);http('/v1/status',api_key)
  cli('api','web','external-url',external)
  assert json.loads(http('/web/session',extra=external_headers))['access_code_required'] is False
  cli('service','restart')
  http('/web/session')
  assert json.loads(http('/web/session',extra=external_headers))['access_code_required'] is False
  cli('api','web','external-url','--clear')
  cli('api','web','access-code','on')
  http('/web/session',expected=401);http('/web/session',code,expected=401)
  code=cli('api','web','code')
  assert json.loads(http('/web/session',code))['access_code_required'] is True
  assert operation('get_settings')['index_workers']==4
  assert operation('get_settings')['search_summary_enabled'] is False
  cli('api','web','stop')
  http('/',expected=404);http('/web/session',code,expected=404);http('/v1/status',api_key)
  cli('api','web','start')
  http('/web/session',code,expected=401)
  cli('api','stop')
  http('/',expected=200);http('/v1/status',api_key,expected=401)
  code=cli('api','web','code')
  deadline=time.monotonic()+15
  while operation('get_index_status')['tasks'] or any(root['status'] in ('pending','indexing') for root in operation('list_roots')):
   assert time.monotonic()<deadline,'fixture index did not finish before clearing data'
   time.sleep(.1)
  operation('clear_all_data')
  assert operation('list_roots')==[]
  assert operation('api',{'action':{'action':'list_keys'}})==[]
  assert operation('get_settings')['answer']['has_key'] is False
  assert json.loads(http('/web/session',code))['access_code_required'] is True
  assert 'Browser edit marker' in source.read_text(encoding='utf-8')
  print('Shared settings persist. Clearing data preserves original files and web access.')
  upload_fixture=root/'上传目录'
  (upload_fixture/'第二层').mkdir(parents=True)
  (upload_fixture/'一级文件.md').write_text('# First level',encoding='utf-8')
  (upload_fixture/'第二层'/'嵌套文件.md').write_text('# Nested markdown',encoding='utf-8')
  subprocess.run(['node',str(Path(__file__).with_name('web-service-browser.cjs'))],env={**env,'SAG_WEB_URL':url,'SAG_WEB_CODE':code,'SAG_WEB_TEST_INTAKE':str(upload_fixture)},check=True,timeout=60)
  roots=operation('list_roots')
  assert [r['name'] for r in roots]==['浏览器资料']
  dest=Path(roots[0]['path'])
  assert dest.parent==Path(operation('web_library'))
  assert (dest/'子文件夹').is_dir()
  assert 'Browser intake keyword GQ-923' in (dest/'单个文件.md').read_text(encoding='utf-8')
  assert (dest/'上传目录'/'第二层'/'嵌套文件.md').read_text(encoding='utf-8')=='# Nested markdown'
  deadline=time.monotonic()+15
  while not any(Path(file['path']).name=='嵌套文件.md' for file in operation('list_files')):
   assert time.monotonic()<deadline,'uploaded files were not indexed'
   time.sleep(.1)
  def upload_at(folder,relative,expected,token=code):
   route='/web/upload?'+urllib.parse.urlencode({'folder':str(folder),'path':relative})
   headers={'X-Sag-Tab':str(uuid.uuid4()),'Content-Type':'application/octet-stream'}
   if token:headers['Authorization']='Bearer '+token
   request=urllib.request.Request(url+route,data=b'upload fixture',headers=headers)
   try:result=browser_opener.open(request,timeout=10)
   except urllib.error.HTTPError as error:result=error
   with result:assert result.status==expected,(result.status,result.read())
  upload_at(dest,'signed-out.md',401,'')
  upload_at(dest,'../outside.md',400)
  upload_at(dest,'.hidden.md',400)
  if os.name!='nt':
   (dest/'linked').symlink_to(docs,target_is_directory=True)
   upload_at(dest,'linked/outside.md',400)
  cli('add',str(docs),'--types','md')
  upload_at(path,'root-upload.md',400)
  assert {r['name'] for r in operation('list_roots')}=={'浏览器资料','项目资料'}
  print('Browser uploads and existing CLI folders are indexed together')
 finally:
  cli('service','stop')
