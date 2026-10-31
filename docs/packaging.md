# Publier qlang dans les gestionnaires de paquets

Chaque release de qlang contient déjà tout ce qu'il faut (`qlang.rb`, `qlang.json`, `PKGBUILD`, les trois fichiers
winget, les `.deb` et `.rpm`) : voir [release.md](release.md). Ce qui manque, ce sont des **dépôts et des comptes** qui
sont à toi. Ce guide les crée un par un, du plus simple au plus long. Chaque étape dit **ce qui est déjà prêt**, **ce
que tu fais**, et **comment vérifier**.

État au départ (`v0.82.0`) : aucun des dépôts ci-dessous n'existe, et l'image `ghcr.io/quenti77/qlang` est privée.

| # | Cible | Ce que ça donne | Compte nécessaire | Durée |
| --- | --- | --- | --- | --- |
| 1 | Image Docker publique | `docker run ghcr.io/quenti77/qlang` | GitHub | 2 min |
| 2 | Tap Homebrew | `brew install quenti77/qlang/qlang` | GitHub | 10 min |
| 3 | Bucket Scoop | `scoop install qlang` | GitHub | 10 min |
| 4 | AUR (Arch) | `yay -S qlang-bin` | AUR + clé SSH | 15 min |
| 5 | winget | `winget install quenti77.qlang` | GitHub (une PR relue par Microsoft) | 20 min + délai de relecture |
| 6 | Dépôts apt et yum | `apt install qlang` | une clé GPG + GitHub Pages | 1 h |
| 7 | Mise à jour automatique | les étapes 2 et 3 à chaque release | un jeton GitHub | 10 min |

Dossiers déjà préparés sur cette machine, commités en local avec la version 0.82.0 (rien n'est publié) :
`~/Projects/homebrew-qlang`, `~/Projects/scoop-qlang`, `~/Projects/aur-qlang-bin` et
`~/Projects/winget-qlang-0.82.0`. Pour une autre version, télécharge les fichiers de sa release :
`gh release download vX.Y.Z -R quenti77/qlang -p qlang.rb -p qlang.json -p PKGBUILD -p 'quenti77.qlang*'`.

## 1. Rendre l'image Docker publique

Une image publiée sur ghcr.io est **privée** tant qu'on ne dit pas le contraire (`curl -I https://ghcr.io/v2/quenti77/qlang/manifests/latest`
répond `401`).

1. Ouvre <https://github.com/users/quenti77/packages/container/package/qlang> (ou *Profil → Packages → qlang*).
2. *Package settings* → tout en bas, **Change package visibility** → **Public** → tape `qlang` pour confirmer.
3. Dans la même page, *Connect repository* → `quenti77/qlang` (le paquet apparaît alors sur la page du dépôt).

Vérifier, sans être connecté : `docker pull ghcr.io/quenti77/qlang:latest && docker run --rm ghcr.io/quenti77/qlang --version`.

## 2. Tap Homebrew

Un *tap* est un dépôt GitHub **nommé `homebrew-<nom>`** : `brew tap quenti77/qlang` va chercher
`github.com/quenti77/homebrew-qlang`.

```sh
cd ~/Projects/homebrew-qlang                                   # déjà prêt : Formula/qlang.rb et README.md
gh repo create quenti77/homebrew-qlang --public --source . --push \
  --description "Homebrew tap for qlang"
```

Vérifier sur un Mac ou un Linux avec Homebrew :

```sh
brew tap quenti77/qlang
brew audit --strict --online quenti77/qlang/qlang       # doit être silencieux
brew install quenti77/qlang/qlang && brew test quenti77/qlang/qlang
qlang --version
```

`brew test` lance le bloc `test do` de la formule (`qlang --version` doit dire la version de la formule).

## 3. Bucket Scoop

Un *bucket* est un dépôt GitHub avec un dossier `bucket/`. Le nom est libre ; ici `scoop-qlang`.

```sh
cd ~/Projects/scoop-qlang                                      # déjà prêt : bucket/qlang.json et README.md
gh repo create quenti77/scoop-qlang --public --source . --push \
  --description "Scoop bucket for qlang"
```

Vérifier sur Windows (PowerShell) :

```powershell
scoop bucket add qlang https://github.com/quenti77/scoop-qlang
scoop install qlang
qlang --version
```

