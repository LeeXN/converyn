"""One encrypted upload, five nodes without local Secret/policy configuration."""
import hashlib,json,os

def check(root,bundle,op,node_config,template,cli,request,until,controllers,start,bases):
    nodes=['node-a']
    for suffix in 'bcde':
        uid='node-'+suffix;nodes.append(uid)
        bootstrap=root/(uid+'-bootstrap.json');cli('admin','bootstrap','--config',op,'--node',uid,'--output',bootstrap)
        cfg=json.loads(template.read_text());cfg['node_uid']=uid;cfg['state_dir']=str(root/(uid+'-state'));cfg.pop('native_policy',None)
        path=root/(uid+'-template.json');path.write_text(json.dumps(cfg))
        hardware=root/(uid+'-hardware');hardware.write_text('isolated-'+uid);hardware.chmod(0o600)
        output=root/(uid+'.json');cli('admin','enroll','--bootstrap',bootstrap,'--state-dir',cfg['state_dir'],'--output',output,'--template',path,'--hardware-id-file',hardware)
        start('agent','--config',output)
    until(lambda:len(request(op,'/api/v1/nodes')[1].get('nodes',{}))>=5,60)
    source=root/'secret-source';source.mkdir()
    password='secret-fixture-'+os.urandom(24).hex();private=root/'secret-input';private.write_text(password);private.chmod(0o600)
    sha=hashlib.sha256(password.encode()).hexdigest()
    (source/'consume.sh').write_text('set -eu\ntest "$(printf %s "$PASSWORD" | sha256sum | cut -d " " -f 1)" = '+sha+'\nprintf "%s\\n" "$PASSWORD"\nprintf "secret consumed\\n"\n')
    inventory={'nodes':[{'uid':uid,'address':'192.0.2.'+str(i+1),'hostname':uid} for i,uid in enumerate(nodes)],'groups':{}}
    group=cli('get','node-groups','hosts','--config',op)['node_groups']['hosts'];old=group['revision'];group['revision']+=1;group['node_uids']=[n['uid'] for n in inventory['nodes']]
    change=root/'node-group-update.json';change.write_text(json.dumps({'action':'update_node_group','expected_revision':old,'group':group}));cli('update','node-groups','--config',op,'--file',change)

    definition={'uid':'stored-secret-proof','name':'One Secret, five nodes','inventory':inventory,'targets':{'qa':{'hosts':'all','node_group_uid':'hosts'}},'playbook':{'api_version':'converyn/v1','name':'secrets','steps':[{'id':'consume','phase':'rollout','scope':'per_node','hosts':'all','operation':{'kind':'exec','script':'consume.sh','environment':{'PASSWORD':{'secret':'database','version':'v1'}},'timeout_seconds':60}}]}}
    (source/'workflow.yaml').write_text(json.dumps(definition));cli('register','workflows','--config',op,'--file',source)
    missing=cli('run','workflows','stored-secret-proof','--target','qa','--request-id','missing-secret','--config',op,check=False)
    assert missing.returncode!=0 and 'database' in missing.stderr,missing.stderr
    assert request(op,'/api/v1/runs/run-missing-secret')[0]==404
    assert cli('register','secrets','database','--version','v1','--file',private,'--config',root/'bob/config.json',check=False).returncode!=0
    cli('register','secrets','database','--version','v1','--file',private,'--config',op)
    cli('register','secrets','database','--version','v1','--file',private,'--config',op)
    private.write_text('conflicting-value')
    assert cli('register','secrets','database','--version','v1','--file',private,'--config',op,check=False).returncode!=0
    catalog=cli('get','secrets','database','--config',op)
    assert catalog['secrets']==[{'name':'database','version':'v1'}] and password not in json.dumps(catalog)
    accepted=cli('run','workflows','stored-secret-proof','--target','qa','--request-id','five-secrets','--config',op)
    run_id=accepted.get('id',accepted.get('run_id','run-five-secrets'))
    until(lambda:request(op,'/api/v1/runs/'+run_id)[1].get('status') in ['succeeded','failed','needs_attention'],120)
    run=request(op,'/api/v1/runs/'+run_id)[1];assert run['status']=='succeeded',run['status']
    logs=cli('logs','runs',run_id,'--config',op,output_format=None)
    assert password not in logs.stdout and 'secret consumed' in logs.stdout
    assert password not in json.dumps(run)
    audit=request(op,'/api/v1/audit')[1];assert password not in json.dumps(audit)
    # Authenticated people cannot use the Node-only Work delivery route.
    work=run['plan']['work'][0];grant=run['attempts'][work['id']]['grant']
    assert request(op,'/api/v1/agents/'+work['node_uid']+'/runs/'+run_id+'/work/'+work['id']+'/inputs?grant='+grant,extra_headers={'x-converyn-protocol':str(request(op,'/health/live')[1]['protocol']),'x-converyn-plugin-manifest':cli('get','plugins','--local')['manifest_sha256']})[0]==403
    backup=root/'secrets.backup';cli('admin','backup','--config',op,'--output',backup)
    assert password.encode() not in backup.read_bytes()
    for index in range(1,len(controllers)+1):
        data=bundle/('controller-'+str(index))/'data'/'state.sqlite3'
        if data.exists():assert password.encode() not in data.read_bytes()
    return {'run':run_id,'nodes':nodes,'single_registration':True,'local_secret_files':False,'missing_rejected_before_run':True,'plaintext_absent_from_logs_audit_backup':True,'immutable_versions':True}
