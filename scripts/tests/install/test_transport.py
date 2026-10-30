"""Authentication transport checks; no installed services or lab hosts are changed."""
import errno,json,os,posixpath,select,shlex,signal,socket,subprocess,sys,tempfile,threading,time,unittest
from pathlib import Path
from test_installer import ROOT,SCRIPT,helper
try:
    import paramiko
except ImportError:
    paramiko=None


class AuthenticationOptions(unittest.TestCase):
    def test_auth_modes_keep_host_verification_and_share_private_ssh_scp_sockets(self):
        for mode,tty,noninteractive,batch in [('auto','true','false','no'),('auto','false','false','yes'),('auto','true','true','yes'),('key','true','false','no'),('password','true','false','no')]:
            result=helper('SSH_AUTH=$1; TTY=$2; NONINTERACTIVE=$3; KNOWN_HOSTS=/verified/hosts; ssh_arguments; trap ssh_cleanup EXIT; printf "%s\\n" "${SSH_ARGS[@]}" "${SCP_ARGS[@]}"; stat -c %a "$SSH_CONTROL_DIR"',mode,tty,noninteractive)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(result.stdout.count('BatchMode='+batch),2)
            policy='ask' if tty=='true' and noninteractive=='false' else 'yes'
            self.assertEqual(result.stdout.count('StrictHostKeyChecking='+policy),2)
            self.assertEqual(result.stdout.count('ControlMaster=auto'),2)
            self.assertIn('\n700\n',result.stdout)
            if mode=='password':self.assertEqual(result.stdout.count('PubkeyAuthentication=no'),2)
            path=next(line.split('=',1)[1].rsplit('/',1)[0] for line in result.stdout.splitlines() if line.startswith('ControlPath='))
            self.assertFalse(Path(path).exists())
    def test_password_mode_without_terminal_and_invalid_options_are_rejected(self):
        for tty,noninteractive in [('false','false'),('true','true')]:
            r=helper('SSH_AUTH=password; TTY=$1; NONINTERACTIVE=$2; ssh_arguments',tty,noninteractive)
            self.assertNotEqual(r.returncode,0);self.assertIn('interactive terminal',r.stderr)
        for args in [('--ssh-auth','unknown'),('--ssh-auth','password','--ssh-key','/unused')]:
            r=subprocess.run(['bash',str(SCRIPT),'client',*args,'--yes','--non-interactive'],capture_output=True,text=True)
            self.assertNotEqual(r.returncode,0);self.assertIn('--ssh-',r.stderr)
    def test_noninteractive_sudo_failure_does_not_prompt_or_reach_installation(self):
        r=helper('NONINTERACTIVE=true; ssh(){ return 1; }; ask_secret(){ printf SHOULD_NOT_PROMPT; }; remote_sudo_preflight operator@host; printf SHOULD_NOT_INSTALL')
        self.assertNotEqual(r.returncode,0);self.assertIn('sudo password',r.stderr)
        self.assertNotIn('SHOULD_NOT_',r.stdout)
    def test_sudo_retries_and_cleanup_do_not_leave_credentials_or_retry_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            secret=Path(directory)/'secret';secret.write_text('fixture-not-in-logs\n');secret.chmod(0o600)
            code='''TTY=true; NONINTERACTIVE=false; fixture=$(cat "$1"); ssh(){ return 1; }
ask_secret(){ printf -v "$1" '%s' "$fixture"; }
remote_sudo(){ return 1; }
remote_sudo_preflight operator@host
printf SHOULD_NOT_INSTALL
'''
            r=helper(code,str(secret));self.assertNotEqual(r.returncode,0)
            self.assertEqual(r.stderr.count('sudo authentication failed'),3)
            self.assertNotIn(secret.read_text().strip(),r.stdout+r.stderr);self.assertNotIn('SHOULD_NOT_INSTALL',r.stdout)


