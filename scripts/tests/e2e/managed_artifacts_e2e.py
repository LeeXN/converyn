"""Real mTLS managed upload, controller failover and five independent Agent caches."""
import hashlib,json,os,shutil,socket,ssl,time,urllib.request,urllib.error,subprocess,pty,select,errno
from pathlib import Path


def check(root,bundle,op,node_config,template,cli,request,until,controllers,start,bases):
    targets={'node-a':root/'result.txt'}
    configs={'node-a':node_config}
    base_template=json.loads(template.read_text())
    for suffix in 'bcde':
        uid='node-'+suffix
        target=root/('artifact-'+suffix)
        targets[uid]=target
        bootstrap=root/(uid+'-bootstrap.json')
        cli('admin','bootstrap','--config',op,'--node',uid,'--output',bootstrap)
        cfg=json.loads(json.dumps(base_template));cfg['node_uid']=uid;cfg['state_dir']=str(root/(uid+'-state'))
        path=root/(uid+'-template.json');path.write_text(json.dumps(cfg))
        hardware=root/(uid+'-hardware');hardware.write_text('isolated-'+uid);hardware.chmod(0o600)
        output=root/(uid+'.json');configs[uid]=output
        cli('admin','enroll','--bootstrap',bootstrap,'--state-dir',cfg['state_dir'],'--output',output,'--template',path,'--hardware-id-file',hardware)
        start('agent','--config',output)
    until(lambda:len(request(op,'/api/v1/nodes')[1].get('nodes',{}))>=5,60)
    directory=root/'managed-source';directory.mkdir()
    payload=directory/'payload.bin';content=(bytes(range(251))*((9*1024*1024)//251+1))[:9*1024*1024]+b'finish'
    payload.write_bytes(content);sha=hashlib.sha256(content).hexdigest();size=len(content)
    bad=cli('register','artifacts','--file',payload,'--config',root/'bob/config.json',check=False)
    assert bad.returncode!=0
    assert request(op,'/api/v1/node-groups/commands',{'action':'artifact','sha256':sha,'size':size})[0]==403
    release=root/'artifact-release'
    (directory/'verify.sh').write_text('set -eu\ntest "$(sha256sum "$TARGET" | cut -d " " -f 1)" = "'+sha+'"\nwhile [ ! -e '+str(release)+' ]; do sleep .1; done\nprintf "verified artifact\\n"\n')
    inventory={'nodes':[{'uid':uid,'address':'192.0.2.'+str(list(targets).index(uid)+1),'hostname':uid,'values':{'TARGET':str(target)}} for uid,target in targets.items()], 'groups':{uid:[uid] for uid in targets}}
    group=cli('get','node-groups','hosts','--config',op)['node_groups']['hosts'];old=group['revision'];group['revision']+=1;group['node_uids']=[n['uid'] for n in inventory['nodes']]
    change=root/'node-group-update.json';change.write_text(json.dumps({'action':'update_node_group','expected_revision':old,'group':group}));cli('update','node-groups','--config',op,'--file',change)

    steps=[{'id':'copy-'+uid,'phase':'rollout','scope':'per_node','hosts':uid,'operation':{'kind':'binary','path':str(target),'mode':420,'artifact':{'$artifact':'payload.bin'}}} for uid,target in targets.items()]
    steps.append({'id':'verify','phase':'verify','scope':'per_node','hosts':'all','operation':{'kind':'exec','script':'verify.sh','timeout_seconds':120}})
    manifest={'uid':'managed-artifacts','name':'Managed artifact proof','inventory':inventory,'targets':{'qa':{'hosts':'all','node_group_uid':'hosts'}},'playbook':{'api_version':'converyn/v1','name':'managed','steps':steps}}
    (directory/'workflow.yaml').write_text(json.dumps(manifest))
    cli('validate','workflows','--file',directory)
    command=[str(root/'converyn'),'-o','json','--config',str(op),'register','workflows','--file',str(directory)]
    registered=subprocess.run(command,capture_output=True,text=True,timeout=60,check=True)
    assert json.loads(registered.stdout)['ok'] is True
    assert 'uploaded' in registered.stderr and 'Workflow registration confirmed' in registered.stderr
    assert 'uploading' in registered.stderr and 'payload.bin' in registered.stderr
    assert '\x1b' not in registered.stderr and '\r' not in registered.stderr
    assert {'sha256':sha,'size':size} in cli('get','artifacts','--config',op)['artifacts']
    # An identical registration is deduplicated and checks retained controller bytes.
    master,slave=pty.openpty()
    child=subprocess.Popen(command,stdout=subprocess.PIPE,stderr=slave,env={**os.environ,'TERM':'xterm'})
    os.close(slave);terminal=bytearray();deadline=time.monotonic()+60
    try:
        while time.monotonic()<deadline:
            readable,_,_=select.select([master],[],[],.2)
            if readable:
                try:chunk=os.read(master,65536)
                except OSError as error:
                    if error.errno==errno.EIO:break
                    raise
                if not chunk:break
                terminal.extend(chunk)
            elif child.poll() is not None:break
        stdout,_=child.communicate(timeout=max(1,deadline-time.monotonic()))
        assert child.returncode==0 and json.loads(stdout)['ok'] is True
    finally:
        if child.poll() is None:child.kill();child.wait()
        os.close(master)
    terminal=terminal.decode()
    assert '\r\x1b[2K' in terminal and 'reused' in terminal and '0 uploaded, 1 reused' in terminal,terminal
    assert 'Workflow registration confirmed' in terminal
    replica_paths={}
    for index in range(1,len(controllers)+1):
        cfg=json.loads((bundle/('controller-'+str(index))/'server.json').read_text())
        data=Path(cfg['data_dir']);data=data if data.is_absolute() else bundle/('controller-'+str(index))/data
        file=data/'artifacts'/sha;replica_paths[index]=file
        assert file.is_file() and hashlib.sha256(file.read_bytes()).hexdigest()==sha, {'expected':str(file),'parent_files':[str(p.relative_to(root)) for p in root.rglob(sha)]}
    shutil.rmtree(directory)
    killed=None
    if len(controllers)==3:
        killed=cli('status','--config',op)['controller_observation']['metrics']['current_leader']
        controllers[killed-1].kill();controllers[killed-1].wait(timeout=10)
        until(lambda:request(op,'/api/v1/artifacts')[0]==200,30)
        current=cli('status','--config',op)['controller_observation']['metrics']['current_leader']
        replica_paths[current].unlink()  # Disposable fixture: force lazy peer recovery on the new leader.
    cli('run','workflows','managed-artifacts','--target','qa','--request-id','managed-proof','--config',op)
    until(lambda:all(path.is_file() for path in targets.values()),90)
    run=cli('get','runs','run-managed-proof','--config',op)
    work=next(w for w in run['plan']['work'] if w['node_uid']=='node-a' and w['operation']['kind']=='binary')
    cfg=json.loads(node_config.read_text());ctx=ssl.create_default_context(cafile=cfg['tls']['ca']);ctx.load_cert_chain(cfg['tls']['cert'],cfg['tls']['key'])
    observation=request(op,'/api/v1/nodes')[1]['nodes']['node-a']
    # Node views wrap observations differently; read the authenticated detail field.
    detail=request(op,'/api/v1/nodes/node-a')[1]
    def find(value,key):
        if isinstance(value,dict):
            if key in value:return value[key]
            for child in value.values():
                found=find(child,key)
                if found is not None:return found
        return None
    session=find(detail,'session_id') or find(observation,'session_id')
    hardware=hashlib.sha256(b'explicit:'+Path(cfg['hardware_id_file']).read_bytes()).hexdigest()
    # Host hardware is hashed with its provider; use the committed enrollment record.
    registry=cli('get','nodes','node-a','--config',op)
    hardware=find(registry,'hardware_sha256') or hardware
    token=Path(cfg['token_file']).read_text().strip()
    headers={'Authorization':'Bearer '+token,'x-converyn-cluster':'workflow-test','x-converyn-protocol':str(next(p['schema']['const'] for p in request(op,'/openapi.json')[1]['paths']['/api/v1/agents/{node}/runs/{run}/work/{work}/artifacts/{sha}']['get']['parameters'] if p['name']=='x-converyn-protocol')),'x-converyn-hardware':hardware,'x-converyn-session':session,'Range':'bytes=1048576-'}
    route='/api/v1/agents/node-a/runs/run-managed-proof/work/'+work['id']+'/artifacts/'+sha
    def raw(path):
        for base in bases:
            try:
                with urllib.request.urlopen(urllib.request.Request(base+path,headers=headers),context=ctx,timeout=30) as response:return response.status,response.read()
            except urllib.error.HTTPError as error:return error.code,error.read()
            except OSError:continue
        raise AssertionError('no artifact endpoint')
    code,body=raw(route);assert code==206,(code,body[:300]);assert body==content[1048576:]
    assert raw(route[:-64]+'f'*64)[0]==403
    release.touch()
    final=until(lambda:(lambda r:r if r.get('status')=='succeeded' else None)(cli('get','runs','run-managed-proof','--config',op)),60)
    for uid,path in targets.items():
        assert hashlib.sha256(path.read_bytes()).hexdigest()==sha
        state=Path(json.loads(configs[uid].read_text())['state_dir'])
        assert hashlib.sha256((state/'artifacts'/sha).read_bytes()).hexdigest()==sha
    native=[a['receipt']['native'] for a in final['attempts'].values() if a.get('receipt',{}).get('native')]
    assert len(native)==5 and all(item['after']['sha256']==sha and item['after']['bytes']==size for item in native)
    assert 'managed-source' not in json.dumps(final)
    if killed is not None:assert hashlib.sha256(replica_paths[current].read_bytes()).hexdigest()==sha
    return {'managed_artifacts':True,'agents':5,'artifact_bytes':size,'sha256':sha,'controller_replicas_verified':len(controllers),'source_directory_removed':True,'killed_leader':killed,'range_download_verified':True,'assignment_authorization':True,'native_receipts_verified':True,'missing_replica_recovered':killed is not None,'progress_stderr_only':True,'progress_terminal_and_reuse':True}
