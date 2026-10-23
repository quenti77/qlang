# Performance de l'interpréteur

qlang est un interpréteur qui parcourt l'arbre du programme (pas de bytecode). Ce document
dit comment mesurer, ce qui a déjà été gagné et ce qui reste.

## Mesurer

Les programmes de `benches/` couvrent les cas courants (appels récursifs, boucles, tableaux,
structures, chaînes, dictionnaires). Pour comparer deux binaires (par exemple celui de `main`
construit à part, et le vôtre), en gardant le meilleur de 5 passages :

```sh
python3 benches/compare.py chemin/vers/qlang-reference ./target/release/qlang
```

Pour savoir *où* part le temps, `perf` sait échantillonner le processus (les événements
matériels ne sont pas toujours disponibles, `cpu-clock:u` marche presque partout) :

```sh
CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release -p qlang-cli
perf record -e cpu-clock:u -F 2000 -o perf.data target/release/qlang run benches/fib.q
perf report -i perf.data --no-children --stdio --percent-limit 2
```

Comparez toujours avec un binaire construit de la même façon (les archives publiées sont
compilées en `musl`, dont l'allocateur est plus lent que celui de `glibc`).

## Déjà fait (premier lot, environ 1,3 à 1,8 fois plus vite)

| Programme | avant | après |
| --- | --- | --- |
| `fib(27)` (appels récursifs) | 0,34 s | 0,23 s |
| boucle de 3 millions de tours | 1,74 s | 0,95 s |
| 400 000 ajouts et lectures de tableau | 0,33 s | 0,20 s |
| 600 000 appels de méthode sur une structure | 0,90 s | 0,52 s |

- Les variables et les champs de structure sont dans de petits tableaux parcourus
  linéairement (`SmallMap`), avec des noms courts stockés sans allocation, au lieu d'une table
  de hachage par portée.
- Les tables que l'interpréteur consulte à chaque nom évalué (`Resolutions::paths`) sont
  indexées par numéro de nœud ; les autres utilisent un hachage rapide (`fx`).
- Un bloc qui ne déclare aucune variable s'exécute dans la portée qui l'entoure au lieu d'en
  créer une à chaque tour de boucle.
- L'affectation à une variable n'alloue plus.

- Les cas rares de `eval` et de `exec_stmt` (chaînes, dictionnaires, `for`, `while`, `match`,
  `?`...) sont dans des fonctions à part, non inlinées : le cadre de pile de `eval` et
  `exec_stmt`, qui se répètent à chaque niveau d'appel, est plus petit. Les appels imbriqués
  tiennent donc plus profond, ce qui compte surtout pour WebAssembly (la pile de V8 est
  d'environ 1 Mo) : 999 niveaux contre 762 avant. `node --test crates/qlang-wasm/test/*.test.mjs`
  vérifie qu'une récursion de 990 appels passe ; si ce test échoue après un changement de
  l'interpréteur, un cadre de pile a grossi.

## Deuxième lot (1,1 à 1,3 fois plus vite)

Mesuré contre la version 0.34.0, sur les mêmes programmes de `benches/` (meilleur de 5) :

| Programme | gain |
| --- | --- |
| boucle de 3 millions de tours (`loop.q`) | ×1,32 |
| tableaux (`array.q`) | ×1,15 |
| `fib(27)` (`fib.q`) | ×1,14 |
| appels de méthode (`struct.q`) | ×1,12 |
| chaînes, dictionnaires | ×1,08 |

- **Cache de recherche des variables** : chaque utilisation d'un nom retient « combien de portées
  plus haut, et à quelle place » il a trouvé sa variable (`Scope::get_cached`). La fois suivante,
  l'interpréteur va droit à la place, après avoir vérifié que le nom y est bien ; si la
  vérification échoue (la portée n'a pas la même forme), il cherche comme avant et retient la
  nouvelle place. Le cache ne peut donc pas donner une mauvaise variable. Il gagne peu à lui
  seul (5 à 7 %) : chercher un nom dans quelques entrées coûtait déjà peu.
- **Valeurs plus petites** : une `Value` passait de 48 à 24 octets (`uuid` et `ulid` en deux moitiés
  de 64 bits, intervalle exclusif et inclusif en deux variantes, énumération avec ses numéros en
  32 bits et ses données derrière un pointeur simple). Elles sont copiées à chaque retour de
  `eval`.
- **Entiers** : `+ - * div mod < <= > >= == !=` entre deux `int` (sans dépassement ni division par
  zéro) sont calculés sur place, sans passer par `binary` ; c'est le plus gros gain de la boucle.
  Les erreurs (`R010`, `R011`) sont toujours construites par le chemin général.
- Un appel réserve d'emblée la place des paramètres de sa portée.

## La machine à registres typée (troisième lot)

Le vérificateur connaît le type de chaque expression, et l'interpréteur d'arbre l'ignorait :
chaque `a + b` redécouvrait à l'exécution que c'étaient deux `int`, chaque variable se cherchait
par son nom, chaque appel construisait une portée. Pour le code qui ne calcule qu'avec des
`int`, des `float` et des `bool`, tout cela est inutile.

Au premier appel d'une fonction numérique (`interp/vm.rs`), elle est compilée en instructions
sur des registres de 64 bits (un `float` est ses bits, un `bool` vaut 0 ou 1), avec l'opération
déjà choisie selon le type (`AddI`, `LtF`, `ForTest`...). Sont compilés :

- les fonctions dont les paramètres, les variables et le résultat sont `int`, `float` ou `bool`,
  qui appellent d'autres fonctions de ce genre (récursion comprise) ;
- les boucles `while` et `for a..b [step s]` **de n'importe quel code** : la boucle emprunte les
  variables qu'elle partage avec le code autour (variables du fichier, variables locales d'une
  fonction qui, elle, n'est pas compilée) dans des registres, et réécrit au retour celles
  qu'elle a modifiées ;
- l'arithmétique mêlant `int` et `float`, `as`, `**`, `abs`, `min`, `max`, `sqrt`, `floor`,
  `sin`... (les méthodes rares passent par l'implémentation de l'interpréteur d'arbre, donc avec
  les mêmes résultats et les mêmes erreurs) ;
- les **tableaux de nombres** (`array<int>`, `array<float>`, `array<bool>`) donnés à une fonction ou
  empruntés par une boucle : lecture et écriture `xs[i]`, `xs[i] += v`, `xs[] = v`, `push`, `len`,
  `for x in xs`, `for i, x in xs`, et `sum`, `sort`, `reverse`, `contains`, `insert`, `remove`...
  (les méthodes passent par l'implémentation de l'interpréteur d'arbre). Les tableaux et les
  textes vivent dans une seconde banque de registres, des valeurs de l'interpréteur (une
  référence) : on peut en **créer** n'importe où, même à chaque tour de boucle (`[]` avec un type
  déclaré, `[a, b, c]` de nombres), ils sont libérés avec le registre qui les tient ;
- les **textes** (`string`) : littéraux, `+` et `+=`, comparaisons, `"valeur {x}"` (interpolation de
  nombres, de booléens, de textes et de tableaux), `print`, et les méthodes courantes
  (`len`, `upper`, `trim`, `contains`, `replace`, `substring`, `repeat`...), qui passent par
  l'implémentation de l'interpréteur d'arbre pour avoir les mêmes résultats et les mêmes erreurs ;
- les **structures** non génériques, dont les champs sont des nombres, des textes ou des tableaux de
  nombres : `s.champ`, `s.champ = v`, `s.champ += v`, `Point { x: 1, y: 2 }` (les valeurs par
  défaut sont celles de l'interpréteur) et les **méthodes** : le corps d'une méthode est compilé (avec
  `self` en premier paramètre), et un appel `p.avance(k)` est envoyé par l'interpréteur comme
  d'habitude, ce qui garde l'héritage, `override` et les traits tels quels ; quand aucune structure
  fille ne redéfinit la méthode, l'appel est direct, comme celui d'une fonction ;
- les **tableaux de textes, de structures ou de tableaux** (`array<string>`, `array<array<int>>`,
  `array<Point>`) : `xs[i]`, `xs[i][j] = v`, `xs[] = v`, `for x in xs`, `join`, et `split`, `words`,
  `lines`, `chars` d'un texte ;
- les valeurs **`none`** (types `T?`) : `x = none`, `x == none`, `x != none` et le rétrécissement du
  type après le test (`if x != none then x + 1 end`), les paramètres et résultats `T?`, les méthodes
  qui rendent un `T?` (`to_int`, `index_of`, `pop`, `remove`...), les tableaux de `T?` ;
- les **dictionnaires** (`map<K, V>`, `K` entier, texte ou booléen) : `{ "a": 1 }`, `m[k]` (qui est un
  `V?`), `m[k] = v`, `m[k] += v`, `for k, v in m`, et toutes les méthodes (`get`, `has`, `keys`,
  `values`, `len`, `remove`...). Les méthodes prédéfinies sont désormais toutes celles dont le
  vérificateur a donné la signature, sans table à tenir à jour ;
- les **énumérations** et **`match`** : variantes avec ou sans valeurs (`Shape.Circle(2.0)`, `Ok(x)`,
  `Err(e)`), motifs sur les variantes (avec valeurs et gardes `case Circle(r) if r > 3`), sur les
  nombres (`0`, `1..5`, `5..=10`), les textes, les booléens et `none`, `match` comme valeur ou comme
  instruction, l'opérateur `?` (sur un `Result` comme sur un `T?`) et `==` entre deux valeurs. Les
  motifs de tuples ne le sont pas ;
- l'appel d'une fonction que la machine ne sait pas compiler (elle utilise une fermeture, un tuple...) :
  la fonction reste sur l'interpréteur d'arbre, mais celle qui l'appelle est compilée quand même ;
- `(a, b) = (b, a + b)`, qui ne construit aucun tuple ;
- les variables du fichier lues et écrites par une fonction compilée (une lecture ou une écriture
  de la portée à chaque accès, avec le même cache que l'interpréteur).

Tout le reste (fermetures, tuples, structures génériques...) tourne sur
l'interpréteur d'arbre, qui appelle la machine et en reçoit les appels sans difficulté. Une
boucle qui contient un appel à une fonction lit et écrit alors les variables du fichier à leur
place, et non dans des registres (l'appelée pourrait les changer) : un peu plus lent, mais
toujours compilée.

| Programme | avant | après | gain |
| --- | --- | --- | --- |
| `fib(27)` | 0,13 s | 0,03 s | ×4 |
| boucle de 3 millions de tours (`loop.q`, au niveau du fichier) | 0,50 s | 0,05 s | ×10 |
| boucles numériques dans des fonctions (`numloop.q`) | 0,76 s | 0,07 s | ×10 |
| tableaux de nombres : crible, sommes préfixes, parcours (`numarray.q`) | 2,0 s | 0,36 s | ×5,5 |
| 400 000 ajouts et lectures de tableau (`array.q`) | 0,12 s | 0,03 s | ×4,5 |
| 3 millions d'appels d'une fonction qui fait un petit tableau (`smallarray.q`) | 1,7 s | 0,65 s | ×2,6 |
| boucle de texte : voyelles de 20 000 mots (`textloop.q`) | 0,50 s | 0,12 s | ×4 |
| 600 000 appels d'une méthode qui change des champs (`struct.q`) | 0,58 s | 0,13 s | ×4,4 |
| `Result` et `?` : appels en chaîne (`result.q`) | 1,20 s | 0,20 s | ×5,9 |
| textes et tableaux de textes (`strings.q`) | 0,13 s | 0,05 s | ×2,6 |
| dictionnaires : comptage de mots (`map.q`) | 0,27 s | 0,11 s | ×2,4 |
| ensembles (`set.q`) | 0,27 s | 0,19 s | ×1,4 |
| énumérations et `match` : aires de formes, classement (`enum.q`) | 0,62 s | 0,12 s | ×5,3 |

**Les instructions elles-mêmes.** Trois réglages sur la boucle d'exécution de la machine (environ
×1,5 de plus sur les boucles) : les constantes qui servent d'opérandes sont chargées une fois dans
des registres au début de la fonction ou de la boucle ; une comparaison suivie d'un saut
(`while i < n`, `if a == b`, `for`) est une seule instruction (`JnLtI`, `ForCheck`...) ; les pas
d'évaluation ne sont plus comptés à chaque instruction mais à chaque tour de boucle et à chaque
appel, ce qui borne tout aussi bien une boucle infinie ou une récursion sans fin. Ce qui reste
est surtout le coût de la division (`mod`, `div`) et de l'aiguillage des instructions.

**Ce qui est garanti.** Les deux moteurs donnent le même résultat : mêmes sorties, mêmes erreurs
(`R010` dépassement, `R011` division par zéro, `R013`, `R014`, `R031`...), mêmes lignes et mêmes
piles d'appels, mêmes limites (`R900`, `R901`). `tests/vm_differential.rs` le vérifie sur des
programmes numériques aléatoires (`Program::run_tree_walker` sert de référence ; le balayage
complet de 10 000 programmes se lance avec `--ignored`) et `tests/vm_cases.rs` sur des cas écrits
à la main. `RunResult::vm_functions` dit combien de fonctions et de boucles ont été compilées. Avec `QLANG_VM_DUMP=1`, le programme affiche sur la sortie d'erreur les instructions de chaque fonction et boucle compilée.

**Ce qui reste.** Les fermetures, les structures génériques et les valeurs de types plus rares
(identifiants, dates, tuples) ne sont pas compilés. Une étape suivante, hors du navigateur, serait de produire du code machine à partir de ces mêmes
instructions (un JIT natif).

## Ce qui reste (avant la machine à registres)

Le temps est maintenant réparti dans `eval`, `binary` et la recherche des variables par nom :
c'est le coût du parcours d'arbre lui-même. Pour aller nettement plus loin il faut changer de
modèle :

1. **Variables par emplacement** : le vérificateur sait déjà dans quelle portée chaque nom
   est déclaré ; l'interpréteur pourrait lire un emplacement numéroté d'un cadre d'appel au
   lieu de chercher un nom. Le cache ci-dessus en prend la plus grande part du gain sur la
   recherche ; ce qui resterait à gagner est l'allocation d'une portée par appel (deux
   allocations : la portée et son tableau) et le nom copié à chaque `define`. Les fonctions
   anonymes qui capturent des variables demandent un traitement particulier.
2. **Bytecode** : compiler chaque fonction vers une suite d'instructions pour une machine à
   pile. Plus gros chantier, gain plus grand (de l'ordre de 3 à 5 fois), et il faut garder le
   décompte des pas, les limites mémoire et les messages d'erreur avec leur position.
