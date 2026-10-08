'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const read = (p) => JSON.parse(fs.readFileSync(path.join(root, p), 'utf8'));

test('every file referenced by package.json exists and is valid JSON', () => {
  const pkg = read('package.json');
  assert.ok(fs.existsSync(path.join(root, pkg.main)), 'main');
  for (const l of pkg.contributes.languages) read(l.configuration);
  for (const g of pkg.contributes.grammars) read(g.path);
  for (const s of pkg.contributes.snippets) read(s.path);
});

test('commands declared in package.json are registered by the extension', () => {
  const pkg = read('package.json');
  const source = fs.readFileSync(path.join(root, 'extension.js'), 'utf8');
  for (const c of pkg.contributes.commands) {
    assert.ok(source.includes(`'${c.command}'`), `${c.command} is not registered`);
  }
});

test('language configuration regular expressions compile and behave', () => {
  const cfg = read('language-configuration.json');
  const inc = new RegExp(cfg.indentationRules.increaseIndentPattern);
  const dec = new RegExp(cfg.indentationRules.decreaseIndentPattern);
  for (const line of [
    'fun add(a: int) -> int',
    '  public fun new(x: int) -> Point',
    'struct Point',
    'impl Add for Point',
    'if x > 0 then',
    'elseif x < 0 then',
    'else',
    'while true do',
    'for i in 0..3 do',
    'match n',
    '  case 0 then',
    'let f = fun(x: int) -> int',
    'let v = if a then',
  ]) {
    assert.ok(inc.test(line), `should indent after: ${line}`);
  }
  for (const line of ['let x = 1', 'print(x)', 'if x then print(1) end', 'end', '  x = 2']) {
    assert.ok(!inc.test(line), `should not indent after: ${line}`);
  }
  for (const line of ['end', '  end', 'else', 'elseif a then', '  case 1 then']) {
    assert.ok(dec.test(line), `should dedent: ${line}`);
  }
  assert.ok(!dec.test('endpoint = 1'), '`endpoint` is not `end`');
  new RegExp(cfg.wordPattern);
});

test('snippets have a prefix and a body, and tabstops are well formed', () => {
  const snippets = read('snippets/qlang.json');
  const prefixes = new Set();
  for (const [name, s] of Object.entries(snippets)) {
    assert.ok(s.prefix && s.body, name);
    assert.ok(!prefixes.has(s.prefix), `duplicate prefix ${s.prefix}`);
    prefixes.add(s.prefix);
    const body = Array.isArray(s.body) ? s.body.join('\n') : s.body;
    assert.ok(!/\$\{\d+:[^}]*$/m.test(body.replace(/\$\{\d+:[^}]*\}/g, '')), `${name}: unbalanced placeholder`);
  }
});
