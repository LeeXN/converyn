"""Native output checks inside the shared disposable controller/Agent fixture."""
import hashlib
import json
import os
import time


def check(root, bundle, source, effects, op, cli, request, until, controllers, start, bases):
    token_file=effects/'output-secret'
    (source/'output-produce.sh').write_text('set -euo pipefail\numask 077\ntoken=$(head -c 24 /dev/urandom | base64 | tr -d "\\n")\nprintf "%s" "$token" > '+str(token_file)+'\nprintf "generated=%s\\n" "$token"\nprintf \'{"message":"committed","token":"%s","configuration":"native output configuration\\\\n"}\' "$token" | "$CONVERYN_EXECUTABLE" task emit --stdin\n')
    (source/'output-barrier.sh').write_text('sleep 8\n')
    (source/'output-consume.sh').write_text('set -euo pipefail\ntest "$MESSAGE" = committed\ntest -n "$TOKEN"\nprintf "consumed=%s token=%s\\n" "$MESSAGE" "$TOKEN"\nsleep 2\n')
    configuration=root/'output-configuration'
    manifest={'inventory':{'nodes':[{'uid':'node-a','address':'127.0.0.1','hostname':__import__('socket').gethostname()}]},'uid':'native-outputs','name':'Native output contracts','batch_size':300,
        'targets':{'hosts':{'node_group_uid':'hosts','hosts':'all'}},
        'scripts':{'produce':'output-produce.sh','barrier':'output-barrier.sh','consume':'output-consume.sh'},
        'files':{'configuration':'config.txt'},
        'playbook':{'api_version':'converyn/v1','name':'native-outputs','steps':[
            {'id':'produce','phase':'prepare','scope':'once_per_run','outputs':{
                'message':{'type':'string'},'configuration':{'type':'string'},
                'token':{'type':'string','secret':True,'ttl_seconds':300}},'operation':{'kind':'exec','timeout_seconds':30}},
            {'id':'barrier','phase':'rollout','scope':'per_node','operation':{'kind':'exec','timeout_seconds':30}},
            {'id':'configuration','phase':'rollout','scope':'per_node','inputs':{'/content':{'step':'produce','name':'configuration'}},'operation':{'kind':'file','path':str(configuration),'mode':420}},
            {'id':'consume','phase':'rollout','scope':'per_node','inputs':{'/environment/MESSAGE':{'step':'produce','name':'message'},'/environment/TOKEN':{'step':'produce','name':'token'}},'operation':{'kind':'exec','timeout_seconds':30}},
        ]}}
    # The enrolled Agent's native allowlist is immutable while running. Use the
    # already-authorized demonstration file as the materialized destination.
    manifest['playbook']['steps'][2]['operation']['path']=str(root/'result.txt')
    # This fixture deliberately reuses the demonstration's authorized file.
    # Share its existing owner explicitly instead of attempting an ownership takeover.
    prior_status,prior=request(op,'/api/v1/runs/run-first')
    if prior_status==200:
        manifest['targets']['hosts']['owner_uid']=prior['plan']['input']['owner_uid']

    file=source/'outputs-workflow.json';file.write_text(json.dumps(manifest))
    cli('register', 'workflows','--config',op,'--file',file)
    first=cli('run', 'workflows','native-outputs','--target','hosts','--request-id','native-outputs','--config',op)
    run_id=first['run_id'];path='/api/v1/runs/'+run_id
    def running_barrier():
        run=request(op,path)[1]
        work=next(w for w in run['plan']['work'] if w['step_id']=='barrier')
        return run if run['attempts'][work['id']]['status']=='running' else None
    interim=until(running_barrier,40)
    token=token_file.read_text();assert len(token)>=24
    producer=next(w for w in interim['plan']['work'] if w['step_id']=='produce')
    receipt=interim['attempts'][producer['id']]['receipt'];assert receipt['outputs']['token']['kind']=='secret' and 'logs' not in receipt
    assert token not in json.dumps(interim)
    assert request(op,path+'/logs/'+producer['id']+'/stdout')[0]==409
    assert request(op,'/api/v1/agents/node-a/runs/'+run_id+'/work/'+producer['id']+'/inputs?grant=wrong')[0] in (401,403,426)
    original=receipt['outputs']['token']
    # Rotate once, then distribute the same complete retained keyring to all members.
    keyring=bundle/'controller-1/output-keys.json'
    cli('admin','output-key','--file',keyring,'--rotate')
    for index in range(2,len(controllers)+1):
        target=bundle/f'controller-{index}/output-keys.json';target.write_bytes(keyring.read_bytes());target.chmod(0o600)
    killed=None
    if len(controllers)==3 and all(c.poll() is None for c in controllers):
        for index,address in enumerate(bases):
            metrics=request(op,'/api/v1/controlplane/metrics',address=address)[1]
            if metrics.get('metrics',{}).get('state')=='Leader':
                killed=index+1;controllers[index].kill();controllers[index].wait(timeout=10);break
        assert killed is not None
    final=until(lambda:(lambda r:r if r.get('status') in ('succeeded','failed','needs_attention') else None)(request(op,path)[1]),60)
    assert final['status']=='succeeded',final
    assert final['attempts'][producer['id']]['receipt']['outputs']['token']==original
    assert (root/'result.txt').read_text()=='native output configuration\n'
    consume=next(w for w in final['plan']['work'] if w['step_id']=='consume')
    materialized=final['attempts'][consume['id']]['materialized'];assert materialized['input_sha256']
    assert token not in json.dumps(final)
    logs=cli('logs', 'runs',run_id,'--step','consume','--config',op)
    assert token not in json.dumps(logs) and '[REDACTED]' in json.dumps(logs)
    assert cli('logs', 'runs',run_id,'--step','produce','--config',op,check=False).returncode!=0
    assert cli('get','node-groups','--config',op)['node_groups']['hosts']['revision']==1
    # Restart every controller after completion and verify persisted input/receipt identity.
    for controller in controllers:
        if controller.poll() is None:controller.terminate();controller.wait(timeout=10)
    for index in range(len(controllers)):
        controllers[index]=start('server','--config',bundle/f'controller-{index+1}/server.json')
    restored=until(lambda:(lambda s,r:r if s==200 and r.get('status')=='succeeded' else None)(*request(op,path)),40)
    assert restored['attempts'][producer['id']]['receipt']['outputs']['token']==original
    assert restored['attempts'][consume['id']]['materialized']==materialized
    assert cli('run', 'workflows','native-outputs','--target','hosts','--request-id','native-outputs','--config',op)['run_id']==run_id
    for directory in [root/'agent-state',*(bundle/f'controller-{i+1}/data' for i in range(len(controllers)))]:
        for file in directory.rglob('*'):
            if file.is_file() and file.stat().st_size<=16*1024*1024:
                assert token.encode() not in file.read_bytes(), 'plaintext leaked to '+str(file.relative_to(root))
    return {'output_leader_loss':killed,'native_outputs':True,'encrypted_receipts':True,'grant_scoped_secret_delivery':True,'secret_logs_suppressed':True,
        'output_key_rotation':True,'materialized_file':True,'platform_scope_preserved':True,'output_restart_recovery':True}
