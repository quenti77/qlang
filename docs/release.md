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
4. Le workflow compile `qlang` pour Linux (x86_64 et arm64, statique), macOS (arm64 et
   Intel) et Windows (x86_64), empaquette l'extension VS Code, calcule `SHA256SUMS` et crée la
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
| `qlang-x86_64-pc-windows-msvc.zip`        | Windows x86_64                           |
| `install.sh`, `install.ps1`               | scripts d'installation                   |
| `qlang.rb`, `qlang.json`, `PKGBUILD`      | Homebrew, Scoop, AUR                     |
| `quenti77.qlang*.yaml`                    | winget                                   |
| `reference.json`                          | `qlang doc --prelude`, pour la référence du site |
| `qlang_*.deb`, `qlang-*.rpm`              | Debian/Ubuntu, Fedora/openSUSE (fichiers) |
| `qlang.wasm`, `qlang.mjs`                 | qlang pour le navigateur (WebAssembly)   |
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

## Windows, Docker et gestionnaires de paquets

- **Windows** : `qlang-x86_64-pc-windows-msvc.zip` est dans la release (compilé et lancé une fois sur
  le runner Windows), avec `install.ps1`. Seul le script `install.ps1` reste à essayer sur une
  vraie machine Windows.
- **Docker** : le job `docker` du workflow construit `Dockerfile` (`FROM scratch`, utilisateur 65532)
  avec les deux programmes Linux déjà compilés et publie `ghcr.io/quenti77/qlang:X.Y.Z` et `:latest`
  (amd64 et arm64). Après la **première** publication, rendre le paquet public (GitHub > Packages
  > qlang > Package settings > Change visibility).
- **Homebrew, Scoop, AUR** : la release contient `qlang.rb`, `qlang.json` et `PKGBUILD`, remplis avec
  les sommes de contrôle (`scripts/render-packaging.sh`, modèles dans `packaging/`). Pour les
  proposer : créer les dépôts `homebrew-qlang` (tap), `scoop-qlang` (bucket) et le paquet AUR
  `qlang-bin`, et y copier ces fichiers à chaque version (à automatiser avec un jeton ensuite).
- **winget** : la release contient aussi `quenti77.qlang.yaml`, `quenti77.qlang.installer.yaml` et
  `quenti77.qlang.locale.en-US.yaml` (zip contenant un programme portable). Pour les proposer : les copier dans
  `manifests/q/quenti77/qlang/X.Y.Z/` d'une copie de `microsoft/winget-pkgs` et ouvrir une pull request ; ou
  `wingetcreate update quenti77.qlang --version X.Y.Z --urls <le zip> --submit`.
- **apt et rpm** : le job `packages` construit (`scripts/build-packages.sh`, avec `dpkg-deb` et `rpmbuild`)
  `qlang_X.Y.Z_amd64.deb`, `_arm64.deb`, `qlang-X.Y.Z-1.x86_64.rpm` et `.aarch64.rpm` : le programme statique,
  sans dépendance, et la licence. Il installe le `.deb` amd64 sur le runner et vérifie `qlang --version`, et lit les
  `.rpm` avec `rpm -qpi`. Ce sont des **fichiers de la release** (`sudo dpkg -i qlang_*.deb`,
  `sudo rpm -i qlang-*.rpm`), pas un dépôt apt/yum : en héberger un demande une clé de signature et un endroit
  (GitHub Pages, un seau S3…), à décider par le propriétaire.
- **`qlang self-update`** télécharge `SHA256SUMS` et l'archive de la release, vérifie la somme et remplace
  le programme (`QLANG_BASE_URL` et `QLANG_API_URL` pointent vers un faux serveur dans les tests).

## Pas encore fait

- Les binaires ne sont pas signés (macOS peut demander une confirmation au premier lancement :
  `xattr -d com.apple.quarantine ~/.local/bin/qlang`).
- Publication sur la Marketplace de VS Code ou sur crates.io : non faite.
