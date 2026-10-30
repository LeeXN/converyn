"""Generic directory/snapshot acceptance inside the disposable workflow fixture."""
import json,os,shlex,shutil,socket,time
from pathlib import Path

def check(root,effects,op,cli,request,until,controllers,start,bases):
    directory=root/'portable-workflow';directory.mkdir();(directory/'lib').mkdir()
    result=effects/'directory-result'
    release=effects/'directory-release'
    future=root/'future-business';assert not future.exists()
    (directory/'lib/helper.sh').write_text('printf "package-helper\\n"\n')
    (directory/'run.sh').write_text('set -eu\nsource "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"\ntest "$FIXED_MODE" = validate\npython3 -c \'import json,os; d=json.loads(os.environ["CONVERYN_INVENTORY_JSON"]); assert d["node"]["uid"] == "node-a"; assert d["groups"]["database"] == ["node-a"]\'\nprintf "effect\\n" >> '+str(result)+'\nwhile [ ! -e '+str(release)+' ]; do sleep .1; done\n')
    (directory/'verify.sh').write_text('source "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"\nprintf "original revision\\n"\n')
    inventory={'nodes':[{'uid':'node-a','address':'127.0.0.1','hostname':socket.gethostname()}],'groups':{'database':['node-a'],'monitoring':['node-a']}}
    (directory/'inventory.yaml').write_text(json.dumps(inventory))
    manifest={'uid':'directory-fixture','name':'Portable directory','inventory':'inventory.yaml','package':{'include':['lib']},'targets':{'qa':{'hosts':'all','node_group_uid':'hosts'}},'scripts':{'run':'run.sh'},'playbook':{'api_version':'converyn/v1','name':'directory','steps':[{'id':'run','phase':'rollout','scope':'per_node','hosts':'database[0]','max_parallel':1,'operation':{'kind':'exec','fixed_environment':{'FIXED_MODE':'validate'},'timeout_seconds':30}}]}}
    (directory/'workflow.yaml').write_text(json.dumps(manifest))
    manifest['scripts']['verify']='verify.sh';manifest['playbook']['steps'].append({'id':'verify','phase':'verify','scope':'per_node','hosts':'database[0]','operation':{'kind':'exec','timeout_seconds':30}});(directory/'workflow.yaml').write_text(json.dumps(manifest))
    manifest['playbook']['steps'].insert(0,{'id':'prepare-data','phase':'rollout','scope':'per_node','hosts':'database','operation':{'kind':'plugin','id':'converyn.linux-host','action':'directories','spec':{'paths':[str(future)]}}});(directory/'workflow.yaml').write_text(json.dumps(manifest))
    before=cli('get','node-groups','--config',op)['node_groups']
    cli('validate','workflows','--file',directory)
    maintainer=root/'bob/config.json';identity=json.loads((root/'bob/credential.json').read_text())['identity_uid']
    cli('auth','grant','directory-maintain','--config',op,'--person',identity,'--role','maintainer','--workflow','directory-fixture')
    assert cli('register','workflows','--config',maintainer,'--file',directory,check=False).returncode != 0
    cli('register','workflows','--config',op,'--file',directory)
    inventory['groups']['audited']=['node-a'];(directory/'inventory.yaml').write_text(json.dumps(inventory))
    cli('register','workflows','--config',maintainer,'--file',directory)
    original_manifest=json.loads(json.dumps(manifest));original_inventory=json.loads(json.dumps(inventory))
    manifest['uid']='monitoring-fixture';manifest['name']='Independent monitoring labels'
    manifest['playbook']['steps']=[s for s in manifest['playbook']['steps'] if s['id']!='prepare-data']
    inventory['groups']={'database':['node-a'],'monitoring':['node-a']};(directory/'inventory.yaml').write_text(json.dumps(inventory))
    (directory/'run.sh').write_text('set -eu\nsource "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"\nprintf "independent monitoring\\n"\n')
    (directory/'workflow.yaml').write_text(json.dumps(manifest));cli('register','workflows','--config',op,'--file',directory)
    shutil.rmtree(directory)
    preview=cli('run','workflows','directory-fixture','--target','qa','--dry-run','--config',op)
    assert preview
    assert cli('run','workflows','directory-fixture','--target','qa','--dry-run','--set','FIXED_MODE=unsafe','--config',op,check=False).returncode != 0
    cli('run','workflows','directory-fixture','--target','qa','--request-id','directory-proof','--config',op)
    until(lambda:result.exists())
    assert future.is_dir()
    busy=cli('run','workflows','monitoring-fixture','--target','qa','--request-id','monitor-proof','--config',op,check=False)
    assert busy.returncode != 0 and 'node_busy' in busy.stderr
    original_inventory['nodes'][0]['address']='127.0.0.2'
    update=root/'updated-source';update.mkdir();(update/'lib').mkdir();(update/'lib/helper.sh').write_text('printf "updated helper\\n"\n');(update/'run.sh').write_text('source "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"\nprintf "new revision\\n"\n');(update/'verify.sh').write_text('printf "new verification\\n"\n');(update/'inventory.yaml').write_text(json.dumps(original_inventory));(update/'workflow.yaml').write_text(json.dumps(original_manifest))
    cli('register','workflows','--config',maintainer,'--file',update)
    cli('register','workflows','--config',op,'--file',update);shutil.rmtree(update)
    assert cli('get','runs','run-directory-proof','--config',op)['status']=='running'
    killed=None
    live=[p for p in controllers if p.poll() is None]
    if len(live)==3:
        leader=cli('status','--config',op)['controller_observation']['metrics']['current_leader'];killed=leader;controllers[leader-1].kill();controllers[leader-1].wait(timeout=10)
    release.touch()
    final=until(lambda:(lambda r:r if r.get('status')=='succeeded' else None)(cli('get','runs','run-directory-proof','--config',op)),60)
    assert result.read_text()=='effect\n'
    assert final['plan']['input']['inventory']['groups']['database']==['node-a']
    assert final['plan']['input']['inventory']['nodes'][0]['address']=='127.0.0.1'
    assert cli('get','node-groups','--config',op)['node_groups']==before
    logs=cli('logs','runs','run-directory-proof','--config',op,output_format=None).stdout
    assert 'package-helper' in logs
    assert 'original revision' in logs and 'new verification' not in logs
    cli('run','workflows','directory-fixture','--target','qa','--request-id','updated-proof','--wait','--config',op)
    assert cli('get','runs','run-updated-proof','--config',op)['plan']['input']['inventory']['nodes'][0]['address']=='127.0.0.2'
    latest=cli('logs','runs','run-updated-proof','--config',op,output_format=None).stdout;assert 'updated helper' in latest and 'new verification' in latest
    cli('run','workflows','monitoring-fixture','--target','qa','--request-id','monitor-proof','--wait','--config',op)
    assert result.read_text()=='effect\n'
    # A packaged helper emits typed data without depending on a product adapter.
    portable=root/'portable-outputs';portable.mkdir();(portable/'lib').mkdir()
    payload=json.dumps({'context':{'VALUE':'changed'},'rendered':'rendered configuration\n','reboot_requested':False})
    (portable/'lib/emit.sh').write_text("printf '%s' "+shlex.quote(payload)+" | \"$CONVERYN_EXECUTABLE\" task emit --stdin\n")
    manifest={'uid':'portable-outputs','name':'Packaged typed outputs','inventory':inventory,'targets':{'qa':{'hosts':'all','node_group_uid':'hosts'}},'playbook':{'api_version':'converyn/v1','name':'portable-outputs','steps':[{'id':'emit','phase':'rollout','scope':'per_node','hosts':'all','outputs':{'context':{'type':'object','max_bytes':16384},'rendered':{'type':'string'},'reboot_requested':{'type':'boolean'}},'operation':{'kind':'exec','script':'lib/emit.sh','timeout_seconds':30}}]}}
    (portable/'workflow.yaml').write_text(json.dumps(manifest));cli('register','workflows','--file',portable,'--config',op);shutil.rmtree(portable)
    cli('run','workflows','portable-outputs','--target','qa','--request-id','portable-outputs-proof','--wait','--config',op)
    final=cli('get','runs','run-portable-outputs-proof','--config',op)
    output=next(iter(final['attempts'].values()))['receipt']['outputs'];assert output['context']['value']['VALUE']=='changed';assert output['rendered']['value']=='rendered configuration\n';assert output['reboot_requested']['value'] is False
    return {'directory_snapshot':True,'directory_source_removed':True,'directory_labels':True,'directory_single_effect':True,'directory_killed_leader':killed,'workflow_node_reuse':True,'workflow_node_mutex':True,'inventory_platform_authorization':True,'packaged_typed_outputs':True,'fixed_step_environment':True,'directory_reregister_while_running':True,'inventory_update_authorization':True,'future_profile_paths':True,'generic_directory_action':True}
