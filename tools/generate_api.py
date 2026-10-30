#!/usr/bin/env python3
"""Extract this workspace's Serde wire declarations and route registry.
No third party parser/codegen dependency; --check detects source/schema drift.
Unsupported external diagnostic types remain explicitly unconstrained JSON.
"""
from pathlib import Path
import re,json,argparse
ROOT=Path(__file__).resolve().parents[1]
VERSION=re.search(r'(?s)\[workspace.package\].*?\bversion\s*=\s*"([^"]+)"',(ROOT/'Cargo.toml').read_text())[1]
PROTOCOL=int(re.search(r'PROTOCOL_VERSION: u64 = (\d+)',(ROOT/'crates/runtime/src/controller/network.rs').read_text())[1])
parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
def balanced(s,start,left='{',right='}'):
 depth=1;i=start+1;quoted=False;escape=False
 while depth and i<len(s):
  c=s[i]
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  elif c==left:depth+=1
  elif c==right:depth-=1
  i+=1
 return i
def split(s):
 depth=0;out=[];start=0
 for i,c in enumerate(s):
  if c in '<({[':depth+=1
  elif c in '>)}]':depth-=1
  elif c==',' and depth==0:out.append(s[start:i].strip());start=i+1
 if s[start:].strip():out.append(s[start:].strip())
 return out
sources=[]
for folder in ['crates/core/src','crates/plugin-api/src','crates/runtime/src/controller/server']:
 sources+=sorted((ROOT/folder).rglob('*.rs'))
for folder in ['plugins/docker/src','plugins/kubernetes/src','plugins/linux-host/src','plugins/kubeadm/src']:
 sources+=sorted((ROOT/folder).glob('*.rs'))
sources += [ROOT/'crates/runtime/src/storage/store.rs',ROOT/'crates/runtime/src/storage/backup.rs']
declarations={};by_name={};context={}
for p in sources:
 source=p.read_text();namespace=str(p.relative_to(ROOT)).replace('/src/','/').replace('.rs','').replace('/','_').replace('-','_')
 for m in re.finditer(r'#\[derive\([^\]]*\)\]\s*(?:#\[serde\([^\]]*\)\]\s*)*pub(?:\([^)]*\))?\s+(struct|enum)\s+(\w+)\s*(?:\{)',source):
  if 'Serialize' not in m.group() and 'Deserialize' not in m.group():continue
  end=balanced(source,m.end()-1);name=m.group(2);key=namespace+'_'+name
  declarations[key]=(m.group(1),name,m.group(),source[m.end():end-1]);by_name.setdefault(name,[]).append(key);context[key]=namespace

def resolve(name,ctx):
 short=name.split('::')[-1]
 candidates=by_name.get(short,[])
 if not candidates:return None
 local=[k for k in candidates if context[k]==ctx]
 if local and "::" not in name:return local[0]
 hints=name.lower().split('::')
 aliases={'secrets':'variables','source':'artifact','runner':'receipt','native':'evidence','task_log':'logs','domain':'domain','machine':'machine_types','registration':'registration','identity':'identity','service':'service','package':'package','execution':'execution','access':'access'}
 for hint in reversed(hints[:-1]):
  hint=aliases.get(hint,hint);matches=[k for k in candidates if hint in context[k]]
  if len(matches)==1:return matches[0]
 if short in ['Identity','Call','Observation','Descriptor','ScriptOrigin'] and ('plugin' in name or 'api::' in name):
  matches=[k for k in candidates if 'plugin_api_lib' in k]
  if matches:return matches[0]
 if short=="State" and ctx.endswith("_storage_store"):
  matches=[k for k in candidates if "machine_types" in k]
  if matches:return matches[0]
 if len(candidates)==1:return candidates[0]
 matches=[k for k in candidates if 'crates_core' in k]
 if len(matches)==1:return matches[0]
 return None

