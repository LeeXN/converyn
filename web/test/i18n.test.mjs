import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';

const root = new URL('../src/', import.meta.url);
const parse = name => ts.createSourceFile(name, readFileSync(new URL(name, root), 'utf8'), ts.ScriptTarget.Latest, true, name.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
const catalog = parse('i18n.ts');
const messages = new Map();
function visit(node, visitor) { visitor(node); ts.forEachChild(node, child => visit(child, visitor)); }
visit(catalog, node => {
  if (ts.isVariableDeclaration(node) && node.name.getText(catalog) === 'messages') {
    for (const property of node.initializer.properties) messages.set(property.name.text, property.initializer.text);
  }
});
test('every Chinese interface message has an English translation', () => {
  const missing = [];
  for (const file of readdirSync(root).filter(name => /\.(ts|tsx)$/.test(name) && !['i18n.ts', 'api-types.ts'].includes(name))) {
    visit(parse(file), node => {
      if (ts.isStringLiteral(node) && /[\u3400-\u9fff]/.test(node.text) && !messages.has(node.text)) missing.push(file + ': ' + node.text);
      if (ts.isJsxText(node) && /[\u3400-\u9fff]/.test(node.text)) missing.push(file + ': untranslated JSX text ' + node.text);
    });
  }
  assert.deepEqual(missing, []);
  for (const [source, translation] of messages) {
    assert.ok(translation.trim(), 'Empty translation: ' + source);
    if (source !== '简体中文') assert.ok(!/[\u3400-\u9fff]/.test(translation), 'Chinese left in English translation: ' + source);
  }
});

test('core receipts and execution evidence have translated field labels', () => {
  const labels = new Set();
  visit(catalog, node => {
    if (ts.isVariableDeclaration(node) && node.name.getText(catalog) === 'fieldLabels') {
      for (const property of node.initializer.properties) labels.add(property.name.text);
    }
  });
  for (const name of ['receipt', 'logs', 'evidence', 'execution']) {
    const rust = readFileSync(new URL('../../crates/core/src/model/' + name + '.rs', import.meta.url), 'utf8');
    for (const match of rust.matchAll(/pub (\w+):/g)) assert.ok(labels.has(match[1]), 'Missing field label: ' + name + '.' + match[1]);
  }
});
