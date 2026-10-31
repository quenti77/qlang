# qlang

Un langage simple, lisible et typé, pensé pour apprendre à programmer. Sa syntaxe
est en anglais et ressemble aux vrais langages (Rust, TypeScript, Lua).

```
struct Point
  public x: int
  public y: int
end

impl Add for Point
  public fun add(self, o: Point) -> Point
    Point { x: self.x + o.x, y: self.y + o.y }
  end
end

let p = Point { x: 1, y: 2 } + Point { x: 10, y: 20 }
print("{p.x}, {p.y}")      -- 11, 22
print(7 / 2)               -- 3.5
print(7 div 2)             -- 3
```

- **Le langage** : [`docs/language.md`](docs/language.md) (référence complète).
- **Un tour de toutes les fonctionnalités** : [`examples/tour.q`](examples/tour.q).
- **La grammaire** : [`qlang.g4`](qlang.g4), à titre indicatif.

## Installation

Pas besoin de Rust : on télécharge un programme déjà compilé (Linux, macOS et Windows).

```sh
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh
```

Le programme `qlang` est installé dans `~/.local/bin`, et le script dit s'il faut l'ajouter au
`PATH`. Chaque téléchargement est vérifié avec la somme de contrôle (`SHA256SUMS`) publiée avec
la version. Les fichiers publiés ont aussi une attestation de provenance signée (GitHub) qui
prouve qu'ils sortent du workflow de ce dépôt : `gh attestation verify qlang-x86_64-unknown-linux-musl.tar.gz --repo quenti77/qlang`
(les versions publiées avant 0.36 n'en ont pas).

**Windows** (PowerShell) :

```powershell
irm https://raw.githubusercontent.com/quenti77/qlang/main/install.ps1 | iex
```

Le programme va dans `%LOCALAPPDATA%\qlang\bin` (ajouté au `PATH` de l'utilisateur), après vérification
de la somme de contrôle. Le script n'a été exécuté que dans la CI, pas encore sur beaucoup de machines :
dis-le si quelque chose coince. L'extension VS Code (`qlang.vsix`, sur la page
[Releases](https://github.com/quenti77/qlang/releases)) s'installe avec
`code --install-extension qlang.vsix`.

**Docker** : une image légère (le seul programme `qlang`, utilisateur non root), pour Linux amd64 et arm64 :

```sh
docker run --rm -v "$PWD":/app -p 8080:8080 ghcr.io/quenti77/qlang:latest serve app.q --bind 0.0.0.0
```

**Gestionnaires de paquets** : chaque release publie aussi `qlang.rb` (Homebrew), `qlang.json` (Scoop)
et `PKGBUILD` (AUR), prêts à copier dans un tap, un bucket ou un dépôt AUR ; ces dépôts ne sont pas
encore créés, donc `brew install` et `scoop install` ne sont pas encore possibles.

**Mettre à jour** : `qlang self-update` (ou relancer la même commande d'installation). Il télécharge la
dernière version, vérifie la somme de contrôle et remplace le programme ; `qlang self-update --version 0.63.0`
installe une version précise (même plus ancienne), `qlang self-update --check` dit seulement s'il y en a une nouvelle.

**Version par projet** : `qlang = "0.64"` dans le `[package]` de `qlang.toml` demande au moins cette
version ; avec un qlang plus ancien, le projet refuse de démarrer et dit de lancer `qlang self-update`.
`qlang init` l'écrit pour toi.

**Options** :

```sh
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --version 0.1.0
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --vscode
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --vscode --profile Rust
```

`--vscode` installe aussi l'extension VS Code (il faut la commande `code`) ; `--dir` choisit le
dossier d'installation.

**Désinstaller** : supprimer le fichier `qlang` (et l'extension dans VS Code).

**Sans script** : les programmes, `SHA256SUMS` et l'extension (`qlang.vsix`) sont aussi sur la page
[Releases](https://github.com/quenti77/qlang/releases) du dépôt.

**Depuis les sources** (Rust 1.88 ou plus récent, `rustup update`) :

```sh
cargo install --git https://github.com/quenti77/qlang qlang-cli          # installer ou mettre à jour avec --force
```

## Utilisation


```sh
cargo build --release            # produit target/release/qlang (depuis les sources)

qlang run examples/tour.q        # exécute un programme
qlang check examples/tour.q      # vérifie sans exécuter
qlang run prog.q --max-steps 1000000
```

Un programme peut aussi répondre à des requêtes HTTP : `qlang serve app.q` appelle sa fonction
`handle(req: Request) -> Response` pour chaque requête (voir
[`docs/language.md`](docs/language.md) et [`examples/app.q`](examples/app.q)) ; `--static dossier`
sert des fichiers, `--workers N` répond en parallèle, et un `Router` associe des chemins
(`/items/:id`) à des fonctions.

Les erreurs s'affichent avec leur position et un extrait du code :

```
error[T002]: expected `string`, found `int`
  --> main.q:2:17
  |
2 | let b: string = a
  |                 ^
```

Code de sortie : `0` succès, `1` erreur de compilation, `2` erreur d'exécution.
Les `import` peuvent remonter dans les dossiers (`import f from "../lib/f.q"`, comme
`require` en Node) mais ne lisent que des fichiers `.q`, et jamais de chemin absolu.
`--root dossier` limite les imports à ce dossier :

```sh
qlang run app/main.q --root .      # les imports restent dans le dossier courant
```

### Démarrer un projet

```sh
qlang init mon-app      # qlang.toml, main.q et .gitignore (sans rien écraser)
cd mon-app
qlang run               # lance l'`entry` du qlang.toml
qlang check             # vérifie sans lancer
```

`entry` se trouve dans la table `[package]` du `qlang.toml` ; `qlang run` et `qlang check`
sans fichier le cherchent depuis le dossier courant, vers le haut. (`main`, dans la même
table, est autre chose : le fichier qu'`import "@mon-app"` charge quand le projet est
utilisé comme paquet.)

### Documentation

```sh
qlang doc                  # Markdown de tout le projet, sur la sortie standard
qlang doc lib.q -o API.md  # un fichier, écrit dans API.md
```

Elle est tirée des commentaires `--" … --"` placés juste au-dessus des déclarations : les
noms exportés d'un fichier (tous, s'il n'a pas de `export`) avec leur signature, et les
champs, méthodes et variantes `public` de leurs types. Les fichiers de test sont ignorés.

### Mise en forme

```sh
qlang fmt              # remet en forme tous les fichiers .q du projet
qlang fmt lib.q        # un fichier (ou un dossier)
qlang fmt --check      # liste ceux qui changeraient, sans les toucher (code de sortie 1 : pour la CI)
qlang fmt --stdin      # un programme sur l'entrée, mis en forme sur la sortie (éditeur, pre-commit)
```

Dossiers ou fichiers à laisser tels quels : `[fmt] exclude = ["vendor", "gen/api.q"]` dans `qlang.toml`
(chemins relatifs au projet). La mise en forme elle-même n'a pas de réglage : deux espaces par niveau.

Seuls les **espaces au début et à la fin des lignes** changent (deux espaces par niveau, les
`case` d'un `match` un niveau plus loin que lui) : les jetons, les commentaires et les espaces
à l'intérieur d'une ligne restent tels quels, et c'est vérifié à chaque passage (rien n'est écrit si le
résultat ne contenait plus exactement les mêmes jetons). Les chaînes et commentaires sur plusieurs
lignes ne sont pas touchés. L'extension VS Code propose « Mettre en forme le document » (le serveur
de langage répond à `textDocument/formatting`).

### Analyse statique : `qlang lint`

```sh
qlang lint                         # tous les fichiers .q du projet
qlang lint lib.q --max-complexity 10
```

Ce que le vérificateur de types ne dit pas (code de sortie 1 s'il y a quelque chose, pour la CI) :

| Code | Signalé |
| --- | --- |
| `L001` | une variable jamais utilisée (un nom qui commence par `_` est gardé exprès) |
| `L002` | une fonction privée que rien n'appelle (dans un fichier qui a un `export`) ; `test_…`, `run`, `handle`, `main`, `setup` et les fonctions annotées sont exemptés |
| `L003` | un nom importé (`import a, b from "x.q"`) qui n'est jamais écrit |
| `L004` | du code après `return`, `break` ou `continue` |
| `L005` | une fonction avec trop de chemins (1 + `if`, `elseif`, `while`, `for`, `case`, `and`, `or` ; 15 par défaut) |

`#[used]` devant une fonction dit au lint qu'elle sert par un chemin qu'il ne voit pas (retrouvée par son nom,
par réflexion) ; l'annotation n'a aucun autre effet.

### Session interactive : `qlang repl`

`qlang repl` lit une entrée à la fois, avec historique (`~/.qlang_history`) et complétion (mots-clés, noms
visibles, membres après `Type.`). Une entrée inachevée (un `end` manquant) continue sur la ligne suivante. Les
définitions (`fun`, `struct`, `import`, `let`…) restent d'une entrée à l'autre ; une expression est
exécutée une fois et sa valeur est affichée. Chaque entrée est compilée avec les définitions précédentes, donc
**l'initialisation d'un `let` est rejouée** à chaque entrée : pas d'effet de bord dedans. `--allow-process` et
`--max-steps N` comme pour `qlang run`.

### Paquets

```sh
qlang search [texte]     # les paquets de l'index (nom, description, dépôt git)
qlang add std            # ajoute un paquet de l'index : son dépôt, à sa dernière version taguée
qlang add x --git URL --tag v1.0.0   # ou un dépôt à toi
```

L'index est un petit fichier `packages.json` (`{"nom": {"git": URL, "description": TEXTE}}`) du dépôt
de qlang ; `QLANG_INDEX_URL` en désigne un autre (une URL ou un chemin), et sans réseau les paquets
connus du binaire sont utilisés. L'index ne fait que dire quel dépôt git porte un nom : le paquet est
ensuite récupéré et figé dans `qlang.lock` comme n'importe quel autre.

### Tests

```sh
qlang test                       # tous les tests du projet
qlang test math_test.q           # un fichier (ou un dossier)
qlang test --filter double       # les tests dont le nom contient « double »
qlang test --coverage            # + les lignes de code que les tests n'ont pas exécutées
qlang test --coverage-lcov c.info  # + un fichier LCOV pour un outil de couverture
qlang test --group db --skip-group slow   # avec qlang-test : certains groupes seulement
```

Les fichiers de test se nomment `*_test.q` ou `test_*.q` ; dans chacun, toute fonction
`fun test_...()` sans paramètre est un test. Chaque test repart d'un état propre (le code
du fichier est rejoué) et réussit s'il ne s'arrête pas sur une erreur :

```
import double from "math.q"

fun test_double()
  assert_eq(double(21), 42)       -- affiche les deux valeurs s'ils diffèrent
end

fun test_negatives()
  assert(double(-1) < 0, "doit rester négatif")
  assert_ne(double(1), 3)
end

fun test_load()
  let json = assert_ok(Json.parse("{\"n\": 1}"))   -- la valeur du Ok, ou l'échec en montrant l'erreur
  assert_close(0.1 + 0.2, 0.3)                      -- flottants : à un milliardième près
  assert_contains([1, 2, 3], 2)
end
```

Les autres assertions : `assert_err`, `assert_some`, `assert_none`, `assert_contains_text`,
`assert_lt` / `assert_le` / `assert_gt` / `assert_ge`, `assert_len`, `assert_empty`,
`assert_not_contains`, `assert_starts_with`, `assert_ends_with`,
`assert_within` (tolérance donnée). Un échec est montré à la ligne de votre test, pas dans
la bibliothèque.

**Couverture** : `--coverage` exécute les tests avec l'interpréteur « simple » (plus lent, sans la machine à
registres) et note chaque ligne qui porte une instruction et qu'un test a exécutée ; le rapport donne, par fichier
source (les fichiers de test sont exclus), la part exécutée et les lignes qui ne l'ont pas été.
C'est de la couverture de **lignes** (pas de branches). `--coverage-lcov FICHIER` écrit le format LCOV.
**Groupes** : `--group a,b` et `--skip-group c` posent `QLANG_TEST_GROUP` / `QLANG_TEST_SKIP_GROUP`, que le paquet
[`qlang-test`](https://github.com/quenti77/qlang-test) lit pour ne lancer que les cas des groupes demandés.

Code de sortie : `0` si tout passe (ou s'il n'y a aucun test), `1` sinon. `--seed N` fixe les
nombres aléatoires, `--max-steps N` limite chaque test.

### Lancer un programme

`Process.run("git", ["status"])` démarre un programme de la machine (sans shell). C'est refusé par
défaut : le programme doit être lancé avec `--allow-process`
(`qlang run --allow-process app.q`, aussi pour `qlang test` et `qlang serve`). Voir
« Prélude : lancer un programme » dans [docs/language.md](docs/language.md).

### Paquets

Un projet liste ses dépendances dans son `qlang.toml` et les importe avec `@nom` :

```toml
[dependencies]
text = { git = "https://github.com/x/text", tag = "v1.2.0" }
utils = { path = "../utils" }
```

```sh
qlang add mail --git https://github.com/x/mail --tag v1.0.0   # ajoute et installe
qlang install        # récupère les paquets git dans .qlang/packages (commits de qlang.lock)
qlang update         # avance les tags/branches, réécrit qlang.lock
qlang remove mail
```

```
import "@text" as t                  -- le fichier principal du paquet
import send from "@mail/send.q"      -- un de ses fichiers
```

Mettez `.qlang/` dans le `.gitignore` et versionnez `qlang.lock`. Voir « Paquets » dans
[docs/language.md](docs/language.md) pour le détail.

#### Bibliothèque `qlang-std`

[`qlang-std`](https://github.com/quenti77/qlang-std) est une bibliothèque en qlang pur : CSV,
gabarits de texte et TOML.

```sh
qlang add std --git https://github.com/quenti77/qlang-std --tag v0.1.0
```

```
import "@std/csv.q" as csv
```

### Fournir des `extern fun`

Une `extern fun` est implémentée par un **fournisseur** : n'importe quel programme qui
parle JSON, une ligne par message, sur son entrée et sa sortie standard. Il est lancé une
fois pour toute l'exécution. On le déclare dans un `qlang.toml` placé près du programme
(ou dans un dossier parent), ou avec `--extern` :

```toml
[externs]
mail = "python3 mail.py"     # lancé depuis le dossier du qlang.toml, sans shell
```

```sh
qlang run app.q --extern "python3 mail.py"
```

Le fournisseur répond à deux messages :

```
→ {"op":"list"}                                    ← {"externs":["mail_send"]}
→ {"op":"call","name":"mail_send","args":[…]}      ← {"ok":<valeur>}  ou  {"error":"message"}
```

Les valeurs sont du JSON : une structure ou une table est un objet (les clés `int` ou
`bool` s'écrivent en texte), une énumération le nom de sa variante, un `Json` sa valeur.
Un nom déjà fourni par `qlang` (`sql_*`) ou par un autre fournisseur est refusé. Lancer les
commandes d'un `qlang.toml` revient à exécuter ce fichier, donc `qlang` demande d'abord
(comme `direnv`) : dans un terminal il affiche les commandes et demande `[y/N]`, puis
retient la réponse pour ce fichier, ce nom et cette commande (`trusted-externs` dans
`~/.config/qlang`, ou `$QLANG_CONFIG_DIR`) ; une commande modifiée redemande. Sans terminal
(serveur, CI), rien ne se lance tant qu'on n'ajoute pas `--trust-externs`. Les commandes
données avec `--extern` sont les vôtres et ne demandent rien. Les modes `--json` (éditeurs, protocole) ne lisent pas de `qlang.toml` et ne
lancent aucun fournisseur : une vérification seule (`check --json`, ou `"mode": "check"`)
suppose donc toutes les `extern fun` fournies, comme elle suppose un clavier et une
horloge, alors qu'un `run --json` les refuse (`T263`) ; `qlang serve app.q` en lance un par worker.

## Architecture

Le langage est séparé de toute interface : le cœur ne lit, n'écrit et ne touche
jamais au disque lui-même.

```
crates/
├── qlang-core/   le langage : lexer, parseur, vérificateur de types, interpréteur
│                 (aucune E/S, une seule dépendance : serde pour les diagnostics)
└── qlang-cli/    le programme `qlang` : terminal, protocole JSON, serveur HTTP
```

Tout ce qui sort du cœur passe par un `Host` que l'appelant fournit :

```rust
pub trait Host {
    fn print(&mut self, text: &str);                        // sortie
    fn read_line(&mut self) -> Result<Option<String>, HostError>;   // entrée (facultative)
    fn load_module(&mut self, path: &str) -> Result<String, String>; // import
    fn now_ms(&mut self) -> Result<i64, HostError>;         // horloge (facultative)
    fn random_u64(&mut self) -> Result<u64, HostError>;     // hasard (facultatif)
    fn capabilities(&self) -> Capabilities;                 // ce que l'hôte sait faire
}
```

- Le terminal branche `stdout`, `stdin` et le disque ; un serveur web branche un
  tampon, des entrées pré-fournies et des fichiers en mémoire ; les tests
  utilisent `MemoryHost`.
- `read()`, `time.now()` et le hasard (`random`, `uuid.v4()`, `ulid.new()`…) sont
  **facultatifs** : si l'hôte ne les propose pas (`capabilities().read`, `.time`,
  `.random` faux), un programme qui les appelle est refusé **avant** d'être
  exécuté. Ainsi le cœur reste déterministe : `MemoryHost` fournit une horloge
  et une graine fixes pour les tests.
- Les erreurs sont des **données** (`Diagnostic` : code, message, position,
  notes), jamais du texte déjà affiché. Elles se sérialisent en JSON.
- L'exécution est bornée par des `Limits` (étapes, profondeur d'appels, taille
  de la sortie, taille des tableaux et chaînes).

Utiliser le cœur depuis Rust :

```rust
use qlang_core::{check::compile, host::{Limits, MemoryHost}};

let mut host = MemoryHost::new();
let program = compile("main.q", "print(1 + 2)", &mut host).map_err(|e| e.render())?;
let result = program.run(&mut host, Limits::default());
assert_eq!(host.output, "3\n");
assert!(result.error.is_none());
```

`compile` analyse sans exécuter (utile pour souligner les erreurs dans un
éditeur) ; `run` peut être appelé plusieurs fois sur un même programme.

L'interpréteur est récursif : exécutez-le dans un thread à grande pile (le CLI
utilise 1 Go de pile virtuelle ; 256 Mo suffisent pour les limites par défaut en
mode release).

## Protocole JSON

Pour PHP, Python ou n'importe quel autre langage : une requête JSON en entrée,
une réponse JSON en sortie. Les fichiers sont fournis **dans la requête** : rien
n'est lu sur le disque.

```sh
qlang run --json < requete.json          # une requête, une réponse
qlang serve --port 8080                  # le même protocole par HTTP : POST /run
```

Requête :

```json
{
  "entry": "main.q",
  "files": {
    "main.q": "import add from \"math.q\"\nprint(add(1, 2))\nlet n = read()\nprint(n)",
    "math.q": "fun add(a: int, b: int) -> int\n  a + b\nend\nexport add"
  },
  "input": ["hello"],
  "now": 1791725184727,
  "seed": 42,
  "limits": { "max_steps": 1000000, "max_depth": 200, "max_output": 100000, "max_alloc": 100000 }
}
```

- `entry` : fichier principal (facultatif s'il n'y a qu'un fichier, sinon `main.q`).
- `input` : lignes pour `read()`. **S'il est absent, `read` n'est pas disponible**
  et un programme qui l'appelle est refusé à la compilation.
- `now` : l'heure en millisecondes depuis 1970, pour `time.now()` et les
  `uuid.v7()` / `ulid.new()`. **Absent, il n'y a pas d'horloge.**
- `seed` : graine du hasard (`random`, `uuid.v4()`, `ulid.…`) : la même graine
  donne les mêmes nombres. **Absente, il n'y a pas de hasard** : un serveur
  décide lui-même quoi fournir (une graine tirée au sort pour chaque requête,
  par exemple).
- `limits` : facultatif ; le serveur applique de toute façon ses propres plafonds.

Réponse :

```json
{
  "ok": true,
  "phase": "run",
  "output": "3\nhello\n",
  "diagnostics": [],
  "steps": 15
}
```

- `phase` : `"request"` (requête invalide), `"compile"` (erreurs avant exécution)
  ou `"run"` (le programme a été exécuté ; `ok` dit s'il a réussi).
- `exit_code` : présent seulement si le programme a appelé `exit(code)`.
- `diagnostics` : liste d'erreurs (et avertissements) :

```json
{
  "severity": "error",
  "code": "T002",
  "message": "expected `int`, found `string`",
  "location": { "file": "main.q", "line": 1, "col": 14, "end_line": 1, "end_col": 17 },
  "notes": []
}
```

### Depuis Python

```python
import json, subprocess

def run_qlang(files, input=None):
    request = {"files": files}
    if input is not None:
        request["input"] = input
    done = subprocess.run(["qlang", "run", "--json"], input=json.dumps(request),
                          capture_output=True, text=True, timeout=30)
    return json.loads(done.stdout)

result = run_qlang({"main.q": 'print("hello")'})
print(result["output"], result["ok"])
```

### Depuis PHP

```php
$request = json_encode(["files" => ["main.q" => 'print("hello")']]);
$proc = proc_open(["qlang", "run", "--json"], [0 => ["pipe", "r"], 1 => ["pipe", "w"]], $pipes);
fwrite($pipes[0], $request);
fclose($pipes[0]);
$result = json_decode(stream_get_contents($pipes[1]), true);
echo $result["output"];
```

Ou, avec le serveur HTTP : `POST http://127.0.0.1:8080/run` avec la même requête (d'un programme, pas d'une page web : une requête qui porte `Origin` ou `Sec-Fetch-Site: cross-site` est refusée (403), et, quand le serveur n'écoute que sur cette machine, un `Host` autre que `localhost`, `127.0.0.1` ou `[::1]` aussi, contre le *DNS rebinding*).

### Sécurité

Le protocole JSON n'accède pas au disque (un `../x.q` n'y est qu'un nom de fichier
de la requête) et borne le temps de calcul (étapes,
y compris le travail des méthodes prédéfinies), la récursion, la sortie, la taille
des tableaux et chaînes, et le nombre d'erreurs renvoyées (100 au plus). Un bug
interne du compilateur ou de l'interpréteur est renvoyé comme une erreur `X999`
au lieu d'arrêter le processus ; dans le serveur HTTP, le worker concerné répond
500 et son emplacement de connexion est libéré. Le programme `qlang` arrête un
programme qui tient plus d'un gigaoctet de mémoire (`R904` ; `--max-memory MB` ou
`QLANG_MAX_MEMORY` pour le changer, `0` pour le retirer) : la mémoire du processus entier
est comptée, donc un programme hostile peut gêner les autres requêtes d'un serveur tant
qu'il tourne, mais pas l'épuiser. Les valeurs imbriquées sur des millions de niveaux (une
liste chaînée géante) sont libérées sans récursion et leur comparaison s'arrête sur une
erreur (`R092`). Il ne limite pas le temps réel (les étapes le bornent, pas l'horloge) :
pour du code d'inconnus, lancez `qlang` dans un conteneur ou avec les limites du système
(`ulimit`, cgroups) et un délai d'attente. Le serveur HTTP (`qlang serve`) est minimal :
pas de TLS, pas de connexions persistantes, 16 connexions simultanées au plus, une requête
doit arriver en 20 secondes, les fichiers cachés (`.env`, `.git`) ne sont jamais servis par
`--static`. Placez-le derrière un proxy inverse et ne l'exposez pas directement à Internet.

## Navigateur : WebAssembly

`crates/qlang-wasm` compile `qlang-core` en WebAssembly (`qlang.wasm`, environ 700 ko, publié avec
chaque release avec son enveloppe JavaScript `qlang.mjs`). Même protocole JSON que `run --json`,
sans serveur : de quoi faire tourner du qlang dans une page web, dans un Web Worker. Voir
[`crates/qlang-wasm`](crates/qlang-wasm/README.md).

## Éditeurs : `qlang lsp` et VS Code

`qlang lsp` est un serveur de langage ([LSP](https://microsoft.github.io/language-server-protocol/),
sur l'entrée et la sortie standard) : n'importe quel éditeur qui sait en parler (Neovim, Helix,
Emacs, Zed, Sublime…) a, grâce au vrai compilateur :

- les **erreurs** pendant la frappe, y compris dans les fichiers importés ;
- les **infobulles** (types, signatures, commentaires `--" … --"`) ;
- l'**autocomplétion** (membres après un `.`, champs d'un littéral de structure, noms visibles,
  mots-clés) et l'**aide à la saisie des paramètres** ;
- **aller à la définition**, **trouver les références** et **renommer** (variables, paramètres,
  fonctions, méthodes, types, champs, variantes, dans tout le projet ; le renommage refuse ce
  qui serait ambigu, comme `Point { x }` où `x` est à la fois un champ et une variable) ;
- le **plan du fichier** et la **recherche de symboles** du projet.

Le texte non enregistré des fichiers ouverts est pris en compte ; les imports sont relatifs au
dossier ouvert dans l'éditeur, et les paquets `@nom` sont suivis.

L'extension VS Code (coloration, serveur de langage, bouton ▶ pour exécuter, snippets) est dans
[`editors/vscode`](editors/vscode) :

```sh
curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --vscode
```

(le programme `qlang` et l'extension ; ou, depuis les sources : `cargo install --path crates/qlang-cli`,
puis `cd editors/vscode && npm install && npm run package` et `code --install-extension qlang-*.vsix`).

Pour un autre éditeur, il suffit de lancer `qlang lsp` pour les fichiers `.q`. Réglages Neovim, Helix, Emacs,
Sublime Text, JetBrains et Zed : [docs/editors.md](docs/editors.md).

Pour les éditeurs, `qlang packages` affiche en JSON les paquets du projet courant (nom,
dossier, fichier principal) ; une requête JSON peut ensuite contenir leurs fichiers sous
les noms `@nom/fichier.q` et dire quel fichier `@nom` désigne avec
`"package_mains": {"nom": "src/lib.q"}`. `qlang check --json` lit la même requête JSON que `run --json` mais ne fait que
vérifier le programme (réponse avec `"phase": "check"`) ; `"mode": "check"` dans la requête fait la
même chose avec `run --json`.

Avec `"info": true` dans la requête, la réponse contient aussi ce que le compilateur a appris du
programme, pour les éditeurs : le type de chaque expression (`exprs`), les variables et leur portée
(`locals`), les définitions (`items`), les usages de chaque nom avec sa définition (`refs`), les noms visibles de chaque fichier (`globals`) et les membres
de chaque type rencontré (`members` pour `valeur.`, `statics` pour `Type.` et `module.`). Les
informations sont fournies même si le programme contient des erreurs de types ; elles manquent
seulement si le fichier ne peut pas être analysé (erreur de syntaxe) ou si les déclarations sont
fausses. La documentation des méthodes prédéfinies vient du compilateur ; celle de vos fonctions,
structures et méthodes est lue dans les commentaires `--" … --"` qui les précèdent.

## Développement

```sh
cargo test                       # tests unitaires, de bout en bout, fuzzing
cargo test --release             # idem, plus vite
```

- `crates/qlang-core/tests/run.rs` : programmes complets et leur sortie attendue.
- `crates/qlang-core/tests/check_errors.rs` : erreurs que le compilateur doit signaler.
- `crates/qlang-core/tests/fuzz.rs` : programmes mutés au hasard ; ni le compilateur
  ni l'interpréteur ne doivent jamais paniquer.

Organisation du cœur (`crates/qlang-core/src`) :

| Fichier              | Rôle                                                                 |
| -------------------- | -------------------------------------------------------------------- |
| `lexer.rs`           | texte vers jetons                                                    |
| `parser.rs`          | jetons vers arbre syntaxique (`ast.rs`), avec récupération d'erreurs |
| `check/`             | modules, déclarations, vérification des types (`types.rs`)           |
| `interp.rs`          | exécution (`value.rs` pour les valeurs)                              |
| `diag.rs`, `span.rs` | erreurs et positions                                                 |
| `host.rs`            | l'interface avec l'extérieur, les limites                            |

## Licence

[MIT](LICENSE).
