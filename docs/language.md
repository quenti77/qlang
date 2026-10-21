# Le langage qlang

qlang est un langage simple, lisible et typé statiquement. Il est pensé pour
apprendre à programmer sans rester prisonnier d'un pseudo-code : sa syntaxe est
en anglais et ressemble aux vrais langages (Rust, TypeScript, Lua) tout en
restant sobre.

Ce document décrit le langage tel qu'il est implémenté. La grammaire
[`qlang.g4`](../qlang.g4) en donne un aperçu indicatif de la syntaxe, et le
[README](../README.md) explique comment l'utiliser (terminal, JSON, HTTP).

## Sommaire

- [Le langage qlang](#le-langage-qlang)
  - [Sommaire](#sommaire)
  - [1. Premier programme](#1-premier-programme)
  - [2. Commentaires](#2-commentaires)
  - [3. Valeurs et types](#3-valeurs-et-types)
    - [Nombres](#nombres)
    - [Chaînes](#chaînes)
    - [`none` et les types nullables](#none-et-les-types-nullables)
    - [Conversions](#conversions)
  - [4. Variables](#4-variables)
    - [Valeurs et références](#valeurs-et-références)
    - [Affectation](#affectation)
  - [5. Opérateurs](#5-opérateurs)
    - [Précédence](#précédence)
    - [Calculs sur les nombres](#calculs-sur-les-nombres)
    - [Comparaisons et logique](#comparaisons-et-logique)
    - [Intervalles](#intervalles)
    - [Les opérateurs sont des méthodes](#les-opérateurs-sont-des-méthodes)
  - [6. Conditions](#6-conditions)
    - [`if`](#if)
    - [`if` comme expression](#if-comme-expression)
    - [`match`](#match)
  - [7. Boucles](#7-boucles)
  - [8. Fonctions](#8-fonctions)
    - [Arguments nommés](#arguments-nommés)
    - [Fonctions anonymes](#fonctions-anonymes)
    - [Type d'une fonction](#type-dune-fonction)
    - [Déclarations locales](#déclarations-locales)
  - [9. Structures, méthodes et héritage](#9-structures-méthodes-et-héritage)
    - [Déclarer une structure](#déclarer-une-structure)
    - [Créer une valeur](#créer-une-valeur)
    - [Méthodes avec `impl`](#méthodes-avec-impl)
    - [Visibilité](#visibilité)
    - [Héritage](#héritage)
    - [Copier une valeur](#copier-une-valeur)
    - [Le nom du type : `.class`](#le-nom-du-type--class)
  - [10. Traits et surcharge d'opérateurs](#10-traits-et-surcharge-dopérateurs)
    - [Méthode par défaut](#méthode-par-défaut)
    - [Un trait comme type](#un-trait-comme-type)
    - [Surcharger un opérateur](#surcharger-un-opérateur)
    - [Surcharger une conversion](#surcharger-une-conversion)
    - [Indexation et ajout](#indexation-et-ajout)
  - [11. Énumérations](#11-énumérations)
  - [12. Génériques](#12-génériques)
    - [Bornes](#bornes)
    - [Inférence](#inférence)
  - [13. Modules](#13-modules)
  - [Paquets](#paquets)
  - [Fonctions fournies par l'environnement (`extern`)](#fonctions-fournies-par-lenvironnement-extern)
  - [14. Bibliothèque standard](#14-bibliothèque-standard)
    - [Dépendance à l'environnement](#dépendance-à-lenvironnement)
    - [Fonctions prédéfinies](#fonctions-prédéfinies)
    - [Méthodes prédéfinies](#méthodes-prédéfinies)
    - [Dictionnaires](#dictionnaires)
  - [15. Temps, identifiants et expressions régulières](#15-temps-identifiants-et-expressions-régulières)
    - [Heure et aléatoire](#heure-et-aléatoire)
    - [Identifiants : `uuid` et `ulid`](#identifiants--uuid-et-ulid)
    - [Dates et heures : `datetime`](#dates-et-heures--datetime)
    - [Expressions régulières](#expressions-régulières)
  - [16. Prélude : `Result`, `Error` et JSON](#16-prélude--result-error-et-json)
    - [Prélude : `Result` et `Error`](#prélude--result-et-error)
    - [Prélude : JSON](#prélude--json)
  - [17. Prélude : réseau, bases de données et système](#17-prélude--réseau-bases-de-données-et-système)
    - [Prélude : `http`](#prélude--http)
    - [Prélude : `Db`](#prélude--db)
    - [Prélude : fichiers et environnement](#prélude--fichiers-et-environnement)
  - [18. Fins de ligne](#18-fins-de-ligne)
  - [19. Mots-clés réservés](#19-mots-clés-réservés)
  - [20. Erreurs et limites](#20-erreurs-et-limites)
  - [21. Points encore ouverts](#21-points-encore-ouverts)

---

## 1. Premier programme

```
let name = "monde"
print("Bonjour {name} !")

for i in 0..3 do
  print("i = {i}")
end
```

Un programme est une suite d'instructions, exécutées de haut en bas. Il n'y a
pas de fonction `main`.

---

## 2. Commentaires

Les commentaires commencent par `--`, un caractère facile à taper sur un clavier
AZERTY comme QWERTY.

```
-- commentaire sur une ligne
let x = 1 -- ou en fin de ligne

--(
  commentaire sur
  plusieurs lignes
--)

--"
  commentaire de documentation
--"
```

- Les commentaires ne s'imbriquent pas.
- `--` suivi de `(` ou de `"` ouvre toujours un commentaire de bloc ou de
  documentation, jamais un commentaire de ligne.
- Attention : `a --b` est un commentaire. Pour soustraire un nombre négatif,
  écrire `a - -b`.

### Commentaires de documentation

Un commentaire `--" … --"` placé **juste au-dessus** d'une déclaration (sans ligne vide
entre les deux) la **documente**. Le programme s'exécute comme avec n'importe quel
commentaire, mais le compilateur garde le texte et le rattache à la déclaration : les
éditeurs l'affichent au survol et dans l'autocomplétion.

```
--" Distance entre deux points, en ligne droite. --"
fun distance(a: Point, b: Point) -> float
  ...
end

--"
  Un point du plan.

  Les coordonnées sont des entiers.
--"
struct Point
  --" Position horizontale. --"
  public x: int
  public y: int
end
```

On peut documenter : une fonction (ou une méthode, une signature de `trait`), une `struct`,
un champ, une `enum` et chacune de ses variantes, un `trait`, et une variable ou
constante de premier niveau. Un commentaire ne documente que la déclaration qui le
suit ; avec une ligne vide entre les deux, il ne documente rien. Le texte est libre
(Markdown possible) : les espaces communs en début de ligne et les lignes vides au
début et à la fin sont retirés. `qlang check --json` avec `"info": true` renvoie ces
textes (champ `doc`).

`qlang doc` en fait une page Markdown : pour chaque fichier, ses noms exportés (tous s'il
n'a pas de `export`) avec leur signature et leur commentaire, puis les champs, méthodes et
variantes `public` de leurs types. `qlang doc` seul documente le projet (sans les fichiers
de test) ; `qlang doc lib.q` un fichier ; `-o API.md` écrit dans un fichier.

---

## 3. Valeurs et types

qlang est **typé statiquement** : chaque valeur a un type connu avant
l'exécution, et toutes les erreurs de types sont signalées avant que le
programme ne démarre. Le plus souvent, le type est **inféré** et n'a pas besoin
d'être écrit.

| Type       | Exemples                                | Remarque         |
| ---------- | --------------------------------------- | ---------------- |
| `int`      | `42`, `1_000`, `0xFF_FF`, `0b0101_0101` | entier 64 bits   |
| `float`    | `3.14`, `2.5e3`, `1_000.5`              | nombre à virgule |
| `bool`     | `true`, `false`                         |                  |
| `string`   | `"texte"`                               | non modifiable   |
| `T?`       | `int?`, `string?`                       | `T` ou `none`    |
| `array<T>` | `[1, 2, 3]`                             | tableau de `T`   |
| `map<K, V>` | `{ "a": 1, "b": 2 }`                   | dictionnaire     |

Les types de base s'écrivent en minuscules. Les dictionnaires sont décrits dans
la [section 14](#dictionnaires). Les types que tu définis
(`struct`, `trait`, `enum`) prennent une majuscule par convention.

### Nombres

- `int` et `float` sont deux types distincts : `1` est un `int`, `1.0` est un
  `float`.
- Les nombres en base 2 (`0b…`) et en base 16 (`0x…`) sont acceptés.
- Le caractère `_` peut séparer des chiffres pour la lisibilité : `1_000`,
  `0xFF_FF`, `0b0101_0101`. Il doit se trouver **entre deux chiffres**.
- Un `float` a toujours un chiffre après le point : `0..10` est l'intervalle de
  0 à 10, pas un nombre.
- Un `int` ne dépasse pas 64 bits : un calcul qui sort de cette plage est une
  erreur (« integer overflow »), jamais un résultat faux en silence.

### Chaînes

Une chaîne s'écrit entre guillemets doubles et peut s'étendre sur plusieurs
lignes.

```
let a = "Bonjour"
let b = "ligne 1\nligne 2"
```

Échappements : `\n` `\t` `\r` `\0` `\\` `\"` `\{` `\}`.

**Interpolation.** Une expression entre accolades est évaluée et insérée dans la
chaîne. C'est une aide : la concaténation avec `+` fonctionne aussi.

```
let x = 2
let y = 3
print("{x} + {y} = {x + y}")
```

L'expression à l'intérieur de `{ }` ne peut pas contenir de guillemet `"` non
échappé. Pour afficher une accolade, écrire `\{`. Toute valeur peut être
interpolée : elle est convertie avec `as string` (voir
[Conversions](#conversions)).

Les chaînes se comparent avec `==` et `<`, se concatènent avec `+`, et
s'indexent par caractère : `"héllo"[1]` vaut `"é"`. `len()` compte les
caractères, pas les octets.

### `none` et les types nullables

Une variable ne peut valoir `none` que si son type se termine par `?`.

```
let a: int = none    -- erreur
let b: int? = none   -- ok
```

On ne peut pas utiliser une valeur nullable comme si elle n'était jamais `none`.
Il faut d'abord le vérifier : après un test, le compilateur sait que la valeur
est présente.

```
let age: int? = int.parse("12")

if age != none then
  print(age + 1)      -- ici age est un int
end

if age == none then
  return              -- on sort : après ce if, age est un int
end
print(age + 1)

print(age != none and age > 10)   -- à droite de `and`, age est un int
```

Ce rétrécissement fonctionne pour les variables (locales, paramètres,
variables de fichier) avec `==`/`!=` contre `none`, combinés par `and`, `or` et
`not`. Une affectation le met à jour : après `x = 5`, `x` est présent ; après
`x = none` ou `x = autre_nullable`, il faut revérifier. Il ne s'applique pas aux
champs (`obj.champ`) ni aux résultats d'appels : copier d'abord la valeur dans
une variable, puis tester la variable. Comparer à `none` est toujours permis.

Une variable qu'une **fonction ou une fonction anonyme affecte** (`x = ...` dans son
corps, sans l'y avoir déclarée) n'est jamais rétrécie : un appel entre le test et
l'usage pourrait la remettre à `none`. Il faut alors copier sa valeur dans une variable
locale avant de la tester.

Appeler une méthode ou lire un champ sur une valeur qui peut être `none` est une
erreur de compilation.

### Tuples

Un **tuple** regroupe un nombre fixe de valeurs, de types différents, sans déclarer de
structure. Son type s'écrit `(int, string)`, sa valeur `(1, "a")` : au moins deux éléments.
On lit un élément par sa **position**, à partir de 0 :

```
fun bornes(xs: array<int>) -> (int, int)
  (xs[0], xs[xs.len() - 1])
end

let b = bornes([3, 1, 2])
print(b)             -- (3, 2)
print(b.0)           -- 3
print(b.1)           -- 2
let nested = (1, (2.5, true))
print(nested.1.0)    -- 2.5
```

- Un tuple **ne change pas** : `t.0 = 5` est refusé ; on en construit un nouveau.
- `==` et `!=` comparent les éléments un à un ; l'affichage est `(1, "a")`. `<`, `<=`, `>`,
  `>=`, `sort`, `min` et `max` les comparent aussi, élément par élément en commençant par le
  premier (`(1, 9) < (2, 0)`), tant que chaque élément peut être ordonné (pas `bool`, ni les
  nullables) : `[(2, "b"), (1, "z")].sort()` donne `[(1, "z"), (2, "b")]`.
- Un tuple peut servir de clé de dictionnaire et de valeur d'ensemble quand ses éléments le
  peuvent : `{ (0, 0): "origine" }`, `Set.of([(0, 0), (1, 2)])`.
- Un type de tuple accepte un élément plus large : `let t: (int?, string) = (5, "a")`.
- Une position qui n'existe pas (`(1, 2).2`) est une erreur de compilation (`T286`), comme
  `.0` sur autre chose qu'un tuple (`T287`). Un seul élément n'est pas un tuple : `(1)` est
  juste `1` et `(1,)` est refusé.

**Déstructurer.** `let (a, b) = t` prend un tuple en morceaux, et `for (a, b) in …` fait de
même pour chaque élément :

```
let (lo, hi) = bornes([3, 1, 2])          -- lo = 3, hi = 2
let (nom, (x, y)) = ("p", (1, 2))         -- motifs imbriqués
let (_, hi2) = bornes([1, 2])             -- _ ignore un élément
const (k, v): (string, int) = ("a", 1)    -- const, et type facultatif

let gens: array<(string, int)> = [("ana", 31), ("bob", 27)]
for (nom, age) in gens do
  print("{nom} a {age} ans")
end
for i, (nom, _) in gens do                -- l'indice, puis l'élément déstructuré
  print("{i}: {nom}")
end
```

**Affecter.** À gauche d'un `=`, un tuple de cibles reçoit les éléments d'un tuple ; la valeur est
calculée en entier avant que la première cible ne change, donc `(a, b) = (b, a)` échange :

```
(a, b) = (b, a)                      -- échange sans variable temporaire
(xs[0], xs[2]) = (xs[2], xs[0])      -- aussi avec des éléments de tableau
(p.x, p.y) = (p.y, p.x + p.y)        -- et des champs
(cur, next) = (next, cur + next)     -- suite de Fibonacci en une ligne
((q, _), r) = ((5, 6), 7)            -- motifs imbriqués, `_` ignore un élément
```

Chaque cible doit pouvoir être affectée (une constante est refusée, `T210`) ; `+=` et les
autres opérateurs ne s'appliquent pas à un tuple.

Le motif doit avoir autant de noms que le tuple a d'éléments (`T288`) et ne s'applique qu'à un
tuple (`T289`). Les variables obtenues sont des variables ordinaires (`const` les fige), au
niveau d'un fichier elles s'exportent comme les autres.

### Conversions

Il n'y a pas de conversion implicite entre `int` et `float` pour l'affectation
ou les paramètres. On convertit explicitement avec `as` :

```
let n = 7
let f = n as float
let i = (f * 2.5) as int   -- la partie décimale est perdue : 17
let s = n as string
```

`as` fonctionne pour tout type grâce au trait `As<T>` : `x as T` est du sucre
pour l'appel de la méthode `convert` de l'implémentation `As<T>` du type de `x`.
Le langage fournit :

- `int` vers `float`, et `float` vers `int` (troncature ; erreur si le nombre
  est infini ou trop grand) ;
- tout type vers `string` (nombres, booléens, `none`, tableaux, enums ; une
  structure s'affiche `Point { x: 1, y: 2 }` sauf si elle implémente
  `As<string>`) ;
- un type vers lui-même ou vers un type parent.

Tu peux en ajouter pour tes types, voir
[section 10](#surcharger-une-conversion).

`as` est réservé aux conversions qui **réussissent toujours**. Une conversion qui
peut échouer (comme lire un nombre dans un texte) renvoie un type nullable :
`int.parse(s)` donne un `int?`.

---

## 4. Variables

Une variable est toujours **initialisée** à sa déclaration.

```
let count = 0            -- modifiable, type inféré (int)
let ratio: float = 0.5   -- type écrit
const PI = 3.14          -- non réassignable
let name: string? = none -- variable « vide » : il faut un type nullable
```

- `let` déclare une variable modifiable.
- `const` déclare un nom qui ne peut pas être réassigné. Sa valeur peut être
  calculée à l'exécution (`const size = 2 * 10`). Seul le **nom** est fixé : le
  contenu d'un tableau ou d'une structure reste modifiable.
- Il n'y a pas de variable non initialisée : pour représenter l'absence de
  valeur, utiliser un type nullable et `none`.
- Déclarer deux fois le même nom dans la même portée est une erreur. Dans un bloc
  intérieur, on peut en revanche réutiliser un nom (il masque l'autre).

```
const t = [0]
t[0] = 2        -- ok : on modifie le contenu
t = [1]         -- erreur : on réassigne le nom

const p = Point { x: 1, y: 2 }
p.x = 5         -- ok
p = Point { x: 0, y: 0 }  -- erreur
```

### Valeurs et références

Les tableaux et les structures sont des **références** : affecter ou passer en
argument ne copie pas, cela **partage** le même objet.

```
let a = [1, 2, 3]
let b = a
b[0] = 99
print(a[0])   -- 99 : a et b désignent le même tableau

fun reset(xs: array<int>)
  xs[0] = 0   -- modifie le tableau de l'appelant
end
```

Les `int`, `float`, `bool` et `none` sont de simples valeurs, copiées à chaque
affectation. Une `string` ne se modifie pas sur place : une opération qui la
« change » produit une nouvelle chaîne.

### Affectation

`=` et les opérateurs composés `+=` `-=` `*=` `/=` `div=` `mod=` `%=` modifient une variable,
un champ (`a.b = 1`) ou un élément (`a[i] = 1`). L'affectation est une
expression : `a = b = 0` est valide. `x op= y` signifie `x = x op y`, et le type
du résultat doit convenir à `x`.

```
count += 1
a[0] = 42
```

Attention : `/` donne toujours un `float`. Écrire `n /= 2` pour un `int` est donc
une erreur ; on écrit `n div= 2` (ou `n = n div 2`). `div=` s'écrit collé, sans
espace entre `div` et `=` (de même pour `mod=`).

---

## 5. Opérateurs

### Précédence

De la plus faible à la plus forte :

| Opérateurs                                | Remarque                                                     |
| ----------------------------------------- | ------------------------------------------------------------ |
| `=` `+=` `-=` `*=` `/=` `div=` `mod=` `%=` | affectation, associative à droite                           |
| `..` `..=`                                | intervalle, non associatif                                   |
| `or`                                      |                                                              |
| `and`                                     |                                                              |
| `not`                                     | préfixe                                                      |
| `==` `!=` `<` `<=` `>` `>=`               | non associatif : `a < b < c` est interdit                    |
| `+` `-`                                   |                                                              |
| `*` `/` `div` `mod` `%`                   | `%` est un synonyme de `mod`                                 |
| `as`                                      | conversion de type                                           |
| `-x`                                      | négation (préfixe)                                           |
| `**`                                      | puissance, associative à droite : `-2 ** 2` vaut `-(2 ** 2)` |
| `f()` `a[i]` `a.b`                        | appel, index, accès                                          |

### Calculs sur les nombres

| Expression | Résultat | Remarque                                   |
| ---------- | -------- | ------------------------------------------ |
| `7 / 2`    | `3.5`    | `/` donne toujours un `float`              |
| `7 div 2`  | `3`      | division entière (vers zéro) : `-7 div 2` vaut `-3` |
| `7 mod 2`  | `1`      | reste : du signe du premier nombre         |
| `2 ** 10`  | `1024`   | deux `int` donnent un `int`                |
| `2.0 ** 0.5` | `1.414…` | sinon, un `float`                        |

- `div` ne s'applique qu'à deux `int`. `mod` s'applique aussi aux `float`
  (`5.5 mod 2` vaut `1.5`).
- Diviser par zéro (`/`, `div`, `mod`) est une erreur d'exécution.
- Un exposant négatif avec deux `int` est une erreur (`2 ** -1`) : écrire
  `2.0 ** -1.0`.
- **`int` et `float` se mélangent** pour le calcul et la comparaison : `1 + 2.5`
  vaut `3.5` (un `float`). Cela vient d'implémentations de traits fournies par
  le langage (`impl Add<float> for int`, etc.), et tu peux définir les tiennes.

### Comparaisons et logique

- `==` et `!=` fonctionnent sur les nombres (`1 == 1.0` est vrai), les
  booléens, les chaînes, les énumérations, les tableaux (élément par élément) et
  les valeurs nullables (`x == none`). Pour comparer deux structures, il faut
  implémenter le trait `Eq`.
- `<`, `<=`, `>`, `>=` fonctionnent sur les nombres et les chaînes (ordre
  alphabétique des caractères Unicode). Pour une structure, il faut
  implémenter `Ord` ; les tuples se comparent élément par élément.
- Une condition (`if`, `elseif`, `while`, garde de `match`) doit être de type
  `bool`. Il n'y a ni « vrai » ni « faux » implicite : `if 1 then` et
  `if name then` sont des erreurs.
- `and` et `or` n'évaluent leur côté droit que si c'est nécessaire.

### Intervalles

```
0..10     -- 0 à 9 (la borne de fin est exclue)
0..=10    -- 0 à 10 (la borne de fin est incluse)
```

Un intervalle est une valeur (type `range`) de deux `int`. On s'en sert
surtout avec `for`.

### Les opérateurs sont des méthodes

Les opérateurs sont définis par des **traits**. `a + b` est du sucre pour
`a.add(b)`, y compris pour `int` et `string` : `"a" + "b"` vaut `"a".add("b")`.

| Opérateur                | Trait      | Méthode      |
| ------------------------ | ---------- | ------------ |
| `+` `-` `*`              | `Add` `Sub` `Mul` | `add` `sub` `mul` |
| `/`                      | `Div`      | `divide`     |
| `div`                    | `IntDiv`   | `int_div`    |
| `mod` `%`                | `Mod`      | `modulo`     |
| `**`                     | `Pow`      | `pow`        |
| `-x`                     | `Neg`      | `neg`        |
| `==` `!=`                | `Eq`       | `eq`         |
| `<` `<=` `>` `>=`        | `Ord`      | `cmp` (renvoie un `int` : négatif, nul ou positif) |
| `+=` `-=` `*=` `/=` `div=` `mod=` `%=` | les mêmes traits | `a += b` signifie `a = a + b` |
| `a[i]`                   | `Index<I>` | `index`      |
| `a[i] = v`               | `IndexSet<I, V>` | `set_index` |
| `a[] = v`                | `Push<T>`  | `push`       |
| `x as T`                 | `As<T>`    | `convert`    |

Les traits des opérateurs binaires ont un paramètre pour l'opérande de droite,
qui vaut le type lui-même par défaut : `impl Add for Point` est
`impl Add<Point> for Point`. Un même type peut implémenter un trait pour
plusieurs opérandes (`impl Mul<int> for Point` et `impl Mul<float> for Point`).

Voir [section 10](#10-traits-et-surcharge-dopérateurs) pour les redéfinir sur ses
propres types.

---

## 6. Conditions

### `if`

```
if x > 0 then
  print("positif")
elseif x < 0 then
  print("négatif")
else
  print("nul")
end
```

Un seul `end` ferme toute la chaîne `if` / `elseif` / `else`. Tout tient sur une
ligne si besoin : `if ok then print("ok") end`.

### `if` comme expression

Il n'y a pas d'opérateur ternaire : `if` est une expression. La valeur est celle
de la dernière expression de la branche choisie. Dans ce cas, `else` est
obligatoire, et les branches doivent avoir le même type (ou un type commun :
`1` et `none` donnent `int?`).

```
let sign = if x >= 0 then 1 else -1 end
```

Un `if` sans `else` n'a pas de valeur.

### `match`

```
let label = match n
  case 0 then "zéro"
  case 1..=9 then "petit"
  case x if x > 100 then "énorme"
  else "autre"
end
```

Motifs possibles dans un `case` :

| Motif                  | Exemple                                          |
| ---------------------- | ------------------------------------------------ |
| Valeur littérale       | `case 42 then`, `case "a" then`, `case true then` |
| `none`                 | `case none then` (pour une valeur nullable)      |
| Variante d'énumération | `case Color.Red then`, `case Red then`            |
| Variante avec données  | `case Circle(r) then`, `case Shape.Rect(w, _) then` |
| Tuple                  | `case (0, y) then`, `case ("get", 200..=299) then`, `case (a, b) then` |
| Intervalle             | `case 1..=9 then`, `case 0.5..1.5 then`          |
| Liaison + garde        | `case x if x > 100 then` (`x` désigne la valeur testée) |

Une variante peut s'écrire avec ou sans le nom de l'énumération quand celle-ci est connue
par la valeur testée (`case Red`, `case Circle(r)`) ; un nom qui est une variante de ce
type est la variante, sinon c'est une liaison. Pour une variante qui porte des données,
on écrit un motif par valeur entre parenthèses (`_` pour l'ignorer) : le nombre de motifs doit
être celui des valeurs.

Les motifs **s'imbriquent** : un motif de tuple a un sous-motif par élément, une variante
avec données un sous-motif par valeur portée, et chaque sous-motif est lui-même un motif
(valeur, intervalle, `none`, nom, `_`, tuple, variante). Les noms sont liés dans la garde et
dans le corps.

```
match (shape, color)
  case (Circle(r), Red) then print("cercle rouge de rayon {r}")
  case (Rect(w, _), _) then print("rectangle de largeur {w}")
  else print("autre")
end

match result
  case Ok((n, name)) then print("{n} {name}")
  case Ok(0) then print("zéro")      -- un littéral dans une variante
  case Err(e) then print(e.message)
end
```

Le compilateur vérifie que les `case` **couvrent** toutes les valeurs, en comptant les
variantes, `true`/`false` et `none` jusque dans les motifs imbriqués : `(Red, Red)`,
`(Red, Green)` et `(Green, _)` suffisent pour deux couleurs. Les valeurs littérales et les
intervalles ne couvrent jamais un type (il faut alors un `else` ou un nom), et un `case` avec
garde n'est pas compté. Dans un tuple, un nom ne retire pas le `none` d'un élément nullable
(seul un `case none` au premier niveau le fait).

Les `case` sont essayés dans l'ordre. Un nom seul (`case x then`) accepte toute
valeur et la nomme ; `_` accepte toute valeur sans la nommer. Après un
`case none`, un nom lié ne peut plus être `none`.

Comme `if`, `match` est une expression. Pour utiliser sa valeur, il faut qu'il
soit **exhaustif** : soit un `else`, soit un `case` qui accepte tout, soit toutes
les variantes d'une énumération (ou `true` et `false`), sans garde. Sinon il n'a
pas de valeur. Il faut un `case` par valeur : on ne peut pas écrire
`case 1, 2, 3`.

---

## 7. Boucles

```
while count < 10 do
  count += 1
end

for i in 0..10 do         -- 0 à 9
  print(i)
end

for i in 0..=10 step 2 do -- 0, 2, 4, 6, 8, 10
  print(i)
end

for i in 10..0 step -3 do -- 10, 7, 4, 1
  print(i)
end

for item in items do      -- parcours d'un tableau
  print(item)
end

for c in "abc" do         -- parcours d'une chaîne, caractère par caractère
  print(c)
end

for i, item in items do   -- avec l'indice : 0, 1, 2…
  print("{i}: {item}")
end

for key, value in ages do -- parcours d'un dictionnaire : clé et valeur
  print("{key} a {value} ans")
end
```

- `break` quitte la boucle, `continue` passe à l'itération suivante.
- Il n'y a pas de `loop` : écrire `while true do … end`.
- `break` et `continue` n'ont pas d'étiquette : ils concernent la boucle la plus
  proche.
- **Deux variables** : sur un tableau ou une chaîne, `for i, x in …` donne l'indice
  (un `int`, à partir de 0) puis l'élément ; sur un dictionnaire, `for k, v in m`
  donne la clé puis la valeur, dans l'ordre d'insertion. Un intervalle n'a pas de
  deuxième variable. Boucler sur un dictionnaire avec une seule variable est une
  erreur (clés ou valeurs ?) : écrire `for k, v in m` ou `for k in m.keys()`.
  La boucle sur un dictionnaire travaille sur une copie de ses entrées : on peut
  le modifier dans le corps.
- `step` ne s'utilise qu'avec un intervalle. Un pas positif avance tant que la
  valeur est inférieure à la fin, un pas négatif tant qu'elle est supérieure. Le
  pas ne peut pas être 0. Sans `step`, le pas est 1 : `5..1` ne fait aucun tour.
- Le tableau parcouru peut être modifié pendant la boucle ; la boucle voit les
  éléments ajoutés.

---

## 8. Fonctions

```
fun add(a: int, b: int) -> int
  a + b
end

print(add(1, 2))
```

- Les paramètres sont typés. `-> type` indique le type de retour ; sans lui, la
  fonction ne renvoie pas de valeur.
- **La valeur de retour est la dernière expression du corps**, comme pour `if`.
- `return` est facultatif et sert aux sorties anticipées :

```
fun first_positive(xs: array<int>) -> int?
  for x in xs do
    if x > 0 then return x end
  end
  none
end
```

### Arguments nommés

Un appel peut nommer ses arguments avec `nom: valeur`, dans n'importe quel
ordre :

```
fun greet(greeting: string, name: string) -> string
  greeting + " " + name
end

print(greet(name: "ana", greeting: "hi"))   -- hi ana
print(greet("hi", name: "ana"))             -- mélange positionnel + nommé
```

Les arguments positionnels doivent précéder les arguments nommés, chaque
paramètre ne peut être rempli qu'une fois, et tous les paramètres doivent
être fournis. Les arguments nommés ne sont possibles que pour appeler une
fonction ou une méthode déclarée dans le code qlang (pas les fonctions ou
méthodes natives comme `print` ou `.sort()`).

### Fonctions anonymes

```
let double = fun(x: int) -> int
  x * 2
end
```

Une fonction anonyme est une **fermeture** : elle garde accès aux variables de la
fonction qui l'entoure, et peut les modifier.

```
fun counter() -> fun() -> int
  let n = 0
  fun() -> int
    n += 1
    n
  end
end

let next = counter()
next()
print(next())   -- 2
```

Une fonction anonyme ne peut pas être générique.

### Type d'une fonction

Le type d'une fonction s'écrit comme sa déclaration :

```
fun apply(f: fun(int) -> int, x: int) -> int
  f(x)
end

print(apply(double, 21))
```

`fun(int) -> int?` désigne une fonction qui renvoie un `int?`. Pour une fonction
elle-même nullable, utiliser des parenthèses : `(fun(int) -> int)?`.

### Déclarations locales

Les déclarations `fun` nommées existent seulement au niveau du fichier. Dans un
bloc, une fonction locale s'écrit `let f = fun(…) … end`. Il en va de même pour
`struct`, `impl`, `trait`, `enum`, `import` et `export`.

---

## 9. Structures, méthodes et héritage

### Déclarer une structure

```
struct Point
  public x: int
  public y: int
  private label: string = "point"
  private static count: int = 0
end
```

Chaque champ s'écrit `visibilité? static? nom: type (= défaut)?`.

- `static` marque un champ qui appartient au type et non à chaque instance. Un
  champ `static` doit avoir une valeur initiale. On l'utilise avec le nom du
  type : `Point.count`.
- Un champ avec une valeur par défaut peut être omis à la création.

### Créer une valeur

Un littéral de structure nomme tous les champs qui n'ont pas de valeur par
défaut :

```
let p = Point { x: 1, y: 2 }

let x = 5
let q = Point { x, y: 3 }        -- raccourci : x équivaut à x: x
let r = Point { ..p, y: 9 }      -- reprend les champs de p, sauf y
```

`..valeur` reprend les champs d'une valeur du même type (ou d'un type parent).
C'est une copie superficielle : les tableaux et structures qu'elle contient sont
partagés.

### Méthodes avec `impl`

Les méthodes s'écrivent dans un bloc `impl`.

```
impl Point
  public fun new(x: int, y: int) -> Point
    Point { x: x, y: y }
  end

  public fun origin() -> Point
    Point.new(0, 0)
  end

  public fun length_squared(self) -> int
    self.x * self.x + self.y * self.y
  end
end

let p = Point.new(3, 4)
print(p.length_squared())  -- 25
```

- Une méthode dont le **premier paramètre est `self`** est une méthode
  d'instance : `p.length_squared()`.
- Sans `self`, c'est une méthode **statique**, appelée sur le type :
  `Point.origin()`.
- Il n'y a pas de constructeur spécial : `new` est une méthode statique comme
  une autre.
- Plusieurs blocs `impl` peuvent coexister pour un même type, tant que les noms
  de méthodes sont distincts.
- Un champ qui contient une fonction s'appelle comme une méthode :
  `obj.callback(3)`.

### Visibilité

| Visibilité  | Accessible depuis                                           |
| ----------- | ----------------------------------------------------------- |
| `public`    | partout                                                     |
| `protected` | les `impl` de la structure et de ses sous-structures        |
| `private`   | les `impl` de la structure elle-même (valeur par défaut)    |

Cela s'applique aux champs, aux champs `static` et aux méthodes. En particulier,
un champ `private` ne peut pas être donné dans un littéral de structure en
dehors des `impl` de ce type : on passe par une méthode comme `new`.

### Héritage

Une structure peut hériter d'**une seule** autre structure.

```
struct Account
  protected balance: int = 0
end

struct Savings extends Account
  public rate: float = 0.02
end

impl Account
  public fun describe(self) -> string
    "solde : {self.balance}"
  end
end

impl Savings
  public fun open(rate: float) -> Savings
    Savings { ..Account { }, rate }
  end

  public override fun describe(self) -> string
    super.describe() + " (épargne)"
  end
end
```

- Une `Savings` peut être utilisée partout où une `Account` est attendue ; les
  méthodes redéfinies s'appliquent alors (liaison dynamique).
- Redéfinir une méthode d'instance du parent demande le mot-clé **`override`**,
  avec la même liste de paramètres. L'oublier, ou l'écrire sans méthode à
  redéfinir, est une erreur. Les méthodes statiques ne se redéfinissent pas.
- `super.méthode(…)` appelle la version du parent.
- Un champ ne peut pas reprendre le nom d'un champ du parent.

### Copier une valeur

Il n'y a pas de `copy()` ou `clone()` prédéfini : comme `new`, c'est à toi de
l'écrire si tu en as besoin.

```
impl Point
  public fun clone(self) -> Point
    Point { x: self.x, y: self.y }
  end
end
```

### Le nom du type : `.class`

`Point.class` donne le nom d'une structure ou d'une énumération, sous forme de
texte, à la compilation :

```
print(Point.class) --> "Point"
```

`valeur.class()` donne le même nom, mais à l'exécution, à partir d'une valeur
concrète. C'est surtout utile quand le type statique de `valeur` ne dit pas
tout : un paramètre générique borné par un trait (`T: Nameable`), ou une
variable typée par un trait, peuvent cacher des types concrets différents
selon l'appel :

```
trait Nameable
  fun name(self) -> string
end

struct Dog
  public n: string
end

impl Nameable for Dog
  public fun name(self) -> string
    self.n
  end
end

struct Holder<T: Nameable>
  public item: T
end

impl<T: Nameable> Holder<T>
  public fun describe(self) -> string
    "{self.item.class()}: {self.item.name()}"
  end
end

print(Holder<Dog> { item: Dog { n: "Rex" } }.describe()) --> "Dog: Rex"
```

Si un type définit lui-même une méthode `class`, c'est elle qui est appelée :
`.class()` est un comportement par défaut, pas un mot réservé.

---

## 10. Traits et surcharge d'opérateurs

Un **trait** décrit des méthodes qu'un type doit proposer. Il ne contient que des
signatures.

```
trait Shape
  fun area(self) -> float
  fun name(self) -> string
end

struct Circle
  public r: float
end

impl Shape for Circle
  public fun area(self) -> float
    3.14 * self.r * self.r
  end

  public fun name(self) -> string
    "cercle"
  end
end
```

Une implémentation doit définir toutes les méthodes du trait, avec les mêmes
types — sauf celles qui ont une **méthode par défaut** (voir plus bas). On
peut implémenter un trait pour n'importe quel type, y compris `int`, `string`
ou `array<T>` (`impl Describe for int`).

### Méthode par défaut

Une signature de trait peut avoir un corps : une implémentation qui ne
redéfinit pas la méthode utilise alors ce corps par défaut.

```
trait Greeter
  fun name(self) -> string

  fun greet(self) -> string
    "Hello, " + self.name() + "!"
  end
end

struct Dog
end

impl Greeter for Dog
  public fun name(self) -> string
    "Rex"
  end
  -- pas de `greet` : utilise le comportement par défaut
end

print(Dog { }.greet())   -- Hello, Rex!
```

Dans le corps par défaut, `self` a le type du trait lui-même : on peut y
appeler les autres méthodes du trait (y compris d'autres méthodes abstraites,
comme `name()` ici), mais pas accéder à des champs.

### Un trait comme type

Un trait peut servir de type : une valeur de ce type est n'importe quelle valeur
qui l'implémente, et la méthode appelée est celle de sa vraie structure.

```
fun show(s: Shape)
  print("{s.name()} : {s.area()}")
end

let shapes: array<Shape> = [Circle { r: 1.0 }, Square { side: 2.0 }]
for s in shapes do
  show(s)
end
```

### Surcharger un opérateur

Chaque opérateur correspond à un trait prédéfini (voir
[section 5](#les-opérateurs-sont-des-méthodes)). Pour le redéfinir, on
implémente ce trait.

```
impl Add for Point
  public fun add(self, other: Point) -> Point
    Point { x: self.x + other.x, y: self.y + other.y }
  end
end

impl Mul<int> for Point            -- p * 3
  public fun mul(self, k: int) -> Point
    Point { x: self.x * k, y: self.y * k }
  end
end

impl Mul<Point> for int            -- 3 * p
  public fun mul(self, p: Point) -> Point
    p * self
  end
end

impl Eq for Point
  public fun eq(self, other: Point) -> bool
    self.x == other.x and self.y == other.y
  end
end

let p = Point.new(1, 2) + Point.new(3, 4)  -- Point { x: 4, y: 6 }
```

Pour `Eq` et `Ord`, la méthode reçoit une valeur du même type. Pour les autres
opérateurs, le type de retour est libre.

### Surcharger une conversion

`As<T>` est un trait générique : un même type peut l'implémenter pour plusieurs
cibles.

```
impl As<string> for Point
  public fun convert(self) -> string
    "({self.x}, {self.y})"
  end
end

let s = Point.new(1, 2) as string  -- "(1, 2)"
```

L'interpolation et `print` s'appuient sur la même conversion : `"{p}"` et
`print(p)` utilisent `As<string>` du type de `p`.

### Indexation et ajout

```
impl Index<int> for Bag
  public fun index(self, i: int) -> int
    self.items[i]
  end
end

impl IndexSet<int, int> for Bag
  public fun set_index(self, i: int, v: int)
    self.items[i] = v
  end
end

impl Push<int> for Bag
  public fun push(self, v: int)
    self.items[] = v
  end
end

bag[0]        -- Index
bag[0] = 5    -- IndexSet
bag[] = 6     -- Push
```

Les tableaux implémentent déjà ces trois traits : `xs[] = v` ajoute `v` à la fin
(comme `xs.push(v)`).

---

## 11. Énumérations

Une énumération est une liste de variantes. Une variante est un nom, seul ou avec des
valeurs qu'elle porte (des champs nommés, comme les paramètres d'une fonction) :

```
enum Color
  Red
  Green
  Blue
end

enum Shape
  Circle(radius: float)
  Rect(w: float, h: float)
  Empty
end

let c = Color.Green
let s = Shape.Circle(2.0)             -- ou Shape.Circle(radius: 2.0)

let area = match s
  case Circle(r) then 3.14 * r * r    -- r est la valeur portée
  case Shape.Rect(w, h) then w * h
  case Empty then 0.0
end

impl Color
  public fun is_warm(self) -> bool
    self == Color.Red
  end
end
```

- Une variante sans données s'écrit `Color.Red` (pas de parenthèses) ; une variante avec
  données se **construit** par un appel : `Shape.Circle(2.0)`, par position ou par nom
  comme n'importe quel appel. Écrire `Shape.Circle` seul, ou `Color.Red()`, est une erreur.
- Dans un `match`, le compilateur vérifie que toutes les variantes sont traitées (voir
  `match`). Les valeurs portées se lient par position, et chacune est un motif à part
  entière : `case Circle(0.0)`, `case Ok((n, s))`, `case Ok(Some(x))`… (voir `match`).
- Une énumération peut être **générique** : `enum Maybe<T> Just(value: T) Nothing end`
  (voir les génériques) ; `impl<T> Maybe<T> ... end` lui ajoute des méthodes, et un trait
  s'implémente comme pour une structure.
- Les valeurs s'affichent `Shape.Circle(2.0)`, `Color.Red`. `==` compare la variante puis
  les valeurs portées, une à une.
- Les variantes portent leurs valeurs sans les changer : une valeur d'énumération ne se
  modifie pas (un tableau qu'elle contient reste, lui, un tableau partagé).
- Seules les énumérations **sans aucune donnée** peuvent servir de clé de dictionnaire ou
  traverser une `extern fun` (sous le nom de leur variante).

---

## 12. Génériques

Les fonctions, structures, `impl` et traits peuvent être **génériques** : ils
fonctionnent avec plusieurs types, notés entre chevrons.

```
struct Pair<A, B>
  public first: A
  public second: B
end

impl<A, B> Pair<A, B>
  public fun swap(self) -> Pair<B, A>
    Pair { first: self.second, second: self.first }
  end
end

fun first_of<T>(xs: array<T>) -> T?
  if xs.len() == 0 then none else xs[0] end
end
```

### Bornes

Un paramètre de type peut être **contraint** par un ou plusieurs traits :

```
fun sum<T: Add>(a: T, b: T) -> T
  a + b
end

fun same<T: Eq + Ord>(a: T, b: T) -> bool
  a == b
end
```

Sans borne, on ne peut pas utiliser `+` sur un `T`. Les types de base respectent
les bornes des opérateurs : `sum(1, 2)` et `sum("a", "b")` fonctionnent. Une
borne peut être un trait que tu as défini.

### Inférence

Les types sont déduits des arguments quand c'est possible. Ce qui reste
indéterminé après les arguments se déduit ensuite du **type attendu** : celui
d'un `let` annoté, ou le type de retour déclaré de la fonction en cours (pour
un `return` ou la dernière expression du corps) :

```
let a = sum(1, 2)
let b = sum<float>(1.0, 2.0)
let c: array<array<int>> = [[1], [2, 3]]
let d = Box<int>.new(1)

fun empty<T>() -> array<T>
  []
end

let e: array<int> = empty()        -- T vient du type attendu, pas d'un argument

fun get() -> Result<int, Error>
  Result.Ok(1)                     -- T et E viennent du type de retour de `get`
end
```

Si rien ne permet de déduire un type (ni les arguments, ni le contexte), le
compilateur le demande explicitement : `let xs: array<int> = []`.

---

## 13. Modules

Un fichier peut **exporter** des noms que d'autres fichiers **importent**.

`math.q` :

```
fun add(a: int, b: int) -> int
  a + b
end

fun sub(a: int, b: int) -> int
  a - b
end

export add, sub
```

Autre fichier :

```
import add as addition from "math.q"
import sub from "math.q"
print(addition(1, 2))

import "math.q" as math
print(math.add(1, 2))
```

- `export` nomme des déclarations du fichier (fonctions, structures, énumérations,
  traits, variables), sur sa propre ligne.
- `import a, b as c from "chemin"` importe des noms choisis, avec alias
  facultatif.
- `import "chemin" as m` importe tout le module sous un alias.
- Un type exporté se note `math.Point` : `math.Point { x: 1, y: 2 }`.
- Le chemin est relatif au fichier qui importe : `"math.q"`, `"lib/math.q"` ou
  `"../shared/math.q"` (on peut remonter dans les dossiers). On écrit toujours `/`,
  même sous Windows (`\` est refusé). Les chemins absolus (`/x.q`, `C:/x.q`) sont refusés : un programme ne dépend pas de l'endroit où il est rangé. Seuls
  les fichiers `.q` peuvent être importés.
- Qui a le droit de lire quoi dépend de l'**environnement d'exécution**, pas du
  langage : dans un terminal, `qlang run` lit tout fichier `.q` lisible (comme
  `require` en Node), et `--root dossier` limite les imports à ce dossier ; dans
  le protocole JSON/HTTP, il n'y a pas de disque, les modules sont les fichiers de
  la requête.
- Un même fichier atteint par deux chemins (`a.q`, `../d/a.q`, un lien symbolique)
  est un seul module.
- Un module est exécuté une seule fois, avant le fichier qui l'importe, quel que
  soit le nombre d'imports.
- Deux modules peuvent s'importer mutuellement (un cycle) tant qu'**un seul** des modules
  du cycle exécute du code au niveau du fichier (`print`, `let`, appels…) : ceux qui ne
  font que déclarer des fonctions, structures, traits ou énumérations ne dépendent pas de
  l'ordre d'exécution. Si plusieurs modules du cycle exécutent du code, il n'y a pas
  d'ordre valable et le programme est refusé (`T101`).
- On ne peut pas modifier depuis un autre fichier une variable exportée.

### Paquets

Un **paquet** est un dossier de fichiers `.q` que l'on importe par son nom, avec un `@` :

```
import send from "@mail/send.q"      -- un fichier du paquet
import "@mail" as m                   -- le fichier principal du paquet
```

Les paquets d'un projet se déclarent dans son `qlang.toml`, section `[dependencies]` :

```toml
[dependencies]
mail  = { path = "../mail" }                                      -- un dossier (utilisé sur place)
text  = { git = "https://github.com/x/text", tag = "v1.2.0" }     -- un dépôt git
```

Un dépôt git se désigne par `tag`, `rev` (un commit) ou `branch` (sans rien : la branche par
défaut). `qlang install` le récupère dans `.qlang/packages/<nom>` (à mettre dans le
`.gitignore`) au commit noté dans `qlang.lock`, qu'il écrit s'il n'existe pas et qu'on
versionne : tout le monde obtient les mêmes fichiers. `qlang update [nom…]` avance les
paquets à ce que leur tag ou branche désigne maintenant ; `qlang add nom --git URL --tag T`
(ou `--path DIR`) et `qlang remove nom` modifient `qlang.toml` puis installent. Un paquet
récupéré par git ne peut dépendre que d'autres paquets git, pas d'un dossier local.
- Le nom d'un paquet est fait de lettres, chiffres, `_` et `-`. Un paquet a son propre
  `qlang.toml` : `[package] main = "src/mail.q"` désigne le fichier que `import "@mail"`
  charge (par défaut `lib.q`), et ses propres `[dependencies]` sont installées avec le
  projet, à plat : un nom désigne le même dossier partout, sinon c'est une erreur.
- Dans un paquet, les imports relatifs (`./helper.q`, `../x.q`) restent **dans le
  paquet** ; il peut importer d'autres paquets par `@nom/...`. Les messages d'erreur
  nomment les fichiers `@mail/send.q`.
- Un fichier dont le nom commence par `@` s'importe en écrivant `./@x.q`.
- Les paquets ne sont pas limités par `--root` : c'est le projet qui les choisit.
- L'extension VS Code suit les imports `@nom` (elle interroge `qlang packages`, qui ne fait
  que lire les `qlang.toml`).
- Un projet sans `qlang.toml` n'a pas de paquets ; les environnements sans disque (JSON,
  HTTP, navigateur) les voient comme des fichiers de la requête nommés `@mail/send.q`.

### Fonctions fournies par l'environnement (`extern`)

Le langage ne sait pas tout faire lui-même (base de données, réseau, disque…) : c'est
l'**environnement d'exécution** qui le fait. `extern fun` déclare une fonction que
l'environnement fournit, avec sa signature mais sans corps :

```
extern fun sqlite_open(path: string) -> Result<int, Error>
extern fun sqlite_close(conn: int) -> Result<bool, Error>

export sqlite_open, sqlite_close
```

Une `extern fun` s'utilise et s'exporte comme une `fun` ordinaire. Elle appelle
l'environnement sous le nom où elle est déclarée (`sqlite_open` ici) ; deux modules qui
déclarent le même nom appellent donc la même fonction.

- **Types admis** dans la signature (arguments et retour) : `int`, `float`, `bool`,
  `string`, les énumérations, `T?`, les tuples, `array<T>`, `map<K, V>`, `Json`, et les structures
  non génériques, sans parent et non récursives dont tous les champs sont admis. Rien
  d'autre (fonctions, traits, `uuid`, `ulid`, `datetime`, génériques) : `T260`, `T261`
  ou `T262`.
- **Les valeurs sont copiées** : l'environnement ne reçoit jamais une référence vers un
  tableau ou une structure du programme, et ce qu'il renvoie est reconstruit d'après la
  signature. Une structure devient une table de ses champs, un tuple un tableau de ses
  éléments (dans l'ordre), une énumération le nom de sa variante, `Json` une valeur JSON.
- **Erreurs** : si la signature renvoie `Result<T, Error>`, un échec de l'environnement
  devient un `Result::err` (comme `http.send` ou `Db.connect`). Sinon il arrête le
  programme (`R081`).
- **Disponibilité** : l'environnement décide quels noms il fournit. Appeler une
  `extern fun` qu'il ne fournit pas est une erreur de compilation (`T263`), comme
  `read()` sans clavier ; la déclarer sans l'appeler ne coûte rien. Une réponse qui ne
  correspond pas à la signature est une erreur d'exécution (`R083`).
- **Les noms que la bibliothèque standard utilise** : `Db` (`sql_connect`, `sql_query`,
  `sql_execute`, `sql_close`), `fs` (`fs_read(path) -> Result<string, Error>`,
  `fs_write(path, content) -> Result<bool, Error>`), `env` (`env_get(name) -> string?`,
  `env_args() -> array<string>`) et `http` (`http_send(req: Request) -> Result<Response,
  Error>`) sont écrits en qlang dans le prélude, par-dessus ces `extern fun`. Un
  environnement qui les implémente (un fournisseur, ou `Host::call`) remplace le disque, le
  réseau ou la base de données de ces appels, sans que le langage change.
- Une `extern fun` n'est autorisée qu'au niveau d'un fichier, pas dans un bloc, et ne
  peut pas être générique. Le programme `qlang` fournit celles de `Db` (SQLite et
  Postgres), et d'autres venant de programmes externes déclarés dans un `qlang.toml` ou
  avec `--extern` (voir le [README](../README.md#fournir-des-extern-fun)) ; un programme
  qui embarque le langage en fournit avec `Host::call` et `Host::has_extern` (voir
  `host.rs`).

---

## 14. Bibliothèque standard

### Dépendance à l'environnement

Le langage ne lit et n'écrit rien lui-même : `print`, `read` et `import` passent
par l'**environnement d'exécution** (le terminal, un serveur web…).

- `print` est toujours disponible.
- `time.now()`, `datetime.now()`, `random.…`, `uuid.v4()`, `uuid.v7()`, `ulid.new()` et
  `ulid.at()` dépendent aussi de l'environnement (une horloge, une source
  d'aléatoire) : voir [Heure et aléatoire](#heure-et-aléatoire).
- `read` **dépend de l'environnement**. Dans un terminal, il lit le clavier. Dans
  un environnement sans clavier (par exemple un serveur web), il peut être
  absent : l'appeler est alors une erreur de compilation (« `read` n'est pas
  disponible dans cet environnement »). Un environnement peut aussi fournir à
  l'avance une liste de lignes que `read()` consomme ; quand elle est épuisée,
  `read()` provoque une erreur.
- `fs.read()`/`fs.write()` (disque) et `env.get()`/`env.args()` (variables
  d'environnement, arguments) dépendent aussi de l'environnement : voir
  [Prélude : fichiers et environnement](#prélude--fichiers-et-environnement).

### Fonctions prédéfinies

| Fonction                 | Rôle                                                                  |
| ------------------------ | --------------------------------------------------------------------- |
| `print(x)`               | affiche `x` suivi d'un retour à la ligne (`print()` : une ligne vide) |
| `write(x)`               | affiche `x` sans retour à la ligne                                    |
| `read()`                 | lit une ligne et renvoie une `string` (sans le retour à la ligne)     |
| `read_all()`             | lit toutes les lignes restantes, reliées par des retours à la ligne (aucun après la dernière) |
| `exit(code?)`            | termine le programme tout de suite avec ce code de sortie (0 par défaut) ; rien ne s'exécute après |
| `panic(message)`         | arrête le programme avec une erreur                                   |
| `assert(cond, message?)` | arrête le programme si `cond` est faux                                |
| `assert_eq(a, b)`        | arrête le programme si `a != b`, en montrant les deux valeurs (`assert_ne` : si `a == b`) |
| `assert_lt(a, b)`, `assert_le`, `assert_gt`, `assert_ge` | pour un test : `a < b` (`<=`, `>`, `>=`), pour tout type qui s'ordonne (nombres, chaînes, tuples…) ; montre les deux valeurs sinon |
| `assert_len(tableau, n)`, `assert_empty(tableau)` | pour un test : le tableau a `n` éléments / aucun |
| `assert_not_contains(tableau, v)` | pour un test : `v` n'est pas dans le tableau |
| `assert_starts_with(texte, début)`, `assert_ends_with(texte, fin)` | pour un test : le texte commence / finit par... |
| `assert_ok(r)`, `assert_err(r)` | pour un test : la valeur d'un `Ok` / l'erreur d'un `Err`, ou arrête le programme en montrant ce que contenait le résultat |
| `assert_some(x)`, `assert_none(x)` | pour un test : la valeur d'un nullable qui ne doit pas être `none` / vérifie qu'il l'est |
| `assert_contains(tableau, v)`, `assert_contains_text(texte, morceau)` | pour un test : `v` est dans le tableau / `morceau` est dans le texte |
| `assert_close(a, b)`, `assert_within(a, b, tolérance)` | pour un test : deux `float` égaux à la tolérance près (un milliardième par défaut ; relative dès que `b` dépasse 1) |
| `int.parse(s)`           | renvoie un `int?` : `none` si `s` n'est pas un entier valide          |
| `float.parse(s)`         | renvoie un `float?`                                                   |

`print` et `write` acceptent n'importe quelle valeur (voir
[Conversions](#conversions)) : les tableaux s'affichent `[1, 2, 3]`, les
chaînes dans un tableau sont entre guillemets. `read()` renvoie toujours une
`string` ; pour un nombre : `int.parse(read())`.

### Méthodes prédéfinies

La bibliothèque est volontairement petite, et s'étoffera à l'usage.

`array<T>` :

| Méthode               | Résultat                                                         |
| --------------------- | ---------------------------------------------------------------- |
| `len()`, `is_empty()` | taille ; `int` et `bool`                                         |
| `push(v)`             | ajoute à la fin (comme `xs[] = v`)                               |
| `pop()`               | retire et renvoie le dernier élément, ou `none` si vide (`T?`)   |
| `insert(i, v)`        | insère `v` à l'indice `i` (0 à `len()`)                          |
| `remove(i)`           | retire et renvoie l'élément d'indice `i`                         |
| `clear()`             | vide le tableau                                                  |
| `contains(v)`         | `bool`                                                           |
| `index_of(v)`         | indice de `v`, ou `none` (`int?`)                                |
| `reverse()`           | inverse le tableau sur place                                     |
| `join(sep)`           | assemble les éléments en une `string`                            |
| `sort()`              | trie le tableau sur place, de façon stable (voir ci-dessous)     |
| `min()`, `max()`      | le plus petit / grand élément, ou `none` si vide (`T?`)          |
| `sum()`               | somme d'un tableau de nombres (`0` ou `0.0` si vide)             |
| `slice(début, fin)`   | nouveau tableau des éléments de `début` (inclus) à `fin` (exclu) |
| `map(f)`              | nouveau tableau de `f(élément)` pour chaque élément               |
| `filter(f)`           | nouveau tableau des éléments pour lesquels `f(élément)` est `true` |
| `reduce(initial, f)`  | réduit le tableau en une valeur, en combinant `initial` puis chaque élément via `f(accumulateur, élément)` |
| `any(f)`, `all(f)`    | `bool` : au moins un élément / tous les éléments vérifient `f(élément)` |
| `find(f)`             | premier élément pour lequel `f(élément)` est `true`, ou `none`    |
| `each(f)`             | appelle `f(élément)` pour chaque élément, dans l'ordre            |
| `sort_by(f)`          | trie le tableau sur place avec `f(a, b)` comme comparateur (négatif si `a` avant `b`, positif sinon, `0` si égaux) |
| `unique()`            | nouveau tableau ne gardant que la première occurrence de chaque valeur |
| `flatten()`           | nouveau tableau qui aplatit un tableau de tableaux (un seul niveau) |
| `first()`, `last()`   | premier / dernier élément, ou `none` si vide (`T?`)              |
| `concat(autre)`       | nouveau tableau avec les éléments de ce tableau puis ceux d'`autre` |
| `take(n)`, `drop(n)`  | nouveau tableau des `n` premiers éléments / sans les `n` premiers |
| `swap(i, j)`          | échange les éléments d'indices `i` et `j`, sur place              |
| `chunk(taille)`       | découpe en groupes consécutifs de `taille` éléments (le dernier peut être plus petit) |
| `repeat(n)`           | nouveau tableau avec les éléments répétés `n` fois                 |

```
let names = ["ana", "bob", "cy"]
print(names.map(fun(s: string) -> int s.len() end))        // [3, 3, 2]
print(names.filter(fun(s: string) -> bool s.len() > 2 end)) // ["ana", "bob"]

let total = [1, 2, 3, 4].reduce(0, fun(acc: int, x: int) -> int acc + x end)
print(total) // 10
```

`string` :

| Méthode                                         | Résultat                                                                                                             |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `len()`, `is_empty()`                           | nombre de caractères ; `bool`                                                                                        |
| `upper()`, `lower()`, `trim()`                  | une nouvelle `string`                                                                                                |
| `trim_start()`, `trim_end()`                    | retire les espaces au début / à la fin                                                                               |
| `capitalize()`                                  | première lettre en majuscule                                                                                         |
| `reverse()`                                     | les caractères dans l'ordre inverse                                                                                  |
| `contains(s)`, `starts_with(s)`, `ends_with(s)` | `bool`                                                                                                               |
| `index_of(s)`, `last_index_of(s)`               | indice du caractère (première / dernière occurrence), ou `none` (`int?`)                                             |
| `count(s)`                                      | nombre d'occurrences non chevauchantes de `s` (`s` non vide)                                                         |
| `replace(a, b)`                                 | remplace toutes les occurrences                                                                                      |
| `split(sep)`                                    | `array<string>` (séparateur vide : les caractères)                                                                   |
| `repeat(n)`                                     | répète `n` fois                                                                                                      |
| `chars()`                                       | `array<string>` d'un caractère chacun                                                                                |
| `lines()`, `words()`                            | `array<string>` : les lignes ; les mots séparés par des espaces                                                      |
| `pad_left(largeur, c)`, `pad_right(largeur, c)` | complète jusqu'à `largeur` caractères avec le caractère `c` (une seule lettre)                                       |
| `is_digit()`, `is_alpha()`, `is_blank()`        | `bool` : que des chiffres ; que des lettres ; vide ou que des espaces (`is_digit` et `is_alpha` sont faux pour `""`) |
| `code()`                                        | code Unicode d'une chaîne d'**un seul** caractère (`"A".code()` vaut 65)                                             |
| `to_int()`, `to_float()`                        | comme `int.parse` / `float.parse` : `int?` / `float?`                                                                |
| `substring(début, fin)`                         | caractères de `début` (inclus) à `fin` (exclu)                                                                       |
| `strip_prefix(p)`, `strip_suffix(p)`            | la chaîne sans `p` au début / à la fin, ou `none` si elle ne commence / finit pas par `p`                           |
| `snake_case()`, `camel_case()`, `kebab_case()`  | copie en `snake_case`, `camelCase` ou `kebab-case`                                                                   |
| `title_case()`                                  | copie avec chaque mot capitalisé, séparés par une espace                                                            |
| `eq_ignore_case(autre)`                         | `bool` : égalité en ignorant majuscules / minuscules                                                                |

`sort`, `min` et `max` demandent des éléments qui se comparent : nombres,
chaînes, ou une structure qui implémente `Ord`. Sinon, c'est une erreur de
compilation. `sum` n'accepte que des tableaux d'`int` ou de `float`. `slice`
copie les éléments : modifier le résultat ne change pas le tableau d'origine.
`flatten` demande un tableau de tableaux (`array<array<T>>`) ; sinon c'est une
erreur de compilation. `map`, `filter`, `reduce`, `any`, `all`, `find`,
`each` et `sort_by` prennent une [fonction anonyme](#fonctions-anonymes) : son
type de retour n'a pas besoin d'être écrit s'il se déduit du contexte.

Nombres :

| Méthode / constante                  | Résultat                                                              |
| ------------------------------------ | --------------------------------------------------------------------- |
| `n.char()`                           | (`int`) la chaîne d'un caractère de code `n` (`97.char()` vaut `"a"`) |
| `x.abs()`                            | valeur absolue (`int` ou `float`)                                     |
| `x.min(y)`, `x.max(y)`               | le plus petit / grand des deux (même type)                            |
| `x.pow(y)`                           | `x ** y` (deux `int` donnent un `int`, deux `float` un `float`)       |
| `x.sqrt()`                           | racine carrée, un `float` (erreur si `x` est négatif)                 |
| `f.floor()`, `f.ceil()`, `f.round()` | arrondis d'un `float`, qui renvoient un `int`                         |
| `n.to_hex()`, `n.to_oct()`, `n.to_bin()` | (`int`) texte en base 16 / 8 / 2 (`255.to_hex()` vaut `"ff"`)      |
| `f.to_fixed(décimales)`              | (`float`) texte avec exactement `décimales` chiffres après la virgule (0 à 17) |
| `x.sin()`, `x.cos()`, `x.tan()`      | trigonométrie, `x` en radians (`int` ou `float`), un `float`          |
| `x.asin()`, `x.acos()`, `x.atan()`   | fonctions réciproques, en radians ; `asin`/`acos` hors de −1 à 1 : erreur |
| `y.atan2(x)`                         | l'angle du point (`x`, `y`), en radians                               |
| `x.exp()`, `x.ln()`, `x.log10()`, `x.log2()`, `x.log(base)` | exponentielle et logarithmes ; un nombre qui n'est pas strictement positif : erreur (`R016`) |
| `x.to_radians()`, `x.to_degrees()`   | conversion entre degrés et radians                                    |
| `x.signum()`                         | −1, 0 ou 1 selon le signe (`int` ou `float`)                          |
| `x.clamp(min, max)`                  | ramène `x` dans l'intervalle (même type ; `min` > `max` : erreur)     |
| `f.trunc()`                          | partie entière d'un `float` (vers zéro), un `int`                     |
| `f.is_nan()`, `f.is_infinite()`      | `bool`                                                                |
| `float.PI`, `float.E`                | constantes mathématiques                                              |
| `float.INFINITY`, `float.NAN`        | l'infini, et « pas un nombre » (`float.NAN == float.NAN` est faux : utiliser `is_nan()`) |
| `int.MAX`, `int.MIN`                 | plus grand / plus petit `int`                                         |

```
print([3, 1, 2].max())             -- 3
print(float.PI * 2.0 ** 2)         -- 12.566370614359172
print(16.sqrt())                   -- 4.0
print(3.14159.to_fixed(2))         -- "3.14"
print(255.to_hex())                -- "ff"
```

Ces opérations sont des méthodes et des constantes attachées aux types, pour ne
pas réserver de noms globaux (`PI`, `min`…) que tu voudrais utiliser toi-même.

### Dictionnaires

Un dictionnaire (`map<K, V>`) associe des valeurs à des **clés**. Les clés sont
des `int`, des `float`, des `string`, des `bool` ou des valeurs d'une
énumération : des types qui se comparent par valeur. Les clés gardent leur
**ordre d'insertion**.

```
let ages = { "ana": 31, "bob": 27 }
let empty: map<string, int> = {}      -- le type vient de l'annotation

ages["cleo"] = 45                     -- ajoute ou remplace
print(ages["ana"])                    -- 31
print(ages["zed"])                    -- none : la clé n'existe pas
```

**Lire une clé absente donne `none`** : `ages["ana"]` a le type `int?`, comme
`pop()` ou `int.parse`. Il faut donc vérifier avant d'utiliser la valeur, ou
fournir une valeur par défaut avec `get` :

```
let a = ages["ana"]
if a != none then
  print(a + 1)
end

print(ages.get("zed", 0) + 1)         -- 1
```

Compter des éléments devient simple avec `get` :

```
let counts: map<string, int> = {}
for w in words do
  counts[w] = counts.get(w, 0) + 1
end
```

| Méthode               | Résultat                                                |
| --------------------- | ------------------------------------------------------- |
| `len()`, `is_empty()` | nombre de clés ; `bool`                                 |
| `has(clé)`            | `bool`                                                  |
| `get(clé, défaut)`    | la valeur, ou `défaut` si la clé est absente (type `V`) |
| `remove(clé)`         | retire la clé et renvoie sa valeur, ou `none` (`V?`)    |
| `keys()`              | `array<K>` des clés, dans l'ordre d'insertion           |
| `values()`            | `array<V>` des valeurs, dans le même ordre              |
| `clear()`             | vide le dictionnaire                                    |
| `merge(autre)`        | nouveau dictionnaire avec les entrées des deux ; `autre` gagne sur une clé commune |
| `filter(f)`           | nouveau dictionnaire ne gardant que les entrées où `f(clé, valeur)` est `true` |
| `map_values(f)`       | nouveau dictionnaire avec les mêmes clés, chaque valeur remplacée par `f(clé, valeur)` |

```
let a = { "x": 1, "y": 2 }
let b = { "y": 20, "z": 3 }
print(a.merge(b))                                      -- {"x": 1, "y": 20, "z": 3}
print(a.filter(fun(k: string, v: int) -> bool v > 1 end)) -- {"y": 2}
```

- **Parcours** : `for name, age in ages do … end` donne la clé et la valeur, dans
  l'ordre d'insertion. On peut aussi ne parcourir que les clés
  (`for name in ages.keys() do`). Boucler sur un dictionnaire avec une seule
  variable (`for x in m`) est une erreur.
- **`m[clé] += 1`** lit la valeur elle-même : c'est une erreur d'exécution si la
  clé n'existe pas (utiliser `get` dans ce cas).
- **Partage** : comme les tableaux, un dictionnaire est une référence partagée.
- Les dictionnaires s'affichent `{"ana": 31, "bob": 27}`. Deux dictionnaires sont
  égaux (`==`) s'ils ont les mêmes clés avec des valeurs égales, quel que soit
  l'ordre d'insertion.
- **Pourquoi pas de clés tableau ou structure ?** Une clé doit désigner toujours
  la même entrée, mais un tableau ou une structure est une référence partagée
  qu'on peut modifier après l'avoir rangée (`k[0] = 99`). Pour des coordonnées,
  utilise une chaîne (`m["{x},{y}"]`) ou des dictionnaires imbriqués. Un
  `float` est en revanche autorisé comme clé : il est comparé par ses bits
  (`0.0` et `-0.0` comptent comme la même clé).
- Un littéral vide `{}` demande une annotation de type : `let m = {}` est une
  erreur. Un littéral peut s'étendre sur plusieurs lignes et finir par une
  virgule.
- `map` désigne le type dans les annotations (`map<string, int>`), mais reste
  utilisable comme nom de fonction ou de variable.

Un indice hors limites (`xs[5]` pour 3 éléments, indice négatif) est une erreur
d'exécution.

### Ensembles

Un `Set<T>` est un ensemble de valeurs **sans doublon**, qui garde l'ordre d'ajout. C'est un
type du prélude (sans `import`) :

```
let a = Set.of([1, 2, 2, 3])         -- Set(1, 2, 3)
let seen = Set<string>.new()         -- un ensemble vide
seen.add("x")                        -- true : la valeur était nouvelle
seen.add("x")                        -- false
print(seen.contains("x"))            -- true

let b = Set.of([2, 3, 4])
print(a.union(b))                    -- Set(1, 2, 3, 4)
print(a.intersect(b))                -- Set(2, 3)
print(a.difference(b))               -- Set(1)

for v in a do                        -- dans l'ordre d'ajout
  print(v)
end
```

| Méthode | Rôle |
| --- | --- |
| `Set.of(tableau)`, `Set<T>.new()` | crée un ensemble |
| `add(v)`, `remove(v)` | ajoute / retire ; `true` si l'ensemble a changé |
| `contains(v)`, `len()`, `is_empty()`, `clear()` | |
| `to_array()` | les valeurs, dans l'ordre d'ajout |
| `union(autre)`, `intersect(autre)`, `difference(autre)` | un **nouvel** ensemble |
| `is_subset(autre)`, `equals(autre)` | `bool` (`equals` ignore l'ordre) |

Les valeurs suivent la règle des clés de dictionnaire (`int`, `float`, `string`, `bool`, `uuid`,
`ulid`, `datetime`, énumération sans données) : un `Set<array<int>>` est refusé (`T270`).
Parcourir un ensemble en le modifiant est permis (la boucle voit l'ensemble tel qu'il était au
départ). Il s'affiche `Set(1, 2, 3)`.

---

## 15. Temps, identifiants et expressions régulières

### Heure et aléatoire

Le cœur du langage ne lit ni l'horloge ni le hasard : comme `read`, ils viennent
de l'**environnement**. Un programme qui les appelle alors qu'ils manquent est
refusé **avant** d'être exécuté (erreur `T250`).

| Appel                  | Résultat                                                                 |
| ---------------------- | ------------------------------------------------------------------------ |
| `time.now()`           | l'heure courante, un `int` : millisecondes depuis le 1er janvier 1970 UTC |
| `time.sleep(ms)`       | attend `ms` millisecondes (0 à 3 600 000) ; avec une horloge virtuelle (tests, protocole JSON), avance seulement l'horloge |
| `random.int(min, max)` | un `int` au hasard entre `min` et `max`, **les deux inclus**             |
| `random.float()`       | un `float` au hasard dans `[0, 1[`                                       |

```
let de = random.int(1, 6)
print("Le dé donne {de}")
print(time.now())           -- par exemple 1791725184727
```

`random.int(3, 1)` est une erreur (`R062`) : `min` doit être inférieur ou égal
à `max`.

Dans un terminal, `qlang run programme.q --seed 42` fixe la graine : le même
programme donne alors **les mêmes nombres** (pratique pour reproduire un
résultat). Sans `--seed`, la graine vient du système. Ce hasard n'est **pas
cryptographique** : ne l'utilisez pas pour des mots de passe ou des jetons de
sécurité.

`time` et `random` sont des noms prédéfinis, mais une variable ou une fonction
du même nom reste possible : `let time = 5` fonctionne, et `time` désigne alors
cette variable.

### Identifiants : `uuid` et `ulid`

Deux types pour identifier une chose de façon unique. Tous deux font 128 bits et
sont distincts de `string` : on ne peut pas passer l'un pour l'autre.

| Type   | Forme                                     | Ce qu'il contient                               |
| ------ | ----------------------------------------- | ----------------------------------------------- |
| `uuid` | `550e8400-e29b-41d4-a716-446655440000`    | 128 bits (version 4 : 122 bits aléatoires)      |
| `ulid` | `01ARYZ6S41TSV4RRFFQ69G5FAV` (26 lettres) | l'heure (48 bits) puis du hasard (80 bits)      |

Un `ulid` commence par l'**heure de création** en millisecondes (48 bits) : trier
des `ulid` les range donc dans l'ordre de création. Les 80 bits suivants sont
aléatoires. Deux `ulid.new()` dans la même milliseconde restent dans l'ordre
(le second est le premier plus 1).

| Appel                  | Résultat                                                                  |
| ---------------------- | ------------------------------------------------------------------------- |
| `uuid.v4()`            | un `uuid` aléatoire (version 4)                                           |
| `uuid.v7()`            | un `uuid` de version 7 : l'heure puis du hasard, donc triable par date    |
| `uuid.parse(texte)`    | un `uuid?` : `none` si le texte n'est pas valide (majuscules acceptées)   |
| `uuid.NIL`             | le `uuid` rempli de zéros                                                 |
| `u.version()`          | la version du `uuid` (un `int`)                                           |
| `ulid.new()`           | un `ulid` pour l'instant présent                                          |
| `ulid.at(ms)`          | un `ulid` pour l'heure `ms` (millisecondes depuis 1970), avec du hasard   |
| `ulid.parse(texte)`    | un `ulid?` : `none` si le texte n'est pas valide                          |
| `id.time()`            | l'heure stockée dans le `ulid`, en millisecondes depuis 1970 (un `int`)   |

```
let id = ulid.new()
print("{id} créé à {id.time()}")     -- 01M4NJ99PQPPWW6K8BZY81A0M0 créé à 1791725184727

let a = ulid.at(1000)
let b = ulid.at(2000)
print(a < b)                         -- true : on compare, on trie
let commandes: map<ulid, string> = {}
commandes[a] = "première"            -- un ulid ou un uuid peut être une clé
```

Les `uuid` et `ulid` se comparent avec `==`, `<`… et se trient ; ils
s'affichent et s'interpolent sous leur forme texte (`id as string` donne cette
chaîne). Pour l'inverse, `uuid.parse` et `ulid.parse` vérifient le texte.

### Dates et heures : `datetime`

Un `datetime` est un **instant** : une date et une heure, toujours en **UTC** (pas de fuseau
horaire, donc les mêmes résultats partout). Il va de l'an 1 à l'an 9999. C'est un type à part,
comme `uuid` : il se compare (`<`, `==`), se trie et peut être une clé de dictionnaire.

| Appel                                         | Résultat                                                                       |
| --------------------------------------------- | ------------------------------------------------------------------------------ |
| `datetime.now()`                              | l'instant présent (a besoin d'une horloge, comme `time.now()`)                 |
| `datetime.of(2026, 10, 11)`                   | le 11 octobre 2026 à minuit                                                    |
| `datetime.of(2026, 10, 11, 14, 30, 0)`        | avec l'heure : heure, minute, seconde                                          |
| `datetime.parse(texte)`                       | un `datetime?` : `none` si le texte est invalide (voir ci-dessous)             |
| `datetime.at(ms)`                             | l'instant `ms` millisecondes après le 1er janvier 1970                         |
| `d.year()` `d.month()` `d.day()`              | année, mois (1 à 12), jour du mois                                             |
| `d.hour()` `d.minute()` `d.second()`          | heure (0 à 23), minute, seconde                                                |
| `d.weekday()`                                 | jour de la semaine : 1 = lundi, 7 = dimanche                                   |
| `d.ms()`                                      | millisecondes depuis 1970 (un `int`)                                           |
| `d.add_days(n)`                               | `n` jours plus tard (`n` peut être négatif) ; aussi `add_years`, `add_months`, `add_hours`, `add_minutes`, `add_seconds` |
| `d.start_of_day()`                            | minuit du même jour                                                            |
| `b - a`                                       | le temps écoulé de `a` à `b`, en **millisecondes** (un `int`)                  |
| `a.days_until(b)`                             | le nombre de jours entre le jour de `a` et celui de `b`                        |
| `d.format("DD/MM/YYYY")`                      | le texte selon un modèle                                                       |

```
let rendu = datetime.of(2026, 10, 11, 14, 30, 0)
print(rendu)                          -- 2026-10-11T14:30:00Z
print(rendu.add_days(30).format("DD/MM/YYYY"))   -- 10/11/2026
print(rendu.weekday())                -- 7 (dimanche)

let fin = datetime.of(2026, 12, 25)
print(rendu.days_until(fin))          -- 75
print((fin - rendu) div 3_600_000)    -- les heures qui restent

let d = datetime.parse("2026-10-11 14:30")
if d != none then
  print(d.hour())
end
```

- **Affichage** : un `datetime` s'affiche au format ISO 8601, `2026-10-11T14:30:00Z`
  (avec `.123` avant le `Z` s'il y a des millisecondes). `"{d}"` et `d as string` donnent ce texte.
- **`format`** reconnaît `YYYY` (année), `MM` (mois), `DD` (jour), `HH` (heure), `mm` (minute),
  `ss` (seconde) et `SSS` (millisecondes). Les autres caractères sont gardés tels quels :
  `d.format("YYYY-MM-DD à HH:mm")`.
- **`parse`** lit `AAAA-MM-JJ`, éventuellement suivi de `T` ou d'un espace puis de `HH:MM`,
  `HH:MM:SS` ou `HH:MM:SS.mmm`, et d'un `Z` facultatif. Une date seule est minuit.
- **`add_months` et `add_years`** : un jour qui n'existe pas dans le nouveau mois devient son
  dernier jour (31 janvier + 1 mois = 28 ou 29 février).
- **Erreurs** : `datetime.of(2026, 2, 29)` (une date qui n'existe pas) est une erreur `R064` ;
  sortir des années 1 à 9999 (`add_days`…) est une erreur `R065`.

### Expressions régulières

L'espace de noms `regex` propose les opérations courantes sur les expressions
régulières, à partir de deux chaînes (`text`, `pattern`) :

```
print(regex.matches("hello world", "wor.d"))          --> true
print(regex.find("hello world", "w\\w+"))             --> "world"
print(regex.find_all("a1 b2 c3", "\\d"))               --> ["1", "2", "3"]
print(regex.replace("a1 b2", "\\d", "#"))              --> "a# b2"
print(regex.replace_all("a1 b2", "\\d", "#"))          --> "a# b#"
print(regex.split("a,b,,c", ","))                     --> ["a", "b", "", "c"]
```

Une chaîne qlang n'autorise que les échappements `\n \t \r \0 \\ \" \{ \}` :
un `\d` ou `\w` de la syntaxe regex doit s'écrire `\\d`, `\\w`... dans le
texte du motif (un seul `\` littéral dans la chaîne).

`regex.captures(text, pattern)` donne les groupes de la première
correspondance (`array<string?>?`) : l'indice 0 est la correspondance
entière, 1 et plus les groupes `(...)`. `none` si `pattern` ne correspond pas
du tout ; un groupe qui n'a pas participé (dans un `?` ou une branche non
prise) est `none` lui aussi.

```
let caps = regex.captures("2026-10-13", "(\\d+)-(\\d+)-(\\d+)")
if caps != none then
  print(caps[1])  --> "2026"
end
```

Une syntaxe d'expression régulière invalide est une erreur `R070` : c'est une
erreur de programmation (comme une date qui n'existe pas), pas une
condition à récupérer avec `Result`.

### Encodages et hachages

Quatre espaces de noms travaillent sur le texte (ses octets UTF-8) ; le texte est le seul type,
il n'y a pas de tableau d'octets, donc un décodage renvoie `none` si le résultat n'est pas du
texte UTF-8 valide :

| Fonction | Rôle |
| --- | --- |
| `Base64.encode(s)`, `Base64.encode_url(s)` | texte en base64 (avec `=`), ou en base64 « URL » (`-` `_`, sans `=`) |
| `Base64.decode(s)` | le texte d'une chaîne base64 (les deux alphabets), ou `none` |
| `Hex.encode(s)`, `Hex.decode(s)` | hexadécimal en minuscules ; `none` si ce n'est pas de l'hexadécimal |
| `Url.encode(s)`, `Url.decode(s)` | encodage en pourcentage d'un morceau d'URL (`a b&c` donne `a%20b%26c`) ; `+` se décode en espace ; `none` si un `%` est mal formé |
| `Hash.sha256(s)`, `Hash.sha1(s)`, `Hash.md5(s)` | l'empreinte en hexadécimal (`sha1` et `md5` ne servent pas à la sécurité) |
| `Hash.hmac_sha256(clé, message)` | le HMAC-SHA-256 en hexadécimal |

```
print(Hash.sha256("abc"))        -- ba7816bf...
print(Base64.decode("TWFu"))     -- Man
```

Ces noms commencent par une majuscule pour ne pas être masqués par une variable `url` ou `hash`.

## 16. Prélude : `Result`, `Error` et JSON

### Prélude : `Result` et `Error`

Deux types sont disponibles dans tout fichier, **sans `import`** :

```
struct Error
  public message: string
end

enum Result<T, E>
  Ok(value: T)
  Err(error: E)
end
```

`Result<T, E>` est une [énumération](#11-énumérations) à deux variantes : `Ok(valeur)`
quand tout s'est bien passé, `Err(erreur)` sinon :

```
fun divide(a: int, b: int) -> Result<int, Error>
  if b == 0 then
    return Err(Error { message: "division by zero" })
  end
  Ok(a div b)
end

match divide(10, 0)
  case Ok(v) then print(v)
  case Err(e) then print(e.message)
end
```

- `Ok(valeur)` et `Err(erreur)` construisent le résultat (ce sont aussi les variantes :
  `Result.Ok(valeur)`, `Result.Err(erreur)`) ; `T` et `E` se déduisent du contexte (ici,
  le type de retour de `divide`). Quand rien ne permet de les déduire, écris-les
  explicitement sur le type : `Result<int, Error>.Ok(valeur)` /
  `Result<int, Error>.Err(erreur)`.
- `match` prend un résultat en morceaux, et le compilateur vérifie que `Ok` et `Err` sont
  tous deux traités : la valeur n'est lisible que dans le cas `Ok`.
- `résultat?` (voir plus bas) donne la valeur, ou quitte la fonction avec l'erreur.
- `is_ok()`, `is_err()` : savoir lequel des deux cas on a.
- `unwrap()`, `unwrap_err()` : lisent la valeur ou l'erreur, ou arrêtent le
  programme (`panic`) si on se trompe de cas.
- `Result`/`Error` sont des déclarations ordinaires : une structure `Error` (ou une
  énumération `Result`) que tu écris toi-même dans ton propre fichier **masque** celle du
  prélude, exactement comme une variable locale masque une variable de portée plus large.

#### Propager une erreur avec `?`

Dans une fonction qui renvoie elle-même un `Result`, `expr?` donne la valeur d'un `Ok`, et
**quitte la fonction avec l'erreur** si c'est un `Err` :

```
fun load(path: string) -> Result<Json, Error>
  let text = fs.read(path)?          -- l'erreur de fs.read sort de load
  let json = Json.parse(text)?
  Ok(json)
end
```

- Le type d'erreur doit être le même des deux côtés (`Error` ici) : sinon, erreur de
  compilation (`T282`) ; il n'y a pas de conversion automatique.
- `?` marche aussi sur une valeur **nullable** : `x?` donne la valeur, ou quitte la fonction
  avec `none`, ce qui demande une fonction qui renvoie un `T?`.
  `fun port(s: string) -> int? ... let rest = s.strip_prefix("port=")?` ...
- Il s'écrit derrière n'importe quelle expression (`a()? + b()?`, `f()?.g()`), mais pas en
  dehors d'une fonction (`T280`), et il ne s'applique qu'à un `Result` ou un nullable
  (`T281`). Dans une fonction anonyme, c'est elle seule qui est quittée.

### Prélude : JSON

**Encoder.** Le trait `ToJson` décrit un type qui sait se transformer en texte
JSON :

```
trait ToJson
  fun to_json(self) -> string
end
```

Une structure l'implémente en listant les champs qu'elle veut exposer avec
`json_object` (les valeurs sont elles-mêmes sérialisées via leur propre
`to_json()` : des `int`, `float`, `bool`, `string` ou n'importe quel `ToJson`
imbriqué) :

```
impl ToJson for Post
  public fun to_json(self) -> string
    json_object({ "title": self.title, "views": self.views })
  end
end

print(Post { title: "Hello", views: 42 }.to_json())  -- {"title":"Hello","views":42}
```

`json_array(items)` fait la même chose pour un tableau. `int`, `float`, `bool`,
`string`, `uuid`, `ulid` et `datetime` implémentent déjà `ToJson` ; `array<T>`
aussi, dès que `T` l'implémente, donc un `array<Post>` se sérialise tel quel
comme valeur d'un `json_object`, sans appeler `json_array` à la main :

```
struct Post
  public title: string
  public comments: array<Comment>
end

impl ToJson for Post
  public fun to_json(self) -> string
    json_object({ "title": self.title, "comments": self.comments })
  end
end
```

**Composer avec l'héritage.** `to_json` renvoie un texte déjà fermé (`"{...}"`),
donc une sous-structure qui ajoute des champs n'a pas besoin de recopier ceux
du parent : `json_merge` fusionne deux objets JSON déjà sérialisés, et
`super.to_json()` donne celui du parent.

```
struct AdminEntity extends UserEntity
  public role: string
end

impl ToJson for AdminEntity
  public fun to_json(self) -> string
    json_merge(super.to_json(), json_object({ "role": self.role }))
  end
end
```

**Décoder.** `Json.parse(texte)` renvoie un `Result<Json, Error>` : un arbre de
valeurs JSON (chaîne, nombre, bool, `null`, tableau, objet), à lire avec
`is_string()`/`as_string()` (et les équivalents `number`, `bool`, `array`,
`object`), `get(clé)` (sur un objet, renvoie un `Json?`) et `at(indice)` (sur un
tableau) :

```
let r = Json.parse("\{\"name\": \"Zoe\"\}")
if r.is_err() then
  print(r.unwrap_err().message)
else
  let j = r.unwrap()
  let name = j.get("name")
  if name != none then
    print(name.as_string())
  end
end
```

Il n'y a **pas de conversion automatique** entre un `Json` décodé et une
structure définie par l'utilisateur (qlang n'a pas de réflexion ni de macros
`derive`) : pour reconstruire un `Post` à partir d'un `Json`, écris
toi-même une méthode statique qui lit les champs un par un, par exemple
`Post.from_json(json: Json) -> Result<Post, Error>` (convention, pas une
interface imposée par le langage, comme `new`).

## 17. Prélude : réseau, bases de données et système

### Prélude : `http`

`http.send(req)` envoie une requête HTTP et attend la réponse. `Request` et
`Response` sont fournis par le prélude (pas besoin de les déclarer) :

```
struct Request
  public method: string
  public url: string
  public headers: map<string, string> = {}
  public body: string? = none
  public path: string = ""
  public query: map<string, string> = {}
  public params: map<string, string> = {}
end

struct Response
  public status: int
  public headers: map<string, string> = {}
  public body: string
end
```

`path`, `query` et `params` ne servent que pour une requête **reçue** (voir
[Répondre à des requêtes HTTP](#répondre-à-des-requêtes-http)) ; `headers` d'une `Response` donne
les en-têtes reçus (noms en minuscules).

Besoin d'un accès réseau côté hôte : un hôte qui ne l'offre pas (un bac à
sable, par exemple) fait échouer la vérification avant même l'exécution.

```
let req = Request { method: "GET", url: "https://example.com" }
let r = http.send(req)
if r.is_err() then
  print(r.unwrap_err().message)
else
  print(r.unwrap().status)
end
```

Une erreur de connexion (DNS, délai dépassé, connexion refusée...) donne un
`Result::err`, jamais un arrêt du programme : le réseau est par nature
instable, donc toujours récupérable, comme `int.parse` ou `Json.parse`.
`headers` et `body` ont une valeur par défaut, donc une requête `GET` simple
n'a besoin que de `method` et `url`.

```
let res = http.send(Request {
  method: "POST",
  url: "https://example.com/items",
  headers: { "Content-Type": "application/json" },
  body: "\{\"name\": \"Rex\"\}",
})
```

### Répondre à des requêtes HTTP

Un programme peut aussi **servir** des requêtes : `qlang serve app.q` écoute sur un port
(8080 par défaut, sur `127.0.0.1`), exécute le code du fichier **une fois**, puis appelle
sa fonction `handle` pour chaque requête reçue.

```
let visits = 0

fun handle(req: Request) -> Response
  visits += 1
  match req.path
    case "/hello" then
      let name = req.query.get("name", "toi")
      Response.text("Bonjour {name} !")
    case "/visits" then
      Response.json("\{\"visits\": {visits}\}")
    else
      Response.not_found()
  end
end
```

```
qlang serve app.q --port 8080
curl "http://127.0.0.1:8080/hello?name=Ada"
```

- **La fonction** s'appelle exactement `handle`, prend une `Request` et renvoie une
  `Response` : la vérification le contrôle avant le démarrage.
- **La requête** :
  - `method` (`"GET"`, `"POST"`…) ;
  - `url`, la cible telle qu'elle a été reçue (`/items?id=3`), sans décodage ;
  - `path`, le chemin **décodé** (`/items`), et `query`, les paramètres **décodés** (`id` vaut
    `"3"` ; si un nom est répété, la dernière valeur gagne) ;
  - `headers`, dont les noms sont en **minuscules**, et `req.header("User-Agent")`, qui cherche
    un en-tête sans tenir compte de la casse (`string?`) ;
  - `body`, le corps (`none` s'il n'y en a pas).
- **La réponse** : `status` (de 100 à 599), `body`, et `headers` facultatifs. Des raccourcis :

  | Appel                       | Résultat                                                    |
  | --------------------------- | ----------------------------------------------------------- |
  | `Response.text(corps)`      | statut 200, `text/plain; charset=utf-8`                     |
  | `Response.html(corps)`      | statut 200, `text/html; charset=utf-8`                      |
  | `Response.json(texte)`      | statut 200, `application/json` (le texte JSON est à fournir) |
  | `Response.redirect(url)`    | statut 302 vers `url`                                       |
  | `Response.not_found()`      | statut 404                                                  |
  | `r.with_status(201)`        | la même réponse avec un autre statut                        |
  | `r.with_header("X-Id", "7")` | la même réponse avec un en-tête de plus                    |

  Sans `Content-Type`, le serveur ajoute `text/plain; charset=utf-8`. Les en-têtes qui
  contiennent un retour à la ligne sont ignorés, et `Content-Length` et `Connection` sont gérés
  par le serveur.
- **L'état** : le code du fichier s'exécute une seule fois, donc `visits` se souvient de sa
  valeur d'une requête à l'autre, comme dans un serveur Node. Avec un seul interpréteur (le
  défaut), les requêtes sont traitées **une par une** : les variables ne changent jamais pendant
  l'exécution de `handle`.
- **Les erreurs** : un `panic`, une erreur d'exécution ou une boucle trop longue (limite
  d'étapes, `--max-steps`, **par requête**) donnent une réponse `500 Internal Server Error`
  **sans détail** pour le client ; le détail est écrit dans le journal du serveur (la sortie
  d'erreur), et le serveur continue. Une requête qui attend plus de 30 secondes reçoit `504`.
- **Les capacités** : le programme tourne comme avec `qlang run` : `print`, fichiers,
  variables d'environnement, `http.send`, `Db`, `import` fonctionnent (`--root` limite les
  fichiers et les imports à un dossier). `--seed N` fixe le hasard.

#### Le routeur

Quand il y a plusieurs pages, un `Router` évite un grand `match` sur le chemin : on déclare les
routes, puis `handle` lui passe la requête.

```
let router = Router.new()

router.get("/", fun(req: Request) -> Response
  Response.text("Accueil")
end)

router.get("/items/:id", fun(req: Request) -> Response
  let id = req.params.get("id", "?")
  Response.text("Objet {id}")
end)

router.post("/items", fun(req: Request) -> Response
  Response.text("créé").with_status(201)
end)

fun handle(req: Request) -> Response
  router.handle(req)
end
```

- **Déclarer** : `router.get(chemin, fonction)`, `post`, `put`, `delete`, ou
  `router.add("PATCH", chemin, fonction)` pour une autre méthode. Chaque appel renvoie le
  routeur, donc on peut les enchaîner.
- **Le chemin** est une suite de segments séparés par `/` : un mot doit être identique, `:nom`
  accepte n'importe quel segment **non vide** et le met dans `req.params` (`req.params.get("id",
  "")`), et un `*` final accepte le reste du chemin (`req.params["*"]`). `/items/:id` accepte
  `/items/3`, pas `/items/` ni `/items/3/x`. La première route qui correspond gagne.
- **Sans correspondance** : un chemin connu avec une autre méthode donne `405` avec un en-tête
  `Allow` (`GET, DELETE`) ; tout autre chemin donne `404`, ou la fonction donnée à
  `router.fallback(fonction)`.
- **`Router.matches(modèle, chemin)`** renvoie les paramètres (un `map<string, string>?`) si le
  chemin correspond au modèle, sinon `none` : utile pour vérifier un modèle sans serveur.

Options de `qlang serve app.q` :

| Option           | Rôle                                                                                      |
| ---------------- | ----------------------------------------------------------------------------------------- |
| `--port N`       | le port (8080 par défaut)                                                                 |
| `--bind ADRESSE` | l'adresse d'écoute (`127.0.0.1` par défaut : seulement cette machine)                     |
| `--max-steps N`  | le nombre d'étapes d'évaluation **par requête**                                           |
| `--static DOSSIER` | sert les fichiers de ce dossier tels quels, **avant** d'appeler le programme            |
| `--workers N`    | N interpréteurs (de 1 à 64) qui répondent en parallèle                                    |
| `--root DOSSIER` | limite `import` et `fs` à ce dossier                                                      |
| `--seed N`       | fixe le hasard (chaque worker prend `N` plus son numéro)                                  |

- **Fichiers statiques** (`--static public`) : pour `GET` et `HEAD`, si le chemin désigne un
  fichier du dossier (ou `index.html` pour un dossier), il est servi avec son type
  (`.html`, `.css`, `.js`, `.json`, `.svg`, `.png`…), sans passer par le programme. Un chemin
  qui sort du dossier (`..`, lien symbolique vers l'extérieur) n'est jamais servi. Le reste va
  au programme.
- **Plusieurs workers** (`--workers 4`) : chaque worker exécute le fichier **à part**, avec ses
  propres variables, et prend les requêtes dans une file commune : une requête lente ne bloque
  plus les autres. Ce qui doit être partagé (un compteur, des données) va dans une base de
  données ou des fichiers, pas dans une variable.
- **Arrêt** : Ctrl-C ou un signal `TERM` arrête proprement le serveur : les requêtes déjà
  acceptées reçoivent leur réponse (5 secondes au plus), les nouvelles reçoivent `503`.
- **Le serveur** est minimal : HTTP/1.1 sans TLS ni connexions persistantes, 16 connexions
  en même temps au plus, corps limité à 1 Mo, pas de requêtes en morceaux (`Transfer-Encoding:
  chunked`). Comme `qlang serve` pour le protocole JSON, **placez-le derrière un proxy
  inverse** et ne l'exposez pas directement à Internet.

### Prélude : `Db`

`Db.connect(url)` ouvre une connexion à une base de données ; le schéma de
l'URL choisit le pilote : `sqlite://chemin/vers/fichier.db`, ou
`postgres://utilisateur:mot_de_passe@hôte/base`. Pour Postgres en TLS, ajoute
`?sslmode=require` (ou `prefer`/`verify-ca`/`verify-full`, ils sont traités
pareil ici) : `postgres://user:pass@hôte/base?sslmode=require`. Le certificat
du serveur est toujours vérifié contre les autorités publiques (Mozilla) ;
un certificat auto-signé ou une autorité privée ne sont pas acceptés sans
configuration côté hôte.

```
let c = Db.connect("sqlite://data.db")
if c.is_err() then
  print(c.unwrap_err().message)
else
  let db = c.unwrap()
  let r = db.execute("CREATE TABLE IF NOT EXISTS people (id INTEGER PRIMARY KEY, name TEXT)", [])
  db.execute("INSERT INTO people (name) VALUES (?)", [Json.string("Rex")])

  let rows = db.query("SELECT id, name FROM people", [])
  if rows.is_ok() then
    for row in rows.unwrap() do
      print(row.get("name", Json.null()).as_string())
    end
  end
  db.close()
end
```

- **`query(sql, params)`** renvoie `Result<array<map<string, Json>>, Error>` : une ligne
  par résultat, chaque colonne devenant une valeur `Json` (donc `row.get("col", ...)` ou
  `row["col"]`, puis `as_string()`/`as_number()`/`as_bool()`).
- **`execute(sql, params)`** (pour `INSERT`/`UPDATE`/`DELETE`/DDL) renvoie
  `Result<int, Error>` : le nombre de lignes affectées.
- **`params`** est un `array<Json>` (`Json.string(...)`, `Json.number(...)`...) : ces
  valeurs remplacent les `?` de `sql`, dans l'ordre. Le point d'interrogation `?` est
  toujours la syntaxe à écrire, quel que soit le moteur ; vers Postgres, qlang le
  réécrit lui-même en `$1, $2...`.
- **`close()`** ferme la connexion ; `Result<bool, Error>`.
- **`begin()`**, **`commit()`** et **`rollback()`** : une transaction (`Result<bool,
  Error>`). Les requêtes entre `begin()` et `commit()`/`rollback()` sont groupées ;
  `rollback()` annule tout ce qui a été fait depuis `begin()`.

```
db.begin()
db.execute("UPDATE comptes SET solde = solde - 100 WHERE id = ?", [Json.number(1.0)])
db.execute("UPDATE comptes SET solde = solde + 100 WHERE id = ?", [Json.number(2.0)])
let ok = db.commit()
if ok.is_err() then
  db.rollback()
end
```

- Une erreur de connexion ou de requête (SQL invalide, table absente...) donne un
  `Result::err`, jamais un arrêt du programme, comme `http.send`.
- Besoin d'un accès base de données côté hôte, comme pour `http`. `Db` est écrit en
  qlang au-dessus de quatre [`extern fun`](#fonctions-fournies-par-lenvironnement-extern)
  (`sql_connect`, `sql_query`, `sql_execute`, `sql_close`) que l'hôte fournit :
  `qlang` les fournit pour SQLite et Postgres, et un autre environnement peut brancher
  sa propre base sous les mêmes noms.
- **Limites actuelles** : côté Postgres, seuls les types scalaires courants (`bool`,
  entiers, flottants, texte) sont bien convertis, les autres (date, uuid, json,
  tableau...) retombent en texte si possible, sinon `none` ; pas de BLOB SQLite (devient
  `none`) ; pas de pool de connexions ni de requêtes préparées mises en cache.

### Prélude : fichiers et environnement

`fs.read(chemin)` lit le contenu entier d'un fichier, `fs.write(chemin,
contenu)` le remplace entièrement (le crée si besoin). Les deux renvoient un
`Result` : un fichier absent ou une erreur de permission donnent un
`Result::err`, jamais un arrêt du programme.

```
let r = fs.read("config.json")
if r.is_err() then
  print(r.unwrap_err().message)
else
  print(r.unwrap())
end

fs.write("sortie.txt", "contenu").unwrap()
```

`env.get(nom)` renvoie la valeur d'une variable d'environnement, ou `none` si
elle n'est pas définie (`string?`, pas de `Result` : une variable absente est
une situation normale, pas une erreur). `env.args()` renvoie les arguments
propres au programme (`array<string>`).

```
let lang = env.get("LANG")
print(lang != none)
print(env.args())
```

Besoin d'un accès au disque (`fs`) ou à l'environnement du processus (`env`)
côté hôte : un hôte qui ne les offre pas (un bac à sable, par exemple) fait
échouer la vérification avant même l'exécution, comme `http`/`Db`. Avec
`qlang-cli`, les arguments après `--` sur la ligne de commande sont ceux que
`env.args()` renvoie (`qlang run script.q -- a b`), et `fs` respecte `--root`
comme les imports.

## 18. Fins de ligne

Il n'y a pas de `;`. **Une instruction se termine à la fin de la ligne.** La
ligne continue sur la suivante dans ces cas :

- elle se termine par un opérateur ou une virgule ;
- une parenthèse `(`, un crochet `[` ou une accolade `{` est encore ouvert.

```
let total = a +
  b +
  c

let p = Point {
  x: 1,
  y: 2
}
```

Un bloc (corps de fonction, de `if`, de boucle…) reprend ses propres règles, même
à l'intérieur de parenthèses : chaque instruction y tient sur sa ligne.

### Prélude : lancer un programme

`Process.run(commande, arguments)` lance un programme de la machine, attend sa fin et renvoie un
`Result<ProcessOutput, Error>` ; `ProcessOutput` a trois champs publics : `status` (le code de
sortie, −1 si un signal l'a arrêté), `stdout` et `stderr` (ce qu'il a écrit, en texte).
`Process.run_with_input(commande, arguments, texte)` écrit en plus `texte` sur son entrée standard.

```
let r = Process.run("git", ["rev-parse", "HEAD"])
match r
  case Ok(o) then print(o.stdout.trim())
  case Err(e) then print("pas de git : {e.message}")
end
```

- **Aucun shell** : la commande et chaque argument sont donnés tels quels au système (pas
  d'étoile, de pipe ni de variable). Un programme qui se termine avec un code non nul est quand
  même un `Ok` : c'est à vous de lire `status`. Un programme introuvable est un `Err`.
- **Il faut la permission** : sans elle, `Process.run` est une erreur de compilation (`T250`).
  Dans le terminal, c'est `--allow-process` (sur `qlang run`, `qlang test` et `qlang serve`) ; le
  protocole JSON, le navigateur et un `qlang.toml` ne peuvent pas l'accorder. `qlang check` n'en a
  pas besoin.
- Un programme lancé n'est **pas limité** par `--root` ni par les limites de qlang (pas, mémoire) ;
  il est arrêté s'il tourne plus d'une minute, et on garde au plus 16 Mo de chaque flux.

---

## 19. Mots-clés réservés

```
let const fun return
if then elseif else end
while for in step do break continue
match case
struct impl trait enum extends override
public private protected static
import export from as
and or not div mod
true false none self super
```

`int`, `float`, `bool`, `string`, `uuid`, `ulid`, `datetime`, `array` et `range` **ne sont pas** des
mots-clés : ce sont des noms de types ordinaires, mais on ne peut pas définir un
type ou une fonction qui porte l'un de ces noms, ni celui d'une fonction
prédéfinie (`print`, `write`, `read`, `panic`, `assert`).

---

## 20. Erreurs et limites

**Les erreurs sont des données.** Chaque erreur a un code, un message, une
position (fichier, ligne, colonne) et parfois des notes. Le préfixe du code
indique l'étape :

| Préfixe | Étape                                                        |
| ------- | ------------------------------------------------------------ |
| `L`     | lecture du texte (caractère inconnu, chaîne non terminée…)   |
| `P`     | syntaxe (il manque un `end`, expression attendue…)           |
| `T`     | noms et types (nom inconnu, type incompatible, visibilité…)  |
| `R`     | exécution (division par zéro, indice hors limites, `panic`…) |
| `J`     | requête JSON invalide (voir le README)                       |
| `X`     | erreur interne : un bug de qlang (à signaler)                |

Le compilateur signale **plusieurs erreurs à la fois** quand il le peut, jusqu'à
100 : au-delà, il l'indique (`too many errors`) au lieu de tout lister. Les
erreurs de syntaxe sont signalées seules ; les erreurs de types viennent après.
Une erreur d'exécution affiche aussi la pile des appels.

**Limites d'exécution.** Pour qu'un programme ne puisse pas bloquer son
environnement, l'exécution est bornée :

| Limite                                                                  | Par défaut  | Erreur                       |
| ----------------------------------------------------------------------- | ----------- | ---------------------------- |
| étapes d'évaluation                                                     | 100 000 000 | `R900` (boucle infinie ?)    |
| appels imbriqués                                                        | 1 000       | `R901` (récursion infinie ?) |
| sortie affichée                                                         | 10 Mo       | `R902`                       |
| taille d'un tableau ou dictionnaire (éléments) ou d'une chaîne (octets) | 10 000 000  | `R903`                       |
| mémoire du processus (programme `qlang`, `--max-memory`)                | 1 Go        | `R904`                       |

Le temps passé dans les opérations prédéfinies compte aussi : chercher dans un
tableau (`contains`, `index_of`), le trier, l'assembler (`join`), copier une
chaîne (`+`, `replace`, `split`…) coûte environ une étape pour 4 éléments (ou 32
octets). Une boucle qui fouille un très grand tableau à chaque tour atteint donc
la limite d'étapes, comme n'importe quelle boucle trop longue.

Comparer deux valeurs (`==`, `<`) descend au plus de 1 000 niveaux dans les énumérations, tableaux et
tuples imbriqués : au-delà, `R092` (un programme peut construire des listes chaînées de millions de
maillons ; leur libération, elle, n'a pas de limite).

Afficher une valeur (`print`, interpolation, `as string`) descend au plus de 64
niveaux dans les tableaux et structures : en dessous, elle affiche `...`. Cela
évite qu'une structure qui se contient elle-même (`n.next = n`) ne boucle sans
fin.

Le code source lui-même ne peut pas dépasser 200 niveaux d'imbrication.

---

## 21. Points encore ouverts

- **Bibliothèque standard :** elle couvre le quotidien (chaînes, tableaux, dictionnaires,
  `Set`, tuples, maths, encodages, fichiers, réseau, processus, tests). Le reste vit en
  paquets : voir `qlang-std` (CSV, gabarits, TOML) ; YAML, mail et S3 ne sont pas écrits.
- **Dictionnaires :** pas de clés de type tableau ou structure (voir plus
  haut), et pas de comparaison `<`.
- **Plus petit entier :** `-9223372036854775808` ne peut pas s'écrire comme
  littéral (le chiffre sans signe dépasse 64 bits).
- **Identifiants :** ASCII uniquement (`a-z`, `A-Z`, `0-9`, `_`), alors que les
  chaînes et commentaires acceptent tous les caractères Unicode.
- **Traits qui en exigent d'autres** (un trait qui suppose qu'un autre est
  aussi implémenté) : non prévus pour l'instant.
- **Fonctions anonymes génériques :** non prévues.
- **Format des commentaires de documentation** : le texte reste libre (`qlang doc` le recopie
  tel quel). À prévoir, selon l'usage : un format léger du type `@param` / `@return`.
