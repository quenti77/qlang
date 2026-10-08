'use strict';
// End to end with the real compiler: the same steps as the extension, without VS Code.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const lib = require('../lib');

const repo = path.join(__dirname, '..', '..', '..');
const binary = ['target/release/qlang', 'target/debug/qlang'].map((p) => path.join(repo, p)).find((p) => fs.existsSync(p));

function check(entry, files) {
  const r = cp.spawnSync(binary, ['check', '--json'], { input: lib.buildRequest(entry, files), encoding: 'utf8' });
  assert.strictEqual(r.status, 0, r.stderr);
  return JSON.parse(r.stdout);
}

test('qlang check --json reports type errors with positions', { skip: !binary && 'build qlang first (cargo build)' }, () => {
  const main = 'import add from "math.q"\nlet x: int = "text"\nprint(add(1, 2))';
  const math = 'fun add(a: int, b: int) -> int\n  a +\nend\nexport add';
  const disk = { 'math.q': math };
  const files = lib.collectFiles('main.q', main, (p) => disk[p] ?? null);
  const res = check('main.q', files);
  assert.strictEqual(res.ok, false);
  const byFile = lib.toFileDiagnostics(res, 'main.q');
  // the parse error in math.q is reported there, at the right place
  assert.deepStrictEqual(Object.keys(byFile), ['math.q']);
  assert.match(byFile['math.q'][0].message, /expected an expression/);
  assert.strictEqual(byFile['math.q'][0].startLine, 2);

  disk['math.q'] = 'fun add(a: int, b: int) -> int\n  a + b\nend\nexport add';
  const res2 = check('main.q', lib.collectFiles('main.q', main, (p) => disk[p] ?? null));
  const d = lib.toFileDiagnostics(res2, 'main.q')['main.q'];
  assert.strictEqual(d.length, 1);
  assert.match(d[0].message, /expected `int`, found `string`/);
  assert.deepStrictEqual([d[0].startLine, d[0].startCol, d[0].endLine, d[0].endCol], [1, 13, 1, 19]);
});

test('a correct program has no diagnostics, and does not run', { skip: !binary && 'build qlang first (cargo build)' }, () => {
  const res = check('main.q', { 'main.q': 'print("never printed")\nlet n = read()' });
  assert.strictEqual(res.ok, true);
  assert.strictEqual(res.phase, 'check');
  assert.strictEqual(res.output, '');
  assert.deepStrictEqual(res.diagnostics, []);
});

test('every example in the repository checks cleanly', { skip: !binary && 'build qlang first (cargo build)' }, () => {
  const dir = path.join(repo, 'examples');
  const all = Object.fromEntries(fs.readdirSync(dir).filter((f) => f.endsWith('.q')).map((f) => [f, fs.readFileSync(path.join(dir, f), 'utf8')]));
  for (const f of ['tour.q', 'beginner.q', 'advanced.q']) {
    const files = lib.collectFiles(f, all[f], (p) => all[p] ?? null);
    const res = check(f, files);
    assert.strictEqual(res.ok, true, `${f}: ${JSON.stringify(res.diagnostics)}`);
  }
});
