import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('check_version', ROOT / 'tools/check_version.py')
version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(version)


class ProductVersion(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for path in [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'web/package.json',
                     ROOT / 'web/package-lock.json', ROOT / 'docs/schema/openapi.json',
                     *ROOT.glob('crates/*/Cargo.toml'), *ROOT.glob('plugins/*/Cargo.toml')]:
            target = self.root / path.relative_to(ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        self.expected = version.check(self.root)

    def change_json(self, file, change):
        path = self.root / file
        data = json.loads(path.read_text())
        change(data)
        path.write_text(json.dumps(data))

    def test_tag_must_exactly_match_product_version(self):
        self.assertEqual(version.check(self.root, 'v' + self.expected), self.expected)
        for tag in [self.expected, 'v9.9.9', 'v' + self.expected + '-rc.1']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                version.check(self.root, tag)

    def test_frontend_lockfile_and_api_drift_are_rejected(self):
        for file, change in [
            ('web/package.json', lambda d: d.update(version='9.9.9')),
            ('web/package-lock.json', lambda d: d.update(version='9.9.9')),
            ('web/package-lock.json', lambda d: d['packages'][''].update(version='9.9.9')),
            ('docs/schema/openapi.json', lambda d: d['info'].update(version='9.9.9')),
        ]:
            with self.subTest(file=file):
                original = (self.root / file).read_text()
                self.change_json(file, change)
                with self.assertRaises(ValueError):
                    version.check(self.root)
                (self.root / file).write_text(original)

    def test_stale_rust_lockfile_is_rejected(self):
        path = self.root / 'Cargo.lock'
        text = path.read_text()
        name = 'name = "converyn"\nversion = "' + self.expected + '"'
        self.assertIn(name, text)
        path.write_text(text.replace(name, 'name = "converyn"\nversion = "9.9.9"', 1))
        with self.assertRaises(ValueError):
            version.check(self.root)

    def test_crates_must_inherit_workspace_version(self):
        path = self.root / 'crates/cli/Cargo.toml'
        path.write_text(path.read_text().replace('version.workspace = true', 'version = "9.9.9"'))
        with self.assertRaises(ValueError):
            version.check(self.root)


if __name__ == '__main__':
    unittest.main()
