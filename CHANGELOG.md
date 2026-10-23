# Journal des changements

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
