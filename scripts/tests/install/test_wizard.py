"""Readline and input-cache regressions use a real PTY without installing services."""
import json,os,shutil,subprocess,tempfile,unittest
from pathlib import Path
from test_installer import SCRIPT,helper
from test_transport import terminal_run
HEADER='# Converyn installer input defaults; data only, never shell code\n'

def cache_read(path):
    return dict(line.split('\t',1) for line in path.read_text().splitlines() if '\t' in line)
def cache_write(path,data):
    path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    path.write_text(HEADER+''.join(k+'\t'+v+'\n' for k,v in data.items()));path.chmod(0o600)
def answer(prompt,value=None):return (prompt.encode(),b'\n' if value is None else b'\x15'+value.encode()+b'\n')

class WizardValidation(unittest.TestCase):
    def test_ip_port_and_endpoint_validation_rejects_blank_and_malformed_members(self):
        for value in ['', ' ', '999.1.1.1', '127.00.0.1', '1:::2', ':1', '1:2:3:4:5:6:7:8:9','user@host']:
            self.assertNotEqual(helper('canonical_ip "$1"',value).returncode,0,value)
        for value in ['192.0.2.175','::1','2001:db8::1','::ffff:192.0.2.1']:
            self.assertEqual(helper('canonical_ip "$1"',value).returncode,0,value)
        for value in [':8443','10.0.0.1:0','10.0.0.1:65536','10.0.0.999:8443','::1:8443','[:::]:8443']:
            self.assertNotEqual(helper('valid_endpoint "$1"',value).returncode,0,value)
        for value in ['10.0.0.1:8443','[::1]:8443']:
            self.assertEqual(helper('valid_endpoint "$1"',value).returncode,0,value)
        a=helper('canonical_ip "$1"','2001:db8::1');b=helper('canonical_ip "$1"','2001:DB8:0:0:0:0:0:1');self.assertEqual(a.stdout,b.stdout)
    def test_bare_target_normalization_preserves_user_and_validates_host(self):
        for value,expected in [('192.0.2.175','operator@192.0.2.175'),('::1','operator@[::1]'),('other@host','other@host'),('local','local')]:
            r=helper('SSH_USER=operator; normalize_target "$1"',value);self.assertEqual(r.stdout,expected)
        self.assertNotEqual(helper('valid_target "$1"','operator@999.1.1.1').returncode,0)
    def test_cache_uses_whitelisted_data_without_evaluating_it_or_saving_credentials(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);cache=root/'state/inputs.tsv';sentinel=root/'must-not-exist'
            injected='$(touch '+str(sentinel)+')'
            cache_write(cache,{'operator':injected,'ssh_user':'operator','sudo_password':'secret-marker','binary_url':'https://private.invalid/?token=secret-marker','confirm':'yes','resume':'true'})
            result=helper('TTY=true;INPUT_CACHE=$1;cache_init;printf "%s" "${INPUT_DEFAULTS[operator]}";cache_set cluster demo;cache_set password secret-marker;cache_set binary_url secret-marker',str(cache))
            self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(result.stdout,injected);self.assertFalse(sentinel.exists())
            self.assertEqual(cache.stat().st_mode&0o777,0o600)
            self.assertEqual(cache_read(cache),{'operator':injected,'ssh_user':'operator','cluster':'demo'})
    def test_cache_refuses_symlinks_and_unrelated_files_without_modifying_them(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);private=root/'private';private.mkdir();private.chmod(0o700)
            original=private/'original';original.write_text('must be retained\n');before=original.read_bytes()
            link=private/'link';link.symlink_to(original)
            for path in [link,original]:
                r=helper('TTY=true;INPUT_CACHE=$1;cache_init;cache_set cluster demo',str(path))
                self.assertNotEqual(r.returncode,0);self.assertEqual(original.read_bytes(),before)
    def test_no_cache_and_noninteractive_modes_do_not_read_or_create_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)/'not-created/inputs.tsv'
            for options in ['INPUT_CACHE_ENABLED=false','NONINTERACTIVE=true']:
                r=helper('TTY=true;INPUT_CACHE=$1;'+options+';cache_init;cache_set cluster demo',str(target))
                self.assertEqual(r.returncode,0,r.stderr);self.assertFalse(target.exists())
    @unittest.skipUnless(os.geteuid()==0,'unprivileged-user fixture needs root to choose a test uid')
    def test_ordinary_user_gets_a_home_directory_and_private_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);root.chmod(0o755);script=root/'install.sh';shutil.copy2(SCRIPT,script);script.chmod(0o755)
            home=root/'home';home.mkdir();home.chmod(0o700);os.chown(home,65534,65534)
            code='source "$1"; CLUSTER=demo; default_install_directory; TTY=true;cache_init;cache_set cluster demo'
            env={**os.environ,'HOME':str(home)};env.pop('XDG_STATE_HOME',None)
            r=subprocess.run(['bash','-c',code,'_',str(script)],user=65534,group=65534,capture_output=True,text=True,env=env)
            self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(r.stdout,str(home/'.local/share/converyn-install/demo'))
            cached=home/'.local/state/converyn/installer/inputs.tsv';self.assertEqual(cached.stat().st_uid,65534);self.assertEqual(cached.stat().st_mode&0o777,0o600)


