import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import vm from 'node:vm';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ts from 'typescript';

// Render the actual target components without browser/router dependencies.
const source = readFileSync(new URL('../src/records.tsx', import.meta.url), 'utf8');
const tree = ts.createSourceFile('records.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const names = new Set(['TargetLocation', 'PresetSummary']);
const selected = tree.statements.filter(n => ts.isTypeAliasDeclaration(n) || (ts.isFunctionDeclaration(n) && names.has(n.name.text))).map(n => n.getText(tree)).join('\n');
const { outputText } = ts.transpileModule(selected, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX } });
const exports = {};
vm.runInNewContext(outputText, { exports, require: createRequire(import.meta.url), t: text => text, Fields: ({ value }) => React.createElement('pre', null, JSON.stringify(value)) });
const { TargetLocation, PresetSummary } = exports;

test('workflow labels render separately from global host groups', () => {
  const native = renderToStaticMarkup(React.createElement(TargetLocation, { preset: { hosts: 'database[0]' } }));
  assert.match(native, /流程选择器/);
  assert.match(native, /database\[0\]/);
  assert.doesNotMatch(native, /平台节点组/);
  const global = renderToStaticMarkup(React.createElement(TargetLocation, { preset: { node_group_uid: 'fleet' } }));
  assert.match(global, /平台节点组/);
  assert.match(global, /fleet/);
});

test('inventory summary retains group order and identifies unverified Agent configuration', () => {
  const inventory = { nodes: [{ uid: 'node-a', address: '192.0.2.1', hostname: 'a' }, { uid: 'node-b', address: '192.0.2.2', hostname: 'b' }], groups: { database: ['node-b', 'node-a'], optional: [] } };
  const html = renderToStaticMarkup(React.createElement(PresetSummary, { preset: { hosts: 'database' }, inventory }));
  assert.match(html, /<td>database<\/td><td>node-b → node-a<\/td>/);
  assert.match(html, /<td>all<\/td><td>node-a → node-b<\/td>/);
  assert.match(html, /<td>optional<\/td><td>空<\/td>/);
  assert.match(html, /Secret 在控制面录入一次/);
  assert.equal(renderToStaticMarkup(React.createElement(PresetSummary, {})), '');
});