def typ(t,ctx):
 t=t.strip().replace('std::sync::Arc<','Arc<').replace('std::boxed::Box<','Box<');t=re.sub(r'^&(?:\'\w+\s+)?','',t)
 if t.startswith('(') and t.endswith(')'):
  items=[typ(v,ctx) for v in split(t[1:-1])];return {'type':'array','prefixItems':items,'items':False,'minItems':len(items),'maxItems':len(items)}
 if t.startswith(('Box<','Arc<')):return typ(t[t.index('<')+1:-1],ctx)
 if t.startswith('Option<'):return {'anyOf':[typ(t[7:-1],ctx),{'type':'null'}]}
 if t.startswith(('Vec<','BTreeSet<','std::collections::BTreeSet<')):return {'type':'array','items':typ(t[t.index('<')+1:-1],ctx),**({'uniqueItems':True} if 'BTreeSet' in t else {})}
 if 'BTreeMap<' in t:return {'type':'object','additionalProperties':typ(split(t[t.index('<')+1:-1])[1],ctx)}
 for rust,key in [('LogId<','RaftLogId'),('StoredMembership<','RaftStoredMembership'),('AppendEntriesRequest<','RaftAppendEntriesRequest'),('VoteRequest<','RaftVoteRequest'),('InstallSnapshotRequest<','RaftInstallSnapshotRequest')]:
  if t.startswith(rust):return {'$ref':'#/components/schemas/'+key}
 if t=='BasicNode':return {'type':'object','properties':{'addr':{'type':'string'}},'required':['addr'],'additionalProperties':False}
 if t in ['String','str','PathBuf','std::path::PathBuf']:return {'type':'string'}
 if t=='bool':return {'type':'boolean'}
 if t in ['serde_json::Number','f64']:return {'type':'number'}
 if t in ['u8','u16','u32','u64','usize','i32','i64']:
  return {'type':'integer',**({'minimum':0} if t.startswith('u') else {})}
 if t.endswith('outputs::Specifications'):return {'type':'object','additionalProperties':{'$ref':'#/components/schemas/'+resolve('outputs::Specification',ctx)}}
 if t.endswith('outputs::Bindings'):return {'type':'object','additionalProperties':{'$ref':'#/components/schemas/'+resolve('outputs::Reference',ctx)}}
 if t.endswith('outputs::Values'):return {'type':'object','additionalProperties':{'$ref':'#/components/schemas/'+resolve('outputs::StoredValue',ctx)}}
 if t.endswith('values::Values') or t=='Values':return {'type':'object','additionalProperties':{}}
 if t.endswith('Variables'):return {'type':'object','additionalProperties':{'oneOf':[{'type':'string'},{'type':'object','properties':{'secret':{'type':'string'},'version':{'type':'string'}},'required':['secret','version'],'additionalProperties':False},{'$ref':'#/components/schemas/'+resolve('converyn_plugin_api::variables::OutputSecretReference',ctx)}]}}
 if t in ['Value','serde_json::Value']:return {}
 key=resolve(t,ctx)
 return {'$ref':'#/components/schemas/'+key} if key else {}

def fields(body,ctx,deny=False):
 props={};required=[]
 for raw in split(re.sub(r'///[^\n]*','',body)):
  attrs=' '.join(re.findall(r'#\[([^\]]*)\]',raw,re.S));raw=re.sub(r'#\[[^\]]*\]','',raw,flags=re.S).strip()
  m=re.fullmatch(r'(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.+)',raw,re.S)
  if not m:continue
  name,t=m.groups()
  renamed=re.search(r'rename\s*=\s*"([^"]+)"',attrs)
  if renamed:name=renamed.group(1)
  if re.search(r'(?:^|[, (])skip(?:[, )]|$)',attrs):continue
  props[name]=typ(t,ctx)
  if 'default' not in attrs and not t.startswith('Option<'):required.append(name)
  function=re.search(r'default\s*=\s*"([^"]+)"',attrs)
  defaults={'converyn_plugin_api::default_namespace':'default','retention_days':30,'retention':30,'revision_one':1,'default_download_parallel':16,'loopback':'127.0.0.1','user':65534,'memory':268435456,'cpu':1000,'page_limit':50,'default_limit':50,'audit_stream':'commands','audit_view':'all'}
  if function and function.group(1) in defaults:props[name]['default']=defaults[function.group(1)]
 return {'type':'object','properties':props,**({'required':required} if required else {}),**({'additionalProperties':False} if deny else {})}

