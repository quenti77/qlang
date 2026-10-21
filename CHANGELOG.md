# Journal des changements

Les versions sont numérotées à la publication (tag `vX.Y.Z`) : un numéro qui n'a pas été
publié n'existe pas comme release, ses changements sont dans la suivante.

## À publier

- Maths sur les nombres : `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`,
  `log10`, `log2`, `log`, `to_radians`, `to_degrees`, `signum`, `clamp`, `trunc`, `is_nan`,
  `is_infinite`, `float.INFINITY`, `float.NAN`.
- Encodages et hachages : `Base64`, `Hex`, `Url`, `Hash` (`sha256`, `sha1`, `md5`, `hmac_sha256`).
- Ensembles : `Set<T>` (`Set.of`, `Set<T>.new`, `add`, `contains`, `union`, `intersect`...).
- Tuples : type `(int, string)`, valeur `(1, "a")`, accès `t.0`, comparaison, clés de dictionnaire.
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
