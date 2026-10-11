'use strict';
// Tooltips, completion and signature help, with the real compiler: the same steps as the
// extension, without VS Code.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const lib = require('../lib');
const intel = require('../intel');

const repo = path.join(__dirname, '..', '..', '..');
const binary = ['target/release/qlang', 'target/debug/qlang'].map((p) => path.join(repo, p)).find((p) => fs.existsSync(p));
const skip = !binary && 'build qlang first (cargo build)';

function analyse(files, entry = 'main.q') {
  const r = cp.spawnSync(binary, ['check', '--json'], { input: lib.buildRequest(entry, files, true), encoding: 'utf8' });
  assert.strictEqual(r.status, 0, r.stderr);
  return JSON.parse(r.stdout).info;
}

const SOURCE = [
  '--" A point on the plane. --"', //  0
  'struct Point', //                    1
  '  public x: int', //                 2
  '  public y: int', //                 3
  'end', //                             4
  '', //                                5
  'impl Point', //                      6
  '  --" Distance to the origin. --"', // 7
  '  public fun norm(self) -> float', //8
  '    return (self.x * self.x + self.y * self.y).sqrt()', // 9
  '  end', //                           10
  'end', //                             11
  '', //                                12
  '--"', //                             13
  'Adds two numbers.', //               14
  '--"', //                             15
  'fun add(a: int, b: int) -> int', //  16
  '  let total = a + b', //             17
  '  return total', //                  18
  'end', //                             19
  '', //                                20
  'let p = Point { x: 1, y: 2 }', //     21
  'let names = ["a", "b"]', //          22
  'print(p.norm())', //                 23
  'print(names.len())', //              24
  'print(add(1, 2))', //                25
].join('\n');

const lines = SOURCE.split('\n');
const linesOf = (f) => (f === 'main.q' ? lines : null);

function hover(info, line, needle, nth = 0) {
  let col = -1;
  for (let i = 0; i <= nth; i++) col = lines[line].indexOf(needle, col + 1);
  assert.ok(col >= 0, `${needle} not on line ${line}`);
  return intel.hoverAt(info, 'main.q', line, col, lines[line], linesOf);
}

test('hover shows types, signatures and documentation', { skip }, () => {
  const info = analyse({ 'main.q': SOURCE });
  assert.match(hover(info, 22, 'names').markdown, /names: array<string>/);
  assert.match(hover(info, 23, 'p.norm').markdown, /p: Point/);
  // a method, with the comment above it
  const norm = hover(info, 23, 'norm');
  assert.match(norm.markdown, /\(method\) norm\(\) -> float/);
  assert.match(norm.markdown, /Distance to the origin\./);
  // a built-in method, with the explanation of the catalog
  const len = hover(info, 24, 'len');
  assert.match(len.markdown, /\(method\) len\(\) -> int/);
  assert.match(len.markdown, /Number of elements\./);
  // a function call, with its multi-line comment
  const add = hover(info, 25, 'add');
  assert.match(add.markdown, /fun add\(a: int, b: int\) -> int/);
  assert.match(add.markdown, /Adds two numbers\./);
  // a declaration and a variable of a function
  assert.match(hover(info, 17, 'total').markdown, /let total: int/);
  assert.match(hover(info, 18, 'total').markdown, /total: int/);
  // a type used as a value, a built-in function, a keyword, nothing
  assert.match(hover(info, 21, 'Point').markdown, /struct Point/);
  assert.match(hover(info, 21, 'Point').markdown, /A point on the plane\./);
  assert.match(hover(info, 23, 'print').markdown, /print\(value\)/);
  assert.match(hover(info, 16, 'fun').markdown, /Declares a function/);
  assert.strictEqual(intel.hoverAt(info, 'main.q', 20, 0, '', linesOf), null);
});

test('completion after a dot offers the members of the type', { skip }, () => {
  // the user is typing `names.pu|` on a new line: ask about the code without `.pu`
  const typed = SOURCE + '\nnames.pu';
  const line = typed.split('\n').length - 1;
  const lineText = typed.split('\n')[line];
  const access = intel.memberAccessAt(lineText, lineText.length);
  assert.deepStrictEqual(access, { dot: 5, start: 6 });
  const info = analyse({ 'main.q': intel.blank(typed, line, access.dot, lineText.length) });
  const expr = intel.receiverAt(info, 'main.q', line, access.dot);
  assert.strictEqual(expr.type, 'array<string>');
  const items = intel.memberCompletions(info, expr, 'names', linesOf);
  const push = items.find((i) => i.label === 'push');
  assert.strictEqual(push.detail, 'push(value: string)');
  assert.ok(items.some((i) => i.label === 'len'));

  // a structure: fields first, only the public ones from outside
  const p = analyse({ 'main.q': intel.blank(SOURCE + '\np.', lines.length, 1, 2) });
  const members = intel.memberCompletions(p, intel.receiverAt(p, 'main.q', lines.length, 1), 'p', linesOf);
  assert.deepStrictEqual(members.map((m) => m.label), ['x', 'y', 'norm']);
});

