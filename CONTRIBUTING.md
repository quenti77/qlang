# Contribuer à qlang

Merci de t'y intéresser. Le projet est entretenu par une personne, sur son temps : une issue ou une pull request
claire (ce qui ne va pas ou ce qu'on veut, un exemple, la version de `qlang --version`) sera lue ; pour une faille de
sécurité, ne passe **pas** par une issue publique : voir [SECURITY.md](SECURITY.md).

## Les dépôts

| Dépôt | Contenu |
| --- | --- |
| [`qlang`](https://github.com/quenti77/qlang) | le langage (Rust : `crates/qlang-core` le cœur sans E/S, `qlang-cli` la commande, `qlang-wasm` le navigateur), `docs/`, `fuzz/`, `editors/vscode` |
| [`qlang-std`](https://github.com/quenti77/qlang-std) | la bibliothèque standard, en qlang pur |
| [`qlang-test`](https://github.com/quenti77/qlang-test) | le paquet de tests (`describe` / `it` / `expect`) |
| [`qlang-kit`](https://github.com/quenti77/qlang-kit) | la CLI / TUI : générateurs (`kit make`), lanceur de tests, navigateur de base, tableau de bord |
| [`qlang-conformance`](https://github.com/quenti77/qlang-conformance) | ce que le langage promet, en programmes et résultats attendus |
| [`qlang-learn`](https://github.com/quenti77/qlang-learn) | le site (cours, exercices, documentation, référence générée) |

## Construire et tester

```sh
cargo build                      # la commande est target/debug/qlang
cargo test --workspace           # plusieurs minutes (certains tests lancent de vrais serveurs)
cargo fmt && cargo clippy --workspace --all-targets
```

Rust 1.88 au moins (une CI le vérifie). Le fuzzing demande le compilateur nightly : voir
[docs/security.md](docs/security.md#fuzzing). La suite de conformité se lance avec le programme que tu viens de
compiler : `QLANG=target/debug/qlang target/debug/qlang run --allow-process run.q` dans `qlang-conformance`.

## Ce qu'une pull request contient

- **Le code et ses tests.** Un test d'intégration dans `crates/*/tests` pour un comportement visible ; un cas dans
  `qlang-conformance` quand c'est une promesse du langage (une sortie ou un code d'erreur, jamais un message).
- **La documentation** : `docs/language.md` (en français) pour le langage, `README.md` pour les commandes,
  `CHANGELOG.md` (une rubrique **Cassant** si quelque chose casse : voir [docs/stability.md](docs/stability.md) et
  [docs/migration.md](docs/migration.md)). Le site reprend ces documents : sa traduction anglaise
  (`content/en/reference/` dans `qlang-learn`) se met à jour à la main.
- **La version** : chaque pull request qui change le comportement monte la version de `Cargo.toml`, de
  `editors/vscode/package.json` et de son `package-lock.json` (le workflow « Release » vérifie qu'elles sont d'accord).
- **Une CI verte** : tests sous Linux, macOS et Windows, `cargo fmt`, `clippy`, `cargo audit`, Rust 1.88, build
  WebAssembly, extension VS Code, suite de conformité.

Les pull requests sont fusionnées par rebase. Un message de commit dit **pourquoi**, pas seulement quoi.

## Style

- Du code simple, avec les commentaires que le code voisin a : le *pourquoi* d'un choix, pas le *quoi*.
- Les messages d'erreur du langage disent ce qui s'est passé et comment s'en sortir ; leur **code** (`Txxx`, `Rxxx`) est
  stable, leur texte ne l'est pas.
- Une limite (mémoire, pas, taille) est une erreur claire, jamais un plantage.
- `qlang fmt` et `qlang lint` passent sur les `.q` que tu écris.

## Publier (pour la personne qui fait les versions)

Branche, pull request, CI verte, fusion par rebase, étiquette `vX.Y.Z` : le workflow « Release » construit les programmes
(Linux, macOS, Windows), les paquets `.deb` / `.rpm`, les manifestes, l'extension et l'image Docker. Les étapes de
vérification et ce qui reste à faire à la main sont dans [docs/release.md](docs/release.md).
