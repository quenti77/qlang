# Stabilité et dépréciation

qlang est en 1.0. Ce fichier dit **ce sur quoi on peut s'appuyer**,
**comment un changement cassant est annoncé**, et **ce que la 1.0 promet**. Il remplace l'habitude de lire tout
le CHANGELOG pour savoir si une mise à jour va casser quelque chose.

> La classification ci-dessous est celle de l'auteur du projet : elle dit ce qu'on s'interdit de
> changer à la légère. Si une ligne vous semble fausse, ouvrez une issue.

## Les trois niveaux

| Niveau | Promesse |
| --- | --- |
| **Stable** | Ne change pas sans préavis. Un changement cassant suit le cycle de dépréciation ci-dessous (au moins deux versions mineures entre l'annonce et le retrait) et figure dans la rubrique « Cassant » du CHANGELOG. |
| **Provisoire** | Peut changer d'une version à l'autre, avec une ligne « Cassant » dans le CHANGELOG et, quand c'est possible, un avertissement avant. |
| **Expérimental** | Peut changer sans avertissement ou disparaître. À n'utiliser que si vous suivez le projet. |

Un paquet dit le niveau de ses fichiers dans son `qlang.toml` : `[stability]` avec `stable`, `provisional` et
`experimental`, chacun une liste de fichiers ou de motifs (`"web/*.q"`, `"**/*.q"` ; un nom exact l'emporte sur un
motif). `qlang doc` l'écrit sous le titre de chaque fichier de la documentation, et `qlang doc --require-levels` échoue
pour un fichier qui n'en a pas (à mettre dans la CI du paquet).

Depuis la 1.0, ce qui est *stable* est **garanti** : ce que la 1.0 promet est listé en bas de cette page.

## Le langage

| Partie | Niveau |
| --- | --- |
| Syntaxe : `let`, `fun`, `if`/`elseif`/`else`, `for`, `while`, `match`, `struct`, `enum`, `trait`, `impl`, `import`/`export`, interpolation `"{x}"` | **stable** |
| Paramètres variadiques (`fun f(...xs: int)`) et étalement `...` dans un appel, un tableau et un dictionnaire | **stable** (depuis la 1.1) |
| `BigInt` et `BigDecimal` du prélude (`+ - * ** div %`, comparaisons, arrondis, `to_json`) | provisoire (depuis la 1.2) |
| `File` et `Dir`, `fs.append`, `fs.remove`, `fs.remove_dir`, `fs.rename`, `fs.copy`, `fs.is_file` | provisoire (depuis la 1.1) |
| `Iterator`, `Iterable`, `KeyedIterable` et `Step` du prélude : `for` et `...` sur un type du programme, `map`, `filter`, `take`, `skip`, `collect` | provisoire (depuis la 1.1) |
| `Duration`, et les méthodes de `datetime` ajoutées en 1.1 (comparaisons, `diff_in_*`, `start_of_*` / `end_of_*`, `with`, `format_in`, `humanize`) | provisoire (depuis la 1.1) |
| `datetime.from_unix`, `datetime.parse_with`, `d.each` (`DatePeriod`), `Duration.parse`, `Date`, `Time`, `Zoned`, `d.in_zone`, les langues `es` `de` `it` `pt` `nl` de `format_in` et `humanize` | provisoire (depuis la 1.2) |
| `Self` dans un trait ou un `impl`, une méthode sans `self` appelée par un paramètre de type (`T.zero()`), les traits `Zero` et `One` du prélude | provisoire (depuis la 1.1) |
| Types de base, `array`, `map`, `Set`, tuples, nullables `T?`, `??`, `?`, `Result`/`Error` | **stable** |
| Vérification de types, inférence, génériques et contraintes (`T: Eq`, `Ord`) | **stable** |
| `Json.of` / `Json.to` / `Json.parse`, `string`, `int`, `float`, `bool` et leurs méthodes courantes | **stable** |
| `uuid`, `ulid` (sauf le hasard), `regex`, `Hash`, `Base64`, `Hex`, `Url` (sur du texte), `Format`, `Path` | **stable** (depuis la 1.1) |
| `datetime` (les jetons de `format` ont changé en 1.1 : une version de plus sans changement avant le stable), les variantes `*_bytes` de `Hash`, `Base64`, `Hex` et `Url` | provisoire |
| `Crypto`, `Password`, `bytes`, `time` et `random` (hors valeurs aléatoires), `fs` (lire, écrire, lister), `env`, `http` (client), `Db`, `Redis`, `Process`, `Log` | **stable** (depuis la 1.2) |
| Opérateurs surchargés (`Add`… `Neg`, `Eq`, `Ord`, `Index`, `IndexSet`, `Push`, `As<T>`), alias de type, annotations (`#[annotation]`, leur vérification à la compilation), `Any` | **stable** (depuis la 1.1) |
| `Zone`, `Gzip`, `Zip`, `Tar`, `Panic.guard` | **stable** (depuis la 1.2) |
| `Result<T, E>` qui accepte un type fils pour `T` et pour `E` (assignation, argument, retour, `?`) | **stable** (depuis la 1.3) |
| `Shared` | provisoire (récent) |
| `Worker` (tâches), `Trace` (traces OpenTelemetry), `Reflect` (constructeurs, `callable`), `extern fun` et fournisseurs, `Term` | provisoire (depuis la 1.2, avant : expérimental) |
| Mots réservés (`step`, `from`, `then`, `catch`, `public`, `end`…) | on peut en **ajouter** (annoncé en « Cassant ») |
| Texte des messages d'erreur | **non promis** |
| Codes d'erreur (`Pxxx`, `Txxx`, `Rxxx`, `Wxxx`) | un code publié garde son sens : il n'est pas renuméroté ; de nouveaux apparaissent |

Les **sorties destinées aux programmes** (la sortie de `print`, `--log-format json`, `qlang run --json`) suivent le
niveau de leur partie : une clé ajoutée n'est pas un changement cassant, une clé retirée ou renommée l'est.

## Les commandes et les fichiers

| Partie | Niveau |
| --- | --- |
| `qlang run`, `check`, `fmt` (y compris `fmt --stdin`), `test` (options existantes) | **stable** (on ajoute des options, on n'en retire pas sans cycle) |
| `qlang serve` (options, protocole de `handle`) | provisoire ; `Response.stream` / `sse` / `file`, `--max-workers` et WebSocket provisoires (depuis la 1.2, avant : expérimentaux) |
| `qlang install`, `add`, `update`, `remove`, `search`, `lsp`, `doc`, `lint` (règles `Q001` à `Q005`) | **stable** (depuis la 1.2) |
| `self-update`, `install.sh`, `install.ps1`, image Docker, releases GitHub | provisoire (pas essayés sur macOS ni Windows) |
| `qlang info`, `publish`, `verify`, `migrate`, les contraintes `version = "…"`, `signed`, `token_env`, l'index (`packages.json`), `[externs]` | provisoire (depuis la 1.2, avant : expérimental) |
| `qlang init --web`, `qlang test --coverage` / `--group`, `qlang repl`, `qlang dap` | provisoire (depuis la 1.2, avant : expérimental) |
| `qlang.toml` : `[package]` (`name`, `version`, `entry`, `main`, `qlang`) et `[dependencies]` | provisoire |
| `qlang.toml` : `[externs]` | provisoire (depuis la 1.2, avant : expérimental) |
| `qlang.lock` | **stable** (depuis la 1.2 ; format lisible, ne pas l'éditer) |

## `qlang-std` et les outils

| Partie | Niveau |
| --- | --- |
| `csv.q`, `toml.q`, `template.q`, `jwt.q`, `i18n.q`, `cli.q`, `queue.q`, `schedule.q`, `mail.q`, `notify.q`, `s3.q` | **stable** (std 1.0) |
| `web/app.q`, `web/session.q`, `web/validate.q`, `web/migrate.q`, `web/query.q`, `web/cache.q`, `web/middleware.q`, `web/health.q`, `web/limit.q`, `web/auth.q`, `web/upload.q`, `web/testing.q` | **stable** (std 1.0) |
| `yaml.q`, `xml.q` (des sous-ensembles des formats) et le reste de `web/` (`container`, `openapi`, `ws`, `relations`, `policy`, `resource`, `seed`, `queue_admin`, `metrics`, `events`, `commands`, `locale`, `refresh`, `proxy`, `accounts`, `admin`) | provisoire |
| `kit` (commandes, options, codes de sortie, générateurs `make`) | **stable** (kit 1.0) ; réglages, invites, lanceur de tests et assistant : provisoires ; écrans du terminal, tableau de bord, navigateur de base de données : **expérimentaux** |
| `qlang-test` (paquet) | **expérimental** (0.x) |

`qlang-std` et `kit` ont leur propre version (1.0 chacun) et leurs niveaux sont dans leur `qlang.toml`. Un changement
cassant d'un module *provisoire* est écrit dans le CHANGELOG du dépôt.

## Comment on déprécie

Un cycle fixe : **annoncer, avertir, retirer**.

1. **Annoncer** : la version qui déprécie le dit dans le CHANGELOG (rubrique « Déprécié »), et la fonction reçoit
   `#[deprecated("utilise X ; sera retiré en 0.N")]`.
2. **Avertir** : tout appel à ce qui est déprécié produit l'avertissement `W001` à l'endroit de l'appel, dans
   `qlang check`, `qlang run`, `qlang lsp` (donc dans l'éditeur) et le protocole JSON. Ce n'est qu'un avertissement :
   le programme tourne. `--deny-warnings` (sur `check`, `run`, `test`) en fait une erreur : à mettre dans la CI d'un
   projet pour ne pas se laisser surprendre.
3. **Retirer** : au plus tôt **deux versions mineures** après l'annonce (déprécié en 0.69 : retiré en 0.71 ou plus
   tard), avec une ligne dans la rubrique « Cassant » du CHANGELOG.

```
#[deprecated("utilise Log.warning ; Log.warn sera retiré en 0.72")]
public fun warn(...)
```

Un module qui appelle sa propre fonction dépréciée n'est pas averti : l'avertissement vise les appelants.

**Premier cas** : `Log.warn` (un autre nom de `Log.warning`) a été déprécié en 0.69 et retiré en 0.83. `qlang migrate` réécrit encore les appels qui restent.

## La rubrique « Cassant »

Chaque version du CHANGELOG qui casse quelque chose a une rubrique **Cassant** en tête, avec *avant*, *après* et
*comment s'adapter*. Les changements cassants des versions récentes sont repris dans [migration.md](migration.md).
Les versions d'avant cette règle ne marquaient pas toujours leurs changements cassants : leur liste complète est le
CHANGELOG lui-même.

## Ce que la 1.0 promet

La 1.0 fige un **sous-ensemble** dont la définition exécutable est la suite de conformité
([qlang-conformance](https://github.com/quenti77/qlang-conformance)) : **une ligne n'est promise que quand des cas de
conformance la couvrent**, et chaque ligne dit lesquels (les dossiers de `cases/`).

| Promesse | Cas de conformance |
| --- | --- |
| Variables, constantes, opérateurs, nombres, commentaires, expressions sur plusieurs lignes | `variables_and_const`, `arithmetic`, `comparisons_and_logic`, `number_formatting`, `number_methods_and_constants`, `div_and_mod_assignment`, `comments`, `multiline_expressions`, `conversions` |
| `if` / `elseif` / `else`, `while`, `for` (intervalles, tableaux, dictionnaires), `break`, `continue`, `match` | `control_flow`, `for_with_two_variables`, `match_expressions` |
| Fonctions, fermetures, paramètres nommés, génériques et contraintes | `functions_and_closures`, `closures`, `named_arguments`, `generics`, `compile_generics`, `compile_functions` |
| Chaînes (interpolation, méthodes) | `strings`, `string_library`, `string_prefix_suffix_and_case_conversions` |
| Tableaux et leurs méthodes | `arrays`, `array_higher_order_functions`, `array_sort_min_max_sum_slice` |
| Dictionnaires, ensembles, tuples | `maps`, `maps_basics`, `maps_methods`, `maps_are_shared_and_typed`, `map_equality`, `sets`, `tuples` |
| Valeurs optionnelles (`T?`, `??`, rétrécissement) | `nullables`, `optionals`, `narrowing_follows_assignments`, `compile_nullable_flow` |
| `Result`, `Error` et l'opérateur `?` | `results` |
| `Result<T, E>` qui accepte un type fils pour `T` et pour `E`, `?` qui propage une erreur fille (promis depuis la 1.3) | `result_covariance`, `compile_result_covariance` |
| Structures, méthodes, traits, énumérations, héritage | `structs`, `structs_and_methods`, `traits`, `traits_as_types`, `traits_and_operators`, `enums`, `compile_structs`, `compile_traits_and_ops`, `compile_inheritance` |
| `Json.parse`, `Json.of`, `Json.to` | `json` |
| Modules : `import` / `export`, alias | `modules` |
| Paramètres variadiques `...xs` et étalement `...` (promis depuis la 1.1) | `variadics`, `compile_variadics`, `runtime_variadics` |
| Opérateurs surchargés, alias de type, annotations, `Any` (promis depuis la 1.1) | `operator_overloading`, `type_aliases`, `compile_type_aliases`, `annotations`, `compile_annotations`, `any_values`, `compile_any` |
| Les petits modules de la bibliothèque (promis depuis la 1.1) : `Base64`, `Hex`, `Url`, `Hash`, `regex`, `Path`, `Format`, `uuid`, `ulid` | `encodings`, `hashes`, `regex`, `path`, `format`, `identifiers` |
| `Zone`, `Gzip`, `Zip`, `Tar`, `Panic.guard` (promis depuis la 1.2) | `zones`, `archives`, `panic_guard` |
| `Crypto`, `Password`, `bytes`, `time`, `random`, `fs` (lire, écrire, lister), `env`, `Process`, `Log`, `http` (client), `Db`, `Redis` (promis depuis la 1.2) | **pas dans la suite de conformité** (ils ont besoin du disque, du réseau ou d'un service) : tests d'intégration `fs_env` et `root_fs` (`fs`, `env`, `--root`), `crypto` et `password`, `bytes`, `stdlib_system` (`time`, `random`), `process`, `log_context`, `http` et `http_all`, `db` et `mysql`, `redis` et `redis_absent` |
| `qlang install`, `add`, `update`, `remove`, `search`, `lsp`, `doc`, `lint` (promis depuis la 1.2) | tests d'intégration `packages`, `git_packages`, `registry` et `index_file` (paquets), `lsp`, `docgen`, `doc_levels` et `doc_prelude` (`doc`), `lint` |
| Les **codes** des erreurs de compilation et d'exécution les plus courantes | les dossiers `compile_*`, `runtime_errors`, `runtime_failures`, `arithmetic_errors` |
| Les commandes `run`, `check`, `fmt`, `test` (leurs options actuelles) | **pas dans la suite de conformité** (qui ne couvre que le langage) : tests d'intégration de `qlang` (`crates/qlang-cli/tests`) : `cli_options` (`--max-steps`, `--seed`), `limits` (`--max-memory`), `testrun` (`--filter`), `coverage` (`--group`, `--skip-group`, `--coverage`, `--coverage-lcov`), `deprecated` (`--deny-warnings`), `fmt` et `fmt_config` (`--check`, `--stdin`), `root_fs` et `imports` (`--root`), `providers` (`--extern`, `--trust-externs`), `process` (`--allow-process`) |
| Le format de `qlang.toml` (`[package]`, `[dependencies]`, `[stability]`) et de `qlang.lock` | tests d'intégration `manifest_format` (le texte exact du verrou, un verrou écrit à la main qui est obéi, chaque clé acceptée, `qlang = "…"` qui arrête un projet trop exigeant) |

Hors de la promesse : la réflexion (`Reflect`), `extern`, le serveur HTTP et la bibliothèque web, `Term`, les tâches et
traces, et tout ce qui est marqué provisoire plus haut. Le tag
`v1.0.0` du dépôt de conformité fixe ce que ces lignes veulent dire (les versions d'avant avaient chacune leur tag, `v0.X.Y` = la suite de qlang `0.X.Y`) ; un cas ne change ensuite que si le `CHANGELOG` du dépôt de conformité le dit.
