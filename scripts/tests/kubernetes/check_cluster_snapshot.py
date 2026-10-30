#!/usr/bin/env python3
"""Check registered inventory preservation with real offline plan compilation."""
from _paths import EXAMPLE, TOOLS
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import yaml

import render

PARSER = argparse.ArgumentParser(description=__doc__)
PARSER.add_argument('--converyn', type=Path, required=True)
PARSER.add_argument('--assets', type=Path, required=True)
ARGS = PARSER.parse_args()


class RegisteredNodeGroup(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='converyn-node-group-')
        cls.root = Path(cls.temp.name)
        cls.config = yaml.safe_load((render.ROOT / 'cluster.yaml').read_text())
        cls.assets = json.loads(ARGS.assets.read_text())
        cls.binary = ARGS.converyn.resolve()
        cls.original = cls.root / 'original'
        render.render(cls.config, cls.assets, cls.binary, cls.original)
        cls.snapshot = {'uid': cls.config['node_group_uid'], 'name': 'Approved hosts', 'revision': 7,
            'node_uids': [n['uid'] for n in reversed(cls.config['nodes'])] + ['unused-node']}
        cls.before = copy.deepcopy(cls.snapshot)
        cls.output = cls.root / 'registered'
        render.render(cls.config, cls.assets, cls.binary, cls.output, cls.original / 'private-kubeadm', cls.snapshot)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_platform_membership_does_not_define_deployment_order(self):
        command=json.loads((self.output/'node-group.json').read_text())
        self.assertEqual(command,{'action':'update_node_group','expected_revision':6,'group':self.snapshot})
        self.assertEqual(self.snapshot,self.before)
        expected=[n['uid'] for n in self.config['nodes']]
        for stage in json.loads((self.output/'sequence.json').read_text()):
            inp=json.loads((self.output/'plans'/(stage['stage']+'.input.json')).read_text())
            self.assertEqual(inp['hosts'],{'node_group_uid':self.snapshot['uid'],'revision':7,'node_uids':expected})
            self.assertEqual([n['uid'] for n in inp['inventory']['nodes']],expected)
            self.assertNotIn('unused-node',inp['hosts']['node_uids'])
            compiled=json.loads(subprocess.check_output([str(self.binary),'-o','json','compile','plans','--file',str(self.output/'plans'/(stage['stage']+'.input.yaml'))],text=True))
            self.assertEqual(compiled['sha256'],stage['preview_sha256'])
        native=yaml.safe_load((self.output/'workflow/workflow.yaml').read_text())
        self.assertEqual(native['targets']['default']['node_group_uid'],self.snapshot['uid'])
        self.assertEqual(native['inventory'],'inventory.yaml')
        for step in native['playbook']['steps']:
            self.assertNotIn('node_uids',step)
            self.assertIn('hosts',step)

    def test_cli_accepts_registered_node_group(self):
        snapshot=self.root/'group.json';snapshot.write_text(json.dumps(self.snapshot))
        config=self.root/'config.json';config.write_text(json.dumps(self.config));output=self.root/'cli'
        import hashlib
        content=b'fixture artifact for native managed registration'
        digest=hashlib.sha256(content).hexdigest();blobs=self.root/'blobs';blobs.mkdir()
        (blobs/digest).write_bytes(content)
        assets=copy.deepcopy(self.assets)
        for item in assets['artifacts']:item.update(sha256=digest,size=len(content))
        manifest=self.root/'assets.json';manifest.write_text(json.dumps(assets))
        subprocess.run([sys.executable,str(TOOLS/'render.py'),'--config',str(config),'--assets',str(manifest),'--converyn',str(self.binary),'--output',str(output),'--kubeadm-kit',str(self.original/'private-kubeadm'),'--node-group-snapshot',str(snapshot)],check=True,capture_output=True)
        self.assertEqual(json.loads((output/'node-group.json').read_text())['group'],self.snapshot)
        self.assertEqual((output/'private-kubeadm/ca.crt').read_bytes(),(self.original/'private-kubeadm/ca.crt').read_bytes())

    def test_scope_and_old_snapshot_shapes_fail_before_writing(self):
        changes=[lambda s:s.update(uid='other'),lambda s:s.update(revision=0),lambda s:s.update(revision=True),lambda s:s['node_uids'].pop(0)]
        for i,mutate in enumerate(changes):
            snapshot=copy.deepcopy(self.snapshot);mutate(snapshot);output=self.root/('invalid-'+str(i))
            with self.assertRaises(ValueError):render.render(self.config,self.assets,self.binary,output,node_group_snapshot=snapshot)
            self.assertFalse(output.exists())
        with self.assertRaisesRegex(ValueError,'node group snapshot'):
            render.render(self.config,self.assets,self.binary,self.root/'old',node_group_snapshot={'uid':'hosts','revision':1,'nodes':[]})

if __name__=='__main__':unittest.main(argv=[sys.argv[0]],verbosity=2)
