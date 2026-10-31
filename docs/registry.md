# Publier et utiliser des paquets

qlang n'a pas de serveur de paquets à lui : un paquet est un **dépôt git** avec un `qlang.toml` et **une étiquette par
version** (`v1.2.3`). Un petit **index** (`packages.json` dans le dépôt de qlang) dit quel dépôt porte quel nom, pour
que `qlang search`, `qlang info` et `qlang add nom` fonctionnent. L'index ne contient aucun code : il ne dit que où
aller ; le paquet est ensuite récupéré par git, figé dans `qlang.lock` et vérifié.

## Utiliser un paquet

```sh
qlang search json            # les paquets de l'index (nom, description, licence, dépôt)
qlang info std               # licence, mots-clés, versions disponibles (les retirées sont marquées), comment l'ajouter
qlang add std                # l'ajoute dans [dependencies] avec la version compatible la plus récente
qlang install                # récupère exactement les commits de qlang.lock
qlang update                 # avance dans les intervalles demandés, réécrit qlang.lock
qlang verify                 # les paquets installés sont-ils bien ceux de qlang.lock, sans fichier modifié ?
```

### Contraintes de version

```toml
[dependencies]
std    = { git = "https://github.com/quenti77/qlang-std", version = "^0.19" }
text   = { git = "https://github.com/x/text", version = ">=1.2, <2" }
pinned = { git = "https://github.com/x/pinned", tag = "v1.0.0" }     # ou rev, ou branch
local  = { path = "../local" }
```

Une contrainte choisit la **plus haute étiquette `vX.Y.Z` qui la satisfait** (une pré-version comme `v1.3.0-rc.1`
n'est choisie que si la contrainte en nomme une). `^1.2` : `>=1.2.0, <2.0.0` ; `^0.2.3` : `>=0.2.3, <0.3.0` ;
`^0.0.3` : exactement `0.0.3` ; `~1.2.3` : `>=1.2.3, <1.3.0` ; `1.2` seul vaut `^1.2` ; `>=`, `>`, `<=`, `<`, `=` et `*`
existent, séparés par des virgules (toutes doivent tenir). `qlang.lock` garde la contrainte et le commit choisi :
`qlang install` ne bouge pas tant que la contrainte ne change pas, `qlang update` choisit de nouveau.

Un nom de paquet désigne un seul dossier dans tout le projet. Si le projet et un de ses paquets demandent le même
dépôt avec des intervalles différents, qlang cherche **une étiquette qui satisfait les deux** ; s'il n'y en a pas, il
s'arrête en disant qui demande quoi. Deux sources différentes pour un même nom (un dépôt et un chemin, deux dépôts)
restent une erreur.

### Intégrité et signatures

- `qlang.lock` fixe chaque paquet à un **commit** : l'identifiant d'un commit est l'empreinte de son contenu, donc
  un dépôt ne peut pas changer ce qu'il y a derrière sans que `qlang install` le refuse (« no longer has the locked
  commit »). `qlang verify` contrôle en plus que le dossier installé est bien à ce commit et qu'aucun fichier n'a
  été modifié depuis.
- `signed = true` sur une dépendance exige que le commit (ou l'étiquette annotée qui le nomme) porte une **signature
  que git sait vérifier** avec les clés auxquelles vous faites confiance (`gpg`, ou
  `gpg.ssh.allowedSignersFile` pour les signatures SSH). Sans clé de confiance, l'installation s'arrête : qlang ne
  décide pas à votre place à qui faire confiance.

### Paquets privés

Un dépôt privé se récupère avec ce que git sait déjà faire : une adresse `ssh://` ou `git@…` avec votre clé, ou un
assistant d'identifiants. Pour un dépôt **https** protégé par un jeton (GitHub, GitLab…), la dépendance nomme la
variable d'environnement qui le contient :

```toml
private = { git = "https://github.com/moi/prive", version = "^1", token_env = "PRIVE_TOKEN" }
```

Le jeton n'est donné à git que pour les commandes de **cette** dépendance (jamais pour les autres dépôts), seulement en
https, et par l'environnement de git plutôt que par la ligne de commande (où `ps` le montrerait). Il n'est écrit
nulle part : ni dans `qlang.toml`, ni dans `qlang.lock`.

## Publier un paquet

1. Un dépôt git (public, pour être dans l'index) avec, dans son `qlang.toml`, `[package]` :
   `name`, `version` (X.Y.Z), `description` (une ligne), `license` (un nom SPDX, `MIT`…), éventuellement
   `keywords = [...]` et `main` ; un `README.md` ; le fichier principal qui compile.
2. Une étiquette `vX.Y.Z` sur le commit de la version (`git tag v1.2.3 && git push --tags`).
3. `qlang publish` vérifie tout cela (arbre propre, étiquette sur `HEAD`, dépôt `origin` en https, README, licence…)
   et, si c'est prêt, écrit l'entrée à ajouter à l'index.
4. Une **pull request** sur [`packages.json`](https://github.com/quenti77/qlang/blob/main/packages.json) avec cette
   entrée. Les versions suivantes n'ont rien à faire de plus que leur étiquette : l'index ne les liste pas.

## Ce que l'index promet, et ne promet pas

Entrée de l'index :

```json
"nom": {
  "git": "https://github.com/moi/nom",
  "description": "Une ligne.",
  "license": "MIT",
  "keywords": ["json", "csv"],
  "yanked": ["v1.2.0"],
  "removed": "raison"
}
```

- **Qui peut référencer un paquet** : n'importe qui, par une pull request, si le nom est libre (premier arrivé), si le
  dépôt est public en https, si la description et la licence sont dites, et si `qlang publish` passe. Un nom ne
  se reprend pas à son auteur ; changer de mains passe par une PR des deux.
- **Ce que l'index ne fait pas** : il ne relit pas le code des paquets. Un paquet est du code qui s'exécute chez vous :
  lisez-le, fixez une version (`qlang.lock` est dans git), utilisez `signed = true` quand l'auteur signe.
- **Retrait en cas de problème** : une pull request (ou un signalement privé, voir [SECURITY.md](../SECURITY.md) pour
  une faille) peut ajouter à l'entrée `"yanked": ["v1.2.0"]` : une **nouvelle résolution** ne choisit plus cette
  version, mais les projets qui l'ont déjà dans leur `qlang.lock` continuent de marcher (on ne casse pas une
  installation qui existe) ; ou `"removed": "raison"` : le paquet disparaît de `qlang search` et `qlang add` le refuse
  (les projets existants continuent aussi, git garde les commits). Retraits décidés pour une faille, un paquet
  malveillant, un nom usurpé ou l'abandon demandé par l'auteur.
- **L'index lui-même** est vérifié par un test du dépôt de qlang (noms valides, https, description et licence
  présentes, champs connus) ; le lire ne télécharge ni n'exécute rien. `QLANG_INDEX_URL` en désigne un autre (une URL
  ou un chemin), et sans réseau les paquets connus du binaire sont utilisés.
