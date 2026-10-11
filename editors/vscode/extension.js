'use strict';
// qlang support: errors as you type (through `qlang check --json`) and a Run command.

const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const path = require('path');
const lib = require('./lib');
const intel = require('./intel');

let collection;
let warnedMissingBinary = false;
const timers = new Map(); // document uri -> pending check
const published = new Map(); // entry uri -> uris that carry its diagnostics
let runTerminal;
const analyses = new Map(); // document uri -> { version, info, entry, root } of the last check

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

  const qlang = { language: 'qlang', scheme: 'file' };
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(qlang, { provideHover }),
    vscode.languages.registerCompletionItemProvider(qlang, { provideCompletionItems }, '.'),
    vscode.languages.registerSignatureHelpProvider(qlang, { provideSignatureHelp }, '(', ',')
  );
}

function deactivate() {
  timers.forEach((t) => clearTimeout(t));
  timers.clear();
}

function clear(doc) {
  const key = doc.uri.toString();
  if (timers.has(key)) clearTimeout(timers.get(key));
  timers.delete(key);
  analyses.delete(key);
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

/**
 * Ask the compiler about `doc` (or about `text`, a variant of it). The response carries the
 * diagnostics and what editors need: types, variables, members.
 */
async function analyse(doc, text) {
  const root = rootFor(doc);
  const entry = lib.relativePosix(root, doc.uri.fsPath);
  const files = lib.collectFiles(entry, text === undefined ? doc.getText() : text, (rel) => readFileText(root, rel));
  const response = await runQlang(['check', '--json'], lib.buildRequest(entry, files, true), root);
  return response ? { response, entry, root, info: response.info || null } : null;
}

async function check(doc) {
  const version = doc.version;
  const a = await analyse(doc);
  if (!a) return;
  if (a.info) analyses.set(doc.uri.toString(), { version, info: a.info, entry: a.entry, root: a.root });
  publish(doc, a.root, a.entry, a.response);
}

/** What the last check learned, re-checking first if the document changed since. */
async function currentInfo(doc) {
  const known = analyses.get(doc.uri.toString());
  if (known && known.version === doc.version) return known;
  const version = doc.version;
  const a = await analyse(doc);
  if (a && a.info) {
    const fresh = { version, info: a.info, entry: a.entry, root: a.root };
    analyses.set(doc.uri.toString(), fresh);
    return fresh;
  }
  return known || null; // the code does not compile far enough: the previous answer is better than none
}

function linesReader(root) {
  const cache = new Map();
  return (rel) => {
    if (!cache.has(rel)) {
      const text = readFileText(root, rel);
      cache.set(rel, text === null ? null : text.split('\n'));
    }
    return cache.get(rel);
  };
}

function provideHover(doc, pos) {
  return currentInfo(doc).then((a) => {
    if (!a) return undefined;
    const h = intel.hoverAt(a.info, a.entry, pos.line, pos.character, doc.lineAt(pos.line).text, linesReader(a.root));
    if (!h) return undefined;
    return new vscode.Hover(new vscode.MarkdownString(h.markdown), new vscode.Range(h.range.line, h.range.start, h.range.line, h.range.end));
  });
}

const COMPLETION_KIND = {
  field: vscode.CompletionItemKind.Field,
  method: vscode.CompletionItemKind.Method,
  function: vscode.CompletionItemKind.Function,
  variable: vscode.CompletionItemKind.Variable,
  param: vscode.CompletionItemKind.Variable,
  keyword: vscode.CompletionItemKind.Keyword,
  type: vscode.CompletionItemKind.TypeParameter,
  struct: vscode.CompletionItemKind.Struct,
  enum: vscode.CompletionItemKind.Enum,
  variant: vscode.CompletionItemKind.EnumMember,
  constant: vscode.CompletionItemKind.Constant,
  trait: vscode.CompletionItemKind.Interface,
  module: vscode.CompletionItemKind.Module,
};

function toCompletion(item, range) {
  const c = new vscode.CompletionItem(item.label, COMPLETION_KIND[item.kind]);
  c.detail = item.detail;
  if (item.doc) c.documentation = new vscode.MarkdownString(item.doc);
  c.sortText = item.sort;
  if (range) c.range = range;
  return c;
}

async function provideCompletionItems(doc, pos) {
  const text = doc.lineAt(pos.line).text;
  const access = intel.memberAccessAt(text, pos.character);
  if (access) {
    // `value.par|`: ask the compiler about the code without `.par`, so that it still compiles
    const a = await analyse(doc, intel.blank(doc.getText(), pos.line, access.dot, pos.character));
    if (!a || !a.info) return undefined;
    const expr = intel.receiverAt(a.info, a.entry, pos.line, access.dot);
    const range = new vscode.Range(pos.line, access.start, pos.line, pos.character);
    return intel.memberCompletions(a.info, expr, text.slice(0, access.dot), linesReader(a.root)).map((i) => toCompletion(i, range));
  }
  const a = await currentInfo(doc);
  if (!a) return undefined;
  return intel.nameCompletions(a.info, a.entry, pos.line, pos.character, linesReader(a.root)).map((i) => toCompletion(i));
}

async function provideSignatureHelp(doc, pos) {
  const text = doc.lineAt(pos.line).text;
  const call = intel.callAt(text, pos.character);
  if (!call) return undefined;
  const a = await analyse(doc, intel.blank(doc.getText(), pos.line, call.open, pos.character));
  if (!a || !a.info) return undefined;
  const sig = intel.signatureFor(a.info, a.entry, pos.line, text, call, linesReader(a.root));
  if (!sig) return undefined;
  const info = new vscode.SignatureInformation(sig.label, sig.doc ? new vscode.MarkdownString(sig.doc) : undefined);
  info.parameters = sig.parameters.map((p) => new vscode.ParameterInformation(p));
  const help = new vscode.SignatureHelp();
  help.signatures = [info];
  help.activeSignature = 0;
  help.activeParameter = Math.min(sig.activeParam, Math.max(0, sig.parameters.length - 1));
  return help;
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