test('completion without a dot offers variables in scope, names and keywords', { skip }, () => {
  const info = analyse({ 'main.q': SOURCE });
  const inside = intel.nameCompletions(info, 'main.q', 18, 12, linesOf).map((i) => i.label);
  assert.ok(['a', 'b', 'total'].every((n) => inside.includes(n)), 'variables of the function');
  assert.ok(inside.includes('names'), 'top-level variables are visible in functions');
  const early = intel.nameCompletions(info, 'main.q', 17, 4, linesOf).map((i) => i.label);
  assert.ok(early.includes('a') && !early.includes('total'), '`total` is not declared yet');
  const top = intel.nameCompletions(info, 'main.q', 25, 0, linesOf);
  const labels = top.map((i) => i.label);
  assert.ok(['p', 'names', 'add', 'Point', 'print', 'while', 'int'].every((n) => labels.includes(n)));
  assert.ok(!labels.includes('total'), 'a variable of a function is not visible outside');
  assert.strictEqual(top.find((i) => i.label === 'add').doc, 'Adds two numbers.');
});

test('static members: enum variants, static functions and built-in types', { skip }, () => {
  const src = ['enum Color', '  Red', '  Green', 'end', 'let c = Color.Red', 'print(int.parse("1"))', 'print(Color.)'].join('\n');
  const l = src.split('\n');
  const info = analyse({ 'main.q': intel.blank(src, 6, 11, 12) });
  const color = intel.receiverAt(info, 'main.q', 6, 11);
  assert.deepStrictEqual(intel.membersOf(info, color).map((m) => m.name), ['Red', 'Green']);
  const ints = analyse({ 'main.q': intel.blank('print(int.)', 0, 9, 10) });
  assert.deepStrictEqual(intel.membersOf(ints, intel.receiverAt(ints, 'main.q', 0, 9)).map((m) => m.name), ['parse', 'MAX', 'MIN']);
  assert.ok(l.length > 0);
});

test('signature help finds the signature and the active parameter', { skip }, () => {
  const typed = SOURCE + '\nadd(1, ';
  const line = typed.split('\n').length - 1;
  const text = typed.split('\n')[line];
  const call = intel.callAt(text, text.length);
  assert.deepStrictEqual(call, { open: 3, name: 'add', activeParam: 1 });
  const info = analyse({ 'main.q': intel.blank(typed, line, call.open, text.length) });
  const sig = intel.signatureFor(info, 'main.q', line, text, call, linesOf);
  assert.strictEqual(sig.label, 'add(a: int, b: int) -> int');
  assert.deepStrictEqual(sig.parameters, ['a: int', 'b: int']);
  assert.strictEqual(sig.doc, 'Adds two numbers.');
  assert.strictEqual(sig.activeParam, 1);

  // a method: `names.insert(1, `
  const typed2 = SOURCE + '\nnames.insert(1, ';
  const text2 = 'names.insert(1, ';
  const call2 = intel.callAt(text2, text2.length);
  const info2 = analyse({ 'main.q': intel.blank(typed2, line, call2.open, text2.length) });
  const sig2 = intel.signatureFor(info2, 'main.q', line, text2, call2, linesOf);
  assert.strictEqual(sig2.label, 'insert(index: int, value: string)');
});

test('the helpers on their own', () => {
  assert.deepStrictEqual(intel.wordAt('let total = 1', 6), { word: 'total', start: 4, end: 9 });
  assert.strictEqual(intel.wordAt('let x = 12', 9), null); // a number
  assert.strictEqual(intel.callAt('print(f(1), "a,b", [1, 2], ', 28).name, 'print');
  assert.strictEqual(intel.callAt('print(f(1), "a,b", [1, 2], ', 28).activeParam, 3);
  assert.strictEqual(intel.callAt('let xs = [1, ', 13), null);
  assert.deepStrictEqual(intel.parameters('f(a: map<string, int>, b: fun(int) -> int) -> int'), ['a: map<string, int>', 'b: fun(int) -> int']);
  assert.strictEqual(intel.docAbove(['--" one --"', 'fun f()'], 1), 'one');
  assert.strictEqual(intel.docAbove(['--"', ' a', ' b', '--"', 'fun f()'], 4), 'a\nb');
  assert.strictEqual(intel.docAbove(['-- not a doc', 'fun f()'], 1), null);
  assert.strictEqual(intel.blank('ab.cd\nx', 0, 2, 5), 'ab   \nx');
});
