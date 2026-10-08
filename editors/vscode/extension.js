'use strict';
// qlang support: errors as you type (through `qlang check --json`) and a Run command.

const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const path = require('path');
const lib = require('./lib');

let collection;
let warnedMissingBinary = false;
const timers = new Map(); // document uri -> pending check
const published = new Map(); // entry uri -> uris that carry its diagnostics
let runTerminal;

function settings() {
  return vscode.workspace.getConfiguration('qlang');
}

function activate(context) {
  collection = vscode.languages.createDiagnosticCollection('qlang');
  context.subscriptions.push(
    collection,
    vscode.commands.registerCommand('qlang.run', runFile),
    vscode.commands.registerCommand('qlang.check', () => {
      const doc = vscode.window.activeTextEditor && vscode.window.activeTextEditor.document;
      if (doc && doc.languageId === 'qlang') schedule(doc, 0);
    }),
    vscode.workspace.onDidOpenTextDocument((d) => schedule(d, 0)),
    vscode.workspace.onDidSaveTextDocument((d) => schedule(d, 0)),
    vscode.workspace.onDidChangeTextDocument((e) => schedule(e.document, settings().get('diagnostics.delay', 400))),
    vscode.workspace.onDidCloseTextDocument((d) => clear(d)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('qlang')) {
        vscode.workspace.textDocuments.forEach((d) => schedule(d, 0));
      }
    }),
    vscode.window.onDidCloseTerminal((t) => {
      if (t === runTerminal) runTerminal = undefined;
    })
  );
  vscode.workspace.textDocuments.forEach((d) => schedule(d, 0));
}

function deactivate() {
  timers.forEach((t) => clearTimeout(t));
  timers.clear();
}

function clear(doc) {
  const key = doc.uri.toString();
  if (timers.has(key)) clearTimeout(timers.get(key));
  timers.delete(key);
  for (const uri of published.get(key) || []) collection.delete(vscode.Uri.parse(uri));
  published.delete(key);
}

function schedule(doc, delay) {
  if (doc.languageId !== 'qlang' || doc.uri.scheme !== 'file') return;
  if (!settings().get('diagnostics.enabled', true)) {
    clear(doc);
    return;
  }
  const key = doc.uri.toString();
  if (timers.has(key)) clearTimeout(timers.get(key));
  timers.set(
    key,
    setTimeout(() => {
      timers.delete(key);
      check(doc).catch((e) => console.error('qlang check failed', e));
    }, delay)
  );
}

/** The root that import paths are relative to: the workspace folder, or the file's folder. */
function rootFor(doc) {
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri);
  return folder ? folder.uri.fsPath : path.dirname(doc.uri.fsPath);
}

function readFileText(root, rel) {
  const abs = path.join(root, rel);
  // an open, possibly unsaved editor wins over the disk
  const open = vscode.workspace.textDocuments.find((d) => d.uri.scheme === 'file' && d.uri.fsPath === abs);
  if (open) return open.getText();
  try {
    return fs.readFileSync(abs, 'utf8');
  } catch (_) {
    return null;
  }
}

function runQlang(args, stdin, cwd) {
  return new Promise((resolve) => {
    const exe = settings().get('path', 'qlang') || 'qlang';
    let child;
    try {
      child = cp.spawn(exe, args, { cwd });
    } catch (e) {
      resolve(null);
      return;
    }
    let out = '';
    const timeout = setTimeout(() => child.kill(), 10000);
    child.stdout.on('data', (d) => (out += d));
    child.on('error', (e) => {
      clearTimeout(timeout);
      if (e.code === 'ENOENT' && !warnedMissingBinary) {
        warnedMissingBinary = true;
        vscode.window
          .showWarningMessage(
            `qlang: the executable "${exe}" was not found. Install it (cargo install --path crates/qlang-cli) or set "qlang.path".`,
            'Open settings'
          )
          .then((choice) => {
            if (choice) vscode.commands.executeCommand('workbench.action.openSettings', 'qlang.path');
          });
      }
      resolve(null);
    });
    child.on('close', () => {
      clearTimeout(timeout);
      try {
        resolve(JSON.parse(out));
      } catch (_) {
        resolve(null);
      }
    });
    child.stdin.on('error', () => {});
    child.stdin.end(stdin);
  });
}

async function check(doc) {
  const root = rootFor(doc);
  const entry = lib.relativePosix(root, doc.uri.fsPath);
  const files = lib.collectFiles(entry, doc.getText(), (rel) => readFileText(root, rel));
  const response = await runQlang(['check', '--json'], lib.buildRequest(entry, files), root);
  if (!response) return;
  publish(doc, root, entry, response);
}

const SEVERITY = {
  error: vscode.DiagnosticSeverity.Error,
  warning: vscode.DiagnosticSeverity.Warning,
};

function publish(doc, root, entry, response) {
  const key = doc.uri.toString();
  const byFile = lib.toFileDiagnostics(response, entry);
  const uriOf = (rel) => vscode.Uri.file(path.join(root, rel));
  const now = new Set();
  for (const [file, list] of Object.entries(byFile)) {
    const uri = uriOf(file);
    now.add(uri.toString());
    collection.set(
      uri,
      list.map((d) => {
        const range = new vscode.Range(d.startLine, d.startCol, d.endLine, d.endCol);
        const diag = new vscode.Diagnostic(range, d.message, SEVERITY[d.severity]);
        diag.source = 'qlang';
        diag.code = d.code;
        const related = d.related.map(
          (r) =>
            new vscode.DiagnosticRelatedInformation(
              new vscode.Location(uriOf(r.file), new vscode.Position(r.line, r.col)),
              r.message
            )
        );
        if (related.length) diag.relatedInformation = related;
        if (d.notes.length) diag.message += '\n' + d.notes.join('\n');
        return diag;
      })
    );
  }
  // this document is fine: clear what it published before
  if (!now.has(key)) collection.delete(doc.uri);
  for (const uri of published.get(key) || []) {
    if (!now.has(uri)) collection.delete(vscode.Uri.parse(uri));
  }
  published.set(key, now);
}

function quote(s) {
  return /^[\w@%+=:,./-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
}

async function runFile() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'qlang') {
    vscode.window.showInformationMessage('Open a qlang file (.q) to run it.');
    return;
  }
  await editor.document.save();
  const file = editor.document.uri.fsPath;
  if (!runTerminal) runTerminal = vscode.window.createTerminal({ name: 'qlang' });
  runTerminal.show(true);
  const exe = settings().get('path', 'qlang') || 'qlang';
  runTerminal.sendText(`cd ${quote(path.dirname(file))} && ${quote(exe)} run ${quote(path.basename(file))}`);
}

module.exports = { activate, deactivate };
