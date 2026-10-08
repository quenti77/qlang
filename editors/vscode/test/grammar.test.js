'use strict';
// Tokenizes real qlang code with the TextMate grammar (as VS Code does) and checks the scopes.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vsctm = require('vscode-textmate');
const oniguruma = require('vscode-oniguruma');

const grammarPath = path.join(__dirname, '..', 'syntaxes', 'qlang.tmLanguage.json');

async function loadGrammar() {
  const wasm = fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')).buffer;
  await oniguruma.loadWASM(wasm);
  const registry = new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (p) => new oniguruma.OnigScanner(p),
      createOnigString: (s) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scope) =>
      scope === 'source.qlang' ? vsctm.parseRawGrammar(fs.readFileSync(grammarPath, 'utf8'), grammarPath) : null,
  });
  return registry.loadGrammar('source.qlang');
}

/** All tokens of a text: `{ line, text, scopes }`. */
function tokenize(grammar, text) {
  const tokens = [];
  let state = vsctm.INITIAL;
  text.split('\n').forEach((line, i) => {
    const r = grammar.tokenizeLine(line, state);
    for (const t of r.tokens) tokens.push({ line: i, text: line.slice(t.startIndex, t.endIndex), scopes: t.scopes });
    state = r.ruleStack;
  });
  return tokens;
}

/** The scopes of the first token whose text is exactly `word` (on the given line, if any). */
function scopesOf(tokens, word, line) {
  const t = tokens.find((x) => x.text === word && (line === undefined || x.line === line));
  assert.ok(t, `no token "${word}"`);
  return t.scopes.join(' ');
}

