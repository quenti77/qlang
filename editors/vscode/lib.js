'use strict';
// Pure helpers (no VS Code API), so they can be tested with plain Node.

/** `a\b\c` to `a/b/c`. */
function toPosix(p) {
  return p.split('\\').join('/');
}

/** Path of `file` relative to `root`, with `/` separators. */
function relativePosix(root, file) {
  const r = toPosix(root).replace(/\/+$/, '');
  const f = toPosix(file);
  return f.startsWith(r + '/') ? f.slice(r.length + 1) : f;
}

/**
 * Resolve an import path like the qlang compiler does: relative to the importing
 * file, never absolute, never leaving the project. Returns null if invalid.
 */
function resolveImport(importerRel, importPath) {
  if (importPath.startsWith('/')) return null;
  const parts = [];
  const dir = importerRel.includes('/') ? importerRel.slice(0, importerRel.lastIndexOf('/')) : '';
  const joined = (dir ? dir + '/' : '') + importPath;
  for (const part of joined.split('/')) {
    if (part === '' || part === '.') continue;
    if (part === '..') {
      if (parts.length === 0) return null;
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.length ? parts.join('/') : null;
}

/** Import paths written in a source text (`import a from "x.q"`, `import "x.q" as m`). */
function importPaths(text) {
  const out = [];
  const re = /^[ \t]*import\b[^\n]*?"([^"\n]*)"/gm;
  let m;
  while ((m = re.exec(text)) !== null) out.push(m[1]);
  return out;
}

/**
 * The entry file and everything it imports, transitively.
 * `readText(relPath)` returns the text of a file, or null if it does not exist.
 */
function collectFiles(entryRel, entryText, readText, limit = 200) {
  const files = { [entryRel]: entryText };
  const queue = [entryRel];
  while (queue.length && Object.keys(files).length < limit) {
    const current = queue.shift();
    for (const imp of importPaths(files[current])) {
      const target = resolveImport(current, imp);
      if (target === null || target in files) continue;
      const text = readText(target);
      if (text === null || text === undefined) continue; // the compiler reports it
      files[target] = text;
      queue.push(target);
    }
  }
  return files;
}

/** The JSON request understood by `qlang check --json`. */
function buildRequest(entry, files) {
  // `input: []` allows `read()`: the editor runs programs in a terminal with a keyboard
  return JSON.stringify({ entry, files, input: [] });
}

function severityName(s) {
  return s === 'warning' ? 'warning' : 'error';
}

/**
 * Turn a qlang response into diagnostics grouped by file, with 0-based
 * positions as VS Code wants them.
 */
function toFileDiagnostics(response, fallbackFile) {
  const byFile = {};
  const push = (file, d) => {
    (byFile[file] = byFile[file] || []).push(d);
  };
  for (const d of response.diagnostics || []) {
    const loc = d.location;
    const file = loc ? loc.file : fallbackFile;
    const startLine = loc ? loc.line - 1 : 0;
    const startCol = loc ? loc.col - 1 : 0;
    let endLine = loc ? loc.end_line - 1 : 0;
    let endCol = loc ? loc.end_col - 1 : 0;
    if (endLine < startLine || (endLine === startLine && endCol <= startCol)) {
      endLine = startLine;
      endCol = startCol + 1; // never an empty range: it would be invisible
    }
    push(file, {
      startLine,
      startCol,
      endLine,
      endCol,
      severity: severityName(d.severity),
      code: d.code,
      message: d.message,
      related: (d.notes || [])
        .filter((n) => n.location)
        .map((n) => ({
          file: n.location.file,
          line: n.location.line - 1,
          col: n.location.col - 1,
          message: n.message,
        })),
      notes: (d.notes || []).filter((n) => !n.location).map((n) => n.message),
    });
  }
  return byFile;
}

module.exports = {
  toPosix,
  relativePosix,
  resolveImport,
  importPaths,
  collectFiles,
  buildRequest,
  toFileDiagnostics,
};