class InteractiveWizard(unittest.TestCase):
    def harness(self,root,tail=''):
        script=root/'wizard.sh'
        script.write_text('''#!/bin/bash
set -Eeuo pipefail
source "$1";shift
exec 3<>/dev/tty;TTY=true;TTY_STATE=$(stty -g <&3)
trap installer_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
INPUT_CACHE=$1;KNOWN_HOSTS=$2;cache_init
'''+('''interactive
validate_deployment
printf '%s\\n' "${MEMBERS[@]}" > "$3/members"
printf '%s\\n' "${CONTROLLERS[@]}" > "$3/controllers"
printf '%s\\n' "${AGENTS[@]}" > "$3/agents"
printf '%s\\n' "$CLUSTER" "$OPERATOR" "$DIRECTORY" "$SSH_USER" "$SSH_AUTH" > "$3/settings"
printf 'WIZARD_FINISHED\\n'
''' if not tail else tail))
        return script
    def prepared(self,root):
        known=root/'known_hosts';known.write_text('example.test ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeKeyPlaceholder\n')
        cache=root/'cache/inputs.tsv';install=root/'installation';return known,cache,install
    def values(self,known,install,invalid=False):
        values=[answer('部署方式','3')]
        if invalid:values.append(answer('Cluster name',''))
        values.append(answer('Cluster name','demo'))
        if invalid:values.append(answer('Operator display name',''))
        values.extend([answer('Operator display name','OP'),answer('Private installation directory',str(install)),answer('Controller count')])
        if invalid:values.append(answer('SSH user',''))
        values.append(answer('SSH user','operator'))
        for i,ip in enumerate(['192.0.2.175','192.0.2.176','192.0.2.177'],1):
            prompt=f'Controller {i} stable IP'
            if invalid and i==1:values.extend([answer(prompt,''),answer(prompt,'   '),answer(prompt,'999.1.1.1')])
            if invalid and i==2:values.append(answer(prompt,'192.0.2.175'))
            values.append(answer(prompt,ip))
            if invalid and i==1:values.extend([answer(f'Controller {i} HTTPS port','0'),answer(f'Controller {i} HTTPS port','65536')])
            values.append(answer(f'Controller {i} HTTPS port','8443' if invalid and i==1 else None))
            values.append(answer(f'Controller {i} target',ip if i==1 else None))
        values.append(answer('Agent count','2'))
        if invalid:values.append(answer('Agent 1 UID',''))
        values.extend([answer('Agent 1 UID','node-1'),answer('Agent node-1 target','192.0.2.178')])
        if invalid:values.append(answer('Agent 2 UID','node-1'))
        values.append(answer('Agent 2 UID','node-2'))
        if invalid:values.append(answer('Agent node-2 target','192.0.2.178'))
        values.append(answer('Agent node-2 target','192.0.2.179'))
        if invalid:values.append(answer('SSH known_hosts file',''))
        values.extend([answer('SSH known_hosts file',str(known) if invalid else None),answer('SSH authentication','password')]);return values
    def test_reported_empty_ips_and_duplicate_agents_are_reprompted_without_restart(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,install=self.prepared(root);harness=self.harness(root)
            r,out,count=terminal_run(harness,[SCRIPT,cache,known,root],self.values(known,install,invalid=True))
            self.assertEqual(r,0,out);self.assertIn('WIZARD_FINISHED',out);self.assertIn('Duplicate controller IP',out);self.assertIn('Duplicate Agent host',out)
            self.assertEqual((root/'members').read_text().splitlines(),[f'{i}=192.0.2.{174+i}:8443' for i in range(1,4)])
            self.assertEqual((root/'controllers').read_text().splitlines(),[f'{i}=operator@192.0.2.{174+i}' for i in range(1,4)])
            self.assertEqual((root/'agents').read_text().splitlines(),['node-1=operator@192.0.2.178','node-2=operator@192.0.2.179'])
            self.assertFalse(install.exists(),'wizard must not create deployment state')
            defaults=cache_read(cache);self.assertEqual(defaults['controller_1_ip'],'192.0.2.175');self.assertEqual(defaults['agent_2_target'],'operator@192.0.2.179')
    def test_next_launch_prefills_all_options_and_enter_reuses_them(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,install=self.prepared(root);harness=self.harness(root);values=self.values(known,install)
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],values);self.assertEqual(r,0,out)
            before={p.name:p.read_bytes() for p in [root/'members',root/'controllers',root/'agents',root/'settings']}
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[(prompt,b'\n') for prompt,_ in values]);self.assertEqual(r,0,out)
            self.assertIn('已载入输入缓存',out)
            for name,data in before.items():self.assertEqual((root/name).read_bytes(),data)
    def test_readline_backspace_delete_and_cursor_edit_prefilled_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,_=self.prepared(root)
            harness=self.harness(root,"ask selected 'EDIT_VALUE' 'abcd'; printf '%s' \"$selected\" > \"$3/edited\"\n")
            keys=b'\x1b[D\x1b[D\x7f\x08XY\x1b[FZ\n'
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[(b'EDIT_VALUE',keys)]);self.assertEqual(r,0,out)
            self.assertEqual((root/'edited').read_text(),'XYcdZ')
    def test_abort_retains_completed_fields_and_does_not_cache_confirmation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,_=self.prepared(root);harness=self.harness(root)
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[answer('部署方式','3'),answer('Cluster name','partial-demo')],stop_at=b'Operator display name')
            self.assertEqual(r,1,out);defaults=cache_read(cache);self.assertEqual(defaults['cluster'],'partial-demo');self.assertEqual(defaults['mode'],'3');self.assertNotIn('confirm',defaults)
    def test_confirmation_is_always_no_by_default_on_the_next_invocation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,_=self.prepared(root)
            harness=self.harness(root,"ask_option CLUSTER 'CACHE_CLUSTER' demo cluster valid_identifier;confirm\n")
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[answer('CACHE_CLUSTER'),answer('Proceed','yes')]);self.assertEqual(r,0,out)
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[answer('CACHE_CLUSTER'),answer('Proceed')]);self.assertNotEqual(r,0,out);self.assertIn('Installation canceled',out)
            self.assertEqual(cache_read(cache),{'cluster':'demo'})
    def test_changing_cluster_invalidates_old_directory_before_any_later_prompt(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,_=self.prepared(root);cache_write(cache,{'cluster':'old','directory':str(root/'old-installation')})
            harness=self.harness(root,"ask_option CLUSTER 'NEW_CLUSTER' old cluster valid_identifier;ask unused 'AFTER_CLUSTER' '';\n")
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[answer('NEW_CLUSTER','new')],stop_at=b'AFTER_CLUSTER')
            self.assertEqual(r,1,out);self.assertEqual(cache_read(cache),{'cluster':'new'})
    def test_explicit_cli_values_override_cached_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);known,cache,_=self.prepared(root);cache_write(cache,{'ssh_auth':'key'})
            harness=self.harness(root,"parse --ssh-auth password;ask_option SSH_AUTH 'AUTH_OVERRIDE' \"$SSH_AUTH\" ssh_auth valid_ssh_auth;printf '%s' \"$SSH_AUTH\" > \"$3/auth\"\n")
            r,out,_=terminal_run(harness,[SCRIPT,cache,known,root],[(b'AUTH_OVERRIDE',b'\n')]);self.assertEqual(r,0,out);self.assertEqual((root/'auth').read_text(),'password')

if __name__=='__main__':unittest.main()
