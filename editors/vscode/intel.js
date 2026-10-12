'use strict';
// Hovers, completion and signature help, computed from what `qlang check --json` reports
// (`info`), documentation comments included (the compiler attaches the `--" ... --"` comment
// written above a declaration to it). Pure functions, no VS Code API, so they can be tested
// with plain Node.
//
// Positions: `line` and `col` arguments are 0-based (as in VS Code); the `loc` objects of
// the compiler are 1-based.

const KEYWORDS = {
  fun: 'Declares a function: `fun name(a: int) -> int ... end`.',
  struct: 'Declares a structure: named fields, optionally `extends` another structure.',
  impl: 'Gives methods to a type (`impl Point ... end`) or implements a trait (`impl Trait for Point`).',
  trait: 'Declares a set of method signatures that types can implement.',
  enum: 'Declares a type with a fixed list of named values.',
  let: 'Declares a variable: `let x = 1`.',
  const: 'Declares a constant name: it cannot be assigned again (the value it holds can still change).',
  if: '`if condition then ... elseif condition then ... else ... end`. Conditions must be `bool`.',
  then: 'Starts the body of an `if` or `elseif`.',
  elseif: 'Another condition of an `if`.',
  else: 'What an `if` does when no condition is true.',
  while: '`while condition do ... end`.',
  for: '`for i in 0..10 do ... end` (`..=` includes the end), or `for k, v in map do ... end`.',
  in: 'Used by `for ... in ...`.',
  step: '`for i in 0..10 step 2 do ... end`.',
  do: 'Starts the body of a loop.',
  match: '`match value case A then ... else ... end`.',
  case: 'One branch of a `match`.',
  return: 'Leaves the function, optionally with a value.',
  break: 'Leaves the loop.',
  continue: 'Goes to the next turn of the loop.',
  end: 'Closes a block.',
  import: '`import name from "file.q"` or `import "file.q" as module`.',
  export: 'Makes names usable from other files.',
  from: 'Used by `import ... from "file.q"`.',
  as: 'Converts a value (`3 as float`) or names an import.',
  and: 'Logical and (both sides must be `bool`).',
  or: 'Logical or (both sides must be `bool`).',
  not: 'Logical not.',
  div: 'Integer division: `7 div 2` is 3.',
  mod: 'Remainder: `7 mod 2` is 1.',
  self: 'The value a method is called on.',
  super: 'The parent structure: `super.method()`.',
  public: 'Visible everywhere (the default is `private`).',
  private: 'Visible only inside the `impl` of the type.',
  protected: 'Visible in the type and the types that extend it.',
  static: 'Belongs to the type, not to a value.',
  override: 'Replaces a method of the parent structure.',
  extends: 'Makes a structure inherit from another one.',
  true: 'The boolean `true`.',
  false: 'The boolean `false`.',
  none: 'The absence of a value (`T?` types).',
};

const RESERVED = new Set(Object.keys(KEYWORDS));

const CODE_FENCE = (text) => '```qlang\n' + text + '\n```';

const isWordChar = (c) => /[A-Za-z0-9_]/.test(c);

/** The identifier around column `col` of `text`: {word, start, end}, or null. */
function wordAt(text, col) {
  let s = Math.min(col, text.length);
  let e = s;
  while (s > 0 && isWordChar(text[s - 1])) s--;
  while (e < text.length && isWordChar(text[e])) e++;
  if (s === e || /^[0-9]/.test(text.slice(s, e))) return null;
  return { word: text.slice(s, e), start: s, end: e };
}

const within = (loc, line1, col1) => {
  if (line1 < loc.line || line1 > loc.end_line) return false;
  if (line1 === loc.line && col1 < loc.col) return false;
  if (line1 === loc.end_line && col1 >= loc.end_col) return false;
  return true;
};

const sameRange = (loc, line1, startCol0, endCol0) =>
  loc.line === line1 && loc.end_line === line1 && loc.col === startCol0 + 1 && loc.end_col === endCol0 + 1;

const before = (loc, line1, col1) => loc.line < line1 || (loc.line === line1 && loc.col <= col1);

