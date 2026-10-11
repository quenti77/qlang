# qlang for VS Code

Support for the [qlang](../../README.md) language in VS Code (files ending in `.q`).

- **Syntax highlighting**: keywords, types, numbers (`0xFF_FF`, `0b0101`, `1_000`), strings with
  `{interpolation}`, the three comment forms (`--`, `--( … --)`, `--" … --"`).
- **Errors as you type**: the file is checked by the real qlang compiler (`qlang check --json`),
  so you see the same errors as in the terminal, underlined in the editor and listed in the
  Problems panel. Errors inside imported modules are shown in those files.
- **Tooltips**: hover a name to see its type (`names: array<string>`), a function or method
  signature, and the documentation written above it in a `--" … --"` comment. Built-in methods
  (`len`, `push`, `split`…) are explained too.
- **Completion**: after a `.`, the fields and methods of the value's type (only the public ones, all
  of them on `self`), the variants and static functions after `Type.`, and the contents of a module
  after `module.`. Elsewhere: variables in scope, the names of the file (including imports), built-in
  functions, keywords and types.
- **Parameter hints**: typing `(` or `,` in a call shows the signature with the current parameter.
- **Run**: the ▶ button in the editor title bar (or the command **qlang: Run File**) saves the file
  and runs it in a terminal, where `read()` works with your keyboard.
- **Snippets**: `fun`, `if`, `ifelse`, `for`, `foreach`, `fori`, `formap`, `while`, `match`, `struct`,
  `impl`, `trait`, `enum`, `import`, `readint`… (type the prefix and press Tab).
- Comment toggling, bracket matching, auto-closing quotes and brackets, and automatic indentation
  around `then … end`, `do … end` and `fun … end`.

## Requirements

The extension calls the `qlang` program. Install it (and update it) with the installer of the
repository, which also installs this extension:

```sh
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --vscode
```

On Windows there is no prebuilt program yet: install Rust and run
`cargo install --git https://github.com/quenti77/qlang qlang-cli` (the extension itself, `qlang.vsix`,
is on the Releases page: `code --install-extension qlang.vsix`). From a checkout:
`cargo install --path crates/qlang-cli`.

If `qlang` is not in your `PATH`, set **qlang.path** in the settings to its full path.

`qlang.path` is only read from your **user** settings (machine scope), never from a project's
`.vscode/settings.json`, and it is listed as a restricted setting for untrusted workspaces: a
downloaded project cannot choose which program the extension runs.

Tooltips and completion come from the compiler itself (`qlang check --json` with `"info": true`),
so they follow the real types, including inferred ones. They need a file the compiler can read: with a
syntax error elsewhere in the file the previous answer is used, and completion after a `.` still
works because the unfinished `.name` is left out when asking.

## Settings

| Setting                     | Default | Meaning                                              |
| --------------------------- | ------- | ---------------------------------------------------- |
| `qlang.path`                | `qlang` | The executable to run                                |
| `qlang.diagnostics.enabled` | `true`  | Check files while you type                           |
| `qlang.diagnostics.delay`   | `400`   | Milliseconds to wait after you stop typing           |

## Install the extension

```sh
cd editors/vscode
npm install
npm run package                            # creates qlang-0.1.0.vsix
code --install-extension qlang-0.1.0.vsix  # add --profile <name> to choose a profile
```

To try it without installing: open `editors/vscode` in VS Code and press F5.

## Development

```sh
npm test      # grammar, helpers, extension logic (against a fake VS Code) and the real compiler
```

Layout: `syntaxes/` the TextMate grammar, `language-configuration.json` comments, brackets and
indentation, `snippets/`, `lib.js` pure helpers (imports, request, diagnostics), `extension.js`
the VS Code part. Import paths are resolved like the compiler does: relative to the importing file, and they may
go up (`../lib/x.q`), so the editor and the terminal agree.