def snake(s):return re.sub(r'(?<!^)(?=[A-Z])','_',s).lower()
schemas={}
for key,(kind,name,attrs,body) in declarations.items():
 ctx=context[key]
 if kind=='struct':schemas[key]=fields(body,ctx,'deny_unknown_fields' in attrs)
 else:
  tag=re.search(r'tag\s*=\s*"([^"]+)"',attrs);content_tag=re.search(r'content\s*=\s*"([^"]+)"',attrs);variants=[]
  for raw in split(re.sub(r'///[^\n]*','',body)):
   m=re.match(r'\s*(\w+)\s*(.*)',raw,re.S)
   if not m:continue
   variant,rest=m.groups();wire=snake(variant) if 'snake_case' in attrs else variant
   if rest.startswith('{'):
    value=fields(rest[1:-1],ctx,'deny_unknown_fields' in attrs)
    if tag and content_tag:value={'type':'object','properties':{tag.group(1):{'const':wire},content_tag.group(1):value},'required':[tag.group(1),content_tag.group(1)],'additionalProperties':False}
    elif tag:value['properties'][tag.group(1)]={'const':wire};value.setdefault('required',[]).append(tag.group(1))
    else:value={'type':'object','properties':{wire:value},'required':[wire],'additionalProperties':False}
   elif rest.startswith('('):
    value=typ(rest[1:-1],ctx)
    if tag and content_tag:value={'type':'object','properties':{tag.group(1):{'const':wire},content_tag.group(1):value},'required':[tag.group(1),content_tag.group(1)],'additionalProperties':False}
    elif 'untagged' not in attrs:value={'type':'object','properties':{wire:value},'required':[wire],'additionalProperties':False}
   else:value={'type':'object','properties':{tag.group(1):{'const':wire}},'required':[tag.group(1)],'additionalProperties':False} if tag else {'const':wire}
   variants.append(value)
  schemas[key]={'oneOf':variants}

def ref(short,hint=''):
 candidates=by_name.get(short,[]);keys=[k for k in candidates if hint in k]
 return {'$ref':'#/components/schemas/'+keys[0]} if len(keys)==1 else {}
