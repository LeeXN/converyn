"""A real public boolean output binds a Node plugin call and survives Claim replay."""
import json,os,shutil

def check(root,op,cli,request,until,controllers,start,children):
 source=root/'plugin-input';source.mkdir()
 (source/'produce.sh').write_text('set -eu\nprintf \'{"requested":false}\' | "$CONVERYN_EXECUTABLE" task emit --stdin\n')
 definition={'uid':'plugin-input-proof','name':'Plugin materialization and claim contract proof','inventory':{'nodes':[{'uid':'node-a','address':'192.0.2.1','hostname':'node-a'}],'groups':{}},'targets':{'qa':{'hosts':'all','node_group_uid':'hosts'}},'playbook':{'api_version':'converyn/v1','name':'plugin-input','steps':[{'id':'produce','phase':'prepare','scope':'per_node','hosts':'all','outputs':{'requested':{'type':'boolean'}},'operation':{'kind':'exec','script':'produce.sh','timeout_seconds':30}},{'id':'consume','phase':'rollout','scope':'per_node','hosts':'all','inputs':{'/call/spec/requested':{'step':'produce','name':'requested'}},'operation':{'kind':'plugin','id':'converyn.linux-host','action':'initramfs','spec':{'requested':False}}}]}}
 (source/'workflow.yaml').write_text(json.dumps(definition));cli('register','workflows','--file',source,'--config',op)
 cli('run','workflows',definition['uid'],'--target','qa','--request-id','plugin-input-proof','--config',op)
 until(lambda:request(op,'/api/v1/runs/run-plugin-input-proof')[1].get('status') in ['succeeded','failed'],90)
 run=cli('get','runs','run-plugin-input-proof','--config',op)
 recovery=False
 if os.environ.get('CONVERYN_PLUGIN_FIX_REPLACEMENT'):
  assert run['status']=='failed'
  saved=next(a['receipt'] for a in run['attempts'].values() if a['status']=='succeeded')
  assert any((a.get('reason') or '')=='input_materialization:plugin contract not supplied' for a in run['attempts'].values())
  for child,log in children:
   if child.poll() is None:child.terminate();child.wait(timeout=15)
  shutil.copy2(os.environ['CONVERYN_PLUGIN_FIX_REPLACEMENT'],root/'converyn.fixed');os.replace(root/'converyn.fixed',root/'converyn')
  for i in range(1,len(controllers)+1):controllers[i-1]=start('server','--config',root/'control'/('controller-'+str(i))/'server.json')
  until(lambda:request(op,'/api/v1/runs/run-plugin-input-proof')[0]==200,40)
  start('agent','--config',root/'agent.json')
  cli('resume','runs','run-plugin-input-proof','--config',op)
  until(lambda:request(op,'/api/v1/runs/run-plugin-input-proof')[1].get('status') in ['succeeded','failed'],90)
  run=cli('get','runs','run-plugin-input-proof','--config',op)
  producer=next(w for w in run['plan']['work'] if w['step_id']=='produce');assert run['attempts'][producer['id']]['receipt']==saved
  recovery=True
 assert run['status']=='succeeded'

 assert all(a['status']=='succeeded' for a in run['attempts'].values())
 consumer=next(w for w in run['plan']['work'] if w['step_id']=='consume');attempt=run['attempts'][consumer['id']]
 assert attempt['materialized']['work']['operation']['call']['spec']['requested'] is False
 assert attempt['receipt']['native']['plugin']['verified'] is True
 return {'plugin_input_claim':True,'recovered_same_run':recovery,'materialized_false_preserved':True,'run':run['id']}
