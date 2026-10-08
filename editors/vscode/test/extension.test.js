'use strict';
// Runs extension.js against a small fake of the VS Code API, with the real qlang binary.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Module = require('node:module');

const repo = path.join(__dirname, '..', '..', '..');
const binary = ['target/release/qlang', 'target/debug/qlang'].map((p) => path.join(repo, p)).find((p) => fs.existsSync(p));

/** A fake `vscode` module, recording what the extension does. */
function fakeVscode(config) {
  const log = { diagnostics: new Map(), commands: {}, terminals: [], warnings: [], messages: [] };
  class Position { constructor(line, character) { this.line = line; this.character = character; } }
  class Range { constructor(sl, sc, el, ec) { this.start = new Position(sl, sc); this.end = new Position(el, ec); } }
  class Location { constructor(uri, pos) { this.uri = uri; this.range = pos; } }
  class Diagnostic { constructor(range, message, severity) { this.range = range; this.message = message; this.severity = severity; } }
  class DiagnosticRelatedInformation { constructor(location, message) { this.location = location; this.message = message; } }
  const uriOf = (fsPath) => ({ scheme: 'file', fsPath, toString: () => 'file://' + fsPath });
  const listeners = {};
  const on = (name) => (fn) => { (listeners[name] = listeners[name] || []).push(fn); return { dispose() {} }; };
  const vscode = {
    Position, Range, Location, Diagnostic, DiagnosticRelatedInformation,
    DiagnosticSeverity: { Error: 0, Warning: 1 },
    Uri: { file: uriOf, parse: (s) => uriOf(s.replace(/^file:\/\//, '')) },
    languages: {
      createDiagnosticCollection: () => ({
        set: (uri, list) => log.diagnostics.set(uri.fsPath, list),
        delete: (uri) => log.diagnostics.delete(uri.fsPath),
        dispose() {},
      }),
    },
    commands: {
      registerCommand: (id, fn) => { log.commands[id] = fn; return { dispose() {} }; },
      executeCommand: async () => {},
    },
    window: {
      activeTextEditor: undefined,
      onDidCloseTerminal: on('closeTerminal'),
      showWarningMessage: async (m) => { log.warnings.push(m); },
      showInformationMessage: async (m) => { log.messages.push(m); },
      createTerminal: (opts) => {
        const t = { opts, sent: [], show() {}, sendText(s) { this.sent.push(s); } };
        log.terminals.push(t);
        return t;
      },
    },
    workspace: {
      textDocuments: [],
      getConfiguration: () => ({ get: (key, dflt) => (key in config ? config[key] : dflt) }),
      getWorkspaceFolder: () => undefined,
      onDidOpenTextDocument: on('open'),
      onDidSaveTextDocument: on('save'),
      onDidChangeTextDocument: on('change'),
      onDidCloseTextDocument: on('close'),
      onDidChangeConfiguration: on('config'),
    },
  };
  const makeDoc = (fsPath, text) => ({
    languageId: 'qlang', uri: uriOf(fsPath), getText: () => text, save: async () => true,
  });
  return { vscode, log, listeners, makeDoc };
}

function loadExtension(vscode) {
  const original = Module._load;
  Module._load = function (request, ...rest) {
    return request === 'vscode' ? vscode : original.call(this, request, ...rest);
  };
  try {
    delete require.cache[require.resolve('../extension')];
    return require('../extension');
  } finally {
    Module._load = original;
  }
}

const until = async (cond, ms = 4000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (cond()) return true;
    await new Promise((r) => setTimeout(r, 25));
  }
  return false;
};

const skip = !binary && 'build qlang first (cargo build)';

test('errors are published for the file and for the modules it imports', { skip }, async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qlang-ext-'));
  fs.writeFileSync(path.join(dir, 'math.q'), 'fun add(a: int, b: int) -> int\n  a + b\nend\nexport add\n');
  const mainPath = path.join(dir, 'main.q');
  const { vscode, log, makeDoc } = fakeVscode({ path: binary, 'diagnostics.delay': 10 });
  const ext = loadExtension(vscode);
  const doc = makeDoc(mainPath, 'import add from "math.q"\nlet x: int = "text"\nprint(add(1, 2))\n');
  vscode.workspace.textDocuments.push(doc);
  const context = { subscriptions: [] };
  ext.activate(context);

  assert.ok(await until(() => log.diagnostics.has(mainPath)), 'diagnostics were published');
  const list = log.diagnostics.get(mainPath);
  assert.strictEqual(list.length, 1);
  assert.match(list[0].message, /expected `int`, found `string`/);
  assert.strictEqual(list[0].source, 'qlang');
  assert.strictEqual(list[0].code, 'T002');
  assert.deepStrictEqual(
    [list[0].range.start.line, list[0].range.start.character, list[0].range.end.line, list[0].range.end.character],
    [1, 13, 1, 19]
  );

  // fix the error: the markers disappear
  const fixed = makeDoc(mainPath, 'import add from "math.q"\nlet x: int = add(1, 2)\nprint(x)\n');
  vscode.workspace.textDocuments.length = 0;
  vscode.workspace.textDocuments.push(fixed);
  log.commands['qlang.check'] && (vscode.window.activeTextEditor = { document: fixed });
  await log.commands['qlang.check']();
  assert.ok(await until(() => !log.diagnostics.has(mainPath)), 'diagnostics were cleared');

  // an unsaved edit of an imported module wins over the disk
  const mathDoc = makeDoc(path.join(dir, 'math.q'), 'fun add(a: int, b: int) -> int\n  a +\nend\nexport add\n');
  vscode.workspace.textDocuments.push(mathDoc);
  await log.commands['qlang.check']();
  assert.ok(await until(() => log.diagnostics.has(path.join(dir, 'math.q'))), 'error shown in the imported file');
  assert.match(log.diagnostics.get(path.join(dir, 'math.q'))[0].message, /expected an expression/);
  ext.deactivate();
});

test('the Run command saves the file and starts it in a terminal', { skip }, async () => {
  const { vscode, log, makeDoc } = fakeVscode({ path: '/opt/my tools/qlang' });
  const ext = loadExtension(vscode);
  ext.activate({ subscriptions: [] });
  // no qlang file open
  await log.commands['qlang.run']();
  assert.strictEqual(log.terminals.length, 0);
  assert.match(log.messages[0], /Open a qlang file/);
  const doc = makeDoc('/home/me/my project/hello world.q', 'print(1)');
  let saved = false;
  doc.save = async () => { saved = true; };
  vscode.window.activeTextEditor = { document: doc };
  await log.commands['qlang.run']();
  assert.ok(saved, 'the file was saved first');
  assert.strictEqual(log.terminals.length, 1);
  assert.strictEqual(
    log.terminals[0].sent[0],
    "cd '/home/me/my project' && '/opt/my tools/qlang' run 'hello world.q'"
  );
  await log.commands['qlang.run']();
  assert.strictEqual(log.terminals.length, 1, 'the terminal is reused');
  ext.deactivate();
});

test('a missing executable is reported once, with a hint', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qlang-ext-'));
  const { vscode, log, makeDoc } = fakeVscode({ path: path.join(dir, 'no-such-qlang'), 'diagnostics.delay': 10 });
  const ext = loadExtension(vscode);
  const doc = makeDoc(path.join(dir, 'a.q'), 'print(1)');
  vscode.workspace.textDocuments.push(doc);
  ext.activate({ subscriptions: [] });
  assert.ok(await until(() => log.warnings.length === 1));
  assert.match(log.warnings[0], /was not found/);
  vscode.window.activeTextEditor = { document: doc };
  await log.commands['qlang.check']();
  await new Promise((r) => setTimeout(r, 300));
  assert.strictEqual(log.warnings.length, 1, 'the warning is not repeated');
  ext.deactivate();
});

test('other languages and disabled diagnostics are left alone', { skip }, async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qlang-ext-'));
  const { vscode, log, makeDoc } = fakeVscode({ path: binary, 'diagnostics.enabled': false, 'diagnostics.delay': 10 });
  const ext = loadExtension(vscode);
  const bad = makeDoc(path.join(dir, 'bad.q'), 'let x: int = "a"');
  vscode.workspace.textDocuments.push(bad);
  ext.activate({ subscriptions: [] });
  await new Promise((r) => setTimeout(r, 400));
  assert.strictEqual(log.diagnostics.size, 0, 'nothing published when disabled');
  ext.deactivate();
});
