import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const { outputText } = ts.transpileModule(readFileSync(new URL('../src/node-addresses.ts', import.meta.url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } });
const { nodeAddresses } = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
test('display preference keeps host IPv4 first and retains other networks for expansion', () => {
  const rows = [{ interface:'lo',address:'127.0.0.1' },{ interface:'cni0',address:'10.244.0.1' },{ interface:'ens192',address:'fe80::1' },{ interface:'ens192',address:'2001:db8::1' },{ interface:'ens192',address:'192.0.2.175' },{ interface:'ens192',address:'192.0.2.175' }];
  const before = structuredClone(rows), result = nodeAddresses(rows);
  assert.equal(result[0].address, '192.0.2.175');
  assert.equal(result.length, 5);
  assert.ok(result.some(row => row.interface === 'cni0'));
  assert.deepEqual(rows, before);
});
test('missing or malformed telemetry does not break node management', () => {
  for (const value of [null, undefined, {}, 'unavailable']) assert.deepEqual(nodeAddresses(value), []);
  assert.deepEqual(nodeAddresses([null, {}, 12, { address: 5 }, { address: '' }, { address:'2001:db8::1' }]), [{ address:'2001:db8::1',interface:'' }]);
});
