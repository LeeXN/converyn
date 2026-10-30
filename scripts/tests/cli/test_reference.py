"""Regression for commands that have no help summary."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location('render_cli_docs', ROOT / 'tools/render_cli_docs.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CliReference(unittest.TestCase):
    def test_missing_summaries_and_group_headings_do_not_hide_commands(self):
        text = '''Usage: converyn COMMAND

Commands:
  validate   Validate workflows
  register
  disable
  help       Print help

Options:
  -h, --help   Print help
'''
        self.assertEqual(MODULE.visible_commands(text), [
            ('validate', 'Validate workflows'), ('register', ''), ('disable', '')])
        grouped = '''Query:
  get         Inspect resources

Execution:
  run         Submit workflows
'''
        self.assertEqual(MODULE.visible_commands(grouped), [
            ('get', 'Inspect resources'), ('run', 'Submit workflows')])


if __name__ == '__main__':
    unittest.main()
