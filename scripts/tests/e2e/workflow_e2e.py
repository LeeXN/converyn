#!/usr/bin/env python3
"""Exercise a fresh controller, enrolled Agent and personal credentials over real mTLS.
Uses only isolated temporary files and loopback ports. Never prints credentials.
"""
import pty, select, signal, sys
import urllib.parse
import argparse, hashlib, json, os, re, socket, ssl, subprocess, tempfile, time, urllib.error, urllib.request, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'support'))
from client_profile import load as client_profile, tls as client_tls


def browser(root,base,bundle,config,chrome,report,admin_config=None,script=None,extra=None):
    personal=client_profile(config);ctx=client_tls(personal)
    admin_personal=client_profile(admin_config) if admin_config else None
    admin_token=admin_personal['credentials']['token'] if admin_personal else None
    admin_ctx=client_tls(admin_personal) if admin_personal else ctx
    class Relay(BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):self.forward()
        def do_POST(self):self.forward()
        def forward(self):
            body=self.rfile.read(int(self.headers.get('Content-Length',0))) if self.command=='POST' else None
            headers={k:v for k,v in self.headers.items() if k.lower() not in ['host','connection','accept-encoding','content-length']}
            req=urllib.request.Request(base+self.path,data=body,headers=headers,method=self.command)
            try:response=urllib.request.urlopen(req,context=admin_ctx if admin_token and self.headers.get('Authorization')=='Bearer '+admin_token else ctx,timeout=20)
            except urllib.error.HTTPError as error:response=error
            if '/events' in self.path:
                self.send_response(response.code)
                self.send_header('Content-Type',response.headers.get('Content-Type','text/event-stream'));self.end_headers()
                try:
                    while True:
                        chunk=response.read1(65536)
                        if not chunk:break
                        self.wfile.write(chunk);self.wfile.flush()
                except (BrokenPipeError,ConnectionResetError,__import__("http.client",fromlist=["IncompleteRead"]).IncompleteRead):pass
                finally:response.close()
                return
            data=response.read();self.send_response(response.code)
            for k,v in response.headers.items():
                if k.lower() not in ['connection','transfer-encoding','content-length']:self.send_header(k,v)
            self.send_header('Content-Length',str(len(data)));self.end_headers()
            try:self.wfile.write(data)
            except (BrokenPipeError,ConnectionResetError):pass
    relay=ThreadingHTTPServer(('127.0.0.1',0),Relay);threading.Thread(target=relay.serve_forever,daemon=True).start()
    with socket.socket() as sock:sock.bind(('127.0.0.1',0));chrome_port=sock.getsockname()[1]
    child=subprocess.Popen([str(chrome),'--headless=new','--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--remote-allow-origins=*',f'--remote-debugging-port={chrome_port}',f'--user-data-dir={root / "chrome"}'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        for _ in range(150):
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{chrome_port}/json/version',timeout=1):break
            except OSError:time.sleep(.1)
        cfg=root/'browser.json';cfg.write_text(json.dumps({'url':f'http://127.0.0.1:{relay.server_port}','chrome':chrome_port,'cluster':'workflow-test','token':personal['credentials']['token'],'inventory_log_fixture':True,'report':str(report.resolve()),'admin_token':admin_token,**(extra or {})}));cfg.chmod(0o600)
        subprocess.run(['node',str(Path(__file__).with_name(script or 'workflow_browser.mjs')),str(cfg)],check=True,timeout=240)
    finally:
        child.terminate()
        try:child.wait(timeout=10)
        except subprocess.TimeoutExpired:child.kill();child.wait()
        relay.shutdown();relay.server_close()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/debug/converyn'))
    parser.add_argument('--output',type=Path)
    parser.add_argument('--retain-dir',type=Path,help='new private directory retaining local acceptance evidence')
    parser.add_argument('--api-evidence',type=Path,help='new private JSONL file of API shapes, excluding authentication headers and private-key rejection inputs')
    parser.add_argument('--controllers',type=int,choices=[1,3],default=1,help='three controllers also kill the leader while a non-idempotent script is running')
    parser.add_argument('--outputs-only',action='store_true',help='run native output checks after the shared identity/Agent setup')
    parser.add_argument('--directory-only',action='store_true',help='run generic workflow directory and snapshot acceptance')
    parser.add_argument('--plugin-input-only',action='store_true',help='verify a materialized plugin survives claim contract replay')
    parser.add_argument('--secrets-only',action='store_true',help='test centralized Secret delivery to five enrolled nodes')
    parser.add_argument('--artifacts-only',action='store_true',help='run managed artifact upload and five-Agent delivery acceptance')
    parser.add_argument('--chrome',type=Path)
    parser.add_argument('--browser-report',type=Path,default=Path('/tmp/converyn-workflow-browser.json'))
    a=parser.parse_args(); binary=a.binary.resolve(); children=[]
    if a.chrome and a.controllers!=1:parser.error('browser fixture uses one controller; run HA acceptance separately')
    if a.api_evidence:
        a.api_evidence.parent.mkdir(parents=True,exist_ok=True)
        os.close(os.open(a.api_evidence,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600))
    def cli(*args,check=True,cwd=None,home=None,config_env=None,output_format="json"):
        env=dict(os.environ)
        if home:
            env['HOME']=str(home)
            env.pop('CONVERYN_CONFIG',None)
        if config_env is not None:env['CONVERYN_CONFIG']=str(config_env)
        result=subprocess.run([str(binary),*(['-o',output_format] if output_format else []),*map(str,args)],capture_output=True,text=True,timeout=60,cwd=cwd,env=env)
        if check and result.returncode: raise AssertionError(f'CLI {args[0]} failed: {result.stderr[-2000:]}')
        return json.loads(result.stdout) if result.stdout.strip().startswith(('{','[')) else result
    if a.retain_dir:
        a.retain_dir.mkdir(mode=0o700)
    from contextlib import nullcontext
    with (nullcontext(str(a.retain_dir)) if a.retain_dir else tempfile.TemporaryDirectory(prefix='converyn-workflow-')) as temporary:
        root=Path(temporary).resolve(); root.chmod(0o755)
        # Pin one executable for every process and CLI call, even during a concurrent rebuild.
        import shutil
        frozen=root/'converyn';shutil.copy2(binary,frozen);frozen.chmod(0o755);binary=frozen
        ports=[]
        for _ in range(a.controllers):
            with socket.socket() as sock:sock.bind(('127.0.0.1',0));ports.append(sock.getsockname()[1])
        bases=[f'https://127.0.0.1:{port}' for port in ports];base=bases[0];bundle=root/'control'
        members=[argument for index,port in enumerate(ports,1) for argument in ['--member',f'{index}=127.0.0.1:{port}']]
        cli('admin','init','--directory',bundle,'--cluster','workflow-test',*members)
        def start(*args):
            log=open(root/f'process-{len(children)}.log','w')
            child=subprocess.Popen([str(binary),*map(str,args)],stdout=log,stderr=log);children.append((child,log));return child
        for index in range(1,a.controllers+1):
            profile=bundle/f'controller-{index}/server.json';settings=json.loads(profile.read_text());settings['log_store']={'backend':'filesystem','directory':str(root/'logs'),'shared':a.controllers==3,'retention_days':30};profile.write_text(json.dumps(settings))
        controllers=[start('server','--config',bundle/f'controller-{index}/server.json') for index in range(1,a.controllers+1)]
        killed_leader=None
        admin=bundle/'admin/config.json'
        def observed(path,body,status,value):
            retired=body is not None and path in ['/api/v1/runs','/api/v1/scripts','/api/v1/catalog/commands','/api/v1/deployments']
            if a.api_evidence and not path.partition('?')[0].endswith('/inputs') and not (path=='/api/v1/secrets' and body is not None) and path.startswith('/api/v1/') and not retired and 'PRIVATE KEY' not in json.dumps(body):
                with a.api_evidence.open('a') as output:output.write(json.dumps({'method':'get' if body is None else 'post','path':path,'status':status,'request':body,'response':value})+'\n')
            return status,value
        def request(config,path,body=None,address=None,extra_headers=None):
            config=client_profile(config);ctx=client_tls(config)
            for candidate in ([address] if address else bases):
                req=urllib.request.Request(candidate+path,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+config['credentials']['token'],'x-converyn-cluster':'workflow-test','Content-Type':'application/json',**(extra_headers or {})})
                try:
                    with urllib.request.urlopen(req,context=ctx,timeout=10) as response:return observed(path,body,response.status,json.load(response))
                except urllib.error.HTTPError as error:
                    data=error.read()
                    try:value=json.loads(data)
                    except (ValueError,UnicodeError):value={}
                    if error.code==503 and not address:continue
                    return observed(path,body,error.code,value)
                except (OSError,urllib.error.URLError):
                    if address:raise
            raise urllib.error.URLError('no live quorum endpoint')
        def until(function,timeout=30):
            end=time.monotonic()+timeout;last=None
            while time.monotonic()<end:
                if not any(child.poll() is None for child in controllers):raise AssertionError('all controllers exited: '+(root/'process-0.log').read_text()[-3000:])
                try:
                    last=function()
                    if last:return last
                except (OSError,urllib.error.URLError):pass
                time.sleep(.1)
            raise AssertionError('timed out: '+str(last))
        try:
            until(lambda:request(admin,'/health/live')[0]==200)
            cli('admin','initialize','--config',admin,'--members',bundle/'members.json')
            op=root/'op';created_op=cli('create', 'identities','--config',admin,'--authority',bundle/'user-authority','--name','OP','--super-admin','--output',op)
            assert created_op['identity_uid'] and created_op['credential_uid']
            op_config=op/'config.json';until(lambda:request(op_config,'/api/v1/identity/me')[0]==200)
            alice=root/'alice';cli('create', 'identities','--config',op_config,'--authority',bundle/'user-authority','--name','Alice','--output',alice)
            bob=root/'bob';cli('create', 'identities','--config',op_config,'--authority',bundle/'user-authority','--name','Bob','--output',bob)
            # Every fixture permission is explicit; fresh identities have no access.
            for directory in [alice,bob]:
                identity_uid=json.loads((directory/'credential.json').read_text())['identity_uid']
                cli('auth','grant',directory.name+'-execute','--config',op_config,'--person',identity_uid,'--role','executor')
            cli('auth','grant','alice-platform','--config',op_config,'--person',json.loads((alice/'credential.json').read_text())['identity_uid'],'--role','platform_operator')
            cli_home=root/'cli-home';cli_home.mkdir(mode=0o700)
            imported=cli('config','import',alice/'config.json','--name','alice',home=cli_home)
            assert imported['current_context']=='alice'
            imported=cli('config','import',bob/'config.json','--name','bob',home=cli_home)
            assert imported['current_context']=='alice'
            managed=cli_home/'.converyn/config.json'
            assert managed.stat().st_mode & 0o077 == 0
            assert json.loads(cli('config','current-context',home=cli_home).stdout)=='alice'
            assert cli('config','import',bob/'config.json','--name','alice',home=cli_home,check=False).returncode!=0
            safe=json.dumps(cli('config','view',home=cli_home))
            assert (alice/'token').read_text().strip() not in safe
            assert 'PRIVATE KEY' not in safe and '[REDACTED]' in safe
            portable=root/'portable.json';cli('--context','alice','config','export','--output',portable,home=cli_home)
            assert set(json.loads(portable.read_text())['contexts'])=={'alice'}
            assert json.loads(cli('config','current-context',home=cli_home).stdout)=='alice'
            assert cli('--config',root/'missing.json','get','nodes',home=cli_home,config_env=managed,check=False).returncode!=0
            assert cli('--context','missing','get','nodes',home=cli_home,check=False).returncode!=0
            password=root/'export-password';password.write_text('isolated-test-export-password\n');password.chmod(0o600)
            cli('export', 'credentials','--config',alice/'config.json','--current','--output','browser.p12','--password-file',password,cwd=alice)
            assert (alice/'browser.p12').stat().st_mode & 0o077 == 0
            exported=subprocess.run(['openssl','pkcs12','-in',str(alice/'browser.p12'),'-nokeys','-passin','stdin'],input=password.read_bytes(),capture_output=True,timeout=15)
            assert exported.returncode==0
            def fingerprints(pem):return {hashlib.sha256(ssl.PEM_cert_to_DER_cert(cert)).hexdigest() for cert in re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----',pem,re.S)}
            assert fingerprints((alice/'tls.crt').read_text()).issubset(fingerprints(exported.stdout.decode()))
            assert (alice/'ca.crt').read_bytes()==(bundle/'authority/ca.crt').read_bytes()

            # Registry detail is public material; private export works from embedded configs.
            alice_record=json.loads((alice/'credential.json').read_text());cid=alice_record['credential_uid']
            item=cli('get','identities','Alice','--config',op_config);assert item['identities'][alice_record['identity_uid']]['name']=='Alice' and any(c['uid']==cid for c in item['credentials'])
            human=cli('get','identities',alice_record['identity_uid'],'--config',op_config,output_format=None).stdout;assert 'Registered certificates:' in human and cid in human
            cert=cli('get','credentials',cid,'--config',op_config);assert cert['credential']['uid']==cid and cert['local_credential']['matches_selected_config']==False
            assert (alice/'token').read_text().strip() not in json.dumps(cert) and 'PRIVATE KEY' not in json.dumps(cert) and 'token_sha256' not in json.dumps(cert)
            assert request(alice/'config.json','/api/v1/credentials/'+cid)[0]==403
            assert cli('get','credentials','--identity','Alice','--config',op_config)['credentials'][0]['uid']==cid
            public=root/'alice-public.crt';cli('export', 'credentials','--id',cid,'--output',public,'--config',op_config)
            assert fingerprints(public.read_text())==fingerprints((alice/'tls.crt').read_text());assert public.stat().st_mode&0o077==0
            assert cli('export', 'credentials','--id',cid,'--output',public,'--config',op_config,check=False).returncode!=0
            inline=root/'inline.p12';key=alice/'tls.key';held=alice/'tls.key-held';key.rename(held)
            try:cli('export', 'credentials','--current','--config',alice/'config.json','--output',inline,'--password-file',password)
            finally:held.rename(key)
            decoded=subprocess.run(['openssl','pkcs12','-in',str(inline),'-nokeys','-passin','stdin'],input=password.read_bytes(),capture_output=True,timeout=15);assert decoded.returncode==0 and fingerprints((alice/'tls.crt').read_text()).issubset(fingerprints(decoded.stdout.decode()))
            assert inline.stat().st_mode&0o077==0
            directory_p12=root/'directory.p12'
            cli('export','credentials','--directory',alice,'--output',directory_p12,'--password-file',password)
            decoded_directory=subprocess.run(['openssl','pkcs12','-in',str(directory_p12),'-nokeys','-passin','stdin'],input=password.read_bytes(),capture_output=True,timeout=15)
            assert decoded_directory.returncode==0 and fingerprints((alice/'tls.crt').read_text()).issubset(fingerprints(decoded_directory.stdout.decode()))
            assert directory_p12.stat().st_mode&0o077==0

            ca_out=root/'export-ca.crt';ca_result=cli('export', 'credentials','--ca','--config',alice/'config.json','--output',ca_out)
            assert ca_out.read_bytes()==(alice/'ca.crt').read_bytes() and not ca_result['contains_private_key'] and len(ca_result['certificates'])>=1
            assert cli('export', 'credentials','--ca','--config',alice/'config.json','--output',ca_out,check=False).returncode!=0
            guide=root/'gui-public';info=cli('auth', 'gui-login','--config',alice/'config.json','--output-dir',guide)
            assert set(p.name for p in guide.iterdir())=={'README.md','ca.crt'} and not info['contains_private_credentials']
            text=(guide/'README.md').read_text();assert 'workflow-test' in text and 'Alice' in text and base in text and 'auth status' in text
            assert (alice/'token').read_text().strip() not in text and 'PRIVATE KEY' not in text and password.read_text().strip() not in text
            assert cli('auth', 'gui-login','--config',alice/'config.json','--output-dir',guide,check=False).returncode!=0
            assert cli('auth', 'gui-login','--config',admin,'--output-dir',root/'bootstrap-guide',check=False).returncode!=0 and not (root/'bootstrap-guide').exists()
            delivery=root/'gui-private';package=cli('--context','alice','auth', 'gui-login','--output-dir',delivery,'--with-credentials','--password-file',password,'--language','en',home=cli_home)
            assert package['contains_private_credentials'] and package['identity']['name']=='Alice' and set(p.name for p in delivery.iterdir())=={'README.md','ca.crt','client.p12','token.txt'}
            text=(delivery/'README.md').read_text();assert 'Converyn GUI login guide' in text and base in text and 'Alice' in text
            assert (delivery/'token.txt').read_text()==(alice/'token').read_text().strip() and (delivery/'token.txt').read_text() not in text and password.read_text().strip() not in text and 'PRIVATE KEY' not in text
            assert delivery.stat().st_mode&0o077==0 and all(p.stat().st_mode&0o077==0 for p in delivery.iterdir())
            pem_result=subprocess.run(['openssl','pkcs12','-in',str(delivery/'client.p12'),'-nodes','-passin','stdin'],input=password.read_bytes(),capture_output=True,check=True,timeout=15)
            private_pem=root/'delivery-client.pem';private_pem.write_bytes(pem_result.stdout);private_pem.chmod(0o600)
            delivery_config=root/'delivery-config.json';delivery_config.write_text(json.dumps({'version':1,'current_context':'delivery','contexts':{'delivery':{'cluster':'workflow-test','servers':bases,'credentials':{'ca_pem':(delivery/'ca.crt').read_text(),'certificate_pem':'\n'.join(re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----',private_pem.read_text(),re.S)), 'private_key_pem':private_pem.read_text(),'token':(delivery/'token.txt').read_text().strip()}}}}));delivery_config.chmod(0o600)
            assert request(delivery_config,'/api/v1/identity/me')[1]['identity']['uid']==alice_record['identity_uid']
            failed=root/'gui-failed';bad_password=root/'missing-password'
            assert cli('auth', 'gui-login','--config',alice/'config.json','--output-dir',failed,'--with-credentials','--password-file',bad_password,check=False).returncode!=0 and not failed.exists()
            assert not list(root.glob('.converyn-gui-login-*'))

            wrong_ca=root/'wrong-ca';cli('admin','pki-init','--directory',wrong_ca,'--name','unrelated-test-authority')
            rejected=cli('rotate', 'credentials','--config',op_config,'--authority',wrong_ca,'--identity',json.loads((alice/'credential.json').read_text())['identity_uid'],'--output',root/'untrusted',check=False)
            assert rejected.returncode != 0
            unsafe_registration=json.loads((alice/'operation.json').read_text())
            unsafe_registration['credential']['certificate_pem']+=(alice/'tls.key').read_text()
            audit_before=len(request(op_config,'/api/v1/audit')[1]['records'])
            assert request(op_config,'/api/v1/identities',unsafe_registration)[0]==422
            assert len(request(op_config,'/api/v1/audit')[1]['records'])==audit_before
            alice_config=alice/'config.json';bob_config=bob/'config.json';alice_id=json.loads((alice/'credential.json').read_text())['identity_uid']
            config_file=root/'node-group.json';config_file.write_text(json.dumps({'action':'create_node_group','expected_revision':0,'group':{'uid':'hosts','name':'Test hosts','revision':1,'node_uids':['node-a']}}))
            bootstrap=root/'bootstrap.json';cli('admin','bootstrap','--config',op_config,'--node','node-a','--output',bootstrap)
            result_file=root/'result.txt';tasks=root/'tasks';tasks.mkdir();tasks.chmod(0o755)
            template=root/'template.json';template.write_text(json.dumps({'node_uid':'node-a','cluster':'workflow-test','servers':bases,'state_dir':str(root/'agent-state'),'token_file':'unused','tls':{'cert':'unused','key':'unused','ca':'unused'},'execution':{'uid':65534,'gid':65534,'mode':'process','work_root':str(tasks)}}))
            hardware=root/'hardware-id';hardware.write_text('isolated-workflow-test-hardware');hardware.chmod(0o600)
            node=root/'agent.json';cli('admin','enroll','--bootstrap',bootstrap,'--state-dir',root/'agent-state','--output',node,'--template',template,'--hardware-id-file',hardware)
            cli('update', 'node-groups','--config',op_config,'--file',config_file)
            start('agent','--config',node)
            until(lambda:'node-a' in request(op_config,'/api/v1/nodes')[1].get('nodes',{}))
            assert 'membership' in cli('get','members','--config',op_config)
            assert 'run_sequence' in cli('get','history','--config',op_config)
            assert isinstance(cli('get','audit','--limit','1','--config',op_config),dict)
            assert 'targets' in cli('get','targets','--config',op_config)
            if a.plugin_input_only:
                from plugin_input_e2e import check as check_plugin_input
                report={'passed':True,'controllers':a.controllers,**check_plugin_input(root,op_config,cli,request,until,controllers,start,children)}
                if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report));return
            if a.secrets_only:
                from stored_secrets_e2e import check as check_secrets
                report={'passed':True,'controllers':a.controllers,**check_secrets(root,bundle,op_config,node,template,cli,request,until,controllers,start,bases)}
                if a.chrome:
                    browser(root,base,bundle,op_config,a.chrome,a.browser_report,script='stored_secrets_browser.mjs',extra={'secret_input':str(root/'secret-input')})
                    report['browser']=json.loads(a.browser_report.read_text())
                if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report));return
            if a.artifacts_only:
                from managed_artifacts_e2e import check as check_artifacts
                report={'passed':True,'controllers':a.controllers,**check_artifacts(root,bundle,op_config,node,template,cli,request,until,controllers,start,bases)}
                if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report));return
            remote_plugins=cli('get','plugins','--config',op_config)
            local_plugins=cli('get','plugins','--local')
            assert remote_plugins['manifest_sha256']==local_plugins['manifest_sha256']
            if local_plugins['plugins']:
                plugin_id=local_plugins['plugins'][0]['identity']['id']
                assert cli('get','plugins',plugin_id,'--config',op_config)['plugins']==cli('get','plugins',plugin_id,'--local')['plugins']
            if a.directory_only:
                from directory_e2e import check as check_directory
                effects=root/'directory-effects';effects.mkdir();os.chown(effects,65534,65534);effects.chmod(0o700)
                report={'passed':True,'controllers':a.controllers,**check_directory(root,effects,op_config,cli,request,until,controllers,start,bases)}
                fingerprint=hashlib.sha256()
                with binary.open('rb') as stream:
                    for block in iter(lambda:stream.read(1024*1024),b''):fingerprint.update(block)
                report['binary_sha256']=fingerprint.hexdigest()
                if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report));return
            if a.outputs_only:
                from outputs_e2e import check as check_outputs
                source=root/'source';source.mkdir();(source/'config.txt').write_text('initial configuration\n')
                effects=root/'effects';effects.mkdir();os.chown(effects,65534,65534);effects.chmod(0o700)
                report={'passed':True,'controllers':a.controllers,**check_outputs(root,bundle,source,effects,op_config,cli,request,until,controllers,start,bases)}
                if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report));return
            assert 'node-a' in cli('get','nodes',home=cli_home)['nodes']
            assert cli('get','nodes','--context','bob',home=cli_home,check=False).returncode!=0
            assert cli('auth','status','--context','bob',home=cli_home)['bindings']
            assert 'node-a' in cli('get','nodes',config_env=portable)['nodes']
            assert json.loads(cli('config','current-context',home=cli_home).stdout)=='alice'
            imported=cli('config','import',bob/'config.json','--name','bob','--replace','--use',home=cli_home)
            assert imported['current_context']=='bob'
            cli('config','use-context','alice',home=cli_home)
            # Bootstrap must also use in-memory CA material from a selected context.
            cli('config','import',op/'config.json','--name','op',home=cli_home)
            cli('--context','op','admin','bootstrap','--node','context-proof-node','--output',root/'context-bootstrap.json',home=cli_home)
            context_person=root/'context-person'
            created=cli('--context','op','create', 'identities','--authority',bundle/'user-authority','--name','ContextPerson','--output',context_person,home=cli_home)
            assert request(context_person/'config.json','/api/v1/identity/me')[0]==200
            context_rotated=root/'context-person-new'
            cli('--context','op','rotate', 'credentials','--authority',bundle/'user-authority','--identity',created['identity_uid'],'--output',context_rotated,home=cli_home)
            assert request(context_rotated/'config.json','/api/v1/identity/me')[0]==200
            source=root/'source';source.mkdir();(source/'config.txt').write_text('workflow proof\n');(source/'check.sh').write_text('printf "version=%s\\n" "$VERSION"\n')
            effects=root/'effects';effects.mkdir();os.chown(effects,65534,65534);effects.chmod(0o700)
            effect_file=effects/'executions'
            if a.controllers==3:(source/'check.sh').write_text('printf "execution\\n" >> '+str(effect_file)+'\nprintf "version=%s\\n" "$VERSION"\nsleep 4\n')
            # More than a receipt tail, less than one immutable upload chunk; observe it while running.
            with (source/'check.sh').open('a') as script:
                script.write("i=0; while [ $i -lt 600 ]; do printf 'line-%04d-中文-log\\n' $i; i=$((i+1)); done\nsleep 8\nprintf 'version=%s\\n' \"$VERSION\"\n")
            manifest={'inventory':{'nodes':[{'uid':'node-a','address':'127.0.0.1','hostname':socket.gethostname()}]},'uid':'demo','name':'Demo workflow','targets':{'qa':{'node_group_uid':'hosts','hosts':'all'}},'scripts':{'verify':'check.sh'},'files':{'write':'config.txt'},'playbook':{'api_version':'converyn/v1','name':'demo','steps':[{'id':'write','phase':'rollout','scope':'per_node','operation':{'kind':'file','path':str(result_file),'mode':420}},{'id':'verify','phase':'verify','scope':'per_node','operation':{'kind':'exec','timeout_seconds':30}}]},'parameters':{'VERSION':{'kind':'string','default':'one','choices':['one','two'],'bindings':[{'step_id':'verify','path':'/environment/VERSION'}]}}}
            if a.chrome:
                manifest['parameters'].update({
                    'RETRIES': {'kind':'integer','default':2,'minimum':1,'maximum':3,'bindings':[{'step_id':'verify','path':'/environment/RETRIES'}]},
                    'NOTE': {'kind':'string','default':'ok','max_length':6,'bindings':[{'step_id':'verify','path':'/environment/NOTE'}]},
                    'ENABLED': {'kind':'boolean','default':False,'bindings':[{'step_id':'verify','path':'/environment/ENABLED'}]},
                })
            if a.chrome:
                manifest['parameters']['CONFIG_CONTENT']={'kind':'string','default':'workflow proof\n','max_length':65536,'bindings':[{'step_id':'write','path':'/content'},{'step_id':'verify','path':'/environment/CONFIG_SHA256','transform':'sha256'}]}
                check=source/'check.sh';check.write_text("printf '%s  %s\\n' \"$CONFIG_SHA256\" "+str(result_file)+" | sha256sum --check --status\n"+check.read_text())
            (source/'values.yaml').write_text('REGION: local\nPORT: 80\nimage:\n  repository: fixed\n  tag: v1\n')
            manifest.update(values='values.yaml',immutable=['REGION','image.repository'],node_values={'node-a':{'PORT':81}})
            workflow=source/'workflow.json';workflow.write_text(json.dumps(manifest))
            cli('validate', 'workflows','--file',workflow);cli('register', 'workflows','--config',op_config,'--file',workflow)
            (source/'empty.sh').write_text("printf 'parameter-ui-proof\\n'\n")
            empty={'inventory':{'nodes':[{'uid':'node-a','address':'127.0.0.1','hostname':socket.gethostname()}]},'uid':'zz-empty-values','name':'Empty variables','targets':{'qa':{'node_group_uid':'hosts','hosts':'all'}},'scripts':{'verify':'empty.sh'},'playbook':{'api_version':'converyn/v1','name':'empty-values','steps':[{'id':'verify','phase':'verify','scope':'per_node','operation':{'kind':'exec','timeout_seconds':30}}]}}
            empty_file=source/'empty.json';empty_file.write_text(json.dumps(empty));cli('register', 'workflows','--config',op_config,'--file',empty_file)

            restricted=context_rotated/'config.json'
            assert request(restricted,'/api/v1/workflows')[1]['workflows']==[]
            assert request(restricted,'/api/v1/workflows/demo')[0]==403
            assert request(restricted,'/api/v1/nodes')[0]==403
            group_uid='restricted-team'
            cli('auth','group',group_uid,'--config',op_config,'--name','Restricted team','--member',created['identity_uid'])
            cli('auth','grant','restricted-qa','--config',op_config,'--group',group_uid,'--role','viewer','--workflow','demo','--preset','qa')
            assert request(restricted,'/api/v1/workflows/demo')[0]==200
            assert set(request(restricted,'/api/v1/workflows/demo')[1]['definition']['targets'])=={'qa'}
            assert cli('auth','can-i','view','--config',restricted,'--workflow','demo','--preset','qa')['allowed']
            assert not cli('auth','can-i','execute','--config',restricted,'--workflow','demo','--preset','qa',check=False)['allowed']
            assert not cli('auth','can-i','view','--config',restricted,'--workflow','demo','--preset','prod',check=False)['allowed']
            denied_preview={'workflow_revision':request(op_config,'/api/v1/workflows/demo')[1]['workflow']['current'],'target':'qa','parameters':{},'request_id':'unauthorized-preview'}
            assert request(restricted,'/api/v1/workflows/demo/preview',denied_preview)[0]==403
            cli('auth','grant','temporary-view','--config',op_config,'--person',created['identity_uid'],'--role','viewer')
            page=request(restricted,'/api/v1/workflows?limit=1')[1]
            assert len(page['workflows'])==1 and page['next_cursor']
            old_cursor=page['next_cursor']
            cli('auth','revoke','temporary-view','--config',op_config)
            assert request(restricted,'/api/v1/workflows?limit=1&after='+urllib.parse.quote(old_cursor))[0]==400
            assert len(request(restricted,'/api/v1/workflows?limit=1')[1]['workflows'])==1
            workflows=cli('get','workflows',home=cli_home)
            assert workflows['workflows'][0]['workflow']['uid']=='demo'
            table=cli('get','workflows',home=cli_home,output_format=None).stdout
            assert all(text in table for text in ['ID','NAME','ENABLED','demo','Demo workflow','qa','VERSION'])
            assert cli('get','workflows','demo',home=cli_home)['workflow']['enabled']
            (root/'values-one.yaml').write_text('PORT: 90\nimage:\n  tag: v2\n')
            (root/'values-two.json').write_text(json.dumps({'PORT':91}))
            values_preview=cli('run', 'workflows','demo','-f',root/'values-one.yaml','-f',root/'values-two.json','--set','PORT=92,NEW_VARIABLE=true','--set','image.tag=v3','--set-string','CODE=123','--dry-run',home=cli_home)
            assert values_preview['parameters']['PORT']==92 and values_preview['parameters']['image']=={'repository':'fixed','tag':'v3'}
            assert values_preview['parameters']['NEW_VARIABLE'] is True and values_preview['parameters']['CODE']=='123'
            assert all('NEW_VARIABLE' in script and 'v3' in script for script in values_preview['rendered_scripts'].values())
            assert cli('run', 'workflows','demo','--set','REGION=changed','--dry-run',home=cli_home,check=False).returncode!=0
            assert cli('run', 'workflows','demo','--set','image.repository=changed','--dry-run',home=cli_home,check=False).returncode!=0
            preview=cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first','--dry-run',home=cli_home)
            listing=request(alice_config,'/api/v1/workflows')[1];assert listing['workflows'][0]['workflow']['uid']=='demo'
            detail=request(alice_config,'/api/v1/workflows/demo')[1];assert 'reconciliation' in detail
            api_preview=request(alice_config,'/api/v1/workflows/demo/preview',{k:preview['request'][k] for k in ['workflow_revision','target','parameters','request_id']})[1]
            assert api_preview['request']==preview['request']
            assert api_preview['rendered_scripts']==preview['rendered_scripts']
            verify_preview=next(w for w in preview['plan']['work'] if w['step_id']=='verify')
            assert verify_preview['operation']['environment']['PORT']=='81'

            assert request(op_config,'/api/v1/workflows/register',{'workflow':detail['definition'],'expected_revision':detail['workflow']['revision']})[0]==200
            status,accepted=request(alice_config,'/api/v1/workflows/demo/runs',preview['request']);assert status==200,accepted
            run_path='/api/v1/runs/run-first'
            def live_output():
                value=request(alice_config,run_path)[1]
                verify=next(w for w in value['plan']['work'] if w['step_id']=='verify')
                code,chunk=request(alice_config,run_path+'/logs/'+verify['id']+'/stdout')
                return (verify,chunk) if code==200 and chunk.get('source')=='live_fragment' and chunk['next_offset']>4096 and value['status']=='running' else None
            live_work,live_chunk=until(live_output,30)
            # Identical previews recover the committed request while its node is occupied.
            repeat=request(alice_config,'/api/v1/workflows/demo/preview',{k:preview['request'][k] for k in ['workflow_revision','target','parameters','request_id']})
            assert repeat[0]==200 and repeat[1]['request']==preview['request'],repeat[0]
            different={k:preview['request'][k] for k in ['workflow_revision','target','parameters','request_id']};different['parameters']={'VERSION':'one'}
            assert request(alice_config,'/api/v1/workflows/demo/preview',different)[0]==422

            # TTY default submission returns immediately; explicit waiting never cancels the Run.
            if a.controllers==1:
                master,slave=pty.openpty()
                submitted_tty=subprocess.run([str(binary),'--config',str(alice_config),'run','workflows','demo','--set','VERSION=two','--request-id','first'],stdin=slave,stdout=slave,stderr=slave,env={**os.environ,'NO_COLOR':'1'},timeout=5)
                assert submitted_tty.returncode==0
                piped=subprocess.run([str(binary),'--config',str(alice_config),'run','workflows','demo','--set','VERSION=two','--request-id','first'],capture_output=True,text=True,timeout=5)
                assert piped.returncode==0 and 'run-first' in piped.stdout and 'steps' not in piped.stderr
                terminal=subprocess.Popen([str(binary),'--config',str(alice_config),'run','workflows','demo','--set','VERSION=two','--request-id','first','--wait'],stdin=slave,stdout=slave,stderr=slave,env={**os.environ,'NO_COLOR':'1'})
                os.close(slave);terminal_output=b''
                try:
                    deadline=time.monotonic()+10
                    while time.monotonic()<deadline and terminal.poll() is None:
                        if select.select([master],[],[],.1)[0]:
                            try:terminal_output+=os.read(master,65536)
                            except OSError:break
                        if b'steps' in terminal_output:break
                    assert b'steps' in terminal_output,terminal_output.decode(errors='replace')
                    terminal.send_signal(signal.SIGINT)
                    assert terminal.wait(timeout=5)==130
                    assert b'\x1b[' not in terminal_output
                    assert request(alice_config,run_path)[1]['status']!='canceled'
                finally:
                    if terminal.poll() is None:terminal.kill();terminal.wait()
                    os.close(master)
                timeout=subprocess.run([str(binary),'-o','json','--config',str(alice_config),'watch', 'runs','run-first','--timeout-seconds','1'],capture_output=True,text=True,timeout=5)
                assert timeout.returncode==124 and json.loads(timeout.stdout)['wait_timed_out']

            follow_output=root/'follow.ndjson';follow_errors=root/'follow.stderr'
            with follow_output.open('w') as out,follow_errors.open('w') as err:
                follower=subprocess.Popen([str(binary),'-o','json','--config',str(alice_config),'logs', 'runs','run-first','--step','verify','--follow'],stdout=out,stderr=err)
                until(lambda:follow_output.stat().st_size>100,5)
                follower.send_signal(__import__('signal').SIGINT);assert follower.wait(timeout=5)==0,follow_errors.read_text()
            assert request(alice_config,run_path)[1]['status']=='running'
            assert all(json.loads(line)['work']['step_id']=='verify' for line in follow_output.read_text().splitlines())
            if a.controllers==3:
                until(lambda:effect_file.exists())
                def leader():
                    for index,address in enumerate(bases):
                        if controllers[index].poll() is None and request(admin,'/api/v1/controlplane/metrics',address=address)[1]['metrics']['state']=='Leader':return index+1
                    return None
                killed_leader=until(leader)
                controllers[killed_leader-1].kill();controllers[killed_leader-1].wait()
            run=until(lambda:(lambda value:value if value.get('status') in ['succeeded','failed','needs_attention'] else None)(request(alice_config,'/api/v1/runs/run-first')[1]),60)
            assert run['status']=='succeeded',run
            if a.controllers==3:assert effect_file.read_text().splitlines()==['execution']
            wrong_token=root/'wrong-token';wrong_token.write_text('0'*64);wrong_token.chmod(0o600)
            invalid_config=root/'invalid-config.json';invalid=json.loads(alice_config.read_text());invalid['contexts'][invalid['current_context']]['credentials']['token']='0'*64;invalid_config.write_text(json.dumps(invalid));invalid_config.chmod(0o600)
            start_time=time.monotonic();denied=cli('get','runs','run-first','--config',invalid_config,check=False)
            assert denied.returncode!=0 and time.monotonic()-start_time<4

            # The new discovery/overview works for an executor over real mTLS.
            overview=cli('status',home=cli_home)
            assert overview['business_reads']=='quorum-confirmed'
            assert overview['controller_observation']['metrics']['current_leader'] is not None
            assert 'node-a' in overview['nodes'] and len(overview['runs'])==1
            overview_table=cli('status',home=cli_home,output_format=None).stdout
            assert 'quorum-confirmed' in overview_table and 'node-a' in overview_table
            for args,columns in [(('get','runs'),['ID','WORKFLOW','STATUS','ACTOR','run-first','Alice','succeeded']),
                                 (('get','nodes'),['NAME','HEARTBEAT','LAST SEEN','node-a','Online']),
                                 (('get','nodes'),['NAME','HEARTBEAT','node-a']),
                                 (('get','runs','run-first'),['Run: run-first','Status: succeeded','STEP','NODE','OPERATION','EXIT'])]:
                table=cli(*args,home=cli_home,output_format=None).stdout
                assert all(column in table for column in columns),(args,table)
            assert cli('get','runs',home=cli_home)['runs'][0]['id']=='run-first'
            assert cli('get','nodes','node-a',home=cli_home)['nodes']['node-a']['boot_id']
            assert cli('watch', 'runs','run-first',home=cli_home)['condition']=='succeeded'
            final_wait=cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first','--wait',home=cli_home)
            assert final_wait['id']=='run-first' and final_wait['status']=='succeeded'
            assert cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first',home=cli_home)['run_id']=='run-first'

            assert request(restricted,run_path)[0]==200
            assert request(restricted,run_path+'/cancel',{})[0]==403
            cli('auth','revoke','restricted-qa','--config',op_config)
            assert request(restricted,run_path)[0]==403
            assert request(restricted,run_path+'/logs/'+live_work['id']+'/stdout')[0]==403
            assert request(restricted,'/api/v1/runs')[1]['runs']==[]

            assert cli('get','runs','--status','failed',home=cli_home)['runs']==[]
            assert cli('get','runs','--status','succeeded','--limit','1',home=cli_home)['runs'][0]['id']=='run-first'
            assert request(bob_config,'/api/v1/controlplane/members',{})[0]==403
            assert cli('cancel', 'runs','run-first','--context','bob',home=cli_home,check=False).returncode!=0
            assert cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first',home=cli_home)['run_id']=='run-first'
            assert result_file.read_text()=='workflow proof\n';assert run['execution']['actor']['uid']==alice_id;assert run['execution']['parameters']['VERSION']=='two'
            receipt=next(v for k,v in run['attempts'].items() if any(w['id']==k and w['step_id']=='verify' for w in run['plan']['work']))['receipt']
            assert receipt['outcome']=='succeeded',receipt
            assert request(alice_config,'/api/v1/workflows/demo/runs',preview['request'])[1]['run_id']=='run-first'
            assert cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first',home=cli_home)['run_id']=='run-first'
            cli('run', 'workflows','--config',os.path.relpath(alice_config),'demo','--set','VERSION=two','--request-id','first')
            conflict=cli('run', 'workflows','--config',alice_config,'demo','--set','VERSION=one','--request-id','first',check=False)
            assert conflict.returncode != 0
            manifest['name']='Demo workflow changed';workflow.write_text(json.dumps(manifest));cli('register', 'workflows','--config',op_config,'--file',workflow)
            cli('run', 'workflows','--config',alice_config,'demo','--set','VERSION=two','--request-id','first')
            manifest['name']='Demo workflow';workflow.write_text(json.dumps(manifest));cli('register', 'workflows','--config',op_config,'--file',workflow)
            bad=dict(preview['request']);bad['request_id']='bad';bad['parameters']={'REGION':'other'};assert request(alice_config,'/api/v1/workflows/demo/runs',bad)[0]==422
            for route in ['/api/v1/runs','/api/v1/scripts','/api/v1/catalog/commands','/api/v1/deployments']:
                assert request(alice_config,route,{})[0] in [401,403,404,405]
                assert request(op_config,route,{})[0] in [401,403,404,405]
            pending=cli('run', 'workflows','--config',alice_config,'demo','--request-id','pending','--dry-run')['request']
            # Do not submit another operation merely to test authorization: the
            # terminal original is sufficient for the cross-person control gate.
            assert request(bob_config,'/api/v1/runs/run-first/pause',{'paused':True,'expected_revision':run['revision']})[0]==403
            audit=request(alice_config,'/api/v1/audit')[1]['records'];assert any(r.get('principal')==alice_id and r.get('execution',{}).get('actor',{}).get('uid')==alice_id for r in audit if isinstance(r.get('execution'),dict))
            if a.chrome:
                browser(root,base,bundle,alice_config,a.chrome,a.browser_report,op_config)
                assert result_file.read_text()=='temporary browser configuration\n'
                current=request(alice_config,'/api/v1/workflows/demo')[1]
                assert current['definition']['parameters']['CONFIG_CONTENT']['default']=='workflow proof\n'
                browser_run=json.loads(a.browser_report.read_text())['actual_gui_run']
                actual=request(alice_config,'/api/v1/runs/'+browser_run)[1]
                assert actual['execution']['parameters']['CONFIG_CONTENT']=='temporary browser configuration\n'
                assert actual['execution']['parameters']['BROWSER_VARIABLE']=='from-gui'
                verify_work=next(w for w in actual['plan']['work'] if w['step_id']=='verify')
                rendered=request(alice_config,'/api/v1/runs/'+browser_run+'/work/'+verify_work['id']+'/definition')[1]
                assert "export BROWSER_VARIABLE='from-gui'" in rendered['rendered_content']
                assert "export PORT='81'" in rendered['rendered_content']

                file_work=next(w for w in actual['plan']['work'] if w['step_id']=='write')
                body=request(alice_config,'/api/v1/runs/'+browser_run+'/work/'+file_work['id']+'/definition')[1]
                assert body['content']==result_file.read_text() and body['sha256']==hashlib.sha256(result_file.read_bytes()).hexdigest()

            # Full reads use final stored logs and cannot silently accept a receipt tail.
            def full_logs():
                try:return cli('logs', 'runs','run-first','--step','verify','--full','--config',alice_config)
                except AssertionError:return None
            complete=until(full_logs,20)
            mixed=cli('logs', 'runs','run-first','--full','--config',alice_config);assert len(mixed)==2 and any(v['read']['stdout']['state']['condition']=='native_no_text' for v in mixed.values())
            full_text=complete[live_work['id']]['read']['stdout']['text'];assert 'line-0000-中文-log' in full_text and 'line-0599-中文-log' in full_text and len(full_text.encode())>4096
            default=cli('logs', 'runs','run-first','--step','verify','--node','node-a','--config',alice_config)
            tail_text=default[live_work['id']]['read']['stdout']['text'];assert 'line-0000' not in tail_text and 'line-0599' in tail_text
            export=root/'exported-logs';cli('logs', 'runs','run-first','--step','verify','--full','--output-dir',export,'--config',alice_config)
            assert (export/(live_work['id']+'.stdout.log')).read_text()==full_text
            before={p.name:p.read_bytes() for p in export.iterdir()};assert cli('logs', 'runs','run-first','--step','verify','--full','--output-dir',export,'--config',alice_config,check=False).returncode!=0;assert before=={p.name:p.read_bytes()for p in export.iterdir()}
            assert cli('logs', 'runs','run-first','--step','no-such-step','--config',alice_config,check=False).returncode!=0
            assert run['submitted_ms']>0 and run['finished_ms']>=run['submitted_ms']
            dates=request(alice_config,'/api/v1/runs')[1]['runs']
            assert dates==sorted(dates,key=lambda r:r['submitted_ms'] or 0,reverse=True)
            assert next(r for r in dates if r['id']=='run-first')['finished_ms']==run['finished_ms']
            asset=until(lambda:(lambda n:n if n.get('inventory') and n.get('resources') else None)(request(alice_config,'/api/v1/nodes/node-a')[1]['nodes']['node-a']),30)
            assert asset['hardware_sha256'] and asset['inventory_checked_ms'] and asset['inventory']['data']['hostname']
            rotated=root/'alice-new';cli('rotate', 'credentials','--config',op_config,'--authority',bundle/'user-authority','--identity',alice_id,'--output',rotated)
            assert request(alice_config,'/api/v1/identity/me')[0]==401
            assert request(alice_config,run_path+'/logs/'+live_work['id']+'/stdout')[0]==401
            assert request(rotated/'config.json','/api/v1/identity/me')[0]==200
            cli('config','import',rotated/'config.json','--name','alice','--replace',home=cli_home)
            assert cli('run', 'workflows','demo','--set','VERSION=two','--request-id','first',home=cli_home)['run_id']=='run-first'
            # Resume checks the persisted operation instead of generating a key.
            cli('resume', 'credentials','--config',op_config,'--output',rotated)
            overlap=root/'alice-overlap';cli('rotate', 'credentials','--config',op_config,'--authority',bundle/'user-authority','--identity',alice_id,'--output',overlap,'--overlap','2s')
            assert request(rotated/'config.json','/api/v1/identity/me')[0]==200
            until(lambda:request(rotated/'config.json','/api/v1/identity/me')[0]==401)
            assert request(overlap/'config.json','/api/v1/identity/me')[0]==200
            # Archived and live history obey the same status filter and retain receipts.
            change={'action':'archive','kind':'run','uid':'run-first','expected_revision':run['revision'],'now_ms':int(time.time()*1000)}
            assert request(op_config,'/api/v1/history',change)[0]==200
            assert cli('get','runs','--config',overlap/'config.json','--status','failed')['runs']==[]
            archived=cli('get','runs','--config',overlap/'config.json','--status','succeeded')['runs']
            original=[r for r in archived if r['id']=='run-first'];assert len(original)==1 and original[0]['archived']
            assert cli('get','runs','run-first','--config',overlap/'config.json')['attempts']
            assert cli('logs', 'runs','run-first','--config',overlap/'config.json')
            logs=cli('logs', 'runs','run-first','--config',overlap/'config.json',output_format=None).stdout
            assert 'version=two' in logs and '[stdout]' in logs
            members=cli('get','members','--config',op_config,output_format=None).stdout
            assert 'ADDRESS' in members and 'true' in members
            table=cli('get', 'identities','--config',op_config,output_format=None).stdout
            assert 'NAME' in table and 'OP' in table and 'Alice' in table
            assert 'hosts' in cli('get', 'node-groups','--config',op_config)['node_groups']
            users=cli('get', 'identities','--config',op_config);assert len(users['identities'])==4
            listed=cli('get', 'credentials','--config',op_config);assert all('token_sha256' not in c and 'certificate_pem' not in c for c in listed['credentials'])
            from outputs_e2e import check as check_outputs
            output_report=check_outputs(root,bundle,source,effects,op_config,cli,request,until,controllers,start,bases)
            cli('disable', 'identities','--config',op_config,alice_id);assert request(overlap/'config.json','/api/v1/identity/me')[0]==401
            op_record=json.loads((op/'credential.json').read_text())
            assert cli('revoke', 'credentials','--config',op_config,op_record['credential_uid'],check=False).returncode!=0
            assert request(op_config,'/api/v1/identity/me')[0]==200
            op_revision=request(op_config,'/api/v1/identities')[1]['identities'][op_record['identity_uid']]['revision']
            revoked=request(op_config,'/api/v1/identities',{'action':'revoke','uid':op_record['credential_uid'],'expected_revision':op_revision,'now_ms':0})
            assert revoked[0]==409,revoked
            assert request(op_config,'/api/v1/identity/me')[0]==200
            assert cli('disable', 'identities','--config',op_config,op_record['identity_uid'],check=False).returncode!=0
            report={'workflow_values_files_and_sets':True,'workflow_values_node_defaults':True,'immutable_values':True,'rendered_scripts':True,'passed':True,'controllers':a.controllers,'killed_leader':killed_leader,'non_idempotent_effects':len(effect_file.read_text().splitlines()) if a.controllers==3 else None,'actual_file_sha256':hashlib.sha256(result_file.read_bytes()).hexdigest(),'run_id':'run-first','actor':alice_id,'parameters':run['execution']['parameters'],'receipt_outcome':receipt['outcome'],'personal_identities':4,'live_sub64k_logs':True,'full_log_bytes':len(full_text.encode()),'log_tail_filter_export_conflict':True,'follow_ctrl_c_preserves_run':True,'node_assets':True,'identity_certificate_details':True,'public_certificate_export':True,'embedded_pkcs12_export':True,'ca_export':True,'gui_login_public_guide':True,'gui_login_private_delivery_real_mtls':True,'gui_login_secrets_separate':True,'cli_tables_and_discovery':True,'executor_cluster_overview':True,'archived_status_filter':True,'portable_contexts':True,'default_config_and_env':True,'inline_admin_issuance':True,'rotation':'immediate and timed overlap verified','legacy_writes':'rejected for executor and administrator'}
            report.update(output_report)
            if a.output:a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps(report))
        finally:
            for child,log in reversed(children):
                child.terminate()
                try:child.wait(timeout=10)
                except subprocess.TimeoutExpired:child.kill();child.wait()
                log.close()
if __name__=='__main__':main()
