#!/usr/bin/env python3
"""Isolated browser fixture for pagination scale and failure cases; never contacts lab services."""
import argparse,json,socket,subprocess,tempfile,threading,time,urllib.parse,urllib.request
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--chrome',type=Path,default=Path('/usr/bin/google-chrome'));p.add_argument('--output',type=Path,required=True);args=p.parse_args()
 now=int(time.time()*1000);requests=[];faults=set();empty_targets=False
 def workflow(i):
  uid=f'wf-{i:03}'
  return {'workflow':{'uid':uid,'current':'a'*64,'revision':1,'enabled':True},'definition':{'uid':uid,'name':f'Workflow {i:03}','targets':{'hosts':{'cluster_uid':'hosts','external_target_uid':None,'node_uids':[],'roles':[],'labels':{}}},'parameters':{},'fixed':{},'provenance':{},'playbook':{'steps':[]},'definitions':{}}}
 workflows=[workflow(i) for i in range(25)]
 runs=[{'id':f'run-{i:03}','status':'succeeded','revision':1,'total':1,'completed':1,'submitted_ms':now-i*1000,'finished_ms':now-i*1000+100,'plan':{'work':[],'sha256':'a'*64},'execution':{'actor':{'uid':'person','name':'Fixture'},'request':{'workflow_uid':'wf-000','workflow_revision':'a'*64,'target':'hosts'},'parameters':{},'system':False,'submitted_ms':now-i*1000}}for i in range(25)]
 # Native service evidence and an archived task without timestamps exercise detail compatibility.
 service={'kind':'service','name':'fixture.service','running':True,'enabled':True}
 runs[10]['plan']['work']=[{'id':'service-work','step_id':'start-service','node_uid':'node-010','phase':'rollout','wave':0,'operation':service}]
 runs[10]['attempts']={'service-work':{'status':'succeeded','last_sequence':1,'reports':{},'receipt':{'operation_id':'service-work','script_sha256':'','timeout_seconds':0,'outcome':'succeeded','exit_code':None,'boot_id':'boot','native':{'operation':service,'changed':True,'before':None,'after':None,'error':None,'service_before':{'load':'loaded','active':'inactive','enabled':'disabled','sub':'dead'},'service_after':{'load':'loaded','active':'active','enabled':'enabled','sub':'running','main_pid':123}}}}}
 runs[11].pop('submitted_ms');runs[11].pop('finished_ms');runs[11]['execution'].pop('submitted_ms')
 nodes={f'node-{i:03}':{'boot_id':'boot','session_id':'session','observed_ms':now,'hardware_sha256':'a'*64}for i in range(25)}
 targets={f'external-{i:03}':{'uid':f'external-{i:03}','revision':1,'plugin':{'id':'kubernetes','sha256':'a'*64},'connection':'test-kube-api','spec':{'name':f'Test API {i:03}'}}for i in range(25)}
 audit=[{'identity_name':'Fixture','principal':'person','action':'execute','type':'workflow','code':'ok','observed_ms':now+i*1000,'request_id':f'audit-{i:03}','log_index':i}for i in range(25)]
 class Handler(BaseHTTPRequestHandler):
  def log_message(self,*args):pass
  def reply(self,status,value,kind='application/json'):
   data=value if isinstance(value,bytes) else json.dumps(value).encode();self.send_response(status);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(data)));self.send_header('Cache-Control','no-store');self.end_headers()
   try:self.wfile.write(data)
   except (BrokenPipeError,ConnectionResetError):pass
  def do_GET(self):
   nonlocal empty_targets
   url=urllib.parse.urlsplit(self.path);q=urllib.parse.parse_qs(url.query);path=url.path
   if path=='/fixture/control':
    if q.get('fail'):faults.add((q['fail'][0],q.get('after',[''])[0]))
    if q.get('empty_targets'):empty_targets=True
    return self.reply(200,{'ok':True})
   if not path.startswith('/api/v1/'):
    directory=(ROOT/'web/dist').resolve();file=(directory/('index.html' if path=='/' else path.lstrip('/'))).resolve()
    if file.is_relative_to(directory) and file.is_file():return self.reply(200,file.read_bytes(),{'.js':'text/javascript','.css':'text/css','.html':'text/html'}.get(file.suffix,'application/octet-stream'))
    return self.reply(404,{})
   requests.append(self.path)
   if self.headers.get('Authorization')!='Bearer fixture-token':return self.reply(401,{'code':'fixture_auth'})
   endpoint=path.removeprefix('/api/v1/')
   if endpoint=='identity/me':return self.reply(200,{'capabilities':{'platform':True,'access':False,'super_admin':False},'principal':'person','identity':{'uid':'person','name':'Fixture','disabled':False},'credential':{'uid':'fixture-credential','expires_ms':now+3600000}})
   if endpoint=='workflows':return self.reply(200,{'workflows':workflows})
   if endpoint.startswith('workflows/'):return self.reply(200,next(w for w in workflows if w['workflow']['uid']==endpoint.split('/')[-1]))
   if endpoint.startswith('runs/'):return self.reply(200,next(r for r in runs if r['id']==endpoint.split('/')[-1]))
   if endpoint.startswith('nodes/'):return self.reply(200,{'nodes':{endpoint.split('/')[-1]:nodes[endpoint.split('/')[-1]]}})
   if endpoint in ['runs','nodes','targets','audit']:
    after=q.get('after',[''])[0];limit=int(q.get('limit',['50'])[0]);assert limit in [10,20,50,100]
    if (endpoint,after) in faults:faults.remove((endpoint,after));return self.reply(503,{'code':'fixture_unavailable','retryable':True,'message':'Fixture failure'})
    start=int(after.removeprefix('cursor-') or 0)
    if start:time.sleep(.25)
    source={'runs':runs,'nodes':list(nodes.items()),'targets':[] if empty_targets else list(targets.items()),'audit':audit}[endpoint]
    rows=source[start:start+limit];next_cursor=(start+limit if endpoint=='audit' else f'cursor-{start+limit}') if start+limit<len(source) else None
    return self.reply(200,{('records' if endpoint=='audit' else endpoint):dict(rows) if endpoint in ['nodes','targets'] else rows,'next_cursor':next_cursor})
   return self.reply(404,{})
  def do_POST(self):self.reply(405,{'code':'fixture_read_only'})
 server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
 args.output.parent.mkdir(parents=True,exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='converyn-pagination-') as temp:
  with socket.socket()as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
  child=subprocess.Popen([str(args.chrome),'--headless=new','--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--remote-allow-origins=*',f'--remote-debugging-port={port}',f'--user-data-dir={temp}/chrome'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  try:
   for _ in range(150):
    try:
     with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version',timeout=1):break
    except OSError:time.sleep(.1)
   config=Path(temp)/'browser.json';config.write_text(json.dumps({'chrome':port,'url':f'http://127.0.0.1:{server.server_port}','report':str(args.output.resolve())}));config.chmod(0o600)
   subprocess.run(['node',str(ROOT/'scripts/tests/e2e/pagination_browser.mjs'),str(config)],check=True,timeout=180)
   result=json.loads(args.output.read_text());result['fixture_requests']=len(requests);result['isolated_read_only_fixture']=True;args.output.write_text(json.dumps(result,indent=2)+'\n')
  finally:
   child.terminate()
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait()
   server.shutdown();server.server_close()
if __name__=='__main__':main()