/** The text just before the word is a `.`: the (0-based) column of that dot, or -1. */
function dotBefore(text, wordStart) {
  let i = wordStart - 1;
  while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i--;
  return i >= 0 && text[i] === '.' && text[i - 1] !== '.' ? i : -1;
}

/**
 * The expression that ends right before the dot at (`line`, `dotCol`), the longest one:
 * `a.b.` gives the entry of `a.b`, not the one of `b`.
 */
function receiverAt(info, file, line, dotCol) {
  let best = null;
  for (const e of info.exprs || []) {
    const l = e.loc;
    if (l.file !== file || l.end_line !== line + 1 || l.end_col !== dotCol + 1) continue;
    if (best === null || l.line < best.loc.line || (l.line === best.loc.line && l.col < best.loc.col)) best = e;
  }
  return best;
}

/** The members offered after an expression entry (`value.` or `Type.`). */
function membersOf(info, expr) {
  if (!expr) return [];
  return (expr.statics ? info.statics[expr.statics] : info.members[expr.type]) || [];
}

const KIND_LABEL = { field: 'field', method: 'method', variant: 'variant', constant: 'constant', function: 'function' };

/**
 * What to show when hovering the position.
 */
function hoverAt(info, file, line, col, lineText) {
  const w = wordAt(lineText, col);
  if (!w) return null;
  const line1 = line + 1;
  const range = { line, start: w.start, end: w.end };
  const result = (code, doc) => ({ range, markdown: CODE_FENCE(code) + (doc ? '\n\n' + doc : '') });

  // a declaration
  for (const l of info.locals || []) {
    if (l.loc.file === file && sameRange(l.loc, line1, w.start, w.end)) return result(`${l.kind} ${l.name}: ${l.type}`);
  }
  for (const it of info.items || []) {
    if (it.loc.file === file && sameRange(it.loc, line1, w.start, w.end)) return result(it.label, it.doc);
  }

  // `value.member` and `Type.member`
  const dot = dotBefore(lineText, w.start);
  if (dot !== -1) {
    const member = membersOf(info, receiverAt(info, file, line, dot)).find((m) => m.name === w.word);
    if (!member) return null;
    return result(`(${KIND_LABEL[member.kind] || member.kind}) ${member.detail}`, member.doc);
  }

  // a name used in an expression
  const definition = (kinds) =>
    (info.items || []).find((it) => it.name === w.word && kinds.includes(it.kind) && it.loc.file === file) ||
    (info.items || []).find((it) => it.name === w.word && kinds.includes(it.kind));
  const entry = (info.exprs || []).find((e) => e.loc.file === file && sameRange(e.loc, line1, w.start, w.end));
  if (entry) return result(entry.label || `${w.word}: ${entry.type}`, entry.doc);

  // a call to a function or a built-in: the callee is not an expression of its own
  const calls = lineText.slice(w.end).trimStart().startsWith('(');
  const site = definition(['function', 'struct', 'enum', 'trait']);
  if (site) return result(site.label, site.doc);
  const builtin = (info.functions || []).find((f) => f.name === w.word);
  if (builtin && calls) return result(builtin.detail, builtin.doc);
  const local = (info.locals || []).find((l) => l.name === w.word && l.loc.file === file && within(l.scope, line1, w.start + 1));
  if (local) return result(`${local.kind} ${local.name}: ${local.type}`);
  if (Object.prototype.hasOwnProperty.call(KEYWORDS, w.word)) return { range, markdown: `**${w.word}**: ${KEYWORDS[w.word]}` };
  return null;
}

/** The completion items after a `.`: members of the receiver. */
function memberCompletions(info, expr, receiverText) {
  const showPrivate = /(^|[^A-Za-z0-9_.])self$/.test(receiverText.trimEnd());
  return membersOf(info, expr)
    .filter((m) => m.public || showPrivate)
    .map((m) => ({ label: m.name, kind: m.kind, detail: m.detail, doc: m.doc || null, sort: (m.kind === 'field' ? '0' : '1') + m.name }));
}

