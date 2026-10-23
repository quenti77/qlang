# Étude de conception : qlang pour le web (0.47 → 0.49)

Statut : **proposition, rien n'est codé**. Chaque point marqué **[DÉCISION]** attend ton choix.
Référence : Laravel, Symfony, NestJS, axum, Wire, Micronaut/Dagger, Spring, serde. poo-project ne
sert qu'à la fin comme test de fumée.

## 1. Ce que l'état du langage impose

Faits vérifiés dans le code et la doc (0.46.1) :

- Les valeurs portent leur forme à l'exécution (`Value::Struct` garde son type, `Value::Func`
  est appelable), les types sont vérifiés statiquement. Une valeur « n'importe quel type » est
  donc peu coûteuse : une `Value` plus l'étiquette de type connue du vérificateur.
- `Reflect` existe mais : ne voit que **le fichier courant** (`reflectable_functions(m)` filtre
  sur le module), ne **construit** rien, ne **lit** pas un champ par son nom, ne retrouve une
  fonction que si son type est exactement celui demandé (`Reflect.find<F>`).
- Il n'y a pas d'alias de type (`type Handler = ...` donne `P002`), pas de tableau d'octets
  (« le texte est le seul type »), et `Error` n'a qu'un champ `message`.
- `to_json` / `from_json` s'écrivent à la main, par convention.
- Le serveur exécute le fichier **une fois**, puis appelle `handle` : tout ce qui est construit
  au démarrage (routes, conteneur) est amorti sur toutes les requêtes.

## 2. Ce que font les autres, et ce qu'on en garde

| Sujet | Laravel / Symfony (PHP) | Spring, Micronaut, Dagger, Wire | axum (Rust), NestJS (TS) | Leçon pour qlang |
|---|---|---|---|---|
| Injection | Autowiring par réflexion du constructeur. Laravel résout **à l'exécution** (erreur au premier appel). Symfony **compile** le conteneur et signale les erreurs au build (`lint:container`) | Graphe vérifié à la compilation, sans réflexion (Wire, Dagger, Micronaut) | NestJS : réflexion + métadonnées de décorateurs | Vérifier le graphe **au démarrage** au minimum, à la compilation si possible. Éviter une erreur au milieu d'une requête |
| Durées de vie | `bind` / `singleton` / `scoped` (par requête, nécessaire avec Octane) | Singleton / prototype / request | Singleton par défaut | Trois durées dès le départ : singleton (par worker), par requête, nouvelle à chaque fois |
| Paramètres de handler | Injection de méthode (`App::call`) + résolveurs | `@PathVariable`, `@RequestBody` | Extracteurs typés (`Path<T>`, `Json<T>`), échec = réponse d'erreur avant d'appeler le handler. `@Param/@Body` + pipes | Un handler est une fonction **à paramètres typés** ; le framework les remplit. L'échec de conversion donne un 400/422, jamais un handler appelé avec des données fausses |
| Corps lisible une fois | (n/a) | (n/a) | axum l'impose par les types (le corps en dernier argument) | Sans objet ici : `Request` est une valeur déjà lue (limite 1 Mo) |
| Validation | Règles sur les formulaires / `#[Assert]` | Bean Validation | `ValidationPipe` + classes DTO | Règles en **annotations** sur les champs d'une struct : qlang a déjà `#[Min(0)]` sur un champ |
| Sérialisation | Manuelle ou resources | Jackson (réflexion) | serde : `derive` généré à la compilation | Pas de macros dans qlang. Une primitive **structurelle** native (`Json.of`, `Json.to<T>`) couvre le besoin, la méthode `to_json` manuelle reste prioritaire |
| Erreurs | Exceptions → réponse | `@ExceptionHandler` | `IntoResponse` pour les erreurs | `Result` + `?` existent déjà ; il manque un type d'erreur portant un statut |
| Conflits de routes | — | — | NestJS 12 : détecte les routes masquées (`:id` avant `me`) | `Router` : « la première qui correspond gagne ». Ajouter une vérification au démarrage |