`checkver` et `autoupdate` du manifeste permettent aussi à Scoop de trouver seul une version plus récente.

## 4. AUR (Arch Linux)

Le paquet s'appelle `qlang-bin` (il installe le programme déjà compilé). **Vérifié ici** : sur cette machine (Arch),
`makepkg` construit le paquet depuis la release 0.82.0 et `usr/bin/qlang --version` répond « qlang 0.82.0 ».

1. Crée un compte sur <https://aur.archlinux.org/register>.
2. Génère une clé SSH pour l'AUR et ajoute sa partie publique dans *My Account → SSH Public Key* :

   ```sh
   ssh-keygen -t ed25519 -f ~/.ssh/aur -C "aur"
   cat ~/.ssh/aur.pub
   ```

   et dans `~/.ssh/config` :

   ```
   Host aur.archlinux.org
     IdentityFile ~/.ssh/aur
     User aur
   ```

3. Publie (le premier `git push` crée le paquet) :

   ```sh
   cd ~/Projects/aur-qlang-bin                  # déjà prêt : PKGBUILD et .SRCINFO
   git remote add origin ssh://aur@aur.archlinux.org/qlang-bin.git
   git push -u origin main:master               # l'AUR attend la branche `master`
   ```

4. Pour une nouvelle version : remplacer `PKGBUILD` par celui de la release, `makepkg --printsrcinfo > .SRCINFO`,
   commit, push.

Vérifier : `yay -S qlang-bin` (ou `git clone https://aur.archlinux.org/qlang-bin.git && cd qlang-bin && makepkg -si`).
Le paquet `conflicts` avec un éventuel futur `qlang` compilé depuis les sources.

## 5. winget

