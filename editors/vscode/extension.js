'use strict';
// qlang support: the language server (`qlang lsp`) gives errors as you type, hovers, completion,
// go to definition, references, rename and symbols; this file starts it and adds a Run command.

const vscode = require('vscode');
const path = require('path');
const { LanguageClient } = require('vscode-languageclient/node');

let client;
let starting = Promise.resolve();
let warnedStartFailure = false;
let runTerminal;

function settings() {
  return vscode.workspace.getConfiguration('qlang');
}

function executable() {
  return settings().get('path', 'qlang') || 'qlang';
}

function clientOptions() {
  return {
    documentSelector: [{ scheme: 'file', language: 'qlang' }],
    middleware: {
      handleDiagnostics: (uri, diagnostics, next) =>
        next(uri, settings().get('diagnostics.enabled', true) ? diagnostics : []),
    },
  };
}

function warnStartFailure(error) {
  console.error('qlang: the language server did not start', error);
  if (warnedStartFailure) return;
  warnedStartFailure = true;
  vscode.window
    .showWarningMessage(
      `qlang: could not start the language server ("${executable()} lsp"). Install the latest qlang (cargo install --path crates/qlang-cli) or set "qlang.path".`,
      'Open settings'
    )
    .then((choice) => {
      if (choice) vscode.commands.executeCommand('workbench.action.openSettings', 'qlang.path');
    });
}

async function startClient() {
  client = new LanguageClient('qlang', 'qlang', { command: executable(), args: ['lsp'] }, clientOptions());
  try {
    await client.start();
  } catch (e) {
    client = undefined;
    warnStartFailure(e);
  }
}

async function stopClient() {
  const old = client;
  client = undefined;
  if (old) {
    try {
      await old.stop();
    } catch (_) {
      // it was not running
    }
  }
}

function restart() {
  starting = starting.then(async () => {
    await stopClient();
    await startClient();
  });
  return starting;
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('qlang.run', runFile),
    vscode.commands.registerCommand('qlang.restart', restart),
    // the debugger is `qlang dap` (breakpoints, steps, variables); a launch without a configuration debugs the open file
    vscode.debug.registerDebugAdapterDescriptorFactory('qlang', {
      createDebugAdapterDescriptor: () => new vscode.DebugAdapterExecutable(executable(), ['dap']),
    }),
    vscode.debug.registerDebugConfigurationProvider('qlang', {
      resolveDebugConfiguration: (_folder, config) => {
        if (!config.type) Object.assign(config, { type: 'qlang', request: 'launch', name: 'Debug the current file' });
        if (!config.program) config.program = '${file}';
        return config;
      },
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('qlang.path') || e.affectsConfiguration('qlang.diagnostics.enabled')) restart();
    }),
    vscode.window.onDidCloseTerminal((t) => {
      if (t === runTerminal) runTerminal = undefined;
    })
  );
  return restart();
}

function deactivate() {
  return stopClient();
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
  runTerminal.sendText(`cd ${quote(path.dirname(file))} && ${quote(executable())} run ${quote(path.basename(file))}`);
}

module.exports = { activate, deactivate };
