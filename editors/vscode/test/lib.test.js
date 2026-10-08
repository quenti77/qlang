'use strict';
const test = require('node:test');
const assert = require('node:assert');
const lib = require('../lib');

test('relativePosix', () => {
  assert.strictEqual(lib.relativePosix('/home/me/proj', '/home/me/proj/lib/a.q'), 'lib/a.q');
  assert.strictEqual(lib.relativePosix('C:\\proj', 'C:\\proj\\lib\\a.q'), 'lib/a.q');
  assert.strictEqual(lib.relativePosix('/home/me/proj/', '/home/me/proj/a.q'), 'a.q');
});

test('resolveImport follows the compiler rules', () => {
  assert.strictEqual(lib.resolveImport('main.q', 'math.q'), 'math.q');
  assert.strictEqual(lib.resolveImport('lib/a.q', 'b.q'), 'lib/b.q');
  assert.strictEqual(lib.resolveImport('lib/a.q', '../c.q'), 'c.q');
  assert.strictEqual(lib.resolveImport('lib/a.q', './sub/../b.q'), 'lib/b.q');
  assert.strictEqual(lib.resolveImport('main.q', '../x.q'), null); // leaves the project
  assert.strictEqual(lib.resolveImport('main.q', '/etc/x.q'), null); // absolute
});

test('importPaths finds both import forms and ignores other strings', () => {
  const text = [
    'import add, sub as minus from "math.q"',
    'import "lib/util.q" as util',
    '  import x from "indented.q"',
    'print("import y from \\"not.q\\"")',
    '-- import z from "comment.q"',
    'let s = "plain.q"',
  ].join('\n');
  assert.deepStrictEqual(lib.importPaths(text), ['math.q', 'lib/util.q', 'indented.q']);
});

test('collectFiles follows imports transitively and tolerates missing files', () => {
  const disk = {
    'lib/a.q': 'import g from "b.q"\nimport h from "../top.q"',
    'lib/b.q': 'fun g() end',
    'top.q': 'import loop from "lib/a.q"',
  };
  const read = (p) => (p in disk ? disk[p] : null);
  const files = lib.collectFiles('main.q', 'import f from "lib/a.q"\nimport m from "missing.q"', read);
  assert.deepStrictEqual(Object.keys(files).sort(), ['lib/a.q', 'lib/b.q', 'main.q', 'top.q']);
});

test('collectFiles stops at the limit', () => {
  const read = (p) => `import n from "f${Number(p.slice(1, -2)) + 1}.q"`;
  const files = lib.collectFiles('f0.q', 'import n from "f1.q"', read, 10);
  assert.ok(Object.keys(files).length <= 11);
});

test('buildRequest allows read() and names the entry', () => {
  const req = JSON.parse(lib.buildRequest('main.q', { 'main.q': 'print(1)' }));
  assert.strictEqual(req.entry, 'main.q');
  assert.deepStrictEqual(req.input, []);
  assert.strictEqual(req.files['main.q'], 'print(1)');
});

test('toFileDiagnostics converts to 0-based ranges and groups by file', () => {
  const response = {
    diagnostics: [
      {
        severity: 'error',
        code: 'T002',
        message: 'boom',
        location: { file: 'main.q', line: 3, col: 5, end_line: 3, end_col: 9 },
        notes: [
          { message: 'called from `f`', location: { file: 'lib/a.q', line: 2, col: 1, end_line: 2, end_col: 4 } },
          { message: 'a plain note', location: null },
        ],
      },
      { severity: 'warning', code: 'W1', message: 'empty range', location: { file: 'lib/a.q', line: 1, col: 4, end_line: 1, end_col: 4 }, notes: [] },
      { severity: 'error', code: 'J001', message: 'no location', location: null, notes: [] },
    ],
  };
  const out = lib.toFileDiagnostics(response, 'main.q');
  assert.deepStrictEqual(Object.keys(out).sort(), ['lib/a.q', 'main.q']);
  const first = out['main.q'][0];
  assert.deepStrictEqual([first.startLine, first.startCol, first.endLine, first.endCol], [2, 4, 2, 8]);
  assert.strictEqual(first.severity, 'error');
  assert.deepStrictEqual(first.related, [{ file: 'lib/a.q', line: 1, col: 0, message: 'called from `f`' }]);
  assert.deepStrictEqual(first.notes, ['a plain note']);
  const empty = out['lib/a.q'][0];
  assert.strictEqual(empty.severity, 'warning');
  assert.ok(empty.endCol > empty.startCol, 'an empty range is widened so it stays visible');
  // diagnostics without a location go to the entry file, at the top
  assert.deepStrictEqual([out['main.q'][1].startLine, out['main.q'][1].startCol], [0, 0]);
});
