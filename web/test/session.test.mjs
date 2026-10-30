import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const source = readFileSync(new URL('../src/session.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { readSession, saveSession, clearSession } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
const values = new Map();
globalThis.sessionStorage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value), removeItem: key => values.delete(key) };
test('session survives a fresh read and explicit sign-out removes it', () => {
  const credential = { cluster: 'test', token: 'fixture-token' };
  saveSession(credential, Date.now() + 60000);
  assert.deepEqual(readSession(), credential);
  clearSession();
  assert.equal(readSession(), undefined);
});
test('expired or malformed sessions cannot be restored', () => {
  for (const raw of ['{', 'null', '[]', JSON.stringify({cluster: 'test', token: 'fixture-token', expires: Date.now() - 1}), JSON.stringify({cluster: '', token: 'fixture-token', expires: Date.now() + 60000}), JSON.stringify({cluster: 'test', token: 123, expires: Date.now() + 60000})]) {
    values.set('converyn.session', raw);
    assert.equal(readSession(), undefined);
    assert.equal(values.has('converyn.session'), false);
  }
});
test('blocked browser storage does not prevent login or sign-out', () => {
  const storage = globalThis.sessionStorage;
  globalThis.sessionStorage = { getItem() { throw Error('blocked'); }, setItem() { throw Error('blocked'); }, removeItem() { throw Error('blocked'); } };
  try {
    assert.doesNotThrow(() => saveSession({ cluster: 'test', token: 'fixture-token' }, Date.now() + 60000));
    assert.equal(readSession(), undefined);
    assert.doesNotThrow(clearSession);
  } finally { globalThis.sessionStorage = storage; }
});
