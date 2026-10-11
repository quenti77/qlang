// The WebAssembly build, driven through its JavaScript wrapper.
import test from 'node:test';
import assert from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { load } from '../js/qlang.mjs';

const repo = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const wasmPath = ['wasm', 'release'].map((p) => path.join(repo, 'target/wasm32-unknown-unknown', p, 'qlang_wasm.wasm')).find((p) => fs.existsSync(p));
const skip = !wasmPath && 'build it first: cargo build --profile wasm --target wasm32-unknown-unknown -p qlang-wasm';
const qlang = skip ? null : await load(fs.readFileSync(wasmPath));

test('runs a program', { skip }, () => {
  const b = qlang.handle({ files: { 'main.q': 'print("héllo {6 * 7}")' } });
  assert.strictEqual(b.ok, true);
  assert.strictEqual(b.output, 'héllo 42\n');
});

test('reports the version of the build', { skip }, () => {
  const cargo = fs.readFileSync(path.join(repo, 'Cargo.toml'), 'utf8').match(/^version = "(.*)"/m)[1];
  assert.strictEqual(qlang.version, cargo);
});

test('imports come from the request, read() from its input', { skip }, () => {
  const a = qlang.handle({
    files: {
      'main.q': 'import double from "lib/m.q"\nlet n = read().to_int()\nif n != none then print(double(n)) end',
      'lib/m.q': 'fun double(n: int) -> int\n  n * 2\nend\nexport double',
    },
    input: ['21'],
  });
  assert.strictEqual(a.output, '42\n', JSON.stringify(a));
});

test('errors are data, with positions', { skip }, () => {
  const a = qlang.handle({ files: { 'main.q': 'let x: int = "a"' } });
  assert.strictEqual(a.ok, false);
  assert.strictEqual(a.phase, 'compile');
  assert.strictEqual(a.diagnostics[0].code, 'T002');
  assert.deepStrictEqual([a.diagnostics[0].location.line, a.diagnostics[0].location.col], [1, 14]);
});

test('editor information is available', { skip }, () => {
  const a = qlang.handle({ files: { 'main.q': 'let xs = [1, 2]\nprint(xs.len())' }, mode: 'check', info: true });
  assert.ok(a.info.members['array<int>'].some((m) => m.name === 'push'));
});

test('a deep recursion fits in the stack of the module', { skip }, () => {
  const src = 'fun d(n: int) -> int\n  if n == 0 then return 0 end\n  1 + d(n - 1)\nend\nprint(d(990))';
  const a = qlang.handle({ files: { 'main.q': src } });
  assert.strictEqual(a.output, '990\n', JSON.stringify(a.diagnostics));
});

test('runaway programs are stopped by the limits', { skip }, () => {
  const loop = qlang.handle({ files: { 'main.q': 'while true do end' }, limits: { max_steps: 100000 } });
  assert.strictEqual(loop.ok, false);
  assert.match(loop.diagnostics[0].message, /step/i);
  const deep = qlang.handle({ files: { 'main.q': 'fun f(n: int) -> int\n  f(n + 1)\nend\nprint(f(0))' } });
  assert.strictEqual(deep.ok, false);
  const out = qlang.handle({ files: { 'main.q': 'while true do print("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx") end' } });
  assert.strictEqual(out.ok, false);
  // and the module is still usable afterwards
  assert.strictEqual(qlang.handle({ files: { 'main.q': 'print(1)' } }).output, '1\n');
});

test('garbage requests get an answer, not a crash', { skip }, () => {
  assert.strictEqual(qlang.handle('not json').ok, false);
  assert.strictEqual(qlang.handle('{"files":{}}').ok, false);
});