winget n'a pas de dépôt à toi : on propose les manifestes par une **pull request** sur
[`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs), relue par un robot (validation, installation dans
une machine virtuelle, analyse) puis par un humain.

Les trois fichiers sont prêts dans `~/Projects/winget-qlang-0.82.0/manifests/q/quenti77/qlang/0.82.0/` (identifiant
`quenti77.qlang`, archive zip avec un programme portable `qlang\qlang.exe` et l'alias `qlang`).

Méthode A, avec l'outil de Microsoft (sur Windows) :

```powershell
winget install wingetcreate
wingetcreate update quenti77.qlang --version 0.82.0 `
  --urls https://github.com/quenti77/qlang/releases/download/v0.82.0/qlang-x86_64-pc-windows-msvc.zip `
  --submit
```

(la première fois, `wingetcreate new <url du zip>` pose les questions et ouvre la PR ; il demande un jeton GitHub.)

Méthode B, à la main :

```sh
gh repo fork microsoft/winget-pkgs --clone --remote
cd winget-pkgs && git checkout -b quenti77.qlang-0.82.0
mkdir -p manifests/q/quenti77/qlang/0.82.0
cp ~/Projects/winget-qlang-0.82.0/manifests/q/quenti77/qlang/0.82.0/* manifests/q/quenti77/qlang/0.82.0/
git add manifests && git commit -m "New package: quenti77.qlang version 0.82.0" && git push -u origin HEAD
gh pr create --repo microsoft/winget-pkgs --title "New package: quenti77.qlang version 0.82.0" --body "qlang, a small statically typed language. Portable zip from the GitHub release."
```

Vérifier sur Windows **avant** la PR : `winget validate --manifest <dossier>` puis
`winget settings --enable LocalManifestFiles` et `winget install --manifest <dossier>`. Après fusion (de quelques
heures à quelques jours) : `winget install quenti77.qlang`.

À chaque version suivante : `wingetcreate update quenti77.qlang --version X.Y.Z --urls <zip> --submit` (une PR par
version ; on peut l'automatiser avec l'action `vedantmgoyal9/winget-releaser`).

## 6. Dépôts apt et yum (GitHub Pages)

Les `.deb` et `.rpm` de chaque release s'installent déjà à la main (`sudo dpkg -i`, `sudo rpm -i`). Un **dépôt** permet
`apt install qlang` et les mises à jour. Il lui faut une **clé GPG** qui signe les index : sans elle, apt refuse le
dépôt. **Cette partie n'a pas été essayée** (ni `aptly` ni `createrepo_c` ne sont installés ici) ; les commandes
sont celles des outils, à vérifier au premier essai.

Une clé de signature, dédiée au dépôt :

```sh
gpg --quick-generate-key "qlang packages <ton-courriel>" ed25519 sign 3y
gpg --list-secret-keys --keyid-format long          # note l'identifiant (KEYID)
gpg --armor --export KEYID > qlang.gpg.asc          # la partie publique, à publier
gpg --armor --export-secret-keys KEYID > qlang-private.asc   # sauvegarde: la garder HORS du dépôt, dans un coffre
```

Le dépôt est un site statique : crée `quenti77/qlang-packages` et active GitHub Pages sur sa branche `gh-pages`.

**apt** (avec `aptly`) :

```sh
V=0.82.0
gh release download v$V -R quenti77/qlang -p 'qlang_*_amd64.deb' -p 'qlang_*_arm64.deb' -D debs
aptly repo create -distribution=stable -component=main qlang
aptly repo add qlang debs/
aptly publish repo -architectures=amd64,arm64 -gpg-key=KEYID qlang
# publie ~/.aptly/public/ (+ qlang.gpg.asc) dans la branche gh-pages du dépôt
```

Utilisation :

```sh
curl -fsSL https://quenti77.github.io/qlang-packages/qlang.gpg.asc | sudo gpg --dearmor -o /usr/share/keyrings/qlang.gpg
echo "deb [signed-by=/usr/share/keyrings/qlang.gpg] https://quenti77.github.io/qlang-packages stable main" \
  | sudo tee /etc/apt/sources.list.d/qlang.list
sudo apt update && sudo apt install qlang
```

**yum / dnf** (avec `createrepo_c`) :

```sh
gh release download v$V -R quenti77/qlang -p 'qlang-*.rpm' -D rpm
rpm --addsign rpm/*.rpm                      # avec %_gpg_name KEYID dans ~/.rpmmacros
createrepo_c rpm/ && gpg --detach-sign --armor rpm/repodata/repomd.xml
# publie rpm/ dans le dossier rpm/ de la branche gh-pages
```

```ini
# /etc/yum.repos.d/qlang.repo
[qlang]
name=qlang
baseurl=https://quenti77.github.io/qlang-packages/rpm
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://quenti77.github.io/qlang-packages/qlang.gpg.asc
```

Les `.deb` et `.rpm` des releases ne sont pas signés eux-mêmes ; seul l'index du dépôt l'est (les releases ont leur
attestation et `SHA256SUMS`).

## 7. Mise à jour automatique du tap et du bucket

À chaque release, le workflow peut pousser `qlang.rb` et `qlang.json` dans les deux dépôts, avec
[`scripts/update-package-repos.sh`](../scripts/update-package-repos.sh) (testé ici avec des dépôts locaux : il ne
commit que s'il y a un changement, et saute un dépôt dont l'adresse n'est pas donnée).

1. Crée un **jeton à portée fine** : <https://github.com/settings/personal-access-tokens/new>, *Repository access →
   Only select repositories* → `homebrew-qlang` et `scoop-qlang`, *Permissions → Contents: Read and write*, durée
   d'un an.
2. Ajoute-le comme secret du dépôt qlang :

   ```sh
   gh secret set PACKAGES_TOKEN -R quenti77/qlang      # colle le jeton quand il le demande
   ```

3. C'est tout : l'étape « Update the Homebrew tap and the Scoop bucket » du workflow `Release` s'en sert. Sans le
   secret elle ne fait rien (et elle n'empêche jamais la release : `continue-on-error`).

Les étapes 4 (AUR) et 5 (winget) ne sont pas dans le workflow : l'AUR veut une clé SSH et `makepkg` pour `.SRCINFO`,
winget une PR relue. Chacune a une action GitHub toute faite (`KSXGitHub/github-actions-deploy-aur`,
`vedantmgoyal9/winget-releaser`) à ajouter quand le premier passage à la main aura marché.

## Ordre conseillé

1, puis 2 et 3 (les plus rapides, et l'étape 7 ensuite), puis 4. L'étape 5 prend du temps de relecture : commence-la
tôt. L'étape 6 est la seule qui demande de garder un secret (la clé GPG) : ne la fais que si apt et yum comptent pour
toi, et sinon laisse les `.deb` et `.rpm` en téléchargement.