schemas.update({
 'ApiError':{'type':'object','properties':{'code':{'type':'string'},'message':{'type':'string'},'retryable':{'type':'boolean'},'request_id':{'type':'string'}},'required':['code','message','retryable','request_id']},
 'LogStreamError':{'type':'object','properties':{'status':{'type':'integer'},'code':{'type':'string'},'message':{'type':'string'},'retryable':{'type':'boolean'},'request_id':{'type':'string'}},'required':['status','code','message','retryable','request_id']},
 'Page':{'type':'object','properties':{'items':{'type':'array','items':{'type':'object','properties':{'id':{'type':'string'},'value':{}},'required':['id','value']}},'next_cursor':{'type':['string','null']}},'required':['items','next_cursor']},
})
def obj(**props):return {'type':'object','properties':props,'required':list(props)}
def arr(s):return {'type':'array','items':s}
def nullable(s):return {'anyOf':[s,{'type':'null'}]}
def mapping(s):return {'type':'object','additionalProperties':s}
string={'type':'string'};integer={'type':'integer','minimum':0};boolean={'type':'boolean'}
run_summary=obj(id=string,status=ref('Status','machine_types'),revision=integer,node_group_uid=nullable(string),owner=string,total=integer,completed=integer)
run_summary['properties']['submitted_ms']=nullable(integer)
run_summary['properties']['finished_ms']=nullable(integer)
run_summary['properties']['archived']=boolean
run_summary['properties']['execution']=nullable(ref('Execution','orchestration_workflow'))
responses={
 'secret_catalog':obj(secrets=arr(obj(name=string,version=string))),
 'connections':obj(connections=arr(obj(uid=string,plugin=ref('Identity','plugin_api_lib'),targets=arr(string),profiles=arr(string)))),
 'list_runs':obj(runs=arr(run_summary),next_cursor=nullable(string)),
 'plugin_catalog':obj(plugins=arr(ref('Descriptor','plugin_api_lib')),manifest_sha256=string),
 'targets':obj(targets=mapping(ref('ExternalTarget'))),
 'nodes':obj(nodes=mapping(ref('NodeObservation'))),
 'node':obj(nodes=mapping(ref('NodeObservation'))),
 'node_groups':obj(node_groups=mapping(ref('NodeGroup')),administration=ref('Administration'),resource_owners=mapping(string)),
 'registered_nodes':obj(nodes=arr(obj(uid=string,hardware_sha256=string,fingerprint=string,expires_ms=integer,retired=boolean,renewal_pending=boolean))),
 'certificate_bindings':obj(identities=ref('Identities')),
 'history_catalog':{**ref('History','orchestration_history')},
 'drift_status':obj(policy=nullable(ref('Policy','orchestration_drift'))),
 'issue_bootstrap':obj(grant=ref('Bootstrap','registration')),
 'enroll_node':obj(certificate=nullable(ref('Signed','certificate'))),
 'renew_node':obj(certificate=nullable(ref('Signed','certificate'))),
 'activate_renewal':obj(ok=boolean),'retire_node':obj(ok=boolean),
 'initialize':obj(initialized=boolean),'trigger_snapshot':obj(requested=boolean),'ready':obj(ready=boolean),
 'access_check':obj(allowed=boolean,action=ref('Action','identities_access'),workflow=nullable(string),preset=nullable(string),reason=string),
 'access_me':obj(revision=integer,bindings=arr(ref('Binding','identities_access')),platform=boolean,access=boolean,super_admin=boolean),
 'access_list':obj(revision=integer,groups=mapping(ref('Group','identities_access')),bindings=mapping(ref('Binding','identities_access')),roles=arr(ref('Role','identities_access')),workflows=arr(obj(uid=string,name=string,presets=arr(string)))),
 'me':obj(capabilities=obj(platform=boolean,access=boolean,super_admin=boolean),principal=nullable(string),identity=nullable(ref('Identity','identities_personnel')),credential=nullable(obj(uid=string,fingerprint=string,expires_ms=integer,overlap_until_ms=nullable(integer)))),
 'workflows':obj(workflows=arr(obj(workflow=ref('Workflow','orchestration_workflow'),definition=ref('Revision','orchestration_workflow'),actions=obj(maintain=boolean,execute=arr(string)))),next_cursor=nullable(string)),
 'workflow_detail':obj(actions=obj(maintain=boolean,execute=arr(string)),workflow=ref('Workflow','orchestration_workflow'),definition=ref('Revision','orchestration_workflow'),reconciliation=arr(obj(target=string,deployment=obj(condition=string,last_run=nullable(string)),policy=nullable(ref('Policy','orchestration_drift'))))),
 'workflow_preview':obj(request=ref('Request','orchestration_workflow'),plan=ref('Plan','model_domain'),parameters=mapping({}),rendered_scripts=mapping(string)),
 'workflow_executions':obj(executions=mapping(ref('Execution','orchestration_workflow'))),
 'personnel_list':obj(identities=mapping(ref('Identity','identities_personnel'))),
 'credential_detail':obj(credential=obj(uid=string,identity_uid=string,identity_name=nullable(string),fingerprint=string,not_before_ms={'type':'integer'},expires_ms=integer,revoked_ms=nullable(integer),overlap_until_ms=nullable(integer),active=boolean,serial=nullable(string),certificate_pem=string),storage=mapping(string)),
 'personal_credentials':obj(credentials=arr(obj(uid=string,identity_uid=string,identity_name=nullable(string),fingerprint=string,not_before_ms={'type':'integer'},expires_ms=integer,revoked_ms=nullable(integer),overlap_until_ms=nullable(integer),active=boolean,serial=nullable(string)))),
 'audit':obj(records=arr({}),stream=string,view={'type':'string','enum':['activity','observations','all']},next_cursor=nullable(integer)),
 'work_inputs':obj(values=mapping(string),redactions=arr(arr(integer))),
 'agent_attempt':obj(output_key=nullable(ref('PublicKey','model_outputs')),work=ref('Work','model_domain'),attempt=ref('Attempt','machine_types'),run_status=ref('Status','machine_types'),project=nullable(string),owner_uid=string),
 'agent_script':obj(sha256=string,content=string),
 'poll_work':obj(cursor=nullable(integer),node_uid=string,work=arr(obj(output_key=nullable(ref('PublicKey','model_outputs')),run_id=string,run_sequence=integer,run_revision=integer,project=nullable(string),owner_uid=string,work=ref('Work','model_domain'),attempt=ref('Attempt','machine_types'),claimable=boolean,prefetch=boolean)),artifact_pins=mapping(integer),maintenance_until_ms=nullable(integer)),
 'upload_live':obj(ok=boolean,next_offset=integer), 'upload_log':obj(ok=boolean,next_offset=integer),'read_log':obj(offset=integer,next_offset=integer,data=string,complete=boolean,expired=boolean,truncated=boolean),'log_storage_status':obj(enabled=boolean,available=boolean,store_id=nullable(string)),
 'backup':{},
 'metrics':obj(consistency={'const':'local'},metrics={'type':'object'},writes=obj(**{k:integer for k in ['limit','ordinary_limit','in_flight','ordinary_in_flight','submitted','committed','conflicts','queue_full','outcome_pending','failures','last_success_ms','last_failure_ms']})),
 'member_status':obj(membership={'type':'object'},config_fingerprint=string,voters=arr(integer),operations=ref('Catalog','orchestration_controlplane'),replication=nullable({'type':'object'})),
}
responses['artifact_catalog']=obj(artifacts=arr(obj(sha256=string,size=integer)))
for handler in ['upload_artifact_chunk','peer_artifact_chunk']:responses[handler]=obj(ok=boolean)
responses['upload_artifact_chunk']['properties']['next_offset']=integer
responses['peer_artifact_complete']=obj(ok=boolean)
responses['work_definition']=obj(sha256=string,content=string,rendered_content=string)
responses['read_log']['properties'].update(source=string,sha256=nullable(string),verified=boolean,missing_ranges=arr({'type':'array','prefixItems':[integer,integer],'items':False,'minItems':2,'maxItems':2}),condition=string)
def external(key):return {'$ref':'#/components/schemas/'+key}
schemas['RaftLeaderId']=obj(term=integer,node_id=integer)
schemas['RaftLogId']=obj(leader_id=external('RaftLeaderId'),index=integer)
schemas['RaftVote']=obj(leader_id=external('RaftLeaderId'),committed=boolean)
schemas['RaftMembership']=obj(configs=arr(arr(integer)),nodes=mapping(typ('BasicNode','')))
schemas['RaftStoredMembership']=obj(log_id=nullable(external('RaftLogId')),membership=external('RaftMembership'))
schemas['RaftSnapshotMeta']=obj(last_log_id=nullable(external('RaftLogId')),last_membership=external('RaftStoredMembership'),snapshot_id=string)
schemas['RaftEntry']=obj(log_id=external('RaftLogId'),payload={'oneOf':[{'const':'Blank'},obj(Normal=ref('Command','machine_types')),obj(Membership=external('RaftMembership'))]})
schemas['RaftAppendEntriesRequest']=obj(vote=external('RaftVote'),prev_log_id=nullable(external('RaftLogId')),entries=arr(external('RaftEntry')),leader_commit=nullable(external('RaftLogId')))
schemas['RaftVoteRequest']=obj(vote=external('RaftVote'),last_log_id=nullable(external('RaftLogId')))
schemas['RaftInstallSnapshotRequest']=obj(vote=external('RaftVote'),meta=external('RaftSnapshotMeta'),offset=integer,data=arr({'type':'integer','minimum':0,'maximum':255}),done=boolean)
schemas['RaftMetrics']=obj(running_state={'oneOf':[obj(Ok={'type':'null'}),obj(Err={})]},id=integer,current_term=integer,vote=external('RaftVote'),last_log_index=nullable(integer),last_applied=nullable(external('RaftLogId')),snapshot=nullable(external('RaftLogId')),purged=nullable(external('RaftLogId')),state={'enum':['Learner','Follower','Candidate','Leader','Shutdown'],'type':'string'},current_leader=nullable(integer),millis_since_quorum_ack=nullable(integer),membership_config=external('RaftStoredMembership'),replication=nullable(mapping(nullable(external('RaftLogId')))))
responses['metrics']['properties']['metrics']=external('RaftMetrics')
responses['member_status']['properties']['membership']=external('RaftStoredMembership')
responses['member_status']['properties']['replication']=nullable(mapping(nullable(external('RaftLogId'))))
responses['append']={'oneOf':[obj(Ok={'oneOf':[{'enum':['Success','Conflict'],'type':'string'},obj(PartialSuccess=nullable(external('RaftLogId'))),obj(HigherVote=external('RaftVote'))]}),obj(Err={})]}
responses['vote']={'oneOf':[obj(Ok=obj(vote=external('RaftVote'),vote_granted=boolean,last_log_id=nullable(external('RaftLogId')))),obj(Err={})]}
responses['snapshot']={'oneOf':[obj(Ok=obj(vote=external('RaftVote'))),obj(Err={})]}
history=schemas[resolve('History','orchestration_history')]
responses['history_catalog']={**history,'properties':{**history['properties'],'next_cursor':nullable(string),'run_sequence':integer},'required':[*history.get('required',[]),'next_cursor','run_sequence']}
for handler in ['nodes','node_groups','targets','registered_nodes']:responses[handler]['properties']['next_cursor']=nullable(string);responses[handler]['required'].append('next_cursor')
handlers={}
for p in (ROOT/'crates/runtime/src/controller/server').glob('*.rs'):
 s=p.read_text()
 for m in re.finditer(r'(?:pub(?:\([^)]*\))?\s+)?async fn (\w+)\(',s):
  end=s.find('->',m.end());signature=s[m.end():end];body=re.search(r'Json(?:\([^)]*\))?\s*:\s*Json<([^>]+(?:>[^,)]*)?)>',signature)
  # The last Json extractor is always the body. Generic argument nesting is
  # handled by matching angle brackets instead of guessing at closing > chars.
  typed=re.search(r':\s*Json<',signature)
  request={}
  if typed:
   start=typed.end()-1;finish=balanced(signature,start,'<','>');request=typ(signature[start+1:finish-1],str(p.relative_to(ROOT)).replace('/src/','/').replace('.rs','').replace('/','_').replace('-','_'))
  handlers[m.group(1)]=request
