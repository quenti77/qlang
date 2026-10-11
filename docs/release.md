# Publier une version

Une version se publie en poussant un tag. Tout le reste est automatique
(`.github/workflows/release.yml`).

1. **Choisir le numéro** `X.Y.Z` et le mettre à jour aux **deux** endroits :
   - `version` dans `[workspace.package]` de `Cargo.toml` (les crates en héritent) ;
   - `version` dans `editors/vscode/package.json`.

   Puis `cargo build` pour mettre à jour `Cargo.lock`. Le workflow refuse de publier si
   les deux numéros diffèrent, ou si le tag ne correspond pas.
2. **Fusionner** ce changement sur `main` (PR, CI verte).
3. **Tagger** et pousser :

   ```sh
   git checkout main && git pull
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
4. Le workflow compile `qlang` pour Linux (x86_64 et arm64, statique) et macOS (arm64 et
   Intel), empaquette l'extension VS Code, calcule `SHA256SUMS` et crée la
   release GitHub avec ses notes générées. Cinq à dix minutes.
5. Vérifier la release sur la page *Releases*, puis essayer `install.sh` sur une machine propre : la commande doit afficher
   `Updated: qlang A -> qlang X.Y.Z`.

## Ce que contient une release

| Fichier                                   | Contenu                                  |
| ----------------------------------------- | ---------------------------------------- |
| `qlang-x86_64-unknown-linux-musl.tar.gz`  | Linux x86_64 (statique, sans dépendance) |
| `qlang-aarch64-unknown-linux-musl.tar.gz` | Linux arm64 (statique)                   |
| `qlang-aarch64-apple-darwin.tar.gz`       | macOS Apple Silicon                      |
| `qlang-x86_64-apple-darwin.tar.gz`        | macOS Intel                              |
| `qlang.vsix`                              | extension VS Code                        |
| `SHA256SUMS`                              | sommes de contrôle de tous les fichiers  |

Les noms ne contiennent pas le numéro de version : l'adresse
`https://github.com/quenti77/qlang/releases/latest/download/<fichier>` désigne toujours la
dernière version, ce qui permet aux scripts d'installation de ne pas interroger l'API.

## Essayer sans publier

Une pull request qui touche aux fichiers de publication (`release.yml`, `scripts/`,
`install.sh`, la version) lance les mêmes compilations pour toutes les
plateformes, sans rien publier. `scripts/test-install.sh` teste `install.sh` contre une fausse
release construite avec le binaire local :

```sh
cargo build --release && scripts/test-install.sh
```

## Windows

Pas de programme précompilé pour l'instant : les utilisateurs Windows compilent `qlang` avec
Rust (voir le README). Les tests tournent quand même sur Windows dans la CI (`ci.yml`), ce qui
a déjà fait corriger la gestion des chemins.

Pour proposer un jour un programme Windows : remettre
`{ target: x86_64-pc-windows-msvc, os: windows-latest }` dans la matrice de `release.yml`
(`scripts/package-release.sh` sait déjà faire le `.zip`), écrire un script d'installation
PowerShell (l'ancien `install.ps1` est dans l'historique git, il n'a jamais été exécuté) et
le tester sur une vraie machine Windows.

## Pas encore fait

- Les binaires ne sont pas signés (macOS peut demander une confirmation au premier lancement :
  `xattr -d com.apple.quarantine ~/.local/bin/qlang`).
- Publication sur la Marketplace de VS Code ou sur crates.io : non faite.
