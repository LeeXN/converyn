import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
// Exercise the actual pure cursor helpers without mounting browser hooks.
const source = readFileSync(new URL('../src/pagination.tsx', import.meta.url), 'utf8');
const tree = ts.createSourceFile('pagination.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const functions = new Set(['paginationKey', 'readPosition', 'nextPosition', 'listEndpoint']);
const code = tree.statements.filter(node => ts.isTypeAliasDeclaration(node) || (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => ['sizes', 'initial', 'paginationKey'].includes(d.name.getText(tree)))) || (ts.isFunctionDeclaration(node) && functions.has(node.name.text))).map(node => node.getText(tree)).join('\n');
const { outputText } = ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { readPosition, nextPosition, listEndpoint, paginationKey } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
const values = new Map();
globalThis.sessionStorage = { getItem: key => values.get(key) ?? null };
const first = { limit: 20, cursors: [''], index: 0 };
test('next page retains cursor boundaries and replaces an abandoned forward branch', () => {
  const second = nextPosition(first, 'opaque+/=');
  assert.deepEqual(second, { limit: 20, cursors: ['', 'opaque+/='], index: 1 });
  const third = nextPosition(second, 40);
  const back = { ...third, index: 0 };
  assert.deepEqual(nextPosition(back, 'new-boundary'), { limit: 20, cursors: ['', 'new-boundary'], index: 1 });
  assert.equal(nextPosition(second, 'opaque+/='), second);
  assert.equal(nextPosition(second, null), second);
});
test('saved page positions are isolated by person, cluster and list', () => {
  const key = paginationKey('single', 'alice', 'runs');
  values.set(key, JSON.stringify({ limit: 50, cursors: ['', 'cursor'], index: 1 }));
  assert.equal(readPosition(key).index, 1);
  assert.deepEqual(readPosition(paginationKey('ha', 'alice', 'runs')), first);
  assert.deepEqual(readPosition(paginationKey('single', 'bob', 'runs')), first);
  assert.deepEqual(readPosition(paginationKey('single', 'alice', 'nodes')), first);
});
test('broken and unsupported saved positions safely return to the first page', () => {
  for (const raw of ['{', 'null', JSON.stringify({ limit: 999, cursors: [''], index: 0 }), JSON.stringify({ limit: 20, cursors: ['bad-start'], index: 0 }), JSON.stringify({ limit: 20, cursors: [''], index: 9 })]) {
    values.set('broken', raw);
    assert.deepEqual(readPosition('broken'), first);
  }
  const original = sessionStorage;
  globalThis.sessionStorage = { getItem() { throw Error('blocked'); } };
  try { assert.deepEqual(readPosition('blocked'), first); } finally { globalThis.sessionStorage = original; }
});
test('requests send the selected size and encode opaque cursors exactly once', () => {
  assert.equal(listEndpoint('runs', 'opaque+/=', 100), '/runs?limit=100&after=opaque%2B%2F%3D');
  assert.equal(listEndpoint('audit', '20', 20), '/audit?limit=20&after=20');
  assert.equal(listEndpoint('nodes', '', 10), '/nodes?limit=10');
});
