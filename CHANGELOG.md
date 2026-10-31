# Journal des changements

> À partir de la 0.69, chaque version qui casse quelque chose a une rubrique **Cassant** (avant, après, comment s'adapter), reprise dans [docs/migration.md](docs/migration.md), et ce qui va disparaître est d'abord **déprécié** (rubrique « Déprécié »). Les niveaux de stabilité sont dans [docs/stability.md](docs/stability.md).

## 0.79.0

- **Contraintes de version** : `version = "^1.2"` (aussi `~1.2.3`, `>=1.2, <2`, `=1.2.3`, `*`) sur une dépendance git choisit la plus haute étiquette `vX.Y.Z` qui la satisfait ; `qlang.lock` garde la contrainte et le commit, `qlang update` choisit de nouveau. Si deux paquets demandent des intervalles différents du même dépôt, une étiquette qui suit les deux est cherchée, sinon l'erreur dit qui demande quoi. `qlang add std` pose maintenant `version = "^0.19"` (l'intervalle de la dernière version) au lieu d'une étiquette exacte.
- **`qlang info nom`**, `qlang search` (licence) ; l'index peut dire `license`, `keywords`, `yanked` (versions que plus aucune résolution ne choisit) et `removed` (le paquet disparaît de la recherche, `qlang add` le refuse).
- **`qlang publish`** : contrôle qu'un paquet peut être listé (nom, version, description, licence, README, compile, arbre propre, étiquette sur `HEAD`, `origin` en https) et écrit l'entrée à proposer.
- **`qlang verify`** : les paquets installés sont à leur commit de `qlang.lock`, sans fichier modifié. **`signed = true`** : le commit doit porter une signature que git vérifie. **`token_env = "VAR"`** : jeton d'un dépôt https privé, donné à git pour cette seule dépendance.
- [docs/registry.md](docs/registry.md) : utiliser, publier, ce que l'index promet et comment un retrait se décide.

## 0.78.0

- **`qlang migrate`** : réécrit les renommages mécaniques d'un changement cassant (aujourd'hui `Log.warn` → `Log.warning`), par jetons (jamais dans une chaîne ni un commentaire), en vérifiant que seuls les noms ont changé ; `--check`, `--rules`.
- **Niveaux de stabilité dans les documents** : `[stability]` (`stable`, `provisional`, `experimental` : des fichiers ou des motifs) dans `qlang.toml` ; `qlang doc` écrit le niveau sous le titre de chaque fichier, et `qlang doc --require-levels` échoue pour un fichier qui n'en a pas. Chaque section du prélude de `docs/language.md` porte maintenant son niveau.
- **Ce que la 1.0 figera** : `docs/stability.md` donne le sous-ensemble ligne par ligne, avec les dossiers du dépôt de conformité qui couvrent chaque ligne.

## 0.77.0

- **Traces distribuées (OpenTelemetry)** : `qlang serve --otlp-endpoint URL` (ou `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME` / `--service-name`) envoie à un collecteur OTLP/HTTP un span par requête (enfant du `traceparent` reçu, ou racine), un span par appel `http.send` du programme (qui part avec `traceparent`) et ceux de `Trace.span(nom, f)` ; chaque ligne de log de la requête porte `trace_id` et `span_id`. `Trace.attr`, `Trace.id()` et `Trace.traceparent()`. Sans collecteur, un `traceparent` reçu est quand même transmis. Voir [docs/language.md](docs/language.md).

## 0.76.0

