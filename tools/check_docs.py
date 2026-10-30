#!/usr/bin/env python3
"""Check bilingual documentation, local links and canonical example locations."""
from pathlib import Path
from collections import Counter
import re
import shlex
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
files = [*ROOT.glob('*.md'), *ROOT.glob('docs/**/*.md'),
         *ROOT.glob('example/**/*.md')]


# Keep operator and CI scripts separate, and documentation in docs/.
assert {p.name for p in (ROOT / 'scripts').iterdir()} == {'ops', 'tests'}, 'scripts/ must contain only ops/ and tests/'
assert not list((ROOT / 'scripts').rglob('*.md')), 'Documentation belongs under docs/'
# Public tests and private environment tools have distinct ownership.
for path in (ROOT / 'scripts').iterdir():
    assert not (path.is_file() and (path.name.startswith('test_') or '_e2e.' in path.name or path.suffix == '.mjs')), f'Test belongs under scripts/tests: {path.name}'
for name in ('vmware_lab.py', 'lab_ssh.py', 'check_lab_access.py', 'requirements-e2e.txt'):
    assert not (ROOT / 'scripts' / name).exists(), f'Private environment tool belongs under .local/scripts: {name}'


def counterpart(path):
    relative = path.relative_to(ROOT)
    if relative == Path('docs/README.md'):
        return None  # Shared language selector, not a translated topic.
    if relative.parts[0] == 'docs':
        assert relative.parts[1] in ('en', 'zh-CN'), f'Topic outside language directory: {relative}'
        language = 'zh-CN' if relative.parts[1] == 'en' else 'en'
        assert '.zh-CN' not in path.name, f'Language suffix inside language directory: {relative}'
        return ROOT / 'docs' / language / Path(*relative.parts[2:])
    return path.with_name(path.name.replace('.zh-CN.md', '.md')) if path.name.endswith('.zh-CN.md') else path.with_name(path.stem + '.zh-CN.md')


for path in files:
    text = path.read_text()
    translated = counterpart(path)
    assert translated is None or translated.is_file(), f'Missing language counterpart: {path.relative_to(ROOT)}'
    assert not re.search(r'\b20\d{2}-\d{2}-\d{2}\b|10\s*月\s*20|Converyn\s+v?0\.\d', text), f'Historical date/release prose: {path.relative_to(ROOT)}'
    assert not re.search(r'(?<![\w.])examples/|example/(?:prepare|render|deploy|check)\.py', text), f'Obsolete example location: {path.relative_to(ROOT)}'
    for _, target in re.findall(r'\[([^\]]+)\]\(([^)]+)\)', text):
        if target.startswith(('https://', 'http://', 'mailto:')):
            continue
        target = unquote(target.strip('<>'))
        relative, _, anchor = target.partition('#')
        local = (path.parent / relative).resolve() if relative else path
        assert local.exists(), f'Broken link: {path.relative_to(ROOT)} -> {target}'
        if anchor and local.suffix == '.md':
            content = local.read_text()
            explicit = set(re.findall(r'<a id="([^"]+)"', content))
            headings = {re.sub(r'[^\w -]', '', line.lstrip('#').strip().lower()).replace(' ', '-')
                        for line in content.splitlines() if line.startswith('#')}
            assert anchor in explicit | headings, f'Broken anchor: {path.relative_to(ROOT)} -> {target}'
assert not (ROOT / 'example' / 'grapple').exists(), 'Private product code is not a public example'
assert not (ROOT / 'examples').exists(), 'Only example/ is canonical'
for directory in (ROOT / 'example').iterdir():
    if (directory / 'README.md').is_file() and directory.name != 'ci':
        assert (directory / 'workflow.yaml').is_file(), f'Missing native workflow entry: {directory.name}'
assert not (ROOT / 'example/plan.yaml').exists(), 'Offline PlanInput belongs in test fixtures'
assert (ROOT / 'scripts/tests/fixtures/plan.yaml').is_file()
for name in ['workflow.yaml', 'stages.yaml', 'cluster.yaml', 'cluster-ha.yaml', 'sources.lock.json',
             'scripts/verify.sh']:
    assert (ROOT / 'example/kubernetes' / name).is_file(), name
for name in ['prepare.py', 'render.py', 'render_uninstall.py', 'check_image_inventory.py']:
    assert (ROOT / 'scripts/ops/kubernetes' / name).is_file(), name
for name in ['check.py', 'check_cluster_snapshot.py', 'check_uninstall.py', 'test_password_files.py']:
    assert (ROOT / 'scripts/tests/kubernetes' / name).is_file(), name
for path in (ROOT / 'example').rglob('*.py'):
    assert not path.name.startswith(('test_', 'check')), f'Regression code belongs under scripts/tests: {path.relative_to(ROOT)}'

# A language counterpart must preserve the usable instructions, not merely exist.
def normalized_target(path, target):
    target = unquote(target.strip('<>')).partition('#')[0]
    if target.startswith(('https://', 'http://', 'mailto:')):
        return target
    relative = ((path.parent / target) if target else path).resolve().relative_to(ROOT).as_posix()
    relative = re.sub(r'^docs/assets/screenshots/(?:en|zh-CN)/',
                      'docs/assets/screenshots/', relative)
    return re.sub(r'^docs/(en|zh-CN)/', 'docs/', relative).replace('.zh-CN.md', '.md')


def readme_structure(path):
    text = path.read_text()
    headings = re.findall(r'^(#+) ', text, re.M)
    code = []
    for language, body in re.findall(r'^```(bash|sh|json|yaml|toml)\n(.*?)^```', text, re.M | re.S):
        value = shlex.split(body.replace('\\\n', ' '), comments=True) if language in ('bash', 'sh') else body.strip()
        code.append((language, value))
    targets = Counter(normalized_target(path, target)
                      for target in re.findall(r'\[[^\]]+\]\(([^)]+)\)', text))
    tables = [len(line.split('|')) for line in text.splitlines() if line.startswith('|')]
    return headings, code, targets, tables

pairs = 0
for path in files:
    if path.name != 'README.md':
        continue
    translated = counterpart(path)
    if translated is None or path.relative_to(ROOT).parts[:2] == ('docs', 'zh-CN'):
        continue
    for label, english, chinese in zip(('heading structure', 'executable examples', 'link destinations', 'table structure'),
                                       readme_structure(path), readme_structure(translated)):
        assert english == chinese, f'Bilingual README {label} differs: {path.relative_to(ROOT)}'
    pairs += 1
print('Checked', len(files), 'documents,', pairs, 'README pairs, local links and example/tool/test boundaries')
