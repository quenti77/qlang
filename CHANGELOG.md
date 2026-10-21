# Journal des changements

## À publier

- `qlang serve` (protocole JSON) : refuse les requêtes venant d'une page web (`Origin`, `Sec-Fetch-Site`) et, sur une adresse locale, un `Host` qui n'est pas local (DNS rebinding).
Sécurité (voir la section « Sécurité » du README) :

- Limite de mémoire du programme `qlang` : `R904` au-delà d'un gigaoctet (`--max-memory MB`, `QLANG_MAX_MEMORY`, `0` pour la retirer). Avant, un programme pouvait en réserver une dizaine de gigaoctets avec les limites par défaut et faire tuer le processus (le serveur `qlang serve`, ou `run --json`, pour tout le monde).
- Les valeurs imbriquées très profondément (liste chaînée de millions de maillons) sont libérées sans récursion : leur libération faisait déborder la pile et tuait le processus. Les comparer (`==`, `<`) au-delà de 1 000 niveaux est une erreur `R092`.
- `fs.write` ne suit plus un lien symbolique qui sort de `--root` ; `fs.read` refuse ce qui n'est pas un fichier ordinaire (`/dev/zero`) ou dépasse 64 Mo ; `http.send` a un délai de 60 s.
- `qlang serve` : une requête doit arriver entière en 20 s (un client lent gardait une connexion pour toujours), `--static` ne sert plus les fichiers cachés (`.env`, `.git/...`), un statut hors de 100 à 599 donne 500.
- `Db.connect` : un serveur Postgres distant sans `sslmode` dans l'URL est refusé (le mot de passe partait en clair) ; avec `--root`, le fichier d'une base `sqlite://` doit être dans le dossier.
- Fournisseurs d'`extern` : une réponse de plus de 64 Mo est refusée. Dépendances git : un `tag`, `rev` ou `branch` ne peut pas commencer par `-`.

## 0.35.0

- Performance : cache de recherche des variables, valeurs de 24 octets au lieu de 48, calcul direct des opérations entre `int` (boucles ×1,3, appels ×1,1 ; voir `docs/performance.md`).
- Tests : `assert_lt`, `assert_le`, `assert_gt`, `assert_ge`, `assert_len`, `assert_empty`, `assert_not_contains`, `assert_starts_with`, `assert_ends_with`.
- Motifs imbriqués : une variante dans un motif de tuple (`case (Red, n)`), un motif dans une variante (`case Ok((a, b))`, `case Ok(0)`). La vérification d'exhaustivité suit l'imbrication. `T224` n'existe plus.
- Tuples : `<`, `<=`, `>`, `>=`, `sort`, `min`, `max` (ordre sur les éléments, du premier au dernier), et passage à travers une `extern fun` (un tableau côté environnement).

## 0.34.0

- Docs : `README.md` présente `qlang-std`, section « Points encore ouverts » mise à jour.
- `examples/syntax.q` passe `qlang check` (ajout de `examples/math.q`).

## 0.33.0

- `Process.run` / `Process.run_with_input` : lancer un programme, avec la permission `--allow-process`.
- Affectation par déstructuration : `(a, b) = (b, a)`, `(xs[i], xs[j]) = (xs[j], xs[i])`.

Les versions sont numérotées à la publication (tag `vX.Y.Z`) : un numéro qui n'a pas été
publié n'existe pas comme release, ses changements sont dans la suivante.

## 0.32.0

- Maths sur les nombres : `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`,
  `log10`, `log2`, `log`, `to_radians`, `to_degrees`, `signum`, `clamp`, `trunc`, `is_nan`,
  `is_infinite`, `float.INFINITY`, `float.NAN`.
- Encodages et hachages : `Base64`, `Hex`, `Url`, `Hash` (`sha256`, `sha1`, `md5`, `hmac_sha256`).
- Ensembles : `Set<T>` (`Set.of`, `Set<T>.new`, `add`, `contains`, `union`, `intersect`...).
- Tuples : type `(int, string)`, valeur `(1, "a")`, accès `t.0`, comparaison, clés de dictionnaire.
- Déstructuration : `let (a, b) = t`, `for (nom, age) in paires`, et dans `match` : `case (0, y) then`.
- Test : `assert_ok`, `assert_err`, `assert_some`, `assert_none`, `assert_contains`,
  `assert_contains_text`, `assert_close`, `assert_within` ; une erreur levée dans la bibliothèque
  est montrée à l'appel dans votre code.
- Système : `exit(code)`, `read_all()`, `time.sleep(ms)`.
- Interpréteur 1,3 à 1,9 fois plus rapide ; récursion jusqu'à 999 niveaux sous WebAssembly.

## 0.31.0

**Langage**
- Énumérations avec données : `Circle(radius: float)`, construites par un appel
  (`Shape.Circle(2.0)`), prises en morceaux par `match` (`case Circle(r)`, type facultatif
  quand il est connu), `match` exhaustif vérifié. Énumérations génériques.
- `Result<T, E>` est une énumération `Ok(value)` / `Err(error)` ; `Result.ok` / `Result.err`
  deviennent `Result.Ok` / `Result.Err`.
- Opérateur `?` : `expr?` donne la valeur d'un `Result` (ou d'un nullable), ou quitte la
  fonction avec l'erreur (ou `none`).
- Une variable qu'une fonction ou une fonction anonyme modifie n'est plus jamais rétrécie
  par un test `!= none` (le cas qui donnait une erreur à l'exécution est maintenant refusé à
  la compilation).
- `assert_eq`, `assert_ne`.

**Outils**
- `qlang init`, `[package] entry` : `qlang run` / `qlang check` sans fichier.
- `qlang test` (fichiers `*_test.q` et `test_*.q`, fonctions `test_...`).
- `qlang doc` : Markdown tiré des commentaires `--" --"`.

## 0.26.0 – 0.19.0 (résumé)

- `fs`, `env` et `http` écrits en qlang au-dessus de `extern fun`, comme `Db` (0.26).
- Confirmation avant de lancer les commandes d'un `qlang.toml` (`--trust-externs`) ; correctifs
  d'hygiène des paquets (0.25).
- Gestionnaire de paquets : imports `@nom/fichier.q`, dépendances `path` et `git`,
  `qlang.lock`, `qlang install | update | add | remove`, prise en charge par l'éditeur (0.22–0.24).
- Fournisseurs d'`extern fun` pour le terminal : `qlang.toml` `[externs]` et `--extern` (0.21).
- `extern fun` et `Host::call` ; `Db` écrit en qlang au-dessus (0.19–0.20).