paths={};source=(ROOT/'crates/runtime/src/controller/server/mod.rs').read_text()
for m in re.finditer(r'\.route\(\s*"([^"]+)"\s*,',source):
 path=m.group(1);i=m.end();depth=1
 while depth:
  if source[i]=='(':depth+=1
  elif source[i]==')':depth-=1
  i+=1
 route=source[m.end():i-1]
 for method,handler in re.findall(r'\b(get|post)\((\w+)\)',route):
  operation={'operationId':handler+'_'+re.sub(r'[^A-Za-z0-9]+','_',path).strip('_'),'summary':handler.replace('_',' '),'security':[{'bearerAuth':[],'mutualTLS':[]}],'parameters':[{'name':n,'in':'path','required':True,'schema':{'type':'string'}} for n in re.findall(r'{(\w+)}',path)],'responses':{str(status):{'description':label,'content':{'application/json':{'schema':{'$ref':'#/components/schemas/ApiError'}}}} for status,label in [(400,'Invalid request'),(401,'Authentication failed'),(403,'Permission denied'),(404,'Not found'),(405,'Method not allowed'),(410,'Expired resource'),(409,'Conflict'),(422,'Validation failed'),(426,'Protocol upgrade required'),(501,'Feature not configured'),(429,'Capacity full'),(503,'Temporarily unavailable')]}}
  schema=ref('Reply','machine_types') if method=='post' else {}
  if handler in responses:schema=responses[handler]
  if handler=='run':schema={'allOf':[ref('Run','machine_types'),obj(execution=nullable(ref('Execution','orchestration_workflow')))]}
  if handler=='deployment':schema=ref('Deployment','machine_types')
  if operation['security']:
   operation['parameters'] += [{'name':'x-converyn-cluster','in':'header','required':True,'schema':{'type':'string'},'description':'Control-plane cluster identity, not the managed host cluster UID'},{'name':'x-request-id','in':'header','schema':{'type':'string','minLength':1,'maxLength':128,'pattern':'^[A-Za-z0-9-]+$'},'description':'Optional correlation ID; not authorization'}]
  if '/agents/' in path or path=='/api/v1/identity/enroll':
   operation['parameters'] += [{'name':'x-converyn-protocol','in':'header','required':True,'schema':{'type':'integer','const':PROTOCOL}}]
  if '/agents/' in path:
   operation['parameters'] += [{'name':name,'in':'header','schema':{'type':'string'},'description':description} for name,description in [('x-converyn-hardware','Required when this Node is bound to hardware'),('x-converyn-session','Required for bound Nodes except heartbeat/renew/activate')]]
  if path.startswith('/internal/'):
   operation['parameters'] += [{'name':'x-converyn-protocol','in':'header','required':True,'schema':{'type':'integer','const':PROTOCOL}},{'name':'x-converyn-plugin-manifest','in':'header','required':True,'schema':{'type':'string','pattern':'^[a-f0-9]{64}$'}}]
  operation['responses']['200']={'description':'Success (submission is not execution completion)','content':{'application/json':{'schema':schema}}}
  if method=='post' and handlers.get(handler):operation['requestBody']={'required':True,'content':{'application/json':{'schema':handlers[handler]}}}
  if handler=='node_group_command':
   key=resolve('Change','crates_core_orchestration_administration');operation['requestBody']['content']['application/json']['schema']={'oneOf':[v for v in schemas[key]['oneOf'] if v.get('properties',{}).get('action',{}).get('const') not in ['secret','log_storage','log_final','log_pruned','artifact']]}
  if handler=='agent_command':
   key=resolve('Command','');operation['requestBody']['content']['application/json']['schema']={'oneOf':[v for v in schemas[key]['oneOf'] if v.get('properties',{}).get('type',{}).get('const') in ['claim','report','preparation']]}
  if path=='/openapi.json':operation['security']=[]
  if handler in ['objects','nodes','node_groups','targets','history_catalog','registered_nodes','list_runs','audit','read_log','log_events','poll_work','workflows','access_check']:
   params={'objects':{'after':'string','limit':'integer','status':'string'},'workflows':{'after':'string','limit':'integer','status':'string','search':'string'},'access_check':{'action':'string','workflow':'string','preset':'string'},'list_runs':{'after':'string','limit':'integer','status':'string','workflow':'string','preset':'string','actor':'string'},'audit':{'after':'integer','limit':'integer','stream':'string','view':'string'},'read_log':{'offset':'integer'},'log_events':{'offset':'integer'},'poll_work':{'after':'integer','wait_ms':'integer'}}.get(handler,{'after':'string','limit':'integer','status':'string'})
   operation['parameters'] += [{'name':n,'in':'query','schema':{'type':t}} for n,t in params.items()]
   for parameter in operation['parameters']:
    name=parameter['name'];schema=parameter['schema']
    if handler=='audit' and name=='view':schema.update(enum=['activity','observations','all'],default='all')
    if name=='limit':schema.update(minimum=1,maximum=100,default=50)
    if name=='wait_ms':schema.update(minimum=0,maximum=10000,default=0)
    if name=='offset':schema.update(minimum=0,maximum=8388608,default=0)
    if name=='after' and schema.get('type')=='integer':schema.update(minimum=0,default=0)
  if handler=='metrics_prometheus':operation['responses']['200']['content']={'text/plain':{'schema':{'type':'string'}}}
  if handler=='log_events':
   operation['responses']['200']['content']={'text/event-stream':{'schema':{'type':'string'}}}
   operation['responses']['200']['description']='SSE event log carries a log chunk; event error carries LogStreamError with status, code, message, retryable and request_id. Resume at the last accepted byte offset only for retryable errors.'
  if handler=='download_log':
   operation['responses']['200']['content']={'application/octet-stream':{'schema':{'type':'string','format':'binary'}}};operation['responses']['200']['headers']={'x-converyn-sha256':{'description':'Expected SHA-256 of all saved log bytes','schema':{'type':'string'}}}
  if handler=='backup':operation['responses']['200']['content']={'application/vnd.converyn.backup.v2':{'schema':{'type':'string','format':'binary'}}}
  if handler in ['upload_artifact_chunk','peer_artifact_chunk','complete_artifact','peer_artifact_complete','peer_artifact_read']:
   operation['parameters'].append({'name':'size','in':'query','required':True,'schema':{'type':'integer','minimum':1,'maximum':10737418240}})
  if handler in ['upload_artifact_chunk','peer_artifact_chunk']:
   operation['parameters'].append({'name':'offset','in':'query','schema':{'type':'integer','minimum':0,'default':0,'multipleOf':1048576}})
   operation['requestBody']={'required':True,'content':{'application/octet-stream':{'schema':{'type':'string','format':'binary','maxLength':1048576}}}}
  if handler in ['peer_artifact_read','work_artifact']:
   operation['parameters'].append({'name':'Range','in':'header','schema':{'type':'string','pattern':'^bytes=[0-9]+-$'}})
   operation['responses']['200']['content']={'application/octet-stream':{'schema':{'type':'string','format':'binary'}}}
   operation['responses']['206']={'description':'Requested immutable byte range','content':{'application/octet-stream':{'schema':{'type':'string','format':'binary'}}}}
  if handler=='trigger_snapshot':operation['responses']['202']=operation['responses'].pop('200')
  if path.startswith('/internal/') or '/prototype/' in path:operation['tags']=['internal'];operation['description']='Internal development/consensus protocol; not a stable business interface.'
  paths.setdefault(path,{})[method]=operation
