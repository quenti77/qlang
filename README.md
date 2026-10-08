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

## Utilisation

```sh
cargo build --release            # produit target/release/qlang

qlang run examples/tour.q        # exécute un programme
qlang check examples/tour.q      # vérifie sans exécuter
qlang run prog.q --max-steps 1000000
```

Les erreurs s'affichent avec leur position et un extrait du code :

```
error[T002]: expected `string`, found `int`
  --> main.q:2:17
  |
2 | let b: string = a
  |                 ^
```

Code de sortie : `0` succès, `1` erreur de compilation, `2` erreur d'exécution.
Les `import` ne peuvent lire que des fichiers situés dans le dossier du fichier
principal.

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
    fn capabilities(&self) -> Capabilities;                 // ce que l'hôte sait faire
}
```

- Le terminal branche `stdout`, `stdin` et le disque ; un serveur web branche un
  tampon, des entrées pré-fournies et des fichiers en mémoire ; les tests
  utilisent `MemoryHost`.
- `read()` est **facultatif** : si l'hôte ne le propose pas (`capabilities().read`
  est faux), un programme qui l'appelle est refusé **avant** d'être exécuté.
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
  "limits": { "max_steps": 1000000, "max_depth": 200, "max_output": 100000, "max_alloc": 100000 }
}
```

- `entry` : fichier principal (facultatif s'il n'y a qu'un fichier, sinon `main.q`).
- `input` : lignes pour `read()`. **S'il est absent, `read` n'est pas disponible**
  et un programme qui l'appelle est refusé à la compilation.
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

Ou, avec le serveur HTTP : `POST http://127.0.0.1:8080/run` avec la même requête.

### Sécurité

Le protocole JSON n'accède pas au disque et borne le temps de calcul (étapes), la
récursion, la sortie et la taille des tableaux et chaînes. Il ne limite pas la
mémoire totale d'un processus ni le temps réel : pour du code d'inconnus, lancez
`qlang` dans un conteneur ou avec les limites du système (`ulimit`, cgroups) et un
délai d'attente. Le serveur HTTP (`qlang serve`) est minimal : pas de TLS, pas de
connexions persistantes, 16 connexions simultanées au plus. Placez-le derrière un
proxy inverse et ne l'exposez pas directement à Internet.

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
