import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const { outputText } = ts.transpileModule(readFileSync(new URL('../src/node-observation.ts', import.meta.url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { observationStatus } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
const now = 10_000_000;
const snapshot = (data = {}, errors = {}) => ({ collected_ms: now - 20_000, data, errors });

test('unchanged assets use their latest check; resources use their own sample time', () => {
  const node = { inventory: { ...snapshot(), collected_ms: 1 }, inventory_checked_ms: now - 120_000, resources: snapshot() };
  assert.equal(observationStatus(node, 'inventory', now).stale, false);
  assert.equal(observationStatus(node, 'inventory', now).count, 2);
  assert.equal(observationStatus(node, 'resources', now).count, 20);
  assert.equal(observationStatus({ resources: { ...snapshot(), collected_ms: now - 61_000 } }, 'resources', now).stale, true);
  assert.equal(observationStatus({}, 'resources', now), undefined);
  assert.equal(observationStatus({ resources: { ...snapshot(), collected_ms: now + 3000 } }, 'resources', now).count, 0);
});
test('omitted lists and collection failures remain distinct, including removed fields', () => {
  const resources = { ...snapshot({ filesystems: [] }, { filesystems: 'observation size limit exceeded; some entries omitted', gpus: 'observation size limit exceeded; omitted', memory: 'permission denied' }), truncated: true };
  const state = observationStatus({ resources }, 'resources', now);
  assert.deepEqual(state.omitted, ['filesystems', 'gpus']);
  assert.deepEqual(state.errors, [['memory', 'permission denied']]);
  assert.equal(state.truncated, true);
  const unknown = observationStatus({ resources: { ...snapshot(), truncated: true } }, 'resources', now);
  assert.deepEqual(unknown.omitted, []);
  assert.deepEqual(unknown.errors, []);
});
