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

## Débogueur (DAP)

`qlang dap` est un adaptateur de débogage ([Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)) sur
l'entrée et la sortie standard : points d'arrêt, pas à pas (entrer, sauter, sortir), pause, pile d'appels, variables
(tableaux, dictionnaires, structures, énumérations dépliables) et `evaluate` pour un nom de variable (`a`, `a.champ`,
`a.0`). La configuration de lancement prend `program` (obligatoire), `args`, `cwd`, `stopOnEntry` et `allowProcess`.
VS Code : l'extension du dépôt le branche (touche F5 sur un fichier `.q`).

Autres éditeurs (même réserve que plus haut : réglages d'après leur documentation) :

```lua
-- Neovim, avec nvim-dap
local dap = require("dap")
dap.adapters.qlang = { type = "executable", command = "qlang", args = { "dap" } }
dap.configurations.qlang = {
  { type = "qlang", request = "launch", name = "Debug the file", program = "${file}" },
}
```

```toml
# Helix, dans languages.toml (dans le bloc [[language]] de qlang)
[language.debugger]
name = "qlang"
transport = "stdio"
command = "qlang"
args = ["dap"]

[[language.debugger.templates]]
name = "file"
request = "launch"
completion = [ { name = "program", completion = "filename" } ]
args = { program = "{0}" }
```

Emacs : `dape` ou `dap-mode` avec la commande `qlang dap` et `:program` ; JetBrains : LSP4IJ gère aussi un adaptateur DAP
(*Debug Adapter Protocol* dans les réglages).

À savoir : le programme tourne dans l'interpréteur « simple » (comme `--coverage`), donc plus lentement ; son
`print` va dans la console de l'éditeur ; la lecture au clavier (`read()`) n'est pas disponible (l'entrée est celle du
protocole) ; un point d'arrêt sur une ligne sans instruction (blanche, commentaire, `end`) ne s'arrête jamais.

## Mise en forme et analyse depuis n'importe quel éditeur

- `qlang fmt --stdin` : le programme sur l'entrée, mis en forme sur la sortie (code 65 si le fichier ne s'analyse pas).
- `qlang lint fichier.q` : les avertissements `fichier:ligne:colonne: warning[L00x]: …`, lisibles par la plupart des
  éditeurs comme une sortie de compilateur.
