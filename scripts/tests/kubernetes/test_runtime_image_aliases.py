"""Execute the distributed runtime's embedded alias normalizer against a fake ctr."""
from _paths import EXAMPLE, TOOLS
import ast
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

ROOT = EXAMPLE
SOURCE = (ROOT / 'scripts/runtime.sh').read_text().split("<<'CONVERYN_CANONICAL_IMAGES'\n", 1)[1].split('\nCONVERYN_CANONICAL_IMAGES', 1)[0]
DIGEST = 'sha256:' + 'a' * 64
OTHER = 'sha256:' + 'b' * 64


class Ctr:
    def __init__(self, images, bad_tag=False, labels_suffix='', extra_label=False):
        self.images = dict(images)
        self.mutations = []
        self.bad_tag = bad_tag
        self.labels_suffix = labels_suffix
        self.extra_label = extra_label

    def __call__(self, argv, universal_newlines):
        assert universal_newlines is True
        self.assert_prefix(argv)
        command = argv[4:]
        if command == ['list']:
            return 'REF TYPE DIGEST SIZE PLATFORMS LABELS\n' + ''.join(
                f'{name} application/vnd.oci.image.index.v1+json {digest} 1.0 MiB linux/amd64 '
                + ('other=value, ' if self.extra_label else '')
                + ('io.cri-containerd.image=managed' if managed else '-') + self.labels_suffix + '\n'
                for name, (digest, managed) in self.images.items())
        self.mutations.append(command)
        if command[0] == 'tag':
            _, source, target = command
            if target in self.images:
                raise subprocess.CalledProcessError(1, argv)
            self.images[target] = (OTHER if self.bad_tag else self.images[source][0], False)
        elif command[0] == 'label':
            _, target, label = command
            assert label == 'io.cri-containerd.image=managed'
            self.images[target] = (self.images[target][0], True)
        else:
            raise AssertionError(command)
        return ''

    @staticmethod
    def assert_prefix(argv):
        assert argv[:4] == ['/usr/local/bin/ctr', '--namespace', 'k8s.io', 'images']
        assert '--force' not in argv


class RuntimeImageAliases(unittest.TestCase):
    def test_embedded_code_parses_on_rocky8_python36(self):
        ast.parse(SOURCE, feature_version=(3, 6))

    def run_normalizer(self, ctr):
        with patch('subprocess.check_output', side_effect=ctr):
            exec(compile(SOURCE, 'runtime.sh:canonical-images', 'exec'), {})

    def test_adds_managed_alias_and_preserves_tagged_digest_source(self):
        source = 'docker.io/library/busybox:1.37.0@' + DIGEST
        target = 'docker.io/library/busybox@' + DIGEST
        ctr = Ctr({source: (DIGEST, False)})
        self.run_normalizer(ctr)
        self.assertEqual(ctr.images[source], (DIGEST, False))
        self.assertEqual(ctr.images[target], (DIGEST, True))
        self.assertEqual([m[0] for m in ctr.mutations], ['tag', 'label'])

    def test_registry_port_is_preserved(self):
        source = 'registry.example:5000/team/image:v1@' + DIGEST
        target = 'registry.example:5000/team/image@' + DIGEST
        ctr = Ctr({source: (DIGEST, True)})
        self.run_normalizer(ctr)
        self.assertEqual(ctr.images[target], (DIGEST, True))

    def test_existing_correct_managed_alias_needs_no_mutation(self):
        source = 'registry.example:5000/team/image:v1@' + DIGEST
        target = 'registry.example:5000/team/image@' + DIGEST
        ctr = Ctr({source: (DIGEST, True), target: (DIGEST, True)})
        self.run_normalizer(ctr)
        self.assertEqual(ctr.mutations, [])

    def test_existing_correct_unmanaged_alias_only_gets_label(self):
        source = 'example/image:v1@' + DIGEST
        target = 'example/image@' + DIGEST
        ctr = Ctr({source: (DIGEST, False), target: (DIGEST, False)})
        self.run_normalizer(ctr)
        self.assertEqual(ctr.mutations, [['label', target, 'io.cri-containerd.image=managed']])

    def test_padded_ctr_labels_and_multiple_labels_need_no_mutation(self):
        source = 'docker.io/library/busybox:1.37.0@' + DIGEST
        target = 'docker.io/library/busybox@' + DIGEST
        for extra_label in [False, True]:
            with self.subTest(extra_label=extra_label):
                ctr = Ctr({source: (DIGEST, True), target: (DIGEST, True)},
                          labels_suffix=' ' * 33, extra_label=extra_label)
                self.run_normalizer(ctr)
                self.assertEqual(ctr.mutations, [])

    def test_wrong_source_or_existing_target_digest_fails_before_mutation(self):
        source = 'example/image:v1@' + DIGEST
        target = 'example/image@' + DIGEST
        for images in [{source: (OTHER, False)},
                       {source: (DIGEST, False), target: (OTHER, True)}]:
            with self.subTest(images=images):
                ctr = Ctr(images)
                with self.assertRaisesRegex(ValueError, 'digest'):
                    self.run_normalizer(ctr)
                self.assertEqual(ctr.mutations, [])

    def test_rechecks_metadata_after_tagging_before_managing_alias(self):
        source = 'example/image:v1@' + DIGEST
        ctr = Ctr({source: (DIGEST, False)}, bad_tag=True)
        with self.assertRaisesRegex(ValueError, 'different digest'):
            self.run_normalizer(ctr)
        self.assertEqual([m[0] for m in ctr.mutations], ['tag'])

    def test_plain_tags_digest_refs_and_registry_ports_are_not_rewritten(self):
        images = {'example/image:v1': (DIGEST, True),
                  'registry.example:5000/team/image@' + DIGEST: (DIGEST, False),
                  'example/image:v1@sha256:bad': (DIGEST, False)}
        ctr = Ctr(images)
        self.run_normalizer(ctr)
        self.assertEqual(ctr.images, images)
        self.assertEqual(ctr.mutations, [])


if __name__ == '__main__':
    unittest.main()