/** The completion items without a dot: variables in scope, names of the file, built-ins. */
function nameCompletions(info, file, line, col) {
  const out = new Map();
  const add = (item) => {
    if (!out.has(item.label)) out.set(item.label, item);
  };
  const line1 = line + 1;
  const col1 = col + 1;
  for (const l of info.locals || []) {
    if (l.loc.file !== file || !before(l.loc, line1, col1) || !within(l.scope, line1, col1)) continue;
    add({ label: l.name, kind: l.kind === 'param' ? 'param' : 'variable', detail: `${l.kind} ${l.name}: ${l.type}`, sort: '0' + l.name });
  }
  for (const g of (info.globals || {})[file] || []) {
    add({ label: g.name, kind: g.kind, detail: g.detail, doc: g.doc || null, sort: '1' + g.name });
  }
  for (const f of info.functions || []) {
    add({ label: f.name, kind: 'function', detail: f.detail, doc: f.doc, sort: '2' + f.name });
  }
  for (const k of Object.keys(KEYWORDS)) {
    add({ label: k, kind: 'keyword', detail: 'keyword', doc: KEYWORDS[k], sort: '3' + k });
  }
  for (const t of ['int', 'float', 'bool', 'string', 'uuid', 'ulid', 'datetime', 'array', 'map']) {
    add({ label: t, kind: 'type', detail: 'type', sort: '4' + t });
  }
  // `time.now()` and `random.int(1, 6)`: namespaces of the language
  add({ label: 'time', kind: 'module', detail: 'namespace', doc: 'The clock: `time.now()`.', sort: '4time' });
  add({ label: 'random', kind: 'module', detail: 'namespace', doc: 'Random numbers: `random.int(1, 6)`, `random.float()`.', sort: '4random' });
  return [...out.values()];
}

/** The field already given by the segment `text[segStart, commaPos)`, if any (`name: value`
 * or the shorthand `name`). */
function givenFieldOf(text, segStart, commaPos) {
  const m = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(:|$)/.exec(text.slice(segStart, commaPos));
  return m ? m[1] : null;
}

/**
 * The struct literal (`Point { x: 1, | }`) enclosing `offset` in the full document `text`,
 * if the cursor sits where a field name goes: the type name written before the `{`, the
 * fields already given (so they are not suggested again) and the partial name being typed.
 * Returns null when the cursor is typing a value instead (after a `:`), or is not inside a
 * struct literal at all.
 */
function structLitAt(text, offset) {
  const stack = [];
  let inString = false;
  for (let i = 0; i < offset; i++) {
    const c = text[i];
    if (inString) {
      if (c === '\\') i++;
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') {
      inString = true;
    } else if (c === '{') {
      let j = i - 1;
      while (j >= 0 && /\s/.test(text[j])) j--;
      let name = null;
      if (j >= 0 && isWordChar(text[j])) {
        let k = j;
        while (k >= 0 && isWordChar(text[k])) k--;
        const word = text.slice(k + 1, j + 1);
        if (!/^[0-9]/.test(word) && !RESERVED.has(word)) name = word;
      }
      stack.push({ kind: '{', name, given: new Set(), segStart: i + 1 });
    } else if (c === '(' || c === '[') {
      stack.push({ kind: c });
    } else if (c === '}' || c === ')' || c === ']') {
      stack.pop();
    } else if (c === ',' && stack.length && stack[stack.length - 1].kind === '{') {
      const top = stack[stack.length - 1];
      const given = givenFieldOf(text, top.segStart, i);
      if (given) top.given.add(given);
      top.segStart = i + 1;
    }
  }
  const top = stack[stack.length - 1];
  if (!top || top.kind !== '{' || !top.name) return null;
  const seg = text.slice(top.segStart, offset);
  const m = /^\s*([A-Za-z_][A-Za-z0-9_]*)?\s*$/.exec(seg);
  if (!m) return null; // a value, or something else, is being typed: not a field name
  return { name: top.name, given: top.given, prefix: m[1] || '' };
}

/** The completion items for the fields of a struct literal, excluding those already given. */
function structFieldCompletions(info, lit) {
  return (info.members[lit.name] || [])
    .filter((m) => m.kind === 'field' && !lit.given.has(m.name))
    .map((m) => ({ label: m.name, kind: m.kind, detail: m.detail, doc: m.doc || null, sort: '0' + m.name }));
}