if paramiko:
    def set_attributes(target,attr):
        # Paramiko's path helper opens size updates with w+, losing uploaded
        # bytes. Both SETSTAT and FSETSTAT must preserve bytes when resizing.
        try:
            if attr._flags & attr.FLAG_SIZE:os.truncate(target,attr.st_size)
            if attr._flags & attr.FLAG_PERMISSIONS:os.chmod(target,attr.st_mode)
            if attr._flags & attr.FLAG_UIDGID:os.chown(target,attr.st_uid,attr.st_gid)
            if attr._flags & attr.FLAG_AMTIME:os.utime(target,(attr.st_atime,attr.st_mtime))
            return paramiko.SFTP_OK
        except OSError as e:return paramiko.SFTPServer.convert_errno(e.errno)

    class SftpServer(paramiko.SFTPServer):
        def finish_subsystem(self):
            # scp waits for the remote subsystem's status after successful SFTP.
            if not self.sock.closed:self.sock.send_exit_status(0)
            super().finish_subsystem()

    class FileHandle(paramiko.SFTPHandle):
        # OpenSSH scp updates metadata through the open handle, not only a path.
        def stat(self):
            try:return paramiko.SFTPAttributes.from_stat(os.fstat(self.readfile.fileno()))
            except OSError as e:return paramiko.SFTPServer.convert_errno(e.errno)
        def chattr(self,attr):
            return set_attributes(self.readfile.fileno(),attr)

    class Files(paramiko.SFTPServerInterface):
        def __init__(self,server,*args,**kwargs):super().__init__(server,*args,**kwargs);self.root=server.fixture.root;self.events=server.fixture.events
        def local(self,path):
            candidate=(self.root/posixpath.normpath('/'+path).lstrip('/')).resolve()
            if not candidate.is_relative_to(self.root):raise PermissionError(errno.EACCES,'outside fixture')
            return candidate
        def canonicalize(self,path):self.events.append('sftp-canonicalize '+path);return posixpath.normpath('/'+path)
        def stat(self,path):
            self.events.append('sftp-stat '+path)
            try:return paramiko.SFTPAttributes.from_stat(self.local(path).stat())
            except OSError as e:return paramiko.SFTPServer.convert_errno(e.errno)
        lstat=stat
        def open(self,path,flags,attr):
            self.events.append('sftp-open '+path)
            try:
                fd=os.open(self.local(path),flags,getattr(attr,'st_mode',None) or 0o600)
                stream=os.fdopen(fd,'r+b' if flags&os.O_RDWR else 'wb' if flags&os.O_WRONLY else 'rb')
                handle=FileHandle(flags);handle.readfile=stream;handle.writefile=stream;return handle
            except OSError as e:return paramiko.SFTPServer.convert_errno(e.errno)
        def chattr(self,path,attr):
            try:return set_attributes(self.local(path),attr)
            except OSError as e:return paramiko.SFTPServer.convert_errno(e.errno)

    class Server(paramiko.ServerInterface):
        def __init__(self,fixture):self.fixture=fixture
        def get_allowed_auths(self,username):return 'publickey,password'
        def check_auth_publickey(self,username,key):
            self.fixture.key_attempts+=1
            return paramiko.AUTH_SUCCESSFUL if username=='operator' and key.asbytes()==self.fixture.client_key.asbytes() else paramiko.AUTH_FAILED
        def check_auth_password(self,username,password):
            self.fixture.auth_attempts+=1
            return paramiko.AUTH_SUCCESSFUL if username=='operator' and password==self.fixture.ssh_password else paramiko.AUTH_FAILED
        def check_channel_request(self,kind,chanid):return paramiko.OPEN_SUCCEEDED if kind=='session' else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
        def check_channel_exec_request(self,channel,command):
            command=command.decode();self.fixture.commands.append(command)
            threading.Thread(target=self.execute,args=(channel,command),daemon=True).start();return True
        def execute(self,channel,command):
            # Return the exec-request acknowledgement before completing a short command.
            time.sleep(.02)
            try:
                self.fixture.events.append('exec-start')
                if command.startswith('test "$(uname -s)"'):
                    channel.sendall((os.uname().machine+'\n').encode());status=0
                elif command.startswith('scp -t '):
                    args=shlex.split(command);destination=(self.fixture.root/args[-1].lstrip('/')).resolve();assert destination.is_relative_to(self.fixture.root)
                    process=subprocess.Popen(['scp','-t',str(destination)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                    def feed():
                        try:
                            while chunk:=channel.recv(4096):process.stdin.write(chunk);process.stdin.flush()
                        finally:process.stdin.close()
                    threading.Thread(target=feed,daemon=True).start()
                    while chunk:=os.read(process.stdout.fileno(),4096):channel.sendall(chunk)
                    errors=process.stderr.read()
                    if errors:channel.sendall_stderr(errors)
                    status=process.wait(timeout=10);process.stdout.close();process.stderr.close()
                else:
                    data=b''
                    while chunk:=channel.recv(4096):data+=chunk
                    self.fixture.events.append('stdin-eof')
                    r=subprocess.run(['bash','-c',command],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=self.fixture.root,env=self.fixture.env,timeout=15)
                    if r.stdout:channel.sendall(r.stdout)
                    if r.stderr:channel.sendall_stderr(r.stderr)
                    status=r.returncode
                self.fixture.events.append('exec-exit '+str(status));channel.send_exit_status(status);channel.shutdown_write()
            except OSError:
                if not self.fixture.stopping:raise
            finally:channel.close()


class SshFixture:
    def __init__(self,root,nopass=False):
        self.root=root;self.auth_attempts=0;self.key_attempts=0;self.commands=[];self.events=[];self.transports=[];self.stopping=False
        self.ssh_password="ssh fixture '$`\\ not shell input"
        self.sudo_password="sudo fixture ' $`\\ ; not argv"
        self.host_key=paramiko.RSAKey.generate(2048)
        self.client_key=paramiko.RSAKey.generate(2048);self.client_key.write_private_key_file(str(root/'client-key'));(root/'client-key').chmod(0o600)
        self.listener=socket.socket();self.listener.bind(('127.0.0.1',0));self.listener.listen();self.listener.settimeout(.1);self.port=self.listener.getsockname()[1]
        tools=root/'tools';tools.mkdir();(root/'incoming').mkdir()
        password_file=root/'fixture-sudo-password';password_file.write_text(self.sudo_password+'\n');password_file.chmod(0o600)
        fake=tools/'sudo'
        fake.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:]
secret=pathlib.Path(os.environ['FIXTURE_SUDO_FILE']).read_text().rstrip('\\n')
assert not any(secret in item for item in sys.argv)
assert 'SUDO_PASSWORDS' not in os.environ and 'sudo_secret' not in os.environ
with pathlib.Path(os.environ['FIXTURE_SUDO_LOG']).open('a') as f:f.write(json.dumps(args)+'\\n')
if os.environ.get('FIXTURE_NOPASS')!='1' and not pathlib.Path(os.environ['FIXTURE_NOPASS_SWITCH']).exists():
 if '-n' in args:sys.exit(1)
 if '-S' not in args or sys.stdin.buffer.readline().rstrip(b'\\n').decode()!=secret:sys.exit(1)
command=args[args.index('--')+1:]
os.execvpe(command[0],command,os.environ)
''');fake.chmod(0o700)
        self.env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH'],'FIXTURE_SUDO_FILE':str(password_file),'FIXTURE_SUDO_LOG':str(root/'sudo-args.jsonl'),'FIXTURE_NOPASS_SWITCH':str(root/'nopass-switch'),'FIXTURE_NOPASS':'1' if nopass else '0'}
        threading.Thread(target=self.accept,daemon=True).start()
    def accept(self):
        while not self.stopping:
            try:conn,_=self.listener.accept()
            except socket.timeout:continue
            except OSError:break
            threading.Thread(target=self.serve,args=(conn,),daemon=True).start()
    def serve(self,conn):
        transport=paramiko.Transport(conn);self.transports.append(transport);transport.add_server_key(self.host_key)
        transport.set_subsystem_handler('sftp',SftpServer,Files)
        try:
            transport.start_server(server=Server(self))
            channels=[]
            while transport.is_active() and not self.stopping:
                channel=transport.accept(.1)
                if channel is not None:channels.append(channel)
                channels=[c for c in channels if not c.closed]
        except (EOFError,OSError):pass
        finally:transport.close()
    def close(self):
        self.stopping=True;self.listener.close()
        for transport in self.transports:transport.close()


def terminal_run(script,args,answers,stop_at=None,timeout=25):
    import pty,termios
    master,slave=pty.openpty()
    # A fresh single-threaded helper acquires the controlling TTY before exec;
    # avoid forking the threaded SSH fixture and running Python in that child.
    launcher="import os,sys,fcntl,termios; os.setsid(); fd=os.open(sys.argv[1],os.O_RDWR); fcntl.ioctl(fd,termios.TIOCSCTTY,0); [os.dup2(fd,n) for n in range(3)]; os.execve('/bin/bash',['bash',*sys.argv[2:]],os.environ)"
    process=subprocess.Popen([sys.executable,'-c',launcher,os.ttyname(slave),str(script),*map(str,args)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env={**os.environ,'LC_ALL':'C.UTF-8','TERM':'xterm-256color'})
    pid=process.pid
    output=b'';pending=b'';index=0;end=time.monotonic()+timeout;status=None
    try:
        while time.monotonic()<end:
            ready,_,_=select.select([master],[],[],.05)
            if ready:
                try:chunk=os.read(master,65536)
                except OSError as e:
                    if e.errno!=errno.EIO:raise
                    break
                if not chunk:break
                output+=chunk;pending+=chunk
            if stop_at and stop_at in pending:
                os.kill(pid,signal.SIGTERM);stop_at=None
            if index<len(answers) and answers[index][0] in pending:
                # Send only while terminal echo is disabled, as a person entering a password would.
                entry=answers[index];hidden=entry[2] if len(entry)>2 else True
                if not hidden or not (termios.tcgetattr(master)[3]&termios.ECHO):
                    marker,value=entry[:2];os.write(master,value if isinstance(value,bytes) else value.encode()+b'\n');pending=b'';index+=1
            finished,result=os.waitpid(pid,os.WNOHANG)
            if finished:status=result;break
        if status is None:
            finished,result=os.waitpid(pid,os.WNOHANG)
            if finished:status=result
        if status is None:
            os.kill(pid,signal.SIGTERM)
            for _ in range(40):
                finished,result=os.waitpid(pid,os.WNOHANG)
                if finished:status=result;break
                time.sleep(.05)
            if status is None:os.kill(pid,signal.SIGKILL);_,status=os.waitpid(pid,0);process.returncode=os.waitstatus_to_exitcode(status);raise AssertionError('interactive fixture timed out: '+output.decode(errors='replace'))
        process.returncode=os.waitstatus_to_exitcode(status)
        return process.returncode,output.decode(errors='replace'),index
    finally:os.close(master);os.close(slave)


@unittest.skipUnless(paramiko and os.environ.get('CONVERYN_INSTALL_LIVE')=='1','explicit loopback SSH fixture with test-only Paramiko')
class NativePasswordTransport(unittest.TestCase):
    def test_sftp_open_handle_supports_scp_metadata_updates(self):
        with tempfile.TemporaryDirectory(prefix='converyn-sftp-test-') as directory:
            server=SshFixture(Path(directory))
            try:
                with paramiko.Transport(('127.0.0.1',server.port)) as transport:
                    transport.connect(hostkey=server.host_key,username='operator',pkey=server.client_key)
                    with paramiko.SFTPClient.from_transport(transport) as client:
                        with client.open('/incoming/payload','wb') as remote:
                            remote.write(b'reviewed-payload')
                            remote.truncate(len(b'reviewed-payload'))
                            remote.chmod(0o640)
                            self.assertEqual(remote.stat().st_size,len(b'reviewed-payload'))
                            self.assertEqual(remote.stat().st_mode & 0o777,0o640)
                        client.truncate('/incoming/payload',len(b'reviewed-payload'))
                self.assertEqual((server.root/'incoming/payload').read_bytes(),b'reviewed-payload')
            finally:server.close()

    def fixture(self,nopass=False,wrong_key=False,cancel=False,policy_change=False,key_auth=False,retry_sudo=False,sudo_backspace=False,first_contact=False,trust=True,sftp=False):
        with tempfile.TemporaryDirectory(prefix='converyn-ssh-test-') as directory:
            root=Path(directory);server=SshFixture(root,nopass=nopass)
            try:
                known=root/'known_hosts';key=paramiko.RSAKey.generate(2048) if wrong_key else server.host_key
                if not first_contact:known.write_text(f'[127.0.0.1]:{server.port} {key.get_name()} {key.get_base64()}\n')
                harness=root/'harness.sh';source=root/'payload';source.write_text('only-reviewed-payload\n')
                harness.write_text('''#!/bin/bash
set -Eeuo pipefail
source "$1";shift
exec 3<>/dev/tty;TTY=true;SSH_AUTH=password;SSH_PORT=$1;KNOWN_HOSTS=$2;SSH_KEY='';target=operator@127.0.0.1
trap installer_cleanup EXIT
trap 'exit 143' TERM
if [[ $4 == key ]];then SSH_AUTH=key;SSH_KEY=$3/client-key;fi
prepare_known_hosts
ssh_arguments
SSH_ARGS=(-F /dev/null "${SSH_ARGS[@]}");SCP_ARGS=(-F /dev/null "${SCP_ARGS[@]}")
if [[ $4 == sftp ]];then SCP_ARGS+=(-s);fi
printf '%s' "$SSH_CONTROL_DIR" > "$3/control-path"
remote_preflight "$target"
remote_preflight "$target"
if [[ $4 == policy ]];then touch "$3/nopass-switch";fi
if [[ $4 == cancel ]];then ask_secret canceled 'CANCEL_AT_THIS_PASSWORD';fi
scp "${SCP_ARGS[@]}" "$3/payload" "$target:/incoming/"
remote_sudo "$target" 'if IFS= read -r unexpected;then printf SECRET_STDIN_LEAK;exit 1;fi;printf "privileged-ok\\n"'
ssh_cleanup
[[ ! ${SUDO_PASSWORDS[$target]+present} ]]
printf 'TRANSPORT_FINISHED\\n'
''')
                answers=[(b'Are you sure you want to continue connecting', 'yes' if trust else 'no',False)] if first_contact else []
                if not key_auth and (not first_contact or trust):answers.append((b'password:',server.ssh_password))
                sudo_prompt=b'sudo \xe5\xaf\x86\xe7\xa0\x81 for operator@127.0.0.1:'
                if retry_sudo:answers.append((sudo_prompt,'deliberately-wrong-fixture-password'))
                if not nopass and not wrong_key and (not first_contact or trust):answers.append((sudo_prompt,server.sudo_password.encode()+b'xx\x7f\x08\n' if sudo_backspace else server.sudo_password))
                try:result,output,count=terminal_run(harness,[SCRIPT,server.port,known,root,'cancel' if cancel else 'policy' if policy_change else 'key' if key_auth else 'sftp' if sftp else 'normal'],answers,stop_at=b'CANCEL_AT_THIS_PASSWORD:' if cancel else None)
                except AssertionError as error:raise AssertionError(str(error)+'\nserver events: '+repr(server.events)+'\ncommands: '+repr(server.commands)) from error
                self.assertNotIn(server.ssh_password,output);self.assertNotIn(server.sudo_password,output)
                self.assertFalse(Path((root/'control-path').read_text()).exists())
                if first_contact and not trust:
                    self.assertNotEqual(result,0);self.assertEqual(server.auth_attempts,0);self.assertFalse(server.commands);self.assertNotIn(server.host_key.get_base64(),known.read_text())
                elif wrong_key:
                    self.assertNotEqual(result,0);self.assertEqual(server.auth_attempts,0);self.assertFalse(server.commands)
                elif cancel:
                    self.assertEqual(result,1);self.assertFalse((root/'incoming/payload').exists())
                else:
                    self.assertEqual(result,0,output+'\nserver events: '+repr(server.events)+'\ncommands: '+repr(server.commands));self.assertIn('TRANSPORT_FINISHED',output);self.assertIn('privileged-ok',output);self.assertNotIn('SECRET_STDIN_LEAK',output)
                    self.assertEqual((root/'incoming/payload').read_bytes(),source.read_bytes())
                    self.assertEqual(server.auth_attempts,0 if key_auth else 1,'SSH/scp must share the authenticated connection')
                    if key_auth:self.assertGreater(server.key_attempts,0)
                    self.assertEqual(count,(1 if first_contact else 0)+(0 if key_auth else 1)+(0 if nopass else 1)+(1 if retry_sudo else 0))
                    if first_contact:self.assertIn(server.host_key.get_base64(),known.read_text())
                    self.assertEqual('sudo 密码' in output,not nopass)
                    self.assertNotIn(server.sudo_password,(root/'sudo-args.jsonl').read_text())
                for _ in range(20):
                    if not any(t.is_active() for t in server.transports):break
                    time.sleep(.05)
                self.assertFalse(any(t.is_active() for t in server.transports),'installer must close its authenticated SSH master')
            finally:server.close()
    def test_native_password_prompt_scp_reuse_and_sudo_stdin_isolation(self):self.fixture()
    def test_sftp_scp_reuses_connection_and_finishes_cleanly(self):self.fixture(sftp=True)
    def test_password_ssh_with_passwordless_sudo_does_not_ask_for_sudo_password(self):self.fixture(nopass=True)
    def test_cached_sudo_password_cannot_reach_program_after_policy_becomes_passwordless(self):self.fixture(policy_change=True)
    def test_key_ssh_still_allows_interactive_sudo_password(self):self.fixture(key_auth=True)
    def test_wrong_sudo_password_can_be_retried_without_reauthenticating_ssh(self):self.fixture(retry_sudo=True)
    def test_sudo_password_accepts_both_backspace_encodings_without_echo(self):self.fixture(sudo_backspace=True)
    def test_first_contact_allows_user_to_confirm_fingerprint(self):self.fixture(first_contact=True)
    def test_first_contact_rejection_never_authenticates(self):self.fixture(first_contact=True,trust=False)
    def test_first_contact_confirmation_also_works_in_key_mode(self):self.fixture(first_contact=True,key_auth=True)
    def test_changed_host_key_fails_before_any_password_authentication(self):self.fixture(wrong_key=True)
    def test_noninteractive_first_contact_remains_strict(self):
        with tempfile.TemporaryDirectory(prefix='converyn-ssh-strict-') as directory:
            root=Path(directory);server=SshFixture(root,nopass=True)
            try:
                known=root/'known_hosts';known.write_text('');known.chmod(0o600)
                code='''source "$1";shift;TTY=false;NONINTERACTIVE=true;SSH_AUTH=key;SSH_PORT=$1;KNOWN_HOSTS=$2;SSH_KEY=$3;ssh_arguments;trap ssh_cleanup EXIT;SSH_ARGS=(-F /dev/null "${SSH_ARGS[@]}");remote_preflight operator@127.0.0.1'''
                r=subprocess.run(['bash','-c',code,'_',str(SCRIPT),str(server.port),str(known),str(root/'client-key')],capture_output=True,text=True,timeout=15)
                self.assertNotEqual(r.returncode,0);self.assertEqual(server.auth_attempts,0);self.assertEqual(server.key_attempts,0);self.assertEqual(known.read_text(),'')
            finally:server.close()
    def test_termination_removes_private_socket_and_closes_master(self):self.fixture(cancel=True)

if __name__=='__main__':unittest.main()
