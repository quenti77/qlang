# qlang for VS Code

Support for the [qlang](../../README.md) language in VS Code (files ending in `.q`).

- **Syntax highlighting**: keywords, types, numbers (`0xFF_FF`, `0b0101`, `1_000`), strings with
  `{interpolation}`, the three comment forms (`--`, `--( … --)`, `--" … --"`).
- **Errors as you type**: the file is checked by the real qlang compiler (`qlang check --json`),
  so you see the same errors as in the terminal, underlined in the editor and listed in the
  Problems panel. Errors inside imported modules are shown in those files.
- **Run**: the ▶ button in the editor title bar (or the command **qlang: Run File**) saves the file
  and runs it in a terminal, where `read()` works with your keyboard.
- **Snippets**: `fun`, `if`, `ifelse`, `for`, `foreach`, `fori`, `formap`, `while`, `match`, `struct`,
  `impl`, `trait`, `enum`, `import`, `readint`… (type the prefix and press Tab).
- Comment toggling, bracket matching, auto-closing quotes and brackets, and automatic indentation
  around `then … end`, `do … end` and `fun … end`.

## Requirements

The extension calls the `qlang` program. Build and install it from the repository:

```sh
cargo install --path crates/qlang-cli     # installs `qlang` in ~/.cargo/bin
```

If `qlang` is not in your `PATH`, set **qlang.path** in the settings to its full path.

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
the VS Code part. Import paths are resolved like the compiler does: relative to the importing
file, inside the workspace folder.
