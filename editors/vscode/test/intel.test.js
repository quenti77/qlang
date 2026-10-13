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

function hover(info, line, needle, nth = 0) {
  let col = -1;
  for (let i = 0; i <= nth; i++) col = lines[line].indexOf(needle, col + 1);
  assert.ok(col >= 0, `${needle} not on line ${line}`);
  return intel.hoverAt(info, 'main.q', line, col, lines[line]);
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
  assert.strictEqual(intel.hoverAt(info, 'main.q', 20, 0, ''), null);
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
  const items = intel.memberCompletions(info, expr, 'names');
  const push = items.find((i) => i.label === 'push');
  assert.strictEqual(push.detail, 'push(value: string)');
  assert.ok(items.some((i) => i.label === 'len'));

  // a structure: fields first, only the public ones from outside
  const p = analyse({ 'main.q': intel.blank(SOURCE + '\np.', lines.length, 1, 2) });
  const members = intel.memberCompletions(p, intel.receiverAt(p, 'main.q', lines.length, 1), 'p');
  assert.deepStrictEqual(members.map((m) => m.label), ['x', 'y', 'norm', 'class']);
});

test('completion inside a struct literal offers the fields not yet given', { skip }, () => {
  const info = analyse({ 'main.q': SOURCE });
  const text = 'let p = Point { x: 1,  }';
  const offset = text.indexOf(', ') + 2; // right after `x: 1, `, before the final `}`
  const lit = intel.structLitAt(text, offset);
  assert.deepStrictEqual(lit, { name: 'Point', given: new Set(['x']), prefix: '' });
  const items = intel.structFieldCompletions(info, lit);
  assert.deepStrictEqual(items.map((i) => i.label), ['y']);

  // a partial field name narrows further: `Point { x: 1, y|` offers nothing new (`y` is
  // the only field left and is itself the word being typed)
  const partial = 'let p = Point { x: 1, y';
  const lit2 = intel.structLitAt(partial, partial.length);
  assert.deepStrictEqual(lit2, { name: 'Point', given: new Set(['x']), prefix: 'y' });

  // typing the value, after `:`, is not a field name position
  const value = 'let p = Point { x: 1, y: ';
  assert.strictEqual(intel.structLitAt(value, value.length), null);

  // not inside a struct literal at all
  assert.strictEqual(intel.structLitAt('let m = { "k": 1 }', 11), null);
});

test('completion without a dot offers variables in scope, names and keywords', { skip }, () => {
  const info = analyse({ 'main.q': SOURCE });
  const inside = intel.nameCompletions(info, 'main.q', 18, 12).map((i) => i.label);
  assert.ok(['a', 'b', 'total'].every((n) => inside.includes(n)), 'variables of the function');
  assert.ok(inside.includes('names'), 'top-level variables are visible in functions');
  const early = intel.nameCompletions(info, 'main.q', 17, 4).map((i) => i.label);
  assert.ok(early.includes('a') && !early.includes('total'), '`total` is not declared yet');
  const top = intel.nameCompletions(info, 'main.q', 25, 0);
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
  assert.deepStrictEqual(intel.membersOf(info, color).map((m) => m.name), ['Red', 'Green', 'class']);
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
  const sig = intel.signatureFor(info, 'main.q', line, text, call);
  assert.strictEqual(sig.label, 'add(a: int, b: int) -> int');
  assert.deepStrictEqual(sig.parameters, ['a: int', 'b: int']);
  assert.strictEqual(sig.doc, 'Adds two numbers.');
  assert.strictEqual(sig.activeParam, 1);

  // a method: `names.insert(1, `
  const typed2 = SOURCE + '\nnames.insert(1, ';
  const text2 = 'names.insert(1, ';
  const call2 = intel.callAt(text2, text2.length);
  const info2 = analyse({ 'main.q': intel.blank(typed2, line, call2.open, text2.length) });
  const sig2 = intel.signatureFor(info2, 'main.q', line, text2, call2);
  assert.strictEqual(sig2.label, 'insert(index: int, value: string)');
});

test('the helpers on their own', () => {
  assert.deepStrictEqual(intel.wordAt('let total = 1', 6), { word: 'total', start: 4, end: 9 });
  assert.strictEqual(intel.wordAt('let x = 12', 9), null); // a number
  assert.strictEqual(intel.callAt('print(f(1), "a,b", [1, 2], ', 28).name, 'print');
  assert.strictEqual(intel.callAt('print(f(1), "a,b", [1, 2], ', 28).activeParam, 3);
  assert.strictEqual(intel.callAt('let xs = [1, ', 13), null);
  assert.deepStrictEqual(intel.parameters('f(a: map<string, int>, b: fun(int) -> int) -> int'), ['a: map<string, int>', 'b: fun(int) -> int']);
  assert.strictEqual(intel.blank('ab.cd\nx', 0, 2, 5), 'ab   \nx');
});

test('uuid, ulid, time and random: members and static functions', { skip }, () => {
  const src = ['let id = ulid.new()', 'let u = uuid.v4()', 'print(id.time())', 'print(u.version())', 'print(time.)', 'print(random.)'].join('\n');
  const info = analyse({ 'main.q': intel.blank(intel.blank(src, 4, 10, 11), 5, 12, 13) });
  const idType = info.exprs.find((e) => e.type === 'ulid');
  assert.ok(idType, 'the type ulid is known');
  const ulidMembers = intel.memberCompletions(info, idType, 'id').map((m) => m.label);
  assert.ok(ulidMembers.includes('time'));
  const uuidMembers = intel.memberCompletions(info, info.exprs.find((e) => e.type === 'uuid'), 'u').map((m) => m.label);
  assert.ok(uuidMembers.includes('version'));
  const statics = (line, col) => intel.memberCompletions(info, intel.receiverAt(info, 'main.q', line, col), 'x').map((m) => m.label);
  assert.deepStrictEqual(statics(4, 10), ['now']);
  assert.deepStrictEqual(statics(5, 12).sort(), ['float', 'int']);
});

test('datetime: members and static functions', { skip }, () => {
  const src = ['let d = datetime.of(2026, 10, 11)', 'print(d.year())', 'print(datetime.)'].join('\n');
  const info = analyse({ 'main.q': intel.blank(src, 2, 14, 15) });
  const d = info.exprs.find((e) => e.type === 'datetime');
  const members = intel.memberCompletions(info, d, 'd').map((m) => m.label);
  assert.ok(['year', 'add_days', 'format', 'weekday'].every((n) => members.includes(n)));
  const statics = intel.memberCompletions(info, intel.receiverAt(info, 'main.q', 2, 14), 'x').map((m) => m.label);
  assert.deepStrictEqual(statics.sort(), ['at', 'now', 'of', 'parse']);
});
