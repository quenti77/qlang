'use strict';
// Runs extension.js against small fakes of the VS Code API and of the language client.
const test = require('node:test');
const assert = require('node:assert');
const Module = require('node:module');

/** A fake `vscode` module, recording what the extension does. */
function fakeVscode(config) {
  const log = { commands: {}, terminals: [], warnings: [], messages: [], opened: [], listeners: {} };
  const on = (name) => (fn) => {
    (log.listeners[name] = log.listeners[name] || []).push(fn);
    return { dispose() {} };
  };
  const vscode = {
    commands: {
      registerCommand: (id, fn) => {
        log.commands[id] = fn;
        return { dispose() {} };
      },
      executeCommand: async (...args) => {
        log.opened.push(args);
      },
    },
    window: {
      activeTextEditor: undefined,
      onDidCloseTerminal: on('closeTerminal'),
      showWarningMessage: async (m) => {
        log.warnings.push(m);
      },
      showInformationMessage: async (m) => {
        log.messages.push(m);
      },
      createTerminal: (opts) => {
        const t = { opts, sent: [], show() {}, sendText(s) { this.sent.push(s); } };
        log.terminals.push(t);
        return t;
      },
    },
    workspace: {
      getConfiguration: () => ({ get: (key, dflt) => (key in config ? config[key] : dflt) }),
      onDidChangeConfiguration: on('config'),
    },
  };
  return { vscode, log };
}

/** A fake language client: it records how it was built and started. */
function fakeClient(fail) {
  const made = [];
  class LanguageClient {
    constructor(id, name, serverOptions, clientOptions) {
      Object.assign(this, { id, name, serverOptions, clientOptions, started: 0, stopped: 0 });
      made.push(this);
    }
    async start() {
      this.started++;
      if (fail) throw new Error('spawn ENOENT');
    }
    async stop() {
      this.stopped++;
    }
  }
  return { module: { LanguageClient }, made };
}

function loadExtension(vscode, client) {
  const original = Module._load;
  Module._load = function (request, ...rest) {
    if (request === 'vscode') return vscode;
    if (request === 'vscode-languageclient/node') return client;
    return original.call(this, request, ...rest);
  };
  try {
    delete require.cache[require.resolve('../extension')];
    return require('../extension');
  } finally {
    Module._load = original;
  }
}

test('the language server is started with `qlang lsp` for qlang files', async () => {
  const { vscode } = fakeVscode({ path: '/opt/my tools/qlang' });
  const client = fakeClient();
  const ext = loadExtension(vscode, client.module);
  await ext.activate({ subscriptions: [] });
  assert.strictEqual(client.made.length, 1);
  const c = client.made[0];
  assert.deepStrictEqual(c.serverOptions, { command: '/opt/my tools/qlang', args: ['lsp'] });
  assert.deepStrictEqual(c.clientOptions.documentSelector, [{ scheme: 'file', language: 'qlang' }]);
  assert.strictEqual(c.started, 1);
  await ext.deactivate();
  assert.strictEqual(c.stopped, 1);
});

test('the restart command and a changed setting start the server again', async () => {
  const { vscode, log } = fakeVscode({});
  const client = fakeClient();
  const ext = loadExtension(vscode, client.module);
  await ext.activate({ subscriptions: [] });
  await log.commands['qlang.restart']();
  assert.strictEqual(client.made.length, 2);
  assert.strictEqual(client.made[0].stopped, 1);
  assert.strictEqual(client.made[1].started, 1);

  const changed = { affectsConfiguration: (k) => k === 'qlang.path' };
  log.listeners.config[0](changed);
  await new Promise((r) => setTimeout(r, 50));
  assert.strictEqual(client.made.length, 3);
  log.listeners.config[0]({ affectsConfiguration: () => false });
  await new Promise((r) => setTimeout(r, 50));
  assert.strictEqual(client.made.length, 3, 'unrelated settings change nothing');
  await ext.deactivate();
});

test('errors are dropped when the setting turns them off', async () => {
  const config = { 'diagnostics.enabled': true };
  const { vscode } = fakeVscode(config);
  const client = fakeClient();
  const ext = loadExtension(vscode, client.module);
  await ext.activate({ subscriptions: [] });
  const handle = client.made[0].clientOptions.middleware.handleDiagnostics;
  const seen = [];
  handle('file:///a.q', ['error'], (uri, list) => seen.push(list));
  config['diagnostics.enabled'] = false;
  handle('file:///a.q', ['error'], (uri, list) => seen.push(list));
  assert.deepStrictEqual(seen, [['error'], []]);
  await ext.deactivate();
});

test('a server that does not start is reported once, with a hint', async () => {
  const { vscode, log } = fakeVscode({ path: 'no-such-qlang' });
  const client = fakeClient(true);
  const ext = loadExtension(vscode, client.module);
  await ext.activate({ subscriptions: [] });
  assert.strictEqual(log.warnings.length, 1);
  assert.match(log.warnings[0], /could not start the language server \("no-such-qlang lsp"\)/);
  await log.commands['qlang.restart']();
  assert.strictEqual(log.warnings.length, 1, 'the warning is not repeated');
  await ext.deactivate();
});

test('the Run command saves the file and starts it in a terminal', async () => {
  const { vscode, log } = fakeVscode({ path: '/opt/my tools/qlang' });
  const ext = loadExtension(vscode, fakeClient().module);
  await ext.activate({ subscriptions: [] });
  // no qlang file open
  await log.commands['qlang.run']();
  assert.strictEqual(log.terminals.length, 0);
  assert.match(log.messages[0], /Open a qlang file/);
  let saved = false;
  const doc = {
    languageId: 'qlang',
    uri: { fsPath: '/home/me/my project/hello world.q' },
    save: async () => {
      saved = true;
    },
  };
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
  await ext.deactivate();
});
