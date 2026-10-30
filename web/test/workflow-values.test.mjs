import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const { outputText } = ts.transpileModule(readFileSync(new URL('../src/workflow-values.ts', import.meta.url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { locked, replace, rows, override } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
test('only explicit edits are submitted so node defaults survive', () => {
  const defaults = { PORT: 80, image: { repository: 'fixed', tag: 'v1' } };
  const edited = replace(defaults, ['image', 'tag'], 'v2');
  assert.deepEqual(JSON.parse(JSON.stringify(override({}, edited, ['image', 'tag']))), { image: { tag: 'v2' } });
  assert.equal(defaults.image.tag, 'v1');
  assert.ok(locked(['image', 'repository'], ['image.repository']));
  assert.ok(locked(['image'], ['image.repository']));
  assert.ok(!locked(['image', 'tag'], ['image.repository']));
  assert.deepEqual(override({}, defaults, ['PORT']), { PORT: 80 });
  assert.deepEqual(override({}, { ports: [80, 81] }, ['ports', '1']), { ports: [80, 81] });
});
test('rows retain array indices and keys containing dots without prototype mutation', () => {
  assert.deepEqual(rows({ 'a.b': [1, false] }), [{ path: ['a.b', '0'], value: 1 }, { path: ['a.b', '1'], value: false }]);
  const values = replace({}, ['__proto__'], 'literal');
  assert.equal(Object.getPrototypeOf(values), Object.prototype);
  assert.equal(Object.getOwnPropertyDescriptor(values, '__proto__').value, 'literal');
  const nested = replace({}, ['__proto__', 'safe'], 'literal');
  assert.equal(Object.prototype.safe, undefined);
  assert.equal(nested.__proto__.safe, 'literal');
});