/**
 * Replace `[from, to)` of line `line` by spaces, keeping every position: used to ask the
 * compiler about `value.partial|` or `f(arg|` as if the unfinished part was not there.
 */
function blank(text, line, from, to) {
  const lines = text.split('\n');
  const l = lines[line];
  if (l === undefined) return text;
  lines[line] = l.slice(0, from) + ' '.repeat(Math.max(0, to - from)) + l.slice(to);
  return lines.join('\n');
}

/**
 * The dot that starts the member being typed at (`line`, `col`): `xs.pu|` gives the column
 * of the dot. Returns null when the cursor is not right after `.name`.
 */
function memberAccessAt(lineText, col) {
  let s = col;
  while (s > 0 && isWordChar(lineText[s - 1])) s--;
  let d = s - 1;
  while (d >= 0 && (lineText[d] === ' ' || lineText[d] === '\t')) d--;
  if (d >= 0 && lineText[d] === '.' && lineText[d - 1] !== '.') return { dot: d, start: s };
  return null;
}

/** The unfinished call around the cursor: {open, name, activeParam}, or null. */
function callAt(lineText, col) {
  let inString = false;
  const stack = [];
  for (let i = 0; i < col; i++) {
    const c = lineText[i];
    if (inString) {
      if (c === '\\') i++;
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') inString = true;
    else if (c === '(') stack.push({ at: i, commas: 0 });
    else if (c === '[' || c === '{') stack.push({ at: -1, commas: 0 });
    else if (c === ')' || c === ']' || c === '}') stack.pop();
    else if (c === ',' && stack.length) stack[stack.length - 1].commas++;
  }
  const top = stack[stack.length - 1];
  if (!top || top.at === -1) return null;
  const name = lineText.slice(0, top.at).match(/([A-Za-z_][A-Za-z0-9_]*)\s*$/);
  if (!name) return null;
  return { open: top.at, name: name[1], activeParam: top.commas };
}

/** `name(a: int, b: int) -> int` into its parameter labels. */
function parameters(detail) {
  const open = detail.indexOf('(');
  if (open === -1) return [];
  let depth = 0;
  let start = open + 1;
  const out = [];
  for (let i = open; i < detail.length; i++) {
    const c = detail[i];
    if (c === '(' || c === '<') depth++;
    else if (c === ')' || c === '>') {
      if (c === ')' && depth === 1) {
        const last = detail.slice(start, i).trim();
        if (last) out.push(last);
        return out;
      }
      if (c === '>' && detail[i - 1] === '-') continue; // `->`
      depth--;
    } else if (c === ',' && depth === 1) {
      out.push(detail.slice(start, i).trim());
      start = i + 1;
    }
  }
  return out;
}

/** The signature of what `call` (from `callAt`) calls, if known. */
function signatureFor(info, file, line, lineText, call) {
  const nameEnd = lineText.slice(0, call.open).trimEnd().length;
  const nameStart = nameEnd - call.name.length;
  let label = null;
  let doc = null;
  const dot = dotBefore(lineText, nameStart);
  if (dot !== -1) {
    const m = membersOf(info, receiverAt(info, file, line, dot)).find((x) => x.name === call.name);
    if (m) {
      label = m.detail;
      doc = m.doc || null;
    }
  } else {
    const site = (info.items || []).find((it) => it.name === call.name && it.kind === 'function');
    const builtin = (info.functions || []).find((f) => f.name === call.name);
    if (site) {
      label = site.label.replace(/^fun /, '');
      doc = site.doc || null;
    } else if (builtin) {
      label = builtin.detail;
      doc = builtin.doc;
    }
  }
  if (!label) return null;
  return { label, parameters: parameters(label), doc, activeParam: call.activeParam };
}

module.exports = {
  KEYWORDS,
  wordAt,
  dotBefore,
  receiverAt,
  membersOf,
  hoverAt,
  memberCompletions,
  nameCompletions,
  structLitAt,
  structFieldCompletions,
  blank,
  memberAccessAt,
  callAt,
  parameters,
  signatureFor,
};