# Anonymous live handler and static assets do not have JSON request DTOs.
for path,media in [('/openapi.json','application/json'),('/health/live','application/json'),('/','text/html'),('/assets/app.js','text/javascript'),('/assets/index.css','text/css'),('/assets/version.json','application/json')]:
 paths.setdefault(path,{'get':{'operationId':'public_'+path.replace('/','_').replace('.','_'),'security':[],'responses':{'200':{'description':'Public resource (TLS listener still validates client certificate)','content':{media:{'schema':{'type':'object'} if media=='application/json' else {'type':'string'}}}}}}})
paths['/assets/version.json']['get']['responses']['200']['content']['application/json']['schema']=obj(version=string)
paths['/health/live']['get']['responses']['200']['content']['application/json']['schema']=obj(live=boolean,stage=string,protocol=integer,plugin_manifest=string,node_id=integer,control_plane=string,pristine=boolean,log_storage=nullable(obj(id=string,shared=boolean,probe=string)))
openapi={'openapi':'3.1.0','info':{'title':'Converyn API','version':VERSION},'servers':[{'url':'/'}],'components':{'securitySchemes':{'bearerAuth':{'type':'http','scheme':'bearer'},'mutualTLS':{'type':'mutualTLS'}},'schemas':schemas},'paths':dict(sorted(paths.items()))}
def ts(schema):
 if '$ref' in schema:return schema['$ref'].split('/')[-1]
 if 'const' in schema:return json.dumps(schema['const'])
 if 'enum' in schema:return '('+' | '.join(json.dumps(v) for v in schema['enum'])+')'
 if 'oneOf' in schema or 'anyOf' in schema:return '('+' | '.join(ts(s) for s in schema.get('oneOf',schema.get('anyOf',[])))+')'
 t=schema.get('type')
 if isinstance(t,list):return ' | '.join(ts({'type':v}) for v in t)
 if t=='null':return 'null'
 if t=='string':return 'string'
 if t in ['integer','number']:return 'number'
 if t=='boolean':return 'boolean'
 if t=='array':
  if 'prefixItems' in schema:return '['+', '.join(ts(v) for v in schema['prefixItems'])+']'
  return '('+ts(schema.get('items',{}))+')[]'
 if t=='object':
  if 'properties' not in schema:return 'Record<string, '+ts(schema.get('additionalProperties',{}))+'>'
  return '{ '+ '; '.join(json.dumps(k)+('' if k in schema.get('required',[]) else '?')+': '+ts(v) for k,v in schema['properties'].items())+' }'
 return 'unknown'
types='// Generated by tools/generate_api.py; do not edit.\n'+''.join('export type '+k+' = '+ts(v)+';\n' for k,v in sorted(schemas.items()))
for name,hint in [('Run','machine_types'),('Attempt','machine_types'),('Workflow','orchestration_workflow'),('Revision','orchestration_workflow'),('Identity','identities_personnel'),('Credential','identities_personnel'),('Role','identities_access'),('Request','orchestration_workflow')]:
 candidate=ref(name,hint)
 if candidate:types+='export type '+name+' = '+ts(candidate)+';\n'
outputs={ROOT/'docs/schema/openapi.json':json.dumps(openapi,ensure_ascii=False,indent=2)+'\n',ROOT/'web/src/api-types.ts':types}
for p,content in outputs.items():
 if args.check:
  if not p.exists() or p.read_text()!=content:raise SystemExit(f'Generated API contract stale: {p}')
 else:p.write_text(content)
print(f'{len(paths)} paths, {len(schemas)} schemas; contracts '+('verified' if args.check else 'generated'))
