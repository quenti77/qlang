# Stabilité et dépréciation

qlang est en 0.x : une version sort presque chaque jour. Ce fichier dit **ce sur quoi on peut s'appuyer**,
**comment un changement cassant est annoncé**, et **ce que la 1.0 promettra**. Il remplace l'habitude de lire tout
le CHANGELOG pour savoir si une mise à jour va casser quelque chose.

> La classification ci-dessous est une **proposition** de l'auteur du projet : elle dit ce qu'on s'interdit de
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

Aujourd'hui rien n'est « garanti 1.0 » : *stable* veut dire « nous le traiterons comme si la 1.0 était là ».

## Le langage

| Partie | Niveau |
| --- | --- |
| Syntaxe : `let`, `fun`, `if`/`elseif`/`else`, `for`, `while`, `match`, `struct`, `enum`, `trait`, `impl`, `import`/`export`, interpolation `"{x}"` | **stable** |
| Types de base, `array`, `map`, `Set`, tuples, nullables `T?`, `??`, `?`, `Result`/`Error` | **stable** |
| Vérification de types, inférence, génériques et contraintes (`T: Eq`, `Ord`) | **stable** |
| `Json.of` / `Json.to` / `Json.parse`, `string`, `int`, `float`, `bool` et leurs méthodes courantes | **stable** |
| `datetime`, `uuid`, `ulid`, `regex`, `Hash`, `Base64`, `Hex`, `Url`, `Crypto`, `Password`, `Format`, `Path` | provisoire |
| `fs`, `env`, `http`, `Db`, `Redis`, `Process`, `Log` | provisoire |
| Opérateurs surchargés, alias de type, annotations (`#[annotation]`), `Any` | provisoire |
| `Zone`, `Gzip`, `Zip`, `Tar`, `Panic.guard`, `Shared` | provisoire (récents) |
| `Worker` (tâches), `Trace` (traces OpenTelemetry) | **expérimental** |
| `Reflect` (constructeurs, `callable`), `extern fun` et fournisseurs, `Term` | **expérimental** |
| Mots réservés (`step`, `from`, `then`, `catch`, `public`, `end`…) | on peut en **ajouter** (annoncé en « Cassant ») |
| Texte des messages d'erreur | **non promis** |
| Codes d'erreur (`Pxxx`, `Txxx`, `Rxxx`, `Wxxx`) | un code publié garde son sens : il n'est pas renuméroté ; de nouveaux apparaissent |

Les **sorties destinées aux programmes** (la sortie de `print`, `--log-format json`, `qlang run --json`) suivent le
niveau de leur partie : une clé ajoutée n'est pas un changement cassant, une clé retirée ou renommée l'est.

## Les commandes et les fichiers

| Partie | Niveau |
| --- | --- |
| `qlang run`, `check`, `fmt`, `test` (options existantes) | **stable** (on ajoute des options, on n'en retire pas sans cycle) |
| `qlang serve` (options, protocole de `handle`) | provisoire ; `Response.stream` / `sse` / `file` provisoires ; WebSocket **expérimental** |
| `qlang install`, `add`, `update`, `remove`, `search`, `lsp`, `doc`, `self-update` | provisoire |
| `qlang info`, `publish`, `verify`, `migrate`, les contraintes `version = "…"`, `signed`, `token_env`, l'index (`packages.json`) | **expérimental** |
| `qlang init --web`, `qlang test --coverage` / `--group`, `qlang lint`, `qlang repl`, `qlang dap`, `qlang fmt --stdin` | **expérimental** |
| `qlang.toml` : `[package]` (`name`, `version`, `entry`, `main`, `qlang`) et `[dependencies]` | provisoire |
| `qlang.toml` : `[externs]` | **expérimental** |
| `qlang.lock` | provisoire (format lisible, ne pas l'éditer) |
| `install.sh`, `install.ps1`, image Docker, releases GitHub | provisoire |

## `qlang-std` et les outils

| Partie | Niveau |
| --- | --- |
| `csv.q`, `toml.q`, `yaml.q`, `xml.q`, `template.q`, `jwt.q`, `i18n.q`, `cli.q` | provisoire |
| `web/app.q`, `web/session.q`, `web/validate.q`, `web/migrate.q`, `web/query.q`, `web/cache.q`, `web/middleware.q`, `web/health.q`, `web/limit.q`, `web/auth.q`, `web/upload.q`, `web/testing.q` | provisoire |
| `queue.q`, `schedule.q`, `mail.q`, `notify.q`, `s3.q` | provisoire |
| `web/container.q` (injection), `web/openapi.q`, `web/ws.q`, `web/relations.q`, `web/policy.q`, `web/resource.q`, `web/seed.q`, `web/queue_admin.q`, `web/metrics.q`, `web/events.q`, `web/commands.q`, `web/locale.q`, `web/refresh.q`, `web/proxy.q`, `web/accounts.q` | **expérimental** |
| `qlang-test` (paquet), `kit` | **expérimental** (0.x) |

Tant que la std est en 0.x, aucun de ses modules n'est *stable* ; un changement cassant d'un module *provisoire* est
écrit dans le CHANGELOG de `qlang-std`.

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

**Premier cas** : `Log.warn` (un autre nom de `Log.warning`) est déprécié en 0.69, retiré au plus tôt en 0.72.

## La rubrique « Cassant »

Chaque version du CHANGELOG qui casse quelque chose a une rubrique **Cassant** en tête, avec *avant*, *après* et
*comment s'adapter*. Les changements cassants des versions récentes sont repris dans [migration.md](migration.md).
Les versions d'avant cette règle ne marquaient pas toujours leurs changements cassants : leur liste complète est le
CHANGELOG lui-même.

## Ce que la 1.0 promettra (proposition)

La 1.0 figera un **sous-ensemble** dont la définition exécutable est la suite de conformité
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
| Structures, méthodes, traits, énumérations, héritage | `structs`, `structs_and_methods`, `traits`, `traits_as_types`, `traits_and_operators`, `enums`, `compile_structs`, `compile_traits_and_ops`, `compile_inheritance` |
| `Json.parse`, `Json.of`, `Json.to` | `json` |
| Modules : `import` / `export`, alias | `modules` |
| Les **codes** des erreurs de compilation et d'exécution les plus courantes | les dossiers `compile_*`, `runtime_errors`, `runtime_failures`, `arithmetic_errors` |
| Les commandes `run`, `check`, `fmt`, `test` (leurs options actuelles) | **pas dans la suite de conformité** (qui ne couvre que le langage) : tests d'intégration de `qlang` (`crates/qlang-cli/tests`) |

Hors de la promesse : la réflexion (`Reflect`), `extern`, le serveur HTTP et la bibliothèque web, `Term`, les tâches et
traces, et tout ce qui est marqué provisoire ou expérimental plus haut. Quand la 1.0 sortira, le tag
`v1.0.0` du dépôt de conformité fixera ce que ces lignes veulent dire ; avant, chaque version de qlang a son tag
(`v0.X.Y` = la suite de qlang `0.X.Y`) et un cas ne change que si le `CHANGELOG` du dépôt de conformité le dit.
