#!/usr/bin/env python3
"""Run the guarded native dpkg test in the local, network-free fixture image."""
import argparse,hashlib,json,os,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--test-build',type=Path,required=True);p.add_argument('--fixtures',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--image',default='converyn-dpkg-fixture:local');a=p.parse_args()
binary=None
for line in a.test_build.read_text().splitlines():
 item=json.loads(line)
 if item.get('reason')=='compiler-artifact' and item.get('target',{}).get('name')=='package_live' and item.get('executable'):binary=Path(item['executable']).resolve()
assert binary
configured_env=dict(os.environ)
for key in ['DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH']:configured_env.pop(key,None)
docker=['docker','--host=unix:///var/run/docker.sock'];image=subprocess.check_output(docker+['image','inspect','--format','{{.Id}}',a.image],text=True,env=configured_env).strip()
with tempfile.TemporaryDirectory(prefix='dpkg-result-',dir='.local') as result_dir:
 results=Path(result_dir).resolve();cmd=docker+['run','--rm','--network=none','--security-opt=no-new-privileges','-e','CONVERYN_PACKAGE_TEST=isolated-dpkg-fixture','--mount','type=bind,src='+str(binary)+',dst=/test,readonly','--mount','type=bind,src='+str(a.fixtures.resolve())+',dst=/fixtures,readonly','--mount','type=bind,src='+str(results)+',dst=/results','--mount','type=bind,src='+str(Path('.local/dpkg-fixture/rootfs/converyn-test-image').resolve())+',dst=/converyn-test-image,readonly',image,'/test','--ignored','--nocapture']
 outcome=subprocess.run(cmd,env=configured_env,text=True,capture_output=True)
 result=json.loads((results/'dpkg.json').read_text()) if outcome.returncode==0 else {'passed':False,'exit_code':outcome.returncode,'diagnostic':(outcome.stdout+outcome.stderr)[-5000:]}
 result.update(image=image,image_reference=a.image,environment_scope='Package-manager validation in selected OCI image, not a full VM qualification',test_binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest());a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'passed':result['passed'],'image':image}));assert result['passed'],result.get('diagnostic')