Rejeté, avec la raison :

- **Façades globales (Laravel)** : état global mutable caché, contraire au typage explicite.
- **Macros / `derive`** : un système de macros coûterait plus que `Json.of` natif.
- **Parcours du classpath / scan de dossiers** : l'autoloader est abandonné ; la découverte passe
  par `Reflect` sur des modules importés explicitement.
- **async/await** : voir §6, on garde des workers bloquants isolés.
- **Réflexion sur les constructeurs** : qlang n'a pas de constructeurs. Une struct se remplit par
  ses champs.

## 3. Propositions pour la 0.47 (langage)

Chaque ajout est petit, utile hors du web, et vérifié par le typage.

### 3.1 `Any` : une valeur dont le type est connu à l'exécution

```
let a = Any.of(user)            -- Any
let u: User? = a.as<User>()     -- none si ce n'est pas un User
print(a.type_name())            -- "User"  (aussi Reflect.type_name<T>())
```

- Sert de valeur dans un conteneur hétérogène et de liste d'arguments pour `Reflect.call`.
- `as<T>` distingue les types génériques par leurs arguments (`array<int>` ≠ `array<string>`).
- Un trait comme type est déjà possible : `Any` sert quand on n'a pas de trait commun.
- Pas de conversion implicite : sortir d'un `Any` exige `as<T>`.

### 3.2 `Reflect` étendu

| Ajout | Rôle | Inspiré de |
|---|---|---|
| `Reflect.functions(module)` / `structs(module)` avec un module importé (`import "app/controllers" as c`) | Un paquet-framework découvre ce que l'application déclare | Symfony autoconfigure |
| `Reflect.callable(f)` → `Callable` : `params` (nom, type, annotations), `return_type`, `call(args: array<Any>) -> Result<Any, Error>` | Appeler une fonction de signature quelconque, **plan calculé au démarrage**, appel peu coûteux | Laravel `App::call`, Spring |
| `Reflect.get(valeur, "champ") -> Any?` | Lire un champ par son nom (validation, sérialisation personnalisée) | PHP `ReflectionProperty` |
| `Reflect.construct<T>(champs: map<string, Any>) -> Result<T, Error>` | Construire une struct depuis ses champs (valeurs par défaut respectées, type de chaque champ contrôlé) | Symfony autowiring (par les champs) |
| `Reflect.type_name<T>() -> string` | Clé de conteneur | — |