- **Fuzzing guidé par la couverture** (`cargo-fuzz`, dossier `fuzz/`) : huit cibles (lexer, parser, compilateur, formateur, interpréteur sous petites limites, requêtes HTTP de `qlang serve`, chemins de `--static`, trames WebSocket) et un job `Fuzz` du workflow du même nom. Voir [docs/security.md](docs/security.md#fuzzing).
- `qlang-cli` est maintenant une bibliothèque (`qlang_cli`) et un petit `main` : les cibles de fuzzing atteignent ainsi le lecteur de requêtes du serveur. Aucun changement pour l'utilisateur.

## 0.75.0

- **Distribution** : la release contient des paquets `.deb` (amd64, arm64) et `.rpm` (x86_64, aarch64) du programme statique, construits et essayés par le workflow (le `.deb` est installé sur le runner), et les trois manifestes **winget** (`quenti77.qlang*.yaml`) à côté de `qlang.rb`, `qlang.json` et `PKGBUILD`. Publier dans un dépôt apt/yum, un tap, un bucket, AUR ou `winget-pkgs` reste à faire par le propriétaire (voir docs/release.md).

## 0.74.0

- **Débogueur : `qlang dap`** (Debug Adapter Protocol, entrée et sortie standard). Points d'arrêt par fichier et ligne (aussi dans les fichiers importés), pas à pas (`next`, `stepIn`, `stepOut`), pause, pile d'appels, variables dépliables (tableaux, dictionnaires, structures, énumérations, tuples) et `evaluate` d'un nom de variable. Le programme tourne dans l'interpréteur « simple » ; son `print` arrive dans la console de l'éditeur. L'extension VS Code déclare le débogueur (F5).
- Dans `qlang-core` : `qlang_core::debug` (un crochet appelé avant chaque instruction, un instantané fait de texte) ; erreur `R907` quand la session de débogage se termine.

## 0.73.0

- **`qlang lint`** : variables inutilisées (`L001`), fonctions privées que rien n'appelle (`L002`), noms importés jamais écrits (`L003`), code après `return`/`break`/`continue` (`L004`), fonctions avec trop de chemins (`L005`, `--max-complexity N`, 15 par défaut). Code de sortie 1 s'il y a quelque chose. `#[used]` (annotation prédéfinie, sans effet à l'exécution) dit qu'une fonction sert par un chemin que le lint ne voit pas.
- **`qlang repl`** : session interactive avec historique (`~/.qlang_history`), complétion (mots-clés, noms visibles, membres après `Type.`) et entrées sur plusieurs lignes. Les définitions restent ; une expression est exécutée et sa valeur affichée ; l'initialisation d'un `let` est rejouée à chaque entrée.
- **`qlang fmt --stdin`** (programme sur l'entrée, mis en forme sur la sortie) et **`[fmt] exclude = [...]`** dans `qlang.toml`.
- **Autres éditeurs** : [docs/editors.md](docs/editors.md) (Neovim, Helix, Emacs, Sublime Text, JetBrains, Zed) autour de `qlang lsp`.

## 0.72.0

- **Couverture de code** : `qlang test --coverage` dit, par fichier source, quelles lignes portant une instruction ont été exécutées par les tests (et lesquelles non) ; `--coverage-lcov FICHIER` écrit un fichier LCOV. Couverture de lignes, sans l'optimisation à registres pour que chaque instruction soit comptée ; rien ne change (ni ne coûte) sans l'option.
- **`qlang test --group TAGS` et `--skip-group TAGS`** : posent `QLANG_TEST_GROUP` / `QLANG_TEST_SKIP_GROUP`, lus par `qlang-test` (`group(tag, f)`) pour ne lancer que certains groupes.

## 0.71.0

- **Tâches : `Worker`**. `Worker.spawn("fichier.q", entrée)` lance le `fun run()` d'un fichier dans un interpréteur à part, en même temps que le programme ; rien n'est partagé, on s'envoie des `Json` (`send`/`recv` côté programme, `Worker.emit`/`Worker.next`/`Worker.input` côté tâche). `wait(ms)` rend le résultat (`Worker.finish`) avec un délai, ou l'erreur de la tâche ; `cancel()` demande l'arrêt et **arrête** la tâche (`R906`) au prochain contrôle, même dans une boucle qui ne regarde pas `Worker.cancelled()` (vrai aussi quand le serveur s'arrête). 64 tâches au plus, permissions du programme, dans `--root`.

## 0.70.0

- **Délai par requête** : `qlang serve --request-timeout S` (30 par défaut, `0` pour aucun). Une requête qui dure plus longtemps est arrêtée par `R905` et le worker sert la suivante. L'horloge est lue tous les quelques milliers de pas (comme la mémoire) : un programme qui attend dans `time.sleep` ou une requête SQL lente s'arrête quand il reprend la main.
- **File d'attente bornée** : `--max-queue N`. Quand N requêtes attendent déjà un worker, la suivante reçoit `503` avec `Retry-After: 1` tout de suite. Par défaut (`0`) rien ne change : `--max-connections` borne.
- **`Shared`** : des entiers que tous les workers du processus voient et changent de façon atomique (`add`, `get`, `set`, `compare_set`), pour des compteurs exacts sans Redis.

## 0.69.0

- **Dépréciation** : `#[deprecated("utilise X")]` sur une fonction ou une méthode. Un appel venant d'un autre module produit l'avertissement `W001` à l'endroit de l'appel (dans `qlang check`, `run`, le serveur de langage et le protocole JSON), sans arrêter le programme. Un `#[deprecated]` sans texte est l'erreur `T292`.
- **`--deny-warnings`** pour `qlang check`, `run` et `test` : un avertissement fait échouer la commande (pour la CI d'un projet).
- **Déprécié** : `Log.warn`, un autre nom de `Log.warning`. Retrait au plus tôt en 0.72.
- **Documents** : [docs/stability.md](docs/stability.md) (trois niveaux, classement du langage, des commandes, de `qlang-std`, cycle de dépréciation, ce que la 1.0 promettra) et [docs/migration.md](docs/migration.md).

## 0.68.0

- **`Panic.guard(f)`** : lance une fonction sans qu'un échec (`panic`, `assert`, `unwrap` d'une erreur, index hors limites) arrête le programme. Elle rend `none` si `f` a fini, sinon un `Caught` avec `code`, `message` et `places` (`fichier:ligne`, du plus proche au plus lointain). Les limites de pas et de mémoire ne sont pas attrapées. C'est ce qu'il fallait à l'interpréteur pour un lanceur de tests comme `qlang-test` (voir #129).

## 0.67.1

- **Correction de `qlang self-update`** : avec les archives réelles des versions 0.64.0 à 0.67.0, il remplaçait le programme par un **fichier vide** (l'entrée du dossier `qlang/` de l'archive a le même nom de fichier que le programme). Il ne prend plus que les vrais fichiers, refuse un programme de moins de 1 Ko, et lance le programme téléchargé (`--version`) avant de remplacer l'ancien : s'il ne tourne pas, rien n'est changé. **Si un `self-update` a laissé un `qlang` vide, relancer `install.sh` (ou `install.ps1`) ; depuis 0.64–0.67, le plus simple pour passer à 0.67.1 est aussi `install.sh`.**
- Le test de `self-update` utilise maintenant une archive faite comme les vraies (dossier d'abord).

## 0.67.0

- **Compression et archives en mémoire** : `Gzip` (`compress`, `compress_level`, `decompress`, `decompress_max`), `Zip` et `Tar` (`read`, `read_max`, `write`, et `write_gz` pour `.tar.gz`) avec `ArchiveEntry { name, data }`. Un nom qui sort du dossier (`../x`, chemin absolu) est refusé, et le contenu décompressé est borné à 256 Mo par défaut.
- **Fuseaux horaires** : `Zone.of("Europe/Paris")` avec `offset_seconds`, `abbreviation`, `wall`, `format` et `local` (heure d'été comprise ; une heure qui n'existe pas se lit avec le décalage d'avant, une heure qui existe deux fois est la première). La base IANA est dans le programme `qlang` (`T250` dans le navigateur).
- `qlang-std` 0.17.0 : `yaml.q` et `xml.q`.

## 0.66.0

- **Pool de connexions** : `qlang serve --db-pool N` partage au plus N connexions réelles à PostgreSQL ou MySQL entre tous les workers. Un `Db` en emprunte une pour chaque requête et la rend, et garde la même pendant une transaction ; si les N sont prises, la requête attend 30 s au plus. Mesuré sur PostgreSQL : 6 transactions simultanées avec 2 connexions au lieu de 6. SQLite n'est pas concerné.
- Les migrations (statut, retour en arrière, `db:migrate`, `db:rollback`, `db:status`) sont dans `qlang-std` et leur génération dans `kit make migration`.

## 0.65.0

- **Envoyer un fichier** : `Response.file(chemin)` et `Response.download(chemin, nom)` envoient un fichier du disque par morceaux (jamais en entier en mémoire), avec `Content-Length`, le type d'après l'extension, `404` s'il manque et `403` hors de `--root`. Le worker est libre pendant l'envoi.
- **HTTPS** : documenté comme un proxy inverse devant `qlang serve` (`docs/security.md`), avec les en-têtes à faire poser par le proxy. `qlang-std` 0.16.0 : `web/proxy.q`, sessions côté serveur (base de données, Redis) et `cache.DbStore`.

## 0.64.0

- **`qlang self-update`** : remplace le programme par la dernière version (ou `--version X.Y.Z`), après avoir vérifié la somme de contrôle de `SHA256SUMS` ; `--check` dit seulement s'il y a plus récent. Linux, macOS et Windows.
- **Version de qlang par projet** : `qlang = "0.64"` dans le `[package]` de `qlang.toml` (minimum ; `">=0.64.1"` pareil, `"=0.64.2"` exactement). Un qlang plus ancien refuse de lancer le projet et dit de lancer `qlang self-update`. `qlang init` l'écrit.
- **Windows** : programme précompilé (`qlang-x86_64-pc-windows-msvc.zip`) dans les releases et `install.ps1`.
- **Docker** : image officielle `ghcr.io/quenti77/qlang` (`FROM scratch`, utilisateur non root, amd64 et arm64).
- **Gestionnaires de paquets** : chaque release publie `qlang.rb` (Homebrew), `qlang.json` (Scoop) et `PKGBUILD` (AUR), remplis avec les sommes de contrôle.

## 0.63.0

- **`Term`** : le terminal pour écrire une interface en terminal. `Term.session(f)` (mode brut, écran alternatif, curseur caché, tout remis en fin), `Term.event(attente_ms)` (touches avec `ctrl`/`alt`/`shift`, souris, collage, redimensionnement, focus, délai), `Term.size()`, `Term.colors()` (`NO_COLOR` respecté), `Term.write`, `Term.frame` (mise à jour synchronisée), `Term.clear`, `Term.title`, `Term.mouse`, `Term.paste`, `Term.focus`, `Term.alt_screen`, `Term.hide_cursor`, `Term.raw`. Linux, macOS et Windows. Hors terminal, seuls la taille, les couleurs et `write` répondent ; hors de la commande `qlang`, c'est une erreur de compilation (`T250`). Le terminal est remis en état même si le programme plante, avant l'affichage de l'erreur.
- **Échappement `\e`** dans les chaînes : le caractère ESC (`"\e[1mgras\e[0m"`).
## 0.62.0

- **`Path`** : `join`, `join_all`, `normalize`, `base`, `dir`, `ext`, `stem`, `with_ext`, `is_absolute` et `matches` (le motif de `fs.glob`), sur des chemins écrits en texte avec `/`.
- **Parcourir le disque** : `fs.list(dossier)`, `fs.is_dir(chemin)` et `fs.glob("src/**/*.q")`.
- **Lire un gros fichier ligne à ligne** : `fs.lines(chemin)` (`next()`, `close()`), `fs.each_line(chemin, f)` et `fs.read_lines(chemin)` ; une ligne de 16 Mo au plus, 64 fichiers ouverts au plus, tout dans le dossier de `--root`.
- **`Format`** : `number(n, décimales)`, `number_with`, `int`, `int_with`, `bytes` (`1.5 KB`) et `sprintf("%-8s|%05.1f", [Any.of("ab"), Any.of(3.14159)])`.
- **Transactions imbriquées** : un `begin()` dans un `begin()` est un `SAVEPOINT` (SQLite, PostgreSQL, MySQL), et un `rollback()` intérieur n'annule que le bloc intérieur. Nouveau `db.transaction(fun() -> Result<T, Error> ... end)`.
- `qlang-std` 0.14.0 : `create_many` des usines et les seeders s'appuient sur ces transactions imbriquées.

## 0.61.0

- **Page d'erreur de `--dev`** : le message et le code, la ligne fautive avec celles autour, la pile, la requête (sans cookies ni jetons) et le SQL exécuté avec ses durées ; en HTML pour un navigateur, en JSON (`Accept: application/json`) ou en texte. `--dev` est refusé hors `localhost` sauf `--dev-public`.
- **Journal SQL en développement** : une requête répétée au moins trois fois est signalée (`N+1?`) dans la sortie d'erreur, et une réponse réussie porte `Server-Timing: db;dur=...`.
- **Connexion de base coupée** : `Db` rouvre toute seule la connexion que le serveur a fermée (redémarrage, inactivité) et rejoue l'instruction une fois ; dans une transaction, l'erreur le dit. Plus besoin de redémarrer `qlang serve` après un redémarrage de la base.
- **Erreurs en production** : avec `--log-format json`, une erreur du programme est une ligne JSON avec sa pile (`context.stack`), son code et la requête, sans le code source.
- `qlang-std` 0.13.0 : `web/seed.q` (`Factory` avec `field`, `state`, `make`, `create`, `create_many` ; `Seeder`, `run_seeders`) et la commande `db:seed`.

## 0.60.0

- **`qlang init --web [dossier]`** : un projet web prêt à lancer (`main.q`, `app.q`, `providers.q`, `controllers.q`, `app_test.q`, `qlang.toml`, `.gitignore`), qui installe le paquet `std` (`--no-install` pour ne pas le télécharger). `qlang serve main.q --watch --dev`, `qlang test`.
- **`Reflect.declared(module)`** : les fonctions puis les structures d'un module, en `Callable` (`callables` puis `constructors`). **`Reflect.bound_callables_of(valeur)`** : les méthodes publiques de la structure qu'un `Any` contient, liées à elle, sans connaître son type. C'est ce qui permet à `App.boot` de `qlang-std` de trouver les routes et les contrôleurs d'un module.
- **`fs.create_dir(dossier)`** et **`fs.exists(chemin)`**, qui respectent `--root` (un `..` après un dossier manquant est refusé).
- **Le plus petit entier** s'écrit en littéral : `-9223372036854775808` (aussi dans un `case`). Les chiffres sans le `-` restent une erreur (`L005`).
- **Super-traits** : `trait B extends A, C`. Qui implémente `B` doit implémenter `A` et `C` (`T136`) ; une valeur de type `B`, ou un `T: B`, a leurs méthodes et passe là où un `A` est attendu.
- **`qlang doc`** met en forme `@param nom texte` et `@return texte` des commentaires de documentation.
- `qlang-std` 0.12.0 : `App.boot(Reflect.declared(a).concat(Reflect.declared(b)))`, `discover`, `declare` (un contrôleur déclaré mais jamais enregistré est signalé par `validate()`) ; `web/upload.q` (`receive` avec `Rules` : taille, nombre, type lu dans le contenu ; `Upload.save`) ; sessions qui expirent (`Options`, `sessions_using`), `regenerate()`, `flash_input` / `old`, `csrf_field`, `csrf_protect_except`, `protect`, le jeton CSRF d'un formulaire avec fichiers ; `Client.post_multipart`.
- La section « Points encore ouverts » de la référence ne cite plus ni le plus petit entier, ni les super-traits, ni le format des commentaires de documentation.

## 0.59.0

- **Serveur HTTP** : les bornes se règlent. `--max-connections N` (64), `--max-body TAILLE` (1 Mo), `--max-headers TAILLE` (64 Ko, `431` au-delà), `--read-timeout S` (20 s pour envoyer toute la requête) et `--idle-timeout S` (5 s de silence sur une connexion persistante), pour `qlang serve app.q` comme pour `qlang serve`. Un client trop lent reçoit maintenant `408` au lieu d'une connexion fermée sans réponse. Les en-têtes trop gros donnent `431` (c'était `400`).
- **Réponses 40 ms plus vite** : l'en-tête et le corps d'une réponse partent en une seule écriture (et `TCP_NODELAY` est mis sur les connexions). Avant, sur une connexion persistante, l'algorithme de Nagle et l'accusé de réception différé du client retardaient chaque réponse de 40 ms (24 requêtes par seconde et par connexion au banc, contre plus de 10 000 maintenant).
- **`--no-access-log`** : n'écrit plus la ligne `GET /chemin -> 200 (1 ms)` de chaque requête.
- **Banc de charge HTTP** : `python3 benches/http_load.py` (requêtes par seconde, latence p50/p95/p99, erreurs, mémoire du serveur dans la durée), le programme `benches/http/app.q`, le workflow `Bench` (sur demande et chaque lundi, JSON en artefact), et une section « Le serveur HTTP » dans `docs/performance.md`, dont l'introduction ne dit plus « pas de bytecode ».
- **Sécurité** : `SECURITY.md` (comment signaler une faille, délais visés, ce qui en est une), un test de mutation du compilateur et du formateur sur les programmes de `examples/` (graine fixe en CI, balayage long chaque dimanche par le workflow `Fuzz` : 60 000 programmes abîmés sans plantage au premier essai), et un test de requêtes HTTP abîmées.
- **Correctif de sécurité** : `Crypto.verify` avec un secret HMAC vide (`HS256`, `HS384`, `HS512`) est maintenant une erreur, comme `Crypto.sign` : un secret lu dans une variable d'environnement absente aurait laissé fabriquer un jeton valide. `docs/security.md` garde le résultat des revues (crypto, JWT, permissions) ; un test vérifie que `--root` retient `fs` (`..`, chemin absolu, lien symbolique).
- **Documentation** : la section 21 de la référence cite le mail et S3 parmi les paquets de `qlang-std`.

## 0.58.0

- **`Reflect.constructors(module)`** : les structures d'un module, chacune en `Callable` qui la construit à partir de ses champs (dans l'ordre, les champs privés aussi) ; `params`, `return_type` et `annotations` décrivent la structure. C'est ce qui permet à un conteneur d'injection de construire un service sans fonction fournisseur.
- `qlang-std` 0.11.0 : `#[di.Service]` sur une structure (ses champs sont ses dépendances), `app.services(Reflect.constructors(module))`, `app.bind<Interface, Implémentation>()`, `di.services_of(...)`.

## 0.57.1

- **Documentation** : la référence explique la sortie et la sortie d'erreur (`print` et `write` portent ce que le programme produit, `eprint`, `ewrite` et `Log` portent le journal, les avertissements et la progression ; la sortie d'erreur n'est pas réservée aux erreurs).

## 0.57.0

- **Valeurs par défaut des paramètres** : `fun connect(host: string, port: int = 5432, name: string? = none)`. Un appel peut omettre les paramètres qui en ont une (ou les sauter en nommant les suivants). La valeur est un littéral (nombre, texte, `true`, `false`, `none`, `[]`, `{}`). Un paramètre de type générique que seul un paramètre omis mentionne n'a rien à déduire. Le serveur de langage affiche `port: int = 5432` et laisse ces paramètres hors du modèle d'insertion.
- **`Log` comme PSR-3** : `Log.info(message, contexte)`, le contexte (facultatif) étant n'importe quelle valeur, écrite en JSON (`context={...}` en texte, un objet `context` en `--log-format json`). Niveaux : `debug`, `info`, `notice`, `warning` (`warn` en est un autre nom), `error`, `critical`, `alert`, `emergency`, et `Log.log(niveau, message, contexte)`. `Log.write` reste. Changement : `Log.warn` écrit maintenant le niveau `warning` (JSON : `"level":"warning"`).

## 0.56.2

- **Éditeur** : l'autocomplétion marche aussi quand une parenthèse, un crochet ou une accolade est resté ouvert (`fun handle(req: R` propose `Request`) : la copie analysée est fermée à l'endroit du curseur.

## 0.56.1

- **Éditeur** : l'autocomplétion marche aussi quand le fichier ne se compile pas encore parce qu'un bloc est resté ouvert (`fun handle(req: R)` sans son `end` propose `Request`). La copie analysée est complétée par des `end` ; le fichier, lui, n'est pas touché.

## 0.56.0

Dixième version du cycle « frameworks web en qlang » : l'outillage autour du langage (mise en forme, index de paquets, appels en parallèle) et, côté `qlang-std` 0.10.0, ce qu'il faut pour exploiter une application.

- **`qlang fmt`** (`--check` pour la CI) : deux espaces par niveau, `case` d'un `match` un niveau plus loin. Seuls les espaces en début et fin de ligne changent ; les jetons du résultat sont comparés à ceux de la source, rien n'est écrit en cas de différence. Le serveur de langage répond à `textDocument/formatting`.
- **`http.send_all(requêtes)`** : plusieurs requêtes en même temps (16 à la fois), les réponses dans l'ordre, un échec n'arrête pas les autres.
- **Éditeur** : l'autocomplétion marche après `not x.` et `a + b.` (le point appartient au dernier opérande), après `module.Type.` (`web.App.` propose `new`), à la place d'un type (`fun f() -> web.|`, `x: web.|`), et `Reflect.` propose ses fonctions ; une fonction ou une méthode s'insère avec ses parenthèses et ses paramètres (`validate()`, `new(${1:items})`). `qlang fmt` est aussi offert comme « Mettre en forme le document ».
- Le message de `Reflect.struct_info` sur un type qui n'est pas une structure (`R301`) dit quel type il a reçu.
- **Index de paquets** : `qlang add std` (sans `--git`) trouve le dépôt dans `packages.json` et prend la dernière étiquette `vX.Y.Z` ; `qlang search [texte]` liste l'index ; `QLANG_INDEX_URL` en désigne un autre.
- `qlang-std` 0.10.0 : `mail.q` (Resend, Postmark, SendGrid, Mailgun, mémoire, journal, tâche `SendMail`), `s3.q` (signature AWS v4, vérifiée contre les exemples d'AWS et un vrai MinIO : texte, octets, URL signées), `schedule.q` (cron, une seule exécution par minute même avec plusieurs machines, `schedule:run` et `schedule:work`), `notify.q` (courriel, Slack/Discord, boîte de réception), `web/metrics.q` (Prometheus), `web/health.q` (vivacité et disponibilité), et `examples/deploy` (Dockerfile, compose, systemd, Caddy) testé de bout en bout.

## 0.55.0

Neuvième version du cycle « frameworks web en qlang » : la fin de la liste des manques de la file de tâches, des relations et des JWT.

- **`Crypto.verify` et `Crypto.sign`** gèrent maintenant **RSA-PSS** (`PS256`, `PS384`, `PS512`) et **HMAC** (`HS256`, `HS384`, `HS512` : la clé est le secret).
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.9.0 apporte : des **files nommées** avec priorités (`queue.lane`, `#[OnQueue]`, `WorkOptions.queues`, `queue:work --queues high,default`), des **chaînes** (`queue.chain().add(..).add(..).dispatch()`) et des **lots** (`queue.batch().add(..).when_done(..).when_failed(..)`) ; `web/relations.q` (`has_many`, `has_one`, `belongs_to`, `belongs_to_many` : une requête par relation, pas une par ligne) ; `web/refresh.q` (jetons de rafraîchissement avec rotation et détection de réutilisation) ; HS384/HS512/PS256-512 dans `jwt.q` ; le rejeu des messages manqués par `Bus.replay` pour un client WebSocket qui se reconnecte ; `web/queue_admin.q` (tableau de bord de la file).

## 0.54.0

Huitième version du cycle « frameworks web en qlang » : de quoi signer et vérifier des JWT avec une paire de clés, et arrêter proprement un worker.

- **`Crypto.verify(alg, clé, message, signature)`** et **`Crypto.sign(alg, clé, message)`** : RSA PKCS#1 (`RS256`, `RS384`, `RS512`), courbes elliptiques (`ES256`, `ES384`) et Ed25519 (`EdDSA`) ; les clés sont des PEM (publique, PKCS#8, PKCS#1 pour RSA) ou des JWK. Seule la commande `qlang` les offre.
- **`Process.stopping()`** : le programme demande s'il doit s'arrêter. Le premier appel lui donne la main sur Ctrl-C et `kill` (un second signal le termine), pour finir proprement la tâche en cours.
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.8.0 apporte `jwt.q` (HS256 et clés asymétriques, `kid`, JWKS, `exp`/`nbf`/`iss`/`aud`), `web/auth.q` (`jwt_guard`, `scope_guard`, `fetch_jwks`) et des options pour la file de tâches : `#[Retry(tries, backoff)]`, `#[Unique]`, `#[Delay]` sur le type de la tâche, des intergiciels de tâche (`rate_limited`, `without_overlapping`, `Outcome.release`), `work_with(WorkOptions)` et l'arrêt propre du worker.

## 0.53.0

Septième version du cycle « frameworks web en qlang » : un client Redis, pour que plusieurs workers (et plusieurs machines) partagent un cache, des compteurs, une file de tâches et des messages sans interroger une base à vide.

- **`Redis`** : `Redis.connect("redis://hôte:6379/0")` (ou `rediss://` en TLS), `command([...])`, et les commandes courantes en méthodes (`get`, `set`, `set_ex`, `set_if_absent`, `incr`, `expire`, listes, `blpop`, ensembles triés, `eval` pour du Lua, `publish`). **`Redis.subscriber(url, canaux)`** écoute des canaux : `next(ms)` rend le message suivant. Seule la commande `qlang` l'offre ; un serveur distant demande TLS. Testé contre Redis 7 (le CI en lance un).
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.7.0 s'en sert : `RedisStore` pour le cache et la limitation de débit (atomique, partagé), une file de tâches Redis (attente bloquante, pas de sondage), `RedisFeed` pour diffuser des messages WebSocket entre workers, et le `Relay` qui ne lit qu'une fois par worker.

## 0.52.0

Sixième version du cycle « frameworks web en qlang » : les WebSocket, et deux corrections de l'éditeur.

- **WebSocket** dans `qlang serve` : `Response.websocket(WebSocketHandler { on_message, on_open, on_close, on_tick, tick_ms })`. Poignée de main, trames (fragmentées, ping/pong, fermeture avec code), messages jusqu'à 1 Mo. Un worker sert **plusieurs connexions** en même temps, entre deux requêtes (`WebSocket.send`, `send_bytes`, `close_with`, `data`, `request`, `id`, `take_frames` pour les tests). Voir `docs/language.md`.
- **Éditeur** : `import "@paquet/…"` marche aussi quand le projet est un sous-dossier du dossier ouvert (le projet est celui du `qlang.toml` le plus proche, comme en ligne de commande) ; **complétion dans les `import`** : les paquets (`@std/`), les fichiers et dossiers d'un paquet, ceux à côté du fichier ; VS Code propose aussi des suggestions dans les chaînes des fichiers qlang.
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.6.0 apporte `queue.q` (file de tâches dans la base : jobs typés, délais, reprises espacées, tâches abandonnées, commandes `queue:work`, `queue:failed`, `queue:retry`), `web/ws.q` (`Hub`, `Bus` par la base de données, `origin_guard`), `WsClient` pour tester un WebSocket sans réseau, et l'exemple `examples/chat`.

## 0.51.0

Cinquième version du cycle « frameworks web en qlang » : des commandes en ligne de commande et des traductions, écrites dans `qlang-std`.

- **`eprint(x)`** et **`ewrite(x)`** : comme `print` et `write`, sur la sortie d'erreur. Un hôte sans sortie d'erreur les jette ; `MemoryHost` les garde dans `errors`.
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.4.0 apporte `cli.q` (commandes `#[cli.Command]` à paramètres `#[cli.Arg]`, `#[cli.Opt]`, `Console` et services injectés, aide générée, codes de sortie), `i18n.q` (traductions JSON, pluriels, repli de langue, `Accept-Language`, nombres), `web/locale.q` (la langue de la requête) et `web/commands.q` (`routes:list`, `db:migrate`, `db:rollback`, `db:status`, `locale:missing`).

## 0.50.0

Quatrième version du cycle « frameworks web en qlang » : un opérateur qui manquait, de quoi décrire les types pour la documentation d'une API, et ce que `qlang-std` 0.3.0 en fait.

- **`a ?? b`** : la valeur de `a`, ou `b` si `a` est `none`. `b` n'est évalué qu'au besoin ; associatif à droite ; le moins prioritaire des opérateurs de calcul. Un côté gauche non nullable est l'erreur `T010`.
- **`Reflect.bound_callables<T>(valeur)`** : les méthodes publiques de `T`, déjà liées à `valeur`, en `Callable` (des contrôleurs en classes).
- **`Reflect.type_info<T>()`** et `Reflect.type_info_of(a)` : la forme d'un type (`TypeInfo` : genre, nom, nullable, élément, champs tels que `Json.of` les écrit avec leurs annotations, variantes, arguments de type). `ParamInfo.type_info()` et `Callable.return_info()` l'appliquent aux paramètres et au résultat d'une fonction.
- `Reflect.struct_info` et `Reflect.bound_callables` acceptent un paramètre de type d'une fonction générique ; une fonction anonyme garde les types de la fonction générique qui l'a créée (`a.as<T>()` dedans).
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.3.0 apporte `web/` : contrôleurs en classes (`#[Controller]`, `App.controller`), `testing.q` (client de test sans serveur), `events.q`, `cache.q`, `limit.q` (limitation de débit), CORS à options, `openapi.q`.

## 0.49.0

Troisième version du cycle « frameworks web en qlang » : de quoi écrire le paquet `web` de `qlang-std` (conteneur d'injection, routeur à paramètres injectés, middleware, validation, constructeur de requêtes, migrations, sessions).

- **L'interpolation peut contenir des chaînes** : `"{noms.join(", ")}"` marche (plus besoin d'une variable intermédiaire).
- **`Reflect.parts(a)`** ouvre une valeur d'énumération contenue dans un `Any` (un `Result` rendu par `Callable.call`) ; **`Reflect.info_of(a)`** rend le `StructInfo` de la structure qu'un `Any` contient ; **`Reflect.items(a)`** ses éléments si c'est un tableau. Avec `Reflect.get`, de quoi parcourir une valeur dont le type n'est connu qu'à l'exécution (validation de formulaires imbriqués).
- Un champ `float` d'annotation accepte un nombre entier : `#[Min(1)]`.
- Le paquet [`qlang-std`](https://github.com/quenti77/qlang-std) 0.2.0 apporte `web/` : `container.q`, `app.q`, `middleware.q`, `validate.q`, `query.q`, `migrate.q`, `session.q`.

## 0.48.0

Deuxième version du cycle « frameworks web en qlang » : de quoi recevoir des fichiers, garder des secrets, tenir la charge et parler à MySQL.

- **`bytes`** : une suite d'octets qui ne change jamais. `"texte".to_bytes()`, `b.to_string()`, `slice`, `concat`, `at`, `find`, `hex`, `base64`, `bytes.from_array`, `==`. Variantes `*_bytes` et `*_raw` de `Base64`, `Hex`, `Url` et `Hash`, `random.bytes(n)`, `fs.read_bytes` / `fs.write_bytes`. Dans un `Json`, des octets sont du base64. `bytes` reste utilisable comme nom de variable.
- **`Crypto`** : `random_bytes(n)` et `token(n)` depuis la source sécurisée du système (jamais la graine de `random`), `equal(a, b)` en temps constant pour comparer jetons et signatures.
- **`Password`** : `hash`, `verify`, `needs_rehash` en Argon2id (sel au hasard et réglages dans le texte rangé).
- **Journal** : `Log.debug/info/warn/error(message)` et `Log.write(niveau, message, champs)` sur la sortie d'erreur ; `--log-level`, `--log-format json`.
- **`qlang serve`** : connexions persistantes (100 requêtes, 5 s de silence), corps `chunked` et `Expect: 100-continue`, 64 connexions, `req.raw` / `Response.bytes` / `Response.raw` pour les octets, `req.multipart()` (formulaires avec fichiers), `Response.stream` et `Response.sse` (réponses envoyées au fil de l'eau, événements serveur), `--compress` (gzip des textes).
- **MySQL et MariaDB** : `Db.connect("mysql://utilisateur:mot_de_passe@hôte/base")`, colonnes typées, transactions, TLS avec `?sslmode=`. Essayé contre MySQL 8.0 et 9.5.
- `http.send` rend le corps exact dans `Response.raw` quand ce n'est pas du texte, et accepte `Request.raw`.

## 0.47.0

Premier pas vers des frameworks web écrits en qlang (voir `docs/design/web-0.47.md`).

- **Alias de type** : `type Handler = fun(Request) -> Response`. Ce n'est pas un mot réservé (`let type = 3` marche toujours) ; l'alias s'exporte et s'importe, un alias qui se désigne lui-même est refusé (`T122`).
- **`Any`** : une valeur avec son type. `Any.of(v)`, `a.as<T>()` (un `T?` : exactement `T`, une structure fille ou un trait implémenté), `a.type_name()`. Pour un conteneur de services ou une liste d'arguments.
- **Réflexion étendue** : `Reflect.functions(m)` et `Reflect.structs(m)` sur un module importé, `Reflect.callable(m, "nom")` (un `Callable` qui s'appelle avec des `Any` et vérifie leur nombre et leurs types), `Reflect.get(valeur, "champ")`, `Reflect.construct<T>(champs)`, `Reflect.type_name<T>()`.
- **Types à l'exécution dans les fonctions génériques** : une fonction qui nomme son paramètre `T` dans `Reflect.type_name<T>()`, `a.as<T>()`, `Json.to<T>(j)`, `Reflect.construct<T>(...)` ou `Any.of(x)` reçoit le type choisi à chaque appel (`app.get<Logger>()`, `fun body<T>(...)`), y compris par une autre fonction générique qui lui passe son `T`, et pour les paramètres de type d'une méthode.
- **Gestionnaires à paramètres injectés** : `Reflect.callables(m)` rend toutes les fonctions d'un module en `Callable` ; `p.parse(texte)` et `p.decode(json)` d'un `ParamInfo` lisent un texte ou un `Json` selon le type du paramètre ; `AnnotationInfo` a `get_string`, `get_int`, `get_float`, `get_bool`. Exemple : `examples/container/` (un conteneur d'injection de dépendances de 60 lignes).
- **`Json.of(valeur)` et `Json.to<T>(json)`** : conversion selon la forme du type, sans `to_json` ni `from_json` à la main. Seuls les champs `public` sortent et se lisent ; `#[JsonName]` et `#[JsonSkip]` ; les erreurs disent où (`address.city: missing`). `Json` a `to_text()` et implémente `ToJson`.
- **Requêtes et réponses HTTP** (champs ajoutés, rien ne casse) : `req.cookies`, `req.query_all`, `req.headers_all` et `req.header_all(nom)`, `req.form()`, `req.attrs` (un `map<string, Any>` propre à la requête) ; `Cookie` et `res.with_cookie(...)` (un `Set-Cookie` chacun) ; `HttpError`.
- `qlang serve --dev` met le détail d'une erreur du programme dans la réponse 500.
- `a.as<T>()` : `as` est accepté comme nom de méthode après un point.

## 0.46.1

- `editors/vscode/intel.js` et `lib.js` reviennent dans le dépôt (retirés par erreur en 0.46.0) : l'éditeur du site d'apprentissage en dépend. Ils ne sont pas dans l'extension.

## 0.46.0

- **Serveur de langage** : `qlang lsp` parle le Language Server Protocol sur l'entrée et la sortie standard. Pour tout éditeur : erreurs pendant la frappe (y compris dans les fichiers importés), infobulles, autocomplétion, aide à la saisie des paramètres, **aller à la définition**, **références**, **renommage** dans tout le projet (variables, paramètres, fonctions, méthodes, types, champs, variantes ; il refuse le raccourci `Point { x }` qui nomme deux choses à la fois), plan du fichier et recherche de symboles. Le texte non enregistré est pris en compte.
- L'extension VS Code utilise ce serveur à la place de son code JavaScript (`qlang.diagnostics.delay` disparaît ; la commande « qlang: Check File » devient « qlang: Restart Language Server »). Elle demande VS Code 1.82 et embarque `vscode-languageclient`.
- Le protocole JSON (`"info": true`) ajoute `refs` : chaque usage d'un nom avec l'emplacement de sa définition.

## 0.45.0

- Performance : la machine à registres compile les **tuples** (valeurs, paramètres, résultats, `t.0`, `let (q, r) = f(x)`, motifs de tuples dans `match`, `==`) et l'**appel d'une valeur fonction** (`f(x)` avec `f: fun(int) -> int`) : une fonction qui reçoit une fermeture est compilée, la fermeture elle-même reste interprétée. Tuples ×5,8, boucles qui appellent une fermeture ×2,5. 20 000 programmes aléatoires de plus donnent les mêmes résultats sur les deux moteurs.

## 0.44.0

- Performance : la machine à registres compile les **énumérations** et **`match`** (variantes avec valeurs, motifs, gardes, intervalles, `Ok`/`Err`, `?` sur `Result` et sur `T?`, `==` entre valeurs), et une fonction générique ou `extern` appelée depuis du code compilé n'empêche plus de le compiler. Énumérations et `match` ×5, chaînes de `Result` ×6.

## 0.43.0

- Performance : la machine à registres compile aussi les valeurs **`none`** (`T?`, `x != none`, rétrécissement du type) et les **dictionnaires** (`m[k]`, `m[k] = v`, `for k, v in m`, toutes les méthodes) ; les méthodes prédéfinies viennent maintenant des signatures du vérificateur. Comptage de mots ×2,4.
- Correction : une variable du fichier dont le type est rétréci par un test (`if x != none`) dans une fonction restait vue comme une variable locale par l'analyse ; elle reste la variable du fichier.

## 0.42.0

- Dépendances : `ctutils` 0.4.5 (avis RUSTSEC-2026-0385, utilisé par le client Postgres).
- Performance : la machine à registres compile aussi les **textes** (littéraux, `+`, comparaisons, interpolation, `print`, méthodes courantes) et peut créer des tableaux dans une boucle : une boucle de traitement de texte va 4 fois plus vite. Deux banques de registres (nombres, valeurs) remplacent la table de tableaux.
- Performance : et les **structures** (champs, littéraux) avec leurs **méthodes** (le corps est compilé ; l'appel est direct quand aucune structure fille ne redéfinit la méthode, sinon il passe par l'interpréteur, donc héritage, `override` et traits inchangés) : jusqu'à ×4 sur du code orienté objet. Les **tableaux de textes, de structures et de tableaux** sont compilés, et une fonction qui ne l'est pas (dictionnaire, `match`) n'empêche plus sa voisine de l'être. 40 000 programmes aléatoires (nombres, tableaux, textes, structures) donnent les mêmes résultats sur les deux moteurs.

## 0.41.0

- Réflexion : `Reflect.methods<T>()` liste les méthodes d'un type avec leurs annotations (`FunctionInfo.has_self` est nouveau) et `Reflect.find_method<T, F>(valeur, "nom")` rend la méthode liée à la valeur, appelable. Les annotations des méthodes sont maintenant lisibles.

## 0.40.0

- **Annotations** `#[Route("/users", method: "GET")]` sur les fonctions, méthodes, structures, champs, énumérations, variantes, traits et paramètres. Une annotation est une structure déclarée avec `#[annotation]` ; ses arguments sont des littéraux, vérifiés à la compilation (`T290`–`T295`, `P051`). 
- **Réflexion** : `Reflect.struct_info<T>()`, `Reflect.enum_info<T>()`, `Reflect.functions()` décrivent structures, énumérations et fonctions du fichier avec leurs annotations ; `Reflect.find<fun(A) -> B>("nom")` rend la fonction de ce nom si son type correspond. Exemple : `examples/routes.q` (routes par annotations).
- `qlang serve app.q --watch` : le programme est rechargé quand un de ses fichiers change ; une erreur de compilation est affichée et l'ancienne version continue de répondre.

## 0.39.0

- Performance : une fonction compilée par la machine à registres peut **créer** un tableau de nombres (`[]` typé, `[a, b, c]`), hors d'une boucle ; ceux qui naissent pendant un appel sont libérés à son retour. Une boucle qui contient un appel reste compilée quand elle utilise des variables du fichier : 3 millions d'appels ×2,6.

## 0.38.0

- Performance : la machine à registres typée travaille aussi sur les **tableaux de nombres** (`array<int>`, `array<float>`, `array<bool>`) donnés à une fonction ou empruntés par une boucle : crible, sommes préfixes, parcours ×4 à ×4,5. Division entière et modulo plus rapides pour les petits entiers.
- Performance : constantes en registres, comparaison et saut fusionnés, pas comptés par tour de boucle : boucles numériques ×10 par rapport à la 0.36.

## 0.37.0

- Performance : une machine à registres typée compile au premier appel les fonctions et les boucles purement numériques (`int`, `float`, `bool`) : `fib(27)` ×4, une boucle `while` de 3 millions de tours ×6,5 à ×8. Le reste s'exécute comme avant, avec les mêmes résultats et les mêmes erreurs (voir `docs/performance.md`). `RunResult::vm_functions` et `Program::run_tree_walker` pour la mesure et la comparaison.

## 0.36.0

Sécurité (voir la section « Sécurité » du README) :

- `qlang serve` (protocole JSON) : refuse les requêtes venant d'une page web (`Origin`, `Sec-Fetch-Site`) et, sur une adresse locale, un `Host` qui n'est pas local (DNS rebinding).
- Limite de mémoire du programme `qlang` : `R904` au-delà d'un gigaoctet (`--max-memory MB`, `QLANG_MAX_MEMORY`, `0` pour la retirer). Avant, un programme pouvait en réserver une dizaine de gigaoctets avec les limites par défaut et faire tuer le processus (le serveur `qlang serve`, ou `run --json`, pour tout le monde).
- Les valeurs imbriquées très profondément (liste chaînée de millions de maillons) sont libérées sans récursion : leur libération faisait déborder la pile et tuait le processus. Les comparer (`==`, `<`) au-delà de 1 000 niveaux est une erreur `R092`.
- `fs.write` ne suit plus un lien symbolique qui sort de `--root` ; `fs.read` refuse ce qui n'est pas un fichier ordinaire (`/dev/zero`) ou dépasse 64 Mo ; `http.send` a un délai de 60 s.
- `qlang serve` : une requête doit arriver entière en 20 s (un client lent gardait une connexion pour toujours), `--static` ne sert plus les fichiers cachés (`.env`, `.git/...`), un statut hors de 100 à 599 donne 500.
- `Db.connect` : un serveur Postgres distant sans `sslmode` dans l'URL est refusé (le mot de passe partait en clair) ; avec `--root`, le fichier d'une base `sqlite://` doit être dans le dossier.
- Fournisseurs d'`extern` : une réponse de plus de 64 Mo est refusée. Dépendances git : un `tag`, `rev` ou `branch` ne peut pas commencer par `-`.
- Dépendances : rustls, rustls-webpki et webpki-roots mis à jour (6 avis RustSec) ; `cargo audit` tourne chaque semaine. Les actions GitHub sont épinglées par commit, les versions publiées auront une attestation de provenance.
- Performance : les paramètres d'un appel sont ajoutés sans recherche ; benches `result.q` et `set.q`.

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
