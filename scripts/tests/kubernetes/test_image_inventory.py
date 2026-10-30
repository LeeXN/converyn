"""Guard against a runtime pin or offline package drifting from kubeadm."""
from _paths import EXAMPLE, TOOLS
import unittest

from check_image_inventory import validate_images


class ImageInventory(unittest.TestCase):
    def setUp(self):
        self.pause = 'registry.k8s.io/pause:3.10.2'
        self.apiserver = 'registry.k8s.io/kube-apiserver:v1.36.4'
        self.required = [self.apiserver, self.pause]
        self.inventory = {'images': [{'reference': image} for image in self.required]}

    def test_matching_runtime_and_inventory_pass(self):
        self.assertEqual(validate_images(self.required, self.pause, self.inventory)['sandbox_image'], self.pause)

    def test_runtime_mismatch_is_rejected_even_when_both_images_are_archived(self):
        old = 'registry.k8s.io/pause:3.10'
        self.inventory['images'].append({'reference': old})
        with self.assertRaisesRegex(ValueError, 'sandbox differs'):
            validate_images(self.required, old, self.inventory)

    def test_missing_sandbox_and_control_plane_images_are_rejected(self):
        for missing in self.required:
            with self.subTest(missing=missing):
                inventory = {'images': [row for row in self.inventory['images'] if row['reference'] != missing]}
                with self.assertRaisesRegex(ValueError, 'missing required images'):
                    validate_images(self.required, self.pause, inventory)

    def test_archive_reference_takes_precedence_over_source_tag(self):
        self.inventory['images'][-1]['archive_reference'] = 'registry.k8s.io/pause:3.10'
        with self.assertRaisesRegex(ValueError, 'missing required images'):
            validate_images(self.required, self.pause, self.inventory)


if __name__ == '__main__':
    unittest.main()
