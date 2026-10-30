"""Installer checks never mutate host services; live fixtures use temporary mTLS processes."""
import hashlib,json,os,shutil,signal,socket,subprocess,tarfile,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SCRIPT=ROOT/'scripts/ops/install.sh'
BINARY=Path(os.environ.get('CONVERYN_TEST_BINARY',ROOT/'target/debug/converyn'))


def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def shell(*arguments,check=True,env=None):
    result=subprocess.run(['bash',str(SCRIPT),*map(str,arguments)],capture_output=True,text=True,timeout=90,env=env)
    if check and result.returncode:raise AssertionError(result.stderr)
    return result

def helper(code,*arguments):
    return subprocess.run(['bash','-c','source "$1"; shift; '+code,'_',str(SCRIPT),*arguments],capture_output=True,text=True,timeout=10)

class InstallerInputs(unittest.TestCase):
    def test_shell_syntax_and_help_do_not_need_binary_or_network(self):
        subprocess.run(['bash','-n',str(SCRIPT)],check=True)
        text=shell('--help').stdout
        self.assertIn('--prepare-only',text);self.assertIn('--resume',text)
        self.assertNotIn('python',text.lower().split('requires')[0])
    def test_strict_transport_and_controlled_target_syntax(self):
        for target in ['local','root@192.0.2.1','operator@host.example.test','root@[2001:db8::1]']:
            self.assertEqual(helper('validate_target "$1"',target).returncode,0,target)
        for target in ['-oProxyCommand=id','root@host;id','root@$(id)','root@host/other','root@host\nwhoami']:
            self.assertNotEqual(helper('validate_target "$1"',target).returncode,0,target)
        text=SCRIPT.read_text();self.assertIn('StrictHostKeyChecking=$host_check',text)
        self.assertNotIn('StrictHostKeyChecking=no',text);self.assertNotIn('curl -k',text)
    def test_member_and_host_conflicts_are_rejected_before_installation(self):
        cases=[
            ['--member','1=192.0.2.1:8443','--member','2=192.0.2.2:8443'],
            ['--member','1=192.0.2.1:8443','--member','1=192.0.2.2:8443','--member','3=192.0.2.3:8443'],
            ['--member','1=192.0.2.1:8443','--member','2=192.0.2.2:8443','--member','3=192.0.2.3:8443','--controller','1=root@one','--controller','2=user@one','--controller','3=root@three'],
            ['--member','1=127.0.0.1:8443','--agent','a=local','--agent','b=local'],
        ]
        with tempfile.TemporaryDirectory() as directory:
            for i,case in enumerate(cases):
                output=Path(directory)/str(i)
                result=shell('deploy','--binary',BINARY,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--yes','--non-interactive',*case,check=False)
                self.assertNotEqual(result.returncode,0);self.assertFalse(output.exists())
    def test_deploy_requires_explicit_administrator_and_ssh_user_for_derived_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            args=['deploy','--binary',BINARY,'--cluster','fixture','--installation',output,'--member','1=127.0.0.1:8443','--prepare-only','--yes','--non-interactive']
            result=shell(*args,check=False);self.assertNotEqual(result.returncode,0);self.assertIn('--operator-name',result.stderr);self.assertFalse(output.exists())
            args=['deploy','--binary',BINARY,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--prepare-only','--yes','--non-interactive']
            for i in range(1,4):args+=['--member',f'{i}=192.0.2.{i}:8443']
            result=shell(*args,check=False);self.assertNotEqual(result.returncode,0);self.assertIn('--ssh-user',result.stderr);self.assertFalse(output.exists())
    def test_connectivity_check_reports_source_and_unreachable_controller(self):
        with socket.socket() as listener, socket.socket() as blocked:
            listener.bind(('127.0.0.1',0));listener.listen(10);blocked.bind(('127.0.0.1',0))
            opened=listener.getsockname()[1];closed=blocked.getsockname()[1]
            code='''MEMBERS=("1=127.0.0.1:$1" "2=127.0.0.1:$2");CONTROLLERS=("1=local");AGENTS=("n=operator@fixture");SSH_ARGS=()
ssh(){ local target=$1;shift;[[ $target == operator@fixture ]] || return 1;bash -c "$1"; }
check_controller_reachability
'''
            result=helper(code,str(opened),str(closed));self.assertNotEqual(result.returncode,0)
            self.assertIn('local -> 127.0.0.1:'+str(closed),result.stderr);self.assertIn('operator@fixture -> 127.0.0.1:'+str(closed),result.stderr)
            self.assertNotIn('local -> 127.0.0.1:'+str(opened),result.stderr)
    def test_blocked_network_stops_before_creating_operator_and_preserves_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);output=root/'installation';harness=root/'harness.sh'
            harness.write_text('''#!/bin/bash
set -euo pipefail
source "$1";shift
root_host(){ :; }
install_controller(){ :; }
check_controller_reachability(){ log 'TCP unreachable: fixture -> controller';return 1; }
main "$@"
''')
            args=['deploy','--binary',BINARY,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--member','1=127.0.0.1:8443','--controller','1=local','--yes','--non-interactive']
            r=subprocess.run(['bash',str(harness),str(SCRIPT),*map(str,args)],capture_output=True,text=True,timeout=90)
            self.assertNotEqual(r.returncode,0);self.assertIn('TCP unreachable',r.stderr)
            self.assertTrue((output/'bundle.ready').is_file());self.assertTrue((output/'bundle/authority/ca.key').is_file());self.assertFalse((output/'operator').exists())
    def test_download_requires_https_and_reviewed_digest(self):
        for url,digest in [('http://example.test/binary','a'*64),('https://example.test/binary','')]:
            args=['client','--binary-url',url,'--yes','--non-interactive']
            if digest:args+=['--sha256',digest]
            result=shell(*args,check=False)
            self.assertNotEqual(result.returncode,0);self.assertIn('HTTPS',result.stderr)
    def test_download_digest_mismatch_never_publishes_an_executable(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);tools=root/'tools';tools.mkdir();output=root/'install'
            curl=tools/'curl';curl.write_text('#!/bin/bash\nwhile (($#));do if [[ $1 == --output ]];then file=$2;shift;fi;shift;done\nprintf "untrusted bytes" > "$file"\n');curl.chmod(0o755)
            env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH']}
            result=shell('client','--binary-url','https://example.test/binary','--sha256','a'*64,'--installation',output,'--yes','--non-interactive',check=False,env=env)
            self.assertNotEqual(result.returncode,0);self.assertIn('SHA-256 mismatch',result.stderr)
            self.assertFalse((output/'converyn').exists());self.assertEqual((output/'converyn.download').stat().st_mode&0o111,0)
    def test_incompatible_binary_fails_before_creating_an_installation_record(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            result=shell('deploy','--binary','/bin/true','--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--member','1=127.0.0.1:8443','--prepare-only','--yes','--non-interactive',check=False)
            self.assertNotEqual(result.returncode,0);self.assertIn('binary is incompatible',result.stderr);self.assertFalse(output.exists())
    def test_missing_operator_cli_is_rejected_before_bundle_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);binary=root/'converyn'
            binary.write_text('#!/bin/bash\nif [[ $1 == create && $2 == identities ]]; then exit 2; fi\nexec "$CONVERYN_FIXTURE_BINARY" "$@"\n')
            binary.chmod(0o755)
            result=subprocess.run(['bash','-c','source "$1"; BINARY=$2; trap cleanup_binary_probe EXIT; validate_binary_contract','_',str(SCRIPT),str(binary)],capture_output=True,text=True,env={**os.environ,'CONVERYN_FIXTURE_BINARY':str(BINARY)},timeout=30)
            self.assertNotEqual(result.returncode,0)
            self.assertIn("missing 'create identities'",result.stderr)
            self.assertFalse((root/'probe').exists())
    def test_known_hosts_can_be_created_interactively_but_private_keys_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);target=root/'ssh/known_hosts'
            r=helper('TTY=true;KNOWN_HOSTS=$1;prepare_known_hosts',str(target));self.assertEqual(r.returncode,0,r.stderr)
            self.assertTrue(target.is_file());self.assertEqual(target.stat().st_mode&0o777,0o600)
            r=helper('TTY=false;KNOWN_HOSTS=$1;prepare_known_hosts',str(root/'missing'));self.assertNotEqual(r.returncode,0);self.assertFalse((root/'missing').exists())
            sensitive=root/'credentials';sensitive.write_text('-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n');before=sensitive.read_bytes()
            r=helper('TTY=true;KNOWN_HOSTS=$1;prepare_known_hosts',str(sensitive));self.assertNotEqual(r.returncode,0);self.assertEqual(sensitive.read_bytes(),before)
            unrelated=root/'application';unrelated.write_bytes(b'\x7fELF\x02\x00unrelated-binary');before=unrelated.read_bytes()
            r=helper('TTY=true;KNOWN_HOSTS=$1;prepare_known_hosts',str(unrelated));self.assertNotEqual(r.returncode,0);self.assertEqual(unrelated.read_bytes(),before)
    @unittest.skipUnless(os.geteuid()==0,'requires root to simulate a sudo caller')
    def test_new_known_hosts_remains_owned_by_the_sudo_caller(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);root.chmod(0o755);home=root/'caller';home.mkdir();os.chown(home,65534,65534);home.chmod(0o700)
            target=home/'.ssh/known_hosts'
            r=helper('TTY=true;SUDO_UID=65534;SUDO_GID=65534;KNOWN_HOSTS=$1;prepare_known_hosts',str(target))
            self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(target.stat().st_uid,65534);self.assertEqual(target.parent.stat().st_uid,65534)
            self.assertEqual(target.stat().st_mode&0o777,0o600);self.assertEqual(target.parent.stat().st_mode&0o777,0o700)
    def test_wrong_local_digest_is_rejected_before_bundle_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            result=shell('deploy','--binary',BINARY,'--sha256','0'*64,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--member','1=127.0.0.1:8443','--prepare-only','--yes','--non-interactive',check=False)
            self.assertNotEqual(result.returncode,0);self.assertFalse(output.exists())
    def test_private_directory_symlinks_are_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'real').mkdir();(root/'link').symlink_to(root/'real',target_is_directory=True)
            result=helper('private_directory "$1"',str(root/'link/private'))
            self.assertNotEqual(result.returncode,0);self.assertFalse((root/'real/private').exists())
    def test_generated_unit_uses_the_dedicated_account_and_private_umask(self):
        result=helper('controller_unit')
        self.assertEqual(result.returncode,0);self.assertIn('User=converyn-controller',result.stdout)
        self.assertIn('UMask=0077',result.stdout);self.assertIn('server --config /var/lib/converyn-controller/server.json',result.stdout)
    def test_agent_freshness_uses_node_timestamp_not_nested_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);response=root/'response.json';binary=root/'fake-converyn'
            binary.write_text('#!/bin/bash\ncat "'+str(response)+'"\n');binary.chmod(0o755)
            now=int(time.time()*1000)
            for stamp,nested,expected in [(now,0,0),(0,now,1),(now+120000,now,1)]:
                response.write_text(json.dumps({'nodes':{'node':{'inventory':{'data':{'observed_ms':nested}},'observed_ms':stamp}}},indent=2))
                result=helper('BINARY=$1; DIRECTORY=$2; agent_observed node',str(binary),str(root))
                self.assertEqual(result.returncode,expected,result.stderr)
    def test_remote_transfer_excludes_operator_and_offline_authority(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);member=root/'controller-1';member.mkdir()
            for name in ['server.json','tls.key','output-keys.json']:(member/name).write_text('member material')
            (root/'authority').mkdir();(root/'authority/ca.key').write_text('must never ship')
            code='''DIRECTORY=$1; BINARY=/bin/true; SSH_ARGS=(); SCP_ARGS=()
ssh(){ local command=${!#}; printf '%s\\n' "$command" >> "$DIRECTORY/ssh.log"; if [[ $command == *'mktemp -d'* ]];then printf '/tmp/converyn-install.FIXTURE123\\n';fi; }
scp(){ printf '%s\\n' "$*" >> "$DIRECTORY/scp.log"; }
remote_install operator@host controller "$2" "$DIRECTORY/controller.remote"
'''
            result=helper(code,str(root),str(member));self.assertEqual(result.returncode,0,result.stderr)
            with tarfile.open(root/'controller-transfer.tar') as archive:names=archive.getnames()
            self.assertIn('./server.json',names);self.assertFalse(any('authority' in n or 'operator' in n for n in names))
            self.assertIn('--sha256',(root/'ssh.log').read_text());self.assertNotIn('must never ship',(root/'scp.log').read_text())
    def test_preparation_uses_real_native_bundles_and_resume_preserves_ca(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            args=['deploy','--binary',str(BINARY),'--cluster','shell-fixture','--operator-name','Fixture OP','--installation',str(output),
                  '--member','1=127.0.0.1:18443','--controller','1=local','--agent','node=local','--prepare-only','--yes','--non-interactive']
            shell(*args)
            authority=output/'bundle/authority/ca.key';before=sha(authority)
            self.assertEqual(authority.stat().st_mode&0o077,0)
            self.assertEqual(json.loads((output/'bundle/controller-1/server.json').read_text())['id'],1)
            self.assertEqual(sha(output/'install.sh'),sha(SCRIPT))
            shell(*args,'--resume');self.assertEqual(sha(authority),before)
            result=shell(*args,check=False);self.assertNotEqual(result.returncode,0);self.assertEqual(sha(authority),before)
            changed=args[:];changed[changed.index('shell-fixture')]='different'
            self.assertNotEqual(shell(*changed,'--resume',check=False).returncode,0);self.assertEqual(sha(authority),before)
    def test_resume_rejects_changed_membership_without_replacing_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            args=['deploy','--binary',BINARY,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--member','1=127.0.0.1:8443','--prepare-only','--yes','--non-interactive']
            shell(*args);ca=output/'bundle/authority/ca.key';before=sha(ca)
            members=output/'bundle/members.json';members.write_text(members.read_text()+'\n')
            result=shell(*args,'--resume',check=False)
            self.assertNotEqual(result.returncode,0);self.assertIn('membership no longer matches',result.stderr);self.assertEqual(sha(ca),before)
    def test_resume_rejects_an_inconsistent_keyring_without_replacing_the_ca(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            args=['deploy','--binary',BINARY,'--cluster','fixture','--operator-name','Fixture OP','--installation',output,'--ssh-user','operator','--prepare-only','--yes','--non-interactive']
            for i in range(1,4):args+=['--member',f'{i}=192.0.2.{i}:8443']
            shell(*args);ca=output/'bundle/authority/ca.key';before=sha(ca)
            other=output/'bundle/controller-2/output-keys.json';other.write_text('different keyring\n');other.chmod(0o600)
            result=shell(*args,'--resume',check=False)
            self.assertNotEqual(result.returncode,0);self.assertIn('different output keyring',result.stderr)
            self.assertEqual(sha(ca),before);self.assertEqual(other.read_text(),'different keyring\n')
    def test_three_member_offline_preparation_distributes_one_output_keyring(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'install'
            args=['deploy','--binary',BINARY,'--cluster','ha-fixture','--operator-name','Fixture OP','--installation',output,'--ssh-user','operator','--prepare-only','--yes','--non-interactive']
            for i in range(1,4):args+=['--member',f'{i}=192.0.2.{i}:8443']
            shell(*args)
            keys=[sha(output/f'bundle/controller-{i}/output-keys.json') for i in range(1,4)]
            self.assertEqual(len(set(keys)),1)
            leaves=[sha(output/f'bundle/controller-{i}/tls.crt') for i in range(1,4)]
            self.assertEqual(len(set(leaves)),3)


@unittest.skipUnless(os.environ.get('CONVERYN_INSTALL_LIVE')=='1','explicit loopback mTLS fixture')
class LiveCoordinator(unittest.TestCase):
    def fixture(self,count,after=None):
        temporary=tempfile.TemporaryDirectory(prefix='converyn-shell-install-');root=Path(temporary.name)
        output=root/'installation'
        installer=root/'install.sh';shutil.copy2(SCRIPT,installer)
        listeners=[socket.socket() for _ in range(count)]
        for listener in listeners:listener.bind(('127.0.0.1',0))
        ports=[listener.getsockname()[1] for listener in listeners]
        for listener in listeners:listener.close()
        # Replace host service/SSH execution only. Bundle generation, PKI, membership,
        # personal credentials, enrollment, API reads and resume run through production code.
        harness=root/'harness.sh'
        harness.write_text('''#!/bin/bash
set -euo pipefail
source "$1";shift
root_host(){ :; }
remote_preflight(){ :; }
check_controller_reachability(){ :; }
fixture_controller(){
    local profile=$1 pidfile="$1/fixture.pid" pid
    if [[ -f $pidfile ]];then pid=$(cat "$pidfile");if kill -0 "$pid" 2>/dev/null;then return;fi;fi
    "$BINARY" server --config "$profile/server.json" 9>&- > "$profile/fixture.log" 2>&1 &
    printf '%s' "$!" > "$pidfile"
}
install_controller(){ fixture_controller "$BUNDLE"; }
remote_install(){ fixture_controller "$3"; }
install_agent(){
    printf 'isolated-shell-fixture-hardware' > "$DIRECTORY/fixture-hardware"
    cat > "$DIRECTORY/fixture-template.json" <<EOF
{"node_uid":"fixture-node","cluster":"$CLUSTER","servers":["https://${MEMBERS[0]#*=}"],"state_dir":"$DIRECTORY/fixture-agent-state","token_file":"unused","tls":{"cert":"unused","key":"unused","ca":"unused"},"execution":{"uid":65534,"gid":65534,"mode":"process","work_root":"$DIRECTORY/fixture-tasks"}}
EOF
    "$BINARY" admin enroll --bootstrap "$BOOTSTRAP" --state-dir "$DIRECTORY/fixture-agent-state" --output "$DIRECTORY/fixture-agent.json" --template "$DIRECTORY/fixture-template.json" --hardware-id-file "$DIRECTORY/fixture-hardware" >/dev/null
    "$BINARY" agent --config "$DIRECTORY/fixture-agent.json" 9>&- > "$DIRECTORY/fixture-agent.log" 2>&1 &
}
main "$@"
''')
        # Resume must use the same executable even if another check rebuilds target/debug.
        frozen=root/'converyn';shutil.copy2(BINARY,frozen);frozen.chmod(0o755)
        args=['deploy','--binary',str(frozen),'--cluster','shell-live','--operator-name','Fixture OP','--installation',str(output),'--agent','fixture-node=local','--no-default-context','--yes','--non-interactive']
        for i,port in enumerate(ports,1):args+=['--member',f'{i}=127.0.0.1:{port}','--controller',f'{i}='+('local' if count==1 else f'root@controller-{i}')]
        # HA transport is replaced above; the production preflight still requires an existing selector.
        known=root/'known_hosts';known.write_text('fixture transport is overridden\n');args+=['--ssh-known-hosts',str(known)]
        processes=[]
        try:
            def run(extra=()):
                process=subprocess.Popen(['bash',str(harness),str(installer),*args,*extra],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True)
                processes.append(process)
                try:
                    stdout,stderr=process.communicate(timeout=110)
                except subprocess.TimeoutExpired as error:
                    stdout=error.stdout or '';stderr=error.stderr or ''
                    if isinstance(stderr,bytes):stderr=stderr.decode(errors='replace')
                    for name in ['last-check.log','fixture-agent.log']:
                        path=output/name
                        if path.exists():stderr+='\n'+name+': '+path.read_text()[-2000:]
                    status=output/'fixture-agent-state/status.json'
                    if status.exists():
                        value=json.loads(status.read_text());stderr+='\nagent status: '+str({k:value.get(k) for k in ['phase','diagnostic','controller_confirmed']})
                    observation=output/'agent-fixture-node.observation.json'
                    if observation.exists():stderr+='\nobservation field names: '+str(list(json.loads(observation.read_text())))
                    raise AssertionError(stderr) from error
                if process.returncode:
                    for name in ['last-check.log','fixture-agent.log']:
                        path=output/name
                        if path.exists():stderr+='\n'+name+': '+path.read_text()[-2000:]
                self.assertEqual(process.returncode,0,stderr)
                return stdout,stderr
            run()
            self.assertTrue((output/'installation.complete').is_file())
            token=(output/'operator/token').read_text();self.assertGreater(len(token),16)
            before=sha(output/'operator/tls.crt');ca=sha(output/'bundle/authority/ca.key')
            stdout,stderr=run(['--resume'])
            self.assertEqual(sha(output/'operator/tls.crt'),before);self.assertEqual(sha(output/'bundle/authority/ca.key'),ca)
            self.assertNotIn(token,stdout+stderr)
            observation=json.loads((output/'agent-fixture-node.observation.json').read_text())
            self.assertIn('fixture-node',observation['nodes'])
            for i in range(1,count+1):self.assertTrue((output/f'bundle/controller-{i}/fixture.pid').exists())
            if after is not None:after(root,output)
        finally:
            for process in processes:
                try:os.killpg(process.pid,signal.SIGTERM)
                except ProcessLookupError:pass
                if process.poll() is None:
                    try:process.wait(timeout=5)
                    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
            temporary.cleanup()
    def test_single_controller_agent_and_resume_over_real_mtls(self):self.fixture(1)
    def test_three_controllers_agent_and_resume_over_real_mtls(self):self.fixture(3)

if __name__=='__main__':unittest.main()
