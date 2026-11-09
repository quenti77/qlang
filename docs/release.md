# Publier une version

Une version se publie en poussant un tag. Tout le reste est automatique
(`.github/workflows/release.yml`).

1. **Choisir le numéro** `X.Y.Z` et le mettre à jour aux **deux** endroits :
   - `version` dans `[workspace.package]` de `Cargo.toml` (les crates en héritent) ;
   - `version` dans `editors/vscode/package.json`.

   Puis `cargo build` pour mettre à jour `Cargo.lock`. Le workflow refuse de publier si
   les deux numéros diffèrent, ou si le tag ne correspond pas.
2. **Tagger la suite de conformité** si la version a de nouveaux cas : dans `qlang-conformance`, fusionner les
   branches de cas (une par fonctionnalité, de même nom que celle de `qlang`), poser le tag de la version (`v1.1.0`)
   et mettre ce tag dans `conformance.ref`. La CI d'un push sur `main` échoue tant que `conformance.ref` n'est pas
   un tag (`scripts/conformance-ref.sh --check-tag`).
3. **Fusionner** ce changement sur `main` (PR, CI verte ; `scripts/check-docs.py` ne signale aucun écart entre la
   documentation et le programme, la CI le lance aussi).
4. **Tagger** et pousser :

   ```sh
   git checkout main && git pull
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
5. Le workflow compile `qlang` pour Linux (x86_64 et arm64, statique), macOS (arm64 et
   Intel) et Windows (x86_64), empaquette l'extension VS Code, calcule `SHA256SUMS` et crée la
   release GitHub avec ses notes générées. Cinq à dix minutes.
6. Vérifier la release sur la page *Releases*, puis essayer `install.sh` sur une machine propre : la commande doit afficher
   `Updated: qlang A -> qlang X.Y.Z`.
7. **Le site de documentation** (`quenti77/qlang-learn`) suit tout seul : à la fin de la publication, le workflow
   prévient le dépôt du site, dont le workflow *Follow a release* ouvre une pull request qui met à jour
   `versions.json` et les deux fichiers qui nomment les versions épinglées. Un correctif (`1.0.1`) met à jour la ligne `1.0` ; un nouveau
   numéro mineur (`1.1.0`) ajoute une version au site, la rend la dernière et garde le texte écrit à la main de la
   précédente (tag `site-1.0`). Relire la pull request (`npm run sync` signale quand la traduction anglaise de la
   référence doit suivre `docs/language.md`) et la fusionner : le site est publié à la fusion. Sans le secret
   `SITE_TOKEN` (un jeton qui peut écrire dans `qlang-learn`), rien n'est envoyé : lancer *Follow a release* à la
   main depuis l'onglet *Actions* du dépôt du site (`project` = `qlang`, `version` = `X.Y.Z`).

Le workflow `.github/workflows/next.yml` publie en plus, à chaque changement de `main`, une pré-version nommée `next`
(`qlang.wasm`, `qlang.mjs`, `reference.json`) : c'est la version « next » du site. Une pré-version n'est jamais « la
dernière version » : `install.sh` et `qlang self-update` l'ignorent.

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

## L'extension sur la Marketplace de VS Code

Le workflow publie `qlang.vsix` sur la Marketplace à la fin de la publication (étape *Publish the VS Code extension*),
sous l'identifiant **`qlang.qlang-vscode`** (éditeur `qlang`, nom `qlang-vscode` : le nom `qlang` était pris). Un
échec de cette étape n'annule pas la release.

- **Sans jeton** : l'étape utilise *Trusted Publishing* (`vsce publish --oidc`). Le job `publish` (qui a
  `id-token: write`) prouve son identité à la Marketplace, qui lui donne un accès de courte durée. Rien à renouveler.
- **La politique** : sur https://marketplace.visualstudio.com/manage/publishers/qlang, onglet *Trusted Publishing* :
  fournisseur GitHub Actions, propriétaire `quenti77`, dépôt `qlang`, workflow `release.yml`, environnement vide.
  Si le workflow change de nom, la politique doit changer aussi.
- **Rattraper une version** : depuis un poste, `npx @vscode/vsce publish --packagePath qlang.vsix -p <jeton>` avec le
  fichier de la release et un jeton Azure DevOps (scope *Marketplace > Manage*), ou le téléverser à la main sur la page de
  gestion de l'éditeur.

## Windows, Docker et gestionnaires de paquets

- **Windows** : `qlang-x86_64-pc-windows-msvc.zip` est dans la release (compilé et lancé une fois sur
  le runner Windows), avec `install.ps1`. Seul le script `install.ps1` reste à essayer sur une
  vraie machine Windows.
- **Docker** : le job `docker` du workflow construit `Dockerfile` (`FROM scratch`, utilisateur 65532)
  avec les deux programmes Linux déjà compilés et publie `ghcr.io/quenti77/qlang:X.Y.Z` et `:latest`
  (amd64 et arm64). Après la **première** publication, rendre le paquet public (GitHub > Packages
  > qlang > Package settings > Change visibility).
- **Pas à pas pour créer les dépôts** (image publique, tap, bucket, AUR, winget, apt/yum, mise à jour automatique) :
  [packaging.md](packaging.md).
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
- Publication sur Open VSX (VSCodium, Cursor…) et sur crates.io : non faite.