test('grammar', async (t) => {
  const g = await loadGrammar();

  await t.test('comments', () => {
    const tk = tokenize(g, 'let x = 1 -- end of line\n--(\nblock if\n--)\n--"\ndoc\n--"\nlet y = 2');
    assert.match(scopesOf(tk, '-- end of line'), /comment\.line\.double-dash/);
    assert.match(scopesOf(tk, 'block if', 2), /comment\.block\.qlang/);
    assert.match(scopesOf(tk, 'doc', 5), /comment\.block\.documentation/);
    assert.match(scopesOf(tk, 'let', 7), /storage\.type/); // code after the comments is code again
    assert.doesNotMatch(scopesOf(tk, 'block if', 2), /keyword/); // keywords inside a comment stay comment
  });

  await t.test('numbers', () => {
    const tk = tokenize(g, 'let a = [42, 1_000, 0xFF_FF, 0b0101_0101, 3.14, 2.5e3, 1e-2]\nfor i in 0..10 do end');
    assert.match(scopesOf(tk, '42'), /numeric\.integer/);
    assert.match(scopesOf(tk, '1_000'), /numeric\.integer/);
    assert.match(scopesOf(tk, '0xFF_FF'), /numeric\.hex/);
    assert.match(scopesOf(tk, '0b0101_0101'), /numeric\.binary/);
    assert.match(scopesOf(tk, '3.14'), /numeric\.float/);
    assert.match(scopesOf(tk, '2.5e3'), /numeric\.float/);
    assert.match(scopesOf(tk, '1e-2'), /numeric\.float/);
    // `0..10` is a range of integers, not a float
    assert.match(scopesOf(tk, '0', 1), /numeric\.integer/);
    assert.match(scopesOf(tk, '10', 1), /numeric\.integer/);
    assert.match(scopesOf(tk, '..'), /operator\.range/);
  });

  await t.test('strings, escapes and interpolation', () => {
    const tk = tokenize(g, 'print("hi {name.upper()} {a + 1}\\n\\{x\\} \\q")');
    assert.match(scopesOf(tk, '"'), /string\.quoted/);
    assert.match(scopesOf(tk, 'hi '), /string\.quoted/);
    assert.match(scopesOf(tk, '\\n'), /constant\.character\.escape/);
    assert.match(scopesOf(tk, '\\{'), /constant\.character\.escape/);
    assert.match(scopesOf(tk, '\\q'), /invalid\.illegal/);
    assert.match(scopesOf(tk, 'name'), /meta\.interpolation/);
    assert.match(scopesOf(tk, 'upper'), /entity\.name\.function\.member/);
    assert.match(scopesOf(tk, '1'), /meta\.interpolation.*numeric/);
    assert.doesNotMatch(scopesOf(tk, 'hi '), /meta\.interpolation/);
  });

  await t.test('declarations', () => {
    const tk = tokenize(
      g,
      'struct Point extends Base\n  public x: int\nend\nfun add<T: Add>(a: T) -> T\nenum Color\nlet total = 0\nconst PI = 3\ntrait Shape\nimpl Add for Point'
    );
    assert.match(scopesOf(tk, 'struct'), /storage\.type\.class/);
    assert.match(scopesOf(tk, 'Point', 0), /entity\.name\.type/);
    assert.match(scopesOf(tk, 'Base'), /inherited-class/);
    assert.match(scopesOf(tk, 'public'), /storage\.modifier/);
    assert.match(scopesOf(tk, 'int'), /support\.type\.builtin/);
    assert.match(scopesOf(tk, 'fun'), /storage\.type\.function/);
    assert.match(scopesOf(tk, 'add'), /entity\.name\.function/);
    assert.match(scopesOf(tk, 'Color'), /entity\.name\.type/);
    assert.match(scopesOf(tk, 'total'), /variable\.other\.definition/);
    assert.match(scopesOf(tk, 'PI', 6), /variable\.other\.definition/);
    assert.match(scopesOf(tk, 'Shape'), /entity\.name\.type/);
    assert.match(scopesOf(tk, 'impl'), /storage\.type/);
  });

  await t.test('keywords, constants and operators', () => {
    const tk = tokenize(
      g,
      'if a and not b or c then return none end\nwhile x != 1 do break end\nfor k, v in m step 2 do continue end\nlet r = 7 div 2 + 7 mod 2\nn div= 2\nx = true\nself.v += super.f(1)\nimport a as b from "m.q"\nlet f = 1 ** 2 -> y?'
    );
    assert.match(scopesOf(tk, 'if'), /keyword\.control\.flow/);
    assert.match(scopesOf(tk, 'and'), /keyword\.operator\.word/);
    assert.match(scopesOf(tk, 'not'), /keyword\.operator\.word/);
    assert.match(scopesOf(tk, 'none'), /constant\.language\.null/);
    assert.match(scopesOf(tk, 'return'), /keyword\.control/);
    assert.match(scopesOf(tk, '!='), /operator\.comparison/);
    assert.match(scopesOf(tk, 'step'), /keyword\.control/);
    assert.match(scopesOf(tk, 'continue'), /keyword\.control/);
    assert.match(scopesOf(tk, 'div', 3), /keyword\.operator\.word/);
    assert.match(scopesOf(tk, 'mod', 3), /keyword\.operator\.word/);
    assert.match(scopesOf(tk, 'div=', 4), /keyword\.operator\.assignment/);
    assert.match(scopesOf(tk, 'true'), /constant\.language\.boolean/);
    assert.match(scopesOf(tk, 'self'), /variable\.language/);
    assert.match(scopesOf(tk, 'super'), /variable\.language/);
    assert.match(scopesOf(tk, '+='), /operator\.assignment/);
    assert.match(scopesOf(tk, 'import'), /keyword\.control\.import/);
    assert.match(scopesOf(tk, 'from'), /keyword\.control\.import/);
    assert.match(scopesOf(tk, '**'), /operator\.arithmetic/);
    assert.match(scopesOf(tk, '->'), /operator\.arrow/);
    assert.match(scopesOf(tk, '?'), /operator\.nullable/);
  });

  await t.test('calls and members', () => {
    const tk = tokenize(g, 'print(add(1, 2))\nlet p = Point.new(1)\nxs.push(3)\nlet n = p.x\nint.parse("4")\nColor.Red');
    assert.match(scopesOf(tk, 'print'), /support\.function\.builtin/);
    assert.match(scopesOf(tk, 'add'), /entity\.name\.function\.call/);
    assert.match(scopesOf(tk, 'new'), /entity\.name\.function\.member/);
    assert.match(scopesOf(tk, 'Point', 1), /entity\.name\.type/);
    assert.match(scopesOf(tk, 'push'), /entity\.name\.function\.member/);
    assert.match(scopesOf(tk, 'x', 3), /variable\.other\.property/);
    assert.match(scopesOf(tk, 'int', 4), /support\.type\.builtin/);
    assert.match(scopesOf(tk, 'Red'), /entity\.name\.type/);
  });

  await t.test('every example tokenizes without illegal tokens', () => {
    const dir = path.join(__dirname, '..', '..', '..', 'examples');
    const files = fs.readdirSync(dir).filter((f) => f.endsWith('.q'));
    assert.ok(files.length >= 4, 'examples are missing');
    for (const f of files) {
      const tokens = tokenize(g, fs.readFileSync(path.join(dir, f), 'utf8'));
      const bad = tokens.filter((x) => x.scopes.some((s) => s.startsWith('invalid')));
      assert.deepStrictEqual(bad, [], `${f}: illegal tokens`);
      // the grammar must not get stuck in an unterminated construct at the end of a file
      const last = tokens[tokens.length - 1];
      assert.ok(last, `${f}: no tokens`);
      assert.doesNotMatch(last.scopes.join(' '), /string\.quoted|comment\.block/, `${f}: ends inside a string or comment`);
    }
  });
});
