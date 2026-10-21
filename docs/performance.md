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

## Ce qui reste

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
