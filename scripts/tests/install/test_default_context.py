"""Operator context handoff regressions use private fixture profiles, never the real home."""
import hashlib,json,os,pwd,subprocess,unittest
from pathlib import Path
from test_installer import SCRIPT,BINARY,LiveCoordinator,sha

@unittest.skipUnless(os.environ.get('CONVERYN_INSTALL_LIVE')=='1','explicit isolated personal-context fixture')
class DefaultContextHandoff(unittest.TestCase):
    def test_import_is_private_idempotent_and_preserves_selection_and_conflicts(self):
        def exercise(root,installation):
            target=root/'personal/config.json'
            env=dict(os.environ);env['CONVERYN_CONFIG']=str(target)
            for key in ['SUDO_USER','SUDO_UID','SUDO_GID']:env.pop(key,None)
            code='''source "$1";shift;BINARY=$1;DIRECTORY=$2;CLUSTER=shell-live
trap cleanup_cli_stage EXIT
if install_default_context;then exit 0;else exit 1;fi
'''
            def handoff(selected_env=env):
                return subprocess.run(['bash','-c',code,'_',str(SCRIPT),str(BINARY),str(installation)],capture_output=True,text=True,env=selected_env,timeout=30)
            first=handoff();self.assertEqual(first.returncode,0,first.stderr)
            profile=json.loads(target.read_text());self.assertEqual(profile['current_context'],'shell-live')
            self.assertEqual(target.stat().st_mode&0o777,0o600);before=sha(target)
            second=handoff();self.assertEqual(second.returncode,0,second.stderr);self.assertEqual(sha(target),before)
            source=installation/'operator/config.json'
            subprocess.run([str(BINARY),'--config',str(target),'config','import',str(source),'--name','other','--use'],check=True,capture_output=True)
            again=handoff();self.assertEqual(again.returncode,0,again.stderr);self.assertEqual(json.loads(target.read_text())['current_context'],'other')
            altered=root/'altered.json';data=json.loads(source.read_text());data['contexts'][data['current_context']]['credentials']['token']='different-fixture-token-'+'x'*40
            altered.write_text(json.dumps(data));altered.chmod(0o600)
            conflict=root/'conflict/config.json'
            subprocess.run([str(BINARY),'--config',str(conflict),'config','import',str(altered),'--name','shell-live'],check=True,capture_output=True)
            before=sha(conflict);failed=handoff({**env,'CONVERYN_CONFIG':str(conflict)})
            self.assertNotEqual(failed.returncode,0);self.assertEqual(sha(conflict),before)
            if os.geteuid()==0:
                account=pwd.getpwuid(65534);root.chmod(0o755);home=root/'sudo-caller';home.mkdir();home.chmod(0o700);os.chown(home,account.pw_uid,account.pw_gid)
                caller_target=home/'.converyn/config.json'
                result=handoff({**env,'CONVERYN_CONFIG':str(caller_target),'SUDO_USER':account.pw_name,'SUDO_UID':str(account.pw_uid),'SUDO_GID':str(account.pw_gid)})
                self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(caller_target.stat().st_uid,account.pw_uid);self.assertEqual(caller_target.stat().st_mode&0o777,0o600)
        LiveCoordinator.fixture(self,1,after=exercise)

if __name__=='__main__':unittest.main()
