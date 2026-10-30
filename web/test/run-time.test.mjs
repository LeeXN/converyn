import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const source = readFileSync(new URL('../src/run-time.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { elapsedMilliseconds, durationText } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));

test('wall time includes queueing and pauses and freezes at the finish timestamp', () => {
  for (const status of ['pending', 'preparing', 'running', 'paused', 'needs_attention', 'cancel_requested']) {
    assert.equal(elapsedMilliseconds(1000, null, status, 6500), 5500);
  }
  for (const status of ['succeeded', 'failed', 'canceled']) {
    assert.equal(elapsedMilliseconds(1000, 6500, status, 9000), 5500);
    assert.equal(elapsedMilliseconds(1000, undefined, status, 9000), undefined);
  }
});

test('old or inconsistent timestamps never manufacture elapsed time', () => {
  for (const submitted of [undefined, null, NaN, -1]) assert.equal(elapsedMilliseconds(submitted, 5000, 'succeeded', 9000), undefined);
  assert.equal(elapsedMilliseconds(5000, 4000, 'succeeded', 9000), undefined);
  assert.equal(elapsedMilliseconds(5000, Infinity, 'succeeded', 9000), undefined);
  assert.equal(elapsedMilliseconds(5000, null, 'running', 4000), undefined);
  assert.equal(elapsedMilliseconds(0, 0, 'succeeded', 9000), 0);
});

test('elapsed display handles subsecond, minute and multi-day tasks', () => {
  assert.equal(durationText(undefined), '—');
  assert.equal(durationText(999), '0 s');
  assert.equal(durationText(65000), '1 m 5 s');
  assert.equal(durationText(90061000), '25 h 1 m 1 s');
});