La visibilité reste ignorée par `Reflect` (comme aujourd'hui) : `construct` peut remplir des
champs privés, ce qui est voulu pour un conteneur. `Json.of` (§3.3) ne s'en sert pas.

### 3.3 Sérialisation structurelle

```
Json.of(user)                      -- Json : champs `public` seulement
Json.to<CreateUser>(body_json)     -- Result<CreateUser, Error>
```

- **Champs publics seulement** pour `Json.of` (un champ privé ne fuit jamais ; équivalent de
  `$hidden` de Laravel, par construction).
- `Json.to<T>` : champs absents → valeur par défaut, sinon erreur ; champ de mauvais type →
  erreur avec le **chemin** (`user.address.zip: attendu string`) ; champs inconnus ignorés.
- `T?` ↔ `null`, `enum` ↔ objet à étiquette, génériques supportés (`Page<User>`).
- Une méthode `to_json(self)` / une fonction `from_json` déclarées **priment** (cohérent avec la
  convention actuelle et avec `with` de serde).
- Annotations prévues : `#[JsonName("user_id")]`, `#[JsonSkip]`.
- `Json.schema<T>()` (JSON Schema) est repoussé à `@web` (OpenAPI), il se calcule avec
  `struct_info`.

### 3.4 Alias de type

```
type Handler = fun(Request) -> Response
type Middleware = fun(Request, Handler) -> Response
```

Sans effet sur la sémantique (substitution), mais indispensable à la lisibilité d'un framework.

### 3.5 Ce qu'on ne touche pas

Pas de `bytes` en 0.47 (voir §5), pas de traits à méthodes statiques, pas de macros.

## 4. Propositions pour la 0.47–0.48 (modèle HTTP)

| Problème actuel | Proposition | Compatibilité |
|---|---|---|
| `headers: map<string, string>` ne peut pas porter plusieurs `Set-Cookie` | `Response.cookies: array<Cookie>` (`name`, `value`, `path`, `domain`, `max_age`, `secure`, `http_only`, `same_site`) ; `Request.cookies: map<string, string>` analysé ; `req.header_all(nom) -> array<string>` | Champs ajoutés avec valeur par défaut : aucun programme existant ne casse |
| Paramètres de requête répétés perdus | `req.query_all: map<string, array<string>>` ; `query` reste « dernier gagne » | idem |
| Formulaires | `req.form() -> Result<map<string, string>, Error>` (urlencoded) | nouveau |
| Pas d'état par requête | `req.attrs: map<string, Any>` (le middleware d'authentification y met l'utilisateur) | nouveau |
| Erreur sans statut | `HttpError { public status: int, public message: string }` dans le prélude, et `Error` reste inchangé | nouveau |
| `panic` → 500 sans détail | Mode `--dev` : page d'erreur avec message et position ; en production inchangé | option |

## 5. Questions ouvertes qui dépassent la 0.47

- **Octets (`bytes`)** : sans type d'octets, pas d'envoi de fichier binaire fiable, pas de
  `multipart` correct, pas de BLOB, pas de hachage de mots de passe sur du binaire. C'est
  probablement la plus grosse décision du cycle ; je propose de la traiter en début de 0.48.
  Une version minimale : `bytes` immuable, `string.to_bytes()`, `bytes.to_string()`, conversion
  base64/hex, taille, tranche.
- **Mot de passe** : `Password.hash(s)` / `verify` (argon2id) côté hôte, comme `Hash`.
- **Durée de vie dans le conteneur** : voir §7.

## 6. Concurrence : on garde l'actuel

Workers bloquants isolés (comme PHP-FPM), pas d'async. Conséquences à documenter dans `@web` :
un singleton vit **dans un worker** ; ce qui doit être partagé passe par la base ou Redis. Un
conteneur par worker, construit une fois, avec un sous-conteneur par requête pour la durée
« par requête ».

## 7. Conteneur d'injection (paquet `@web`, 0.49) : deux modèles

### Modèle A (recommandé) : fournisseurs explicites + vérification au démarrage

```
fun make_db(config: Config) -> Db ... end            -- singleton par défaut
#[Scoped] fun make_user(req: Request, repo: UserRepo) -> CurrentUser ... end

let app = Container.new()
app.provide_all(Reflect.functions(providers))        -- découverte par module
app.validate()?                                      -- manquants, cycles, durées incompatibles
let repo = app.get<UserRepo>()?
```

- Une fonction qui retourne `T` **fournit** `T` ; ses paramètres sont ses dépendances (style
  Wire/Dagger, mais vérifié au démarrage au lieu de la génération de code).
- Interface → implémentation : `app.bind<Logger, ConsoleLogger>()` (les traits sont des types).
- Les erreurs sont toutes signalées **avant** la première requête (leçon de Symfony et Wire).
- Ne demande que §3.1–3.2.

### Modèle B : autowiring par les champs

```
struct UserController
  repo: UserRepo
  log: Logger
end
let c = app.make<UserController>()?     -- Reflect.construct + résolution par type
```

Pratique pour les contrôleurs (Laravel, Symfony), mais construit à l'exécution ; à ne proposer
qu'**en plus** de A, car il ne s'analyse pas statiquement.

Vérification statique ultérieure : les fournisseurs étant des fonctions de premier niveau, un
futur pass du vérificateur (et du LSP) pourra rejeter `get<T>()` sans fournisseur. Rien dans A ne
l'empêche, B le compliquerait.

## 8. Routeur et handlers (paquet `@web`)

```
#[Route("/users/:id")]
fun show(#[Path] id: int, users: UserRepo) -> Result<User, HttpError> ... end

#[Route("/users", method: "POST")]
fun create(#[Body] input: CreateUser, users: UserRepo) -> Result<User, HttpError> ... end
```

- Les paramètres sont remplis par des **résolveurs** (annotation + type) : `#[Path]`, `#[Query]`,
  `#[Body]`, `#[Header]`, sinon le conteneur. Échec de conversion → 400, validation → 422.
- Le type de retour décide de la réponse : `Response` tel quel, une struct → `Json.of`,
  `Result<_, HttpError>` → erreur avec son statut.
- Le plan (paramètres, résolveurs) est calculé **une fois** à l'enregistrement ; chaque requête
  ne fait que `Callable.call`.
- Au démarrage : routes masquées ou dupliquées, paramètre `#[Path]` absent du chemin, type non
  convertible → erreur claire.
- Middleware : `Middleware` (alias §3.4), groupes et préfixes, aucun besoin du langage.

## 9. Validation (`@web`)

`Json.to<T>` contrôle la **forme** ; les règles (`#[Min]`, `#[Email]`, `#[Length]`) se lisent avec
`struct_info` + `Reflect.get`. Les règles personnalisées s'enregistrent par nom
(`Validator.rule("Slug", fun...)`). Rien de plus côté langage.

## 10. Découpage proposé

| Version | Contenu |
|---|---|
| **0.47** | `Any`, `Reflect` étendu (modules, `callable`, `get`, `construct`, `type_name`), `Json.of` / `Json.to<T>`, alias de type, cookies / `query_all` / `form()` / `req.attrs` / `HttpError`, mode `--dev` |
| **0.48** | `bytes`, `Password`, keep-alive, multipart, requêtes `chunked`, streaming / SSE, gzip, `log` (niveaux, JSON), pilote MySQL |
| **0.49** | `@web` : conteneur (A, puis B), routeur à handlers typés, middleware, validation, query builder, migrations |

## 11. Décisions prises (2026-10-23)

1. **DI** : modèle A (fournisseurs) seul ; l'autowiring par les champs (B) n'est pas prévu.
2. **Handlers** : paramètres injectés par annotations.
3. **`Json.of`** : champs publics seulement.
4. **`bytes`** : la 0.47 ne le débloque pas (rien de ce qu'elle contient n'en dépend) : il reste en 0.48.
5. **Compatibilité** : champs HTTP ajoutés, `headers` inchangé.

## 12. Réalisation (0.47.0)

Tout ce qui est décrit en §3 et §4 est fait, avec ces écarts et ajouts, trouvés en construisant
un conteneur d'injection (`examples/container/`) et un routeur à paramètres injectés pour de vrai :

- **Types à l'exécution** : un `get<T>()` générique ne peut pas nommer `T` si les types sont
  effacés. Les fonctions qui nomment leur `T` dans une opération de réflexion reçoivent maintenant
  les types de chaque appel (équivalent d'un `reified` de Kotlin). Les paramètres de type d'une
  structure générique, et les appels indirects (fonction passée comme valeur), n'en bénéficient pas.
- **`Reflect.callables(m)`** : un module n'est pas une valeur, une bibliothèque ne peut pas le
  recevoir en paramètre ; l'application passe le tableau des `Callable`.
- **`ParamInfo.parse` / `decode`** : pour remplir un paramètre dont le type n'est connu que par la
  réflexion (`#[Path] id: int`, `#[Body] input: CreateUser`).
- **`AnnotationInfo.get_*`** : lire les arguments d'annotation sans passer par `Json`.
- **Non fait** : `Json.of` n'appelle pas un `to_json` écrit à la main (la forme du type seule
  compte) ; `==` entre `Any` est refusé ; pas de complétion des méthodes de `Any` dans l'éditeur.
