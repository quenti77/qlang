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

test('the editor and the terminal agree on imports that go up a folder', { skip: !binary && 'build qlang first (cargo build)' }, () => {
  const os = require('node:os');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qlang-agree-'));
  try {
    fs.mkdirSync(path.join(dir, 'sub'));
    fs.writeFileSync(path.join(dir, 'common.q'), 'fun helper() -> int\n  7\nend\nexport helper\n');
    const main = 'import helper from "../common.q"\nprint(helper())\n';
    fs.writeFileSync(path.join(dir, 'sub', 'main.q'), main);
    // what the extension does: paths relative to the project root, files read from the disk
    const read = (rel) => {
      try { return fs.readFileSync(path.join(dir, rel), 'utf8'); } catch { return null; }
    };
    const files = lib.collectFiles('sub/main.q', main, read);
    assert.deepStrictEqual(Object.keys(files).sort(), ['common.q', 'sub/main.q']);
    assert.strictEqual(check('sub/main.q', files).ok, true);
    // what the Run button does: `cd sub && qlang run main.q`
    const run = cp.spawnSync(binary, ['run', 'main.q'], { cwd: path.join(dir, 'sub'), encoding: 'utf8' });
    assert.strictEqual(run.status, 0, run.stderr);
    assert.strictEqual(run.stdout, '7\n');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('packages of the project are followed through `qlang packages`', { skip: !binary && 'build qlang first (cargo build)' }, () => {
  const dir = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'qlang-ext-pkg-'));
  try {
    fs.mkdirSync(path.join(dir, 'app'));
    fs.mkdirSync(path.join(dir, 'mail/src'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'app/qlang.toml'), '[dependencies]\nmail = { path = "../mail" }\n');
    fs.writeFileSync(path.join(dir, 'mail/qlang.toml'), '[package]\nmain = "src/mail.q"\n');
    fs.writeFileSync(path.join(dir, 'mail/src/mail.q'), 'import h from "./helper.q"\nexport h\n');
    fs.writeFileSync(path.join(dir, 'mail/src/helper.q'), 'fun h() -> int\n  1\nend\nexport h\n');
    const app = path.join(dir, 'app');
    const found = cp.spawnSync(binary, ['packages'], { cwd: app, encoding: 'utf8' });
    const packages = JSON.parse(found.stdout);
    assert.strictEqual(packages.mail.main, 'src/mail.q');
    const read = (rel) => {
      const real = lib.realPath(app, rel, packages);
      try { return real && fs.readFileSync(real, 'utf8'); } catch (_) { return null; }
    };
    const main = 'import "@mail" as m\nprint(m.h())\nlet x: int = "no"';
    const files = lib.collectFiles('main.q', main, read, 200, packages);
    assert.ok('@mail/src/helper.q' in files, Object.keys(files).join());
    const r = cp.spawnSync(binary, ['check', '--json'], { input: lib.buildRequest('main.q', files, false, packages), encoding: 'utf8' });
    const res = JSON.parse(r.stdout);
    const byFile = lib.toFileDiagnostics(res, 'main.q');
    assert.deepStrictEqual(Object.keys(byFile), ['main.q']);
    assert.match(byFile['main.q'][0].message, /expected `int`, found `string`/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
