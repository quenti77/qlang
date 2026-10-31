# Autres éditeurs que VS Code

`qlang lsp` parle le [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) sur l'entrée et la
sortie standard : erreurs pendant la frappe, infobulles, complétion, aller à la définition, références, renommage,
plan du fichier, mise en forme (`textDocument/formatting`). Il suffit de dire à l'éditeur de le lancer pour les
fichiers `*.q`. Ces réglages suivent la documentation de chaque éditeur ; seul VS Code (l'extension du dépôt)
est testé à chaque version.

La coloration vient d'une grammaire TextMate, [`editors/vscode/syntaxes/qlang.tmLanguage.json`](../editors/vscode/syntaxes/qlang.tmLanguage.json),
que Sublime Text, JetBrains et d'autres savent lire.

## Neovim (0.11 ou plus)

```lua
vim.filetype.add({ extension = { q = "qlang" } })
vim.lsp.config("qlang", {
  cmd = { "qlang", "lsp" },
  filetypes = { "qlang" },
  root_markers = { "qlang.toml", ".git" },
})
vim.lsp.enable("qlang")
```

Avant la 0.11, avec `nvim-lspconfig` : `vim.lsp.start({ name = "qlang", cmd = { "qlang", "lsp" }, root_dir = vim.fn.getcwd() })`
dans un `FileType qlang`. Mise en forme : `vim.lsp.buf.format()`, ou `qlang fmt --stdin` comme formateur
(`conform.nvim` : `formatters_by_ft.qlang = { "qlang_fmt" }` avec `command = "qlang"`, `args = { "fmt", "--stdin" }`).

## Helix

Dans `~/.config/helix/languages.toml` :

```toml
[language-server.qlang]
command = "qlang"
args = ["lsp"]

[[language]]
name = "qlang"
scope = "source.qlang"
file-types = ["q"]
roots = ["qlang.toml"]
comment-token = "--"
indent = { tab-width = 2, unit = "  " }
language-servers = ["qlang"]
formatter = { command = "qlang", args = ["fmt", "--stdin"] }
auto-format = true
```

## Emacs (eglot)

```elisp
(add-to-list 'auto-mode-alist '("\\.q\\'" . prog-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs '((prog-mode :language-id "qlang") . ("qlang" "lsp"))))
```

(à restreindre à un mode majeur `qlang-mode` si vous en définissez un, pour ne pas lancer le serveur sur les autres
fichiers de type `prog-mode`).

## Sublime Text (paquet LSP)

`Preferences > Package Settings > LSP > Settings` :

```json
{
  "clients": {
    "qlang": {
      "enabled": true,
      "command": ["qlang", "lsp"],
      "selector": "source.qlang"
    }
  }
}
```

Avec la grammaire TextMate du dépôt pour que `source.qlang` existe.

## JetBrains (IntelliJ IDEA, PyCharm, GoLand…)

Les IDE JetBrains lisent un serveur LSP par le plugin **LSP4IJ** : `Language Servers` > `+` > *New Language Server*,
commande `qlang lsp`, et dans *Mappings* l'extension de fichier `*.q`. La grammaire TextMate se charge par
`Settings > Editor > TextMate Bundles`. Il n'y a pas encore de plugin JetBrains dédié.

## Zed

Zed déclare un langage par une extension (grammaire + serveur). Il n'y en a pas encore pour qlang : tant que ce n'est
pas fait, utilisez `qlang fmt --stdin` comme formateur externe et `qlang check` dans le terminal.

## Mise en forme et analyse depuis n'importe quel éditeur

- `qlang fmt --stdin` : le programme sur l'entrée, mis en forme sur la sortie (code 65 si le fichier ne s'analyse pas).
- `qlang lint fichier.q` : les avertissements `fichier:ligne:colonne: warning[L00x]: …`, lisibles par la plupart des
  éditeurs comme une sortie de compilateur.
