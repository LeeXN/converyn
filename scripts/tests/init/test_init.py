"""Context initialization checks; live tests reuse isolated installer mTLS processes."""
import hashlib,json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SCRIPT=ROOT/'scripts/ops/init.sh'
BINARY=Path(os.environ.get('CONVERYN_TEST_BINARY',ROOT/'target/debug/converyn'))
sys.path.insert(0,str(ROOT/'scripts/tests/install'))
import test_installer as installer_fixture

def call(*args,check=True,env=None):
    args=list(map(str,args));prefix=['bash',str(SCRIPT)]
    if args and args[0] in ['import','operator','user','node','browser']:prefix.append(args.pop(0))
    result=subprocess.run([*prefix,'--binary',str(BINARY),*args],capture_output=True,text=True,timeout=90,env=env)
    if check and result.returncode:raise AssertionError(result.stderr)
    return result

def helper(code,*args):return subprocess.run(['bash','-c','source "$1"; shift; '+code,'_',str(SCRIPT),*args],capture_output=True,text=True,timeout=10)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()

class InitializationInputs(unittest.TestCase):
    def test_help_and_shell_syntax_work_without_home(self):
        subprocess.run(['bash','-n',str(SCRIPT)],check=True)
        env=dict(os.environ);env.pop('HOME',None);env.pop('CONVERYN_CONFIG',None)
        result=call('--help',env=env);self.assertIn('--contexts',result.stdout);self.assertIn('--role',result.stdout)
    def test_business_scope_must_be_explicit_and_platform_roles_cannot_be_scoped(self):
        for role,workflow,presets,all_workflows,expected in [
            ('executor','demo',['qa'],'false',0),('viewer','',[],'true',0),
            ('executor','',[],'false',1),('maintainer','demo',['qa'],'false',1),
            ('platform_operator','demo',[],'false',1),('access_admin','',[],'false',0),('unknown','',[],'false',1),
        ]:
            preset='PRESETS=('+(' '.join(presets) if isinstance(presets,list) else '')+')'
            result=helper('ROLE=$1; WORKFLOW=$2; ALL_WORKFLOWS=$3; '+preset+'; validate_scope',role,workflow,all_workflows)
            self.assertEqual(result.returncode,expected,result.stderr)
    def test_private_outputs_do_not_follow_symlinks(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'real').mkdir();(root/'link').symlink_to(root/'real',target_is_directory=True)
            self.assertNotEqual(helper('private_directory "$1"',str(root/'link/issued')).returncode,0)
            self.assertFalse((root/'real/issued').exists())
    def test_resume_refuses_a_changed_request(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'request.tsv'
            self.assertEqual(helper('record_plan "$1" "role=viewer"',str(path)).returncode,0)
            self.assertEqual(helper('RESUME=true; record_plan "$1" "role=viewer"',str(path)).returncode,0)
            self.assertNotEqual(helper('RESUME=true; record_plan "$1" "role=super_admin"',str(path)).returncode,0)
            self.assertEqual(path.read_text(),'role=viewer\n')
    def test_bootstrap_connectivity_is_not_a_personal_context(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);binary=root/'fake-cli';source=root/'bootstrap-config.json';source.write_text('{}');source.chmod(0o600)
            binary.write_text("#!/bin/bash\ncat <<'JSON'\n{\n  \"connected\": true,\n  \"identity\": null\n}\nJSON\n");binary.chmod(0o755)
            contexts=root/'contexts.json'
            result=call('import','--binary',str(binary),'--file',str(source),'--context-name','qa-op','--contexts',str(contexts),'--yes','--non-interactive',check=False)
            self.assertNotEqual(result.returncode,0);self.assertIn('named personal credential',result.stderr);self.assertFalse(contexts.exists())
    def test_node_delivery_requires_private_install_pair_and_explicit_node(self):
        text=SCRIPT.read_text();self.assertIn('Keep init.sh beside install.sh',text)
        result=call('user','--name','Alice','--context-name','qa-alice','--role','executor','--yes','--non-interactive',check=False)
        self.assertNotEqual(result.returncode,0);self.assertIn('Business roles need',result.stderr)

@unittest.skipUnless(os.environ.get('CONVERYN_INSTALL_LIVE')=='1','explicit temporary mTLS fixture')
class LiveInitialization(unittest.TestCase):
    def test_first_operator_from_manual_bundle_and_retry_preserve_identity(self):
        import signal,socket,time
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);bundle=root/'bundle';contexts=root/'contexts.json';issued=root/'operator'
            with socket.socket() as listener:listener.bind(('127.0.0.1',0));port=listener.getsockname()[1]
            subprocess.run([str(BINARY),'admin','init','--directory',str(bundle),'--cluster','manual-init','--member',f'1=127.0.0.1:{port}'],check=True,capture_output=True)
            log=(root/'server.log').open('w')
            server=subprocess.Popen([str(BINARY),'server','--config',str(bundle/'controller-1/server.json')],stdout=log,stderr=log)
            try:
                for _ in range(40):
                    result=subprocess.run([str(BINARY),'--config',str(bundle/'admin/config.json'),'admin','initialize','--members',str(bundle/'members.json')],capture_output=True,timeout=15)
                    if result.returncode==0:break
                    time.sleep(.1)
                self.assertEqual(result.returncode,0)
                args=['--admin-config',str(bundle/'admin/config.json'),'--authority',str(bundle/'user-authority'),'--contexts',str(contexts),'--name','First OP','--context-name','manual-op','--output',str(issued),'--use','--yes','--non-interactive']
                call('operator',*args)
                before=sha(issued/'tls.crt');ca=sha(bundle/'authority/ca.key');call('operator',*args,'--resume')
                self.assertEqual(sha(issued/'tls.crt'),before);self.assertEqual(sha(bundle/'authority/ca.key'),ca)
                result=subprocess.run([str(BINARY),'--config',str(contexts),'--context','manual-op','auth','can-i','access'],capture_output=True)
                self.assertEqual(result.returncode,0)
            finally:
                server.terminate();server.wait(timeout=10);log.close()
    def test_personal_certificates_roles_contexts_and_node_delivery(self):
        def exercise(root,installation):
            contexts=installation/'contexts.json'
            base=['--yes','--non-interactive']
            # This helper can be invoked with the action first; globals remain ordinary CLI flags.
            def init(action,*args,check=True):
                action_options=[]
                if action!='browser':action_options+=['--installation',str(installation)]
                if action in ['import','operator','user']:action_options+=['--contexts',str(contexts)]
                result=subprocess.run(['bash',str(SCRIPT),action,'--binary',str(BINARY),*base,*action_options,*map(str,args)],capture_output=True,text=True,timeout=90)
                if check:self.assertEqual(result.returncode,0,result.stderr)
                return result
            init('import','--context-name','qa-op','--use')
            operator_cert=sha(installation/'operator/tls.crt')
            alice=installation/'users/qa-alice'
            args=['--name','Alice','--context-name','qa-alice','--role','executor','--workflow','demo','--preset','qa','--output',str(alice)]
            init('user',*args)
            leaf=sha(alice/'tls.crt');init('user',*args,'--resume');self.assertEqual(sha(alice/'tls.crt'),leaf)
            managed=json.loads(contexts.read_text());self.assertEqual(set(managed['contexts']),{'qa-op','qa-alice'})
            self.assertEqual(managed['current_context'],'qa-op');self.assertEqual(sha(installation/'operator/tls.crt'),operator_cert)
            def cli(*args,check=True):
                r=subprocess.run([str(BINARY),'--config',str(contexts),'--context','qa-alice',*args],capture_output=True,text=True,timeout=30)
                if check:self.assertEqual(r.returncode,0,r.stderr)
                return r
            cli('auth','can-i','execute','--workflow','demo','--preset','qa')
            self.assertNotEqual(cli('auth','can-i','platform',check=False).returncode,0)
            self.assertNotEqual(cli('auth','can-i','execute','--workflow','other','--preset','qa',check=False).returncode,0)
            recipient=root/'recipient-contexts.json'
            init('import','--file',str(alice/'config.json'),'--context-name','alice','--contexts',str(recipient),'--use')
            delivered=json.loads(recipient.read_text());self.assertEqual(set(delivered['contexts']),{'alice'})
            before=sha(contexts)
            conflict=init('import','--file',str(installation/'operator/config.json'),'--context-name','qa-alice',check=False)
            self.assertNotEqual(conflict.returncode,0);self.assertEqual(sha(contexts),before)
            delivery=installation/'nodes/node-extra'
            init('node','--node','node-extra','--output',str(delivery))
            bootstrap=sha(delivery/'bootstrap.json')
            init('node','--node','node-extra','--output',str(delivery),'--resume')
            self.assertEqual(sha(delivery/'bootstrap.json'),bootstrap)
            subprocess.run(['sha256sum','--check','checksums.sha256'],cwd=delivery,check=True,capture_output=True)
            self.assertEqual(set(p.name for p in delivery.iterdir()),{'bootstrap.json','converyn','install.sh','INSTALL.txt','checksums.sha256'})
            for name in ['bootstrap.json','checksums.sha256']:self.assertEqual((delivery/name).stat().st_mode&0o077,0)
            self.assertFalse((delivery/'authority').exists());self.assertFalse((delivery/'config.json').exists())
            hardware=root/'extra-hardware';hardware.write_text('new-node-delivery-fixture');hardware.chmod(0o600)
            enrolled=subprocess.run([str(BINARY),'admin','enroll','--bootstrap',str(delivery/'bootstrap.json'),'--state-dir',str(root/'extra-state'),'--output',str(root/'extra-agent.json'),'--hardware-id-file',str(hardware)],capture_output=True,text=True,timeout=30)
            self.assertEqual(enrolled.returncode,0,enrolled.stderr)

            # Reader certificates are separately issued, rather than an alias of the administrator.
            viewer=installation/'users/qa-viewer'
            init('user','--name','Reader','--context-name','qa-viewer','--role','viewer','--workflow','demo','--output',str(viewer))
            self.assertNotEqual(sha(viewer/'tls.crt'),leaf)
            password=root/'browser-password';password.write_text('isolated-browser-password\n');password.chmod(0o600)
            browser=installation/'browser-alice'
            init('browser','--file',str(alice/'config.json'),'--output',str(browser),'--password-file',str(password))
            self.assertEqual(set(p.name for p in browser.iterdir()),{'README.md','ca.crt','client.p12','token.txt'})
            self.assertNotIn((browser/'token.txt').read_text(),(browser/'README.md').read_text())
            result=subprocess.run(['openssl','pkcs12','-in',str(browser/'client.p12'),'-nokeys','-passin','file:'+str(password)],capture_output=True)
            self.assertEqual(result.returncode,0)

        installer_fixture.LiveCoordinator.fixture(self,1,after=exercise)

if __name__=='__main__':unittest.main()
