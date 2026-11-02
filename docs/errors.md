# Codes d'erreur

Chaque diagnostic de qlang a un code. Un code publié garde son sens (`docs/stability.md`) ; de nouveaux apparaissent.
Les messages ci-dessous sont les modèles du compilateur : `{nom}` est remplacé par la valeur réelle. Un code peut avoir plusieurs messages.

Ce fichier est vérifié par un test (`crates/qlang-core/tests/error_codes.rs`) : chaque code émis y figure.

## Lxxx : Lecture du texte

| Code | Message |
| --- | --- |
| `L001` | `unexpected character {c}` (pour `!`, `&`, `|`, `;` et `'`, avec l'indication : `not`, `and`, `or`, fin de ligne, guillemets doubles) |
| `L002` | ``unterminated string (missing closing `"`)`` |
| `L003` | ``unterminated block comment (missing `--)`)`` <br> ``unterminated documentation comment (missing `--"`)`` <br> `this tuple position is too large` |
| `L004` | `expected hexadecimal digits after 0x` <br> `expected binary digits (0 or 1) after 0b` |
| `L005` | `integer literal is too large` <br> `float literal is out of range` |

## Pxxx : Syntaxe

| Code | Message |
| --- | --- |
| `P001` | `expected {expected}, found {}` |
| `P002` | `unexpected {}; expected the end of the line` |
| `P003` | `this code is nested too deeply` |
| `P010` | ``comparison operators cannot be chained (use `and`)`` |
| `P020` | `unknown escape sequence (allowed: \  \t \r \0 \e \\ \" \{ \})` |
| `P021` | ``unterminated `{` in string (write `\{` for a literal brace)`` |
| `P022` | ``empty `{}` in string (write `\{\}` for literal braces)`` |
| `P023` | `unexpected {} in string interpolation` |
| `P030` | ``a type can only be nullable once (`int?`, not `int??`)`` |
| `P041` | ```self` must be the first parameter`` |
| `P042` | ``parameter `{}` needs a type (`{}: type`)`` <br> ``field `{}` needs a type (`{}: type`)`` |
| `P043` | ``the default of `{}` must be a literal (a number, a text without `{{}}`, `true`, `false`, `none`, `[]` or `{{}}`)`` |
| `P050` | ``visibility and `static` only apply to members of a `struct` or `impl``` |
| `P051` | ``annotations only apply to a `fun`, `struct`, `enum`, `trait` or a member of one`` <br> ```{}` is only allowed at the top level of a file`` |
| `P052` | ```extern` must be followed by `fun name(...)``` <br> ``named functions are only allowed at the top level; here write `let name = fun(...) ... end``` |
| `P060` | ``a tuple pattern needs a value: `let (a, b) = expr``` <br> ```{0}` has no initial value: variables must be initialized (write `let {0}: T? = none` for an empty variable)`` <br> ``a tuple type needs at least two types: `(int, string)``` |
| `P061` | ``a tuple pattern needs at least two names: `(a, b)``` |
| `P062` | ``a tuple pattern needs at least two patterns: `(0, y)``` |
| `P070` | ``only one `..base` is allowed`` |
| `P071` | ``expected `:` and a value after field `{}``` |
| `P080` | ```super` must be followed by a method call (`super.method(...)`)`` |
| `P090` | ``a pattern cannot contain `{...}``` |

## Txxx : Noms et types

| Code | Message |
| --- | --- |
| `T000` | `too many errors: only the first {MAX_DIAGNOSTICS} are shown` |
| `T001` | ``unknown {what} `{}``` <br> ```{}` is not a module`` <br> ``unknown module `{}``` |
| `T002` | ``expected `{e}`, found `{f}`{hint}`` |
| `T003` | `this expression has no value` |
| `T004` | ``a condition must be a `bool`, found `{shown}` (write an explicit comparison such as `!= none` or `> 0`)`` <br> ```not` needs a `bool`, found `{shown}``` <br> ```{}` needs `bool` operands, found `{shown}``` |
| `T005` | ``cannot negate a `{shown}``` <br> ``cannot compare `{a}` with `{b}``` <br> ``cannot apply `{}` to `{a}` and `{b}`{hint}`` |
| `T006` | ``cannot convert `{a}` to `{b}` with `as` (a conversion needs an `impl As<{b}> for {a}`){hint}`` |
| `T007` | ``cannot index a `{b}` with a `{i}`{hint}`` <br> ``cannot index a `{b}` with a `{i}``` |
| `T008` | ```x[]` can only be used as the target of an assignment: `x[] = value``` <br> ```x[]` only works with `=`: `x[] = value``` <br> ``cannot append a `{v}` to a `{b}` with `[] =` (an `impl Push<{v}> for {b}` is needed)`` |
| `T009` | ``map {what} must have the same type: found `{a}` and `{b}``` <br> ``array elements must have the same type: found `{a}` and `{b}``` <br> ``the branches have different types: `{a}` and `{b}``` |
| `T010` | ```??` needs a nullable left side, found `{shown}``` |
| `T100` | ``cannot import "{}": use a path relative to this file, written with `/` (`./x.q`, `../lib/x.q`) or a package (`@name/x.q`), not an absolute one`` |
| `T101` | `these modules import each other in a cycle and each runs top-level code, so there is no valid order to run them: {}` |
| `T102` | `cannot import "{}": {e}` |
| `T110` | ```{}` is a built-in type name and cannot be redefined`` <br> ```{}` is a built-in function and cannot be redefined`` |
| `T111` | ```{}` is already declared in this file`` |
| `T112` | ``variant `{}` is declared twice in enum `{}``` |
| `T113` | ```{}` is not exported by "{}" (add `export {}` in that file)`` <br> ```{}` is not exported by "{p}"`` |
| `T114` | ``"{}" has no `{}``` <br> ``"{p}" has no `{}``` |
| `T115` | ``cannot export `{}`: nothing with that name is declared in this file`` |
| `T120` | `a type without value cannot be nullable` <br> `a type without value cannot be in a tuple` |
| `T121` | ```{}` expects {n} type argument(s)`` <br> ```{}` expects {} type argument(s), found {}`` <br> ``type parameter `{name}` takes no type arguments`` |
| `T122` | ``the type alias `{name}` refers to itself`` <br> `expected a trait here` <br> ```{}` is not a trait`` |
| `T123` | ``type parameter `{}` is declared twice`` |
| `T124` | ``method `{}` is declared twice in trait `{}``` |
| `T125` | ``a struct can only extend another struct, not `{shown}``` |
| `T126` | ``field `{}` is declared twice in `{}``` <br> ``field `{n}` already exists in parent struct `{pn}``` |
| `T127` | ``static field `{0}` needs an initial value (`static {0}: type = value`)`` <br> ```self` is only available in methods`` <br> ```self` is not a parameter of an anonymous function`` |
| `T128` | `anonymous functions cannot be generic` |
| `T130` | ``cannot implement methods for `{shown}``` |
| `T131` | ``method `{}` is declared twice in this impl`` <br> ``method `{name}` is already defined for this type`` |
| `T132` | ``only methods with `self` can be overridden`` <br> ```override` applies to methods of a struct, not to trait implementations`` <br> ``method `{name}` redefines a method of `{pn}`: add `override``` |
| `T133` | ``missing method `{}` required by trait `{trait_shown}``` |
| `T134` | ``method `{}` should {} `self``` <br> ``method `{}` should have {} generic parameter(s)`` <br> ``method `{}` should take {} parameter(s), found {}`` |
| `T135` | ```{name}` is not a method of trait `{trait_shown}``` |
| `T136` | `a trait cannot extend itself` <br> ``a trait can only extend another trait, not `{shown}``` <br> ``trait `{trait_shown}` extends `{needed}`: `{target}` must implement it too`` |
| `T140` | ``struct `{name}` cannot extend itself, directly or not`` |
| `T150` | ```{}` is already declared in this scope`` |
| `T151` | ```self` is only available inside a method that takes `self``` |
| `T152` | ```super` can only be used as `super.method(...)``` |
| `T153` | ```{name}` is used before it is declared`` |
| `T154` | ```{}` is a type or a module, not a value`` <br> ```{0}` is a type: use `{0}.new(...)` or `{0} {{ ... }}``` <br> ```{name}` is a type, not a function: use `{name}.new(...)` or `{name} {{ ... }}``` |
| `T155` | ```{name}` is a built-in function: call it as `{name}(...)``` |
| `T160` | ``{what} is {kind} in `{owner_name}``` |
| `T161` | ```{shown}` has no static member `{}``` <br> ```{shown}` has no field `{}``` <br> ```{shown}` has no method `{}``` |
| `T162` | ``this value may be `none` (`{shown}`): check `!= none` before using `.{}`{hint}`` <br> ``this value may be `none` (`{shown}`): check `!= none` before calling `.{}()``` <br> ``this value may be `none` (`{shown}`): check `!= none` before using `.{index}``` |
| `T163` | ```{}` is a static field: use `{}.{}``` |
| `T164` | ```{}.{}` carries data: write `{}.{}(...)` with its values`` <br> ```{}` is a method of `{shown}`: call it with `()``` <br> ```{0}.{1}` carries no data: write `{0}.{1}` without ()`` |
| `T170` | ``cannot infer the type of `{0}` from `none`: write `let {0}: type? = none``` <br> ``cannot infer the types of `{0}`: write `let {0}: map<key, value> = {{}}``` <br> ``cannot infer the element type of `{0}`: write `let {0}: array<type> = []``` |
| `T171` | ``cannot infer the type parameters of `{0}`: write them explicitly, like `{0}<int>(...)``` <br> ``cannot infer the type arguments of `{n}`: write `{n}<...> {{ ... }}``` |
| `T180` | ``this function does not return a value (add `-> type` to its signature)`` <br> ```return` needs a value of type `{r}``` |
| `T181` | ``{what} must give a value of type `{r}``` |
| `T190` | ```step` can only be used with a range`` |
| `T191` | ```{w}` is only allowed inside a loop`` |
| `T210` | ``cannot assign to `{name}`: it is a constant`` |
| `T211` | ``cannot assign to `{name}`: it belongs to another module`` |
| `T212` | ``cannot assign to `{}``` <br> ``cannot assign to `{name}`: it is not a variable`` |
| `T213` | ``a tuple cannot be changed: build a new one (`t = (t.0, new_value)`)`` <br> ``invalid assignment target (use a variable, `a.field` or `a[i]`)`` |
| `T220` | ``this pattern has type `{b}`, but the value matched has type `{a}``` <br> ```none` can never match a `{shown}` (only nullable values, like `{shown}?`)`` <br> ``this pattern is a tuple, but the value matched has type `{shown}``` |
| `T221` | ```{}` is not a variant: the value matched has type `{shown}``` <br> ``expected `Enum.Variant``` <br> ``enum `{en}` has no variant `{}``` |
| `T222` | ```{}` carries {} value(s), but the pattern names {}`` |
| `T230` | ```{n}` is not a struct`` |
| `T231` | ```..base` must be a value of `{b2}` or of one of its parent structs, found `{a}``` <br> ```..base` must be a struct value, found `{shown}``` |
| `T232` | `struct {nom} has no field {champ}` <br> `{champ} is a static field: it belongs to the type, not to each value` |
| `T233` | ``field `{}` is given twice`` |
| `T234` | ``missing field(s) for `{}`: {}`` |
| `T240` | ```{}` expects {wanted} argument(s), found {}`` <br> ```{name}` expects {n} argument(s), found {}`` |
| `T241` | ``cannot call a `{shown}`: it is not a function`` |
| `T242` | ```{a}` cannot be used for `{pn}` in `{}`: it does not implement `{tn}``` |
| `T243` | ```{0}` is a static method: call it as `{shown}.{0}(...)`, not on a value`` |
| `T244` | ```{0}` needs a value: call it on an instance (`value.{0}(...)`), not on `{shown}``` |
| `T245` | ```{}` does not accept named arguments`` <br> ```{name}` does not accept named arguments`` |
| `T246` | `positional arguments must come before named arguments` |
| `T247` | ```{}` has no parameter named `{}``` |
| `T248` | ``parameter `{}` is given more than once`` |
| `T249` | ```{}` is missing the argument for `{pn}``` |
| `T250` | ```{what}` is not available in this environment ({why})`` <br> ```{b}.{fname}` is not available in this environment ({why})`` <br> ```{name}` is not available in this environment (there is no input to read)`` |
| `T260` | ```{name}()` needs elements that can be ordered, but `{shown}` cannot (implement `Ord` for it)`` <br> ```sum()` needs an array of numbers, not `array<{shown}>``` <br> ``an `extern fun` cannot be generic`` |
| `T261` | ```flatten()` needs an array of arrays, not `array<{shown}>``` <br> ``parameter `{}` of an `extern fun`: {why}`` |
| `T262` | ``return type of an `extern fun`: {why}`` |
| `T263` | `{nom} is not available in this environment (the host does not provide it)` |
| `T270` | ```{shown}` cannot be a map key: some of its variants carry data`` <br> ```{shown}` cannot be a map key (use int, float, string, bool, uuid, ulid, datetime or an enum)`` |
| `T280` | ```?` can only be used inside a function (it leaves the function when there is an error)`` |
| `T281` | ```?` needs a `Result` or a nullable value, found `{shown}``` |
| `T282` | ```?` on a `Result` leaves the function with its error, so the function must return a `Result<_, {er}>`, not `{shown}``` <br> ```?` on a nullable value leaves the function with `none`, so the function must return a nullable (`T?`), not `{shown}``` |
| `T286` | ```{shown}` has {} elements, so `.{index}` does not exist (they are numbered from 0)`` |
| `T287` | ```.{index}` needs a tuple, found `{shown}``` |
| `T288` | `the assignment takes {n} values, but the tuple has {}` |
| `T289` | ```{shown}` cannot be taken apart: only a tuple can`` |
| `T290` | ```{name}` is not an annotation (declare it with `#[annotation]` on a `struct`)`` |
| `T291` | ```#[annotation]` only applies to a `struct``` <br> ```#[annotation]` takes no arguments`` |
| `T292` | ```#[deprecated]` takes one text: `#[deprecated("use Other.thing")]``` <br> ```#[used]` takes no arguments`` <br> ```{name}` has no field `{}``` |
| `T293` | ```{name}` needs a value for `{fname}``` |
| `T294` | ``an annotation argument must be a literal (a number, a text, `true`, `false`, `none` or an array of those)`` |
| `T295` | ```{}` is an annotation: it cannot have type parameters, a parent or static fields`` |
| `T300` | ```Reflect.{}` takes {rest} argument(s) after an optional module`` <br> ```Any` has one function: `Any.of(value)``` <br> ```Json.of` takes one value`` |
| `T301` | ``the first argument of `Reflect.{}` must be a module (`import "x.q" as m`)`` <br> ```Reflect.struct_info` needs a struct type`` <br> ```Reflect.enum_info` needs an enum type`` |
| `T302` | ```Reflect` has no `{other}` (it has `struct_info`, `enum_info`, `functions`, `structs`, `callable`, `callables`, `constructors`, `declared`, `invoke`, `decode`, `parse_text`, `parts`, `info_of`, `items`, `get`, `construct`, `type_name`, `find`, `methods`, `find_method`, `bound_callables`, `bound_callables_of`, `type_info` and `type_info_of`)`` |
| `T303` | ``this expression has no value to put in an `Any``` <br> ``the type of this value is not known yet: give it a type (`let xs: array<int> = []`)`` |
| `T304` | ```as<T>` needs a type that is not `none`-able and not `Any``` <br> ``an `Any` has `as<T>()` (the value, or `none`) and `type_name()``` |
| `T305` | ``a `Json` cannot be turned into `{shown}``` |

## Rxxx : Exécution

| Code | Message |
| --- | --- |
| `R001` | `this value is not a tuple of the expected size` <br> ``a `bool` was expected here`` <br> `this value is not a tuple with that element` |
| `R002` | ```{n}` is used before it has a value`` <br> ```{name}` is used before it has a value`` <br> ```self` is not available here`` |
| `R003` | ```super` can only be used for calls`` <br> ```x[]` can only be assigned to`` <br> `invalid assignment target` |
| `R004` | `this value cannot be negated` <br> `these values cannot be ordered` <br> ``cannot apply `{}` to these values`` |
| `R005` | ``this value has no field `{}``` <br> ``this value has no field `{n}``` |
| `R006` | ``cannot read `.{}`: the value is `none``` <br> ``cannot call `.{}()`: the value is `none``` <br> ``cannot set `.{}`: the value is `none``` |
| `R007` | `this value is not a function` |
| `R008` | ``this value has no method `{}``` <br> `this value has no such method` |
| `R009` | `this conversion is not available` |
| `R010` | `integer overflow` |
| `R011` | `division by zero` |
| `R012` | ``negative exponent: use decimals (`2.0 ** -1`) for fractions`` |
| `R013` | `cannot convert {f} to an integer` |
| `R014` | `square root of a negative number` |
| `R016` | ``{x} is outside the domain of `{name}``` <br> ```log` needs a positive number and a positive base other than 1 (found {x} and {base})`` <br> ```clamp` needs min <= max (found {lo} and {hi})`` |
| `R017` | ```time.sleep` waits from 0 to 3600000 milliseconds (found {ms})`` <br> ```random.bytes` gives from 0 to 1048576 bytes (found {n})`` |
| `R020` | `index {i} is out of bounds (length {})` <br> `index {n} is out of bounds (length {len})` <br> `an array index must be an int` |
| `R021` | `cannot count occurrences of an empty string` <br> `the padding must be exactly one character` <br> `invalid repeat count` |
| `R022` | ``this key is not in the map (use `get(key, default)` or check `has(key)` first)`` |
| `R023` | `this value cannot be used as a map key` |
| `R024` | ```code()` needs a string of exactly one character`` <br> `{n} is not a valid character code` |
| `R030` | ```step` must be an integer`` |
| `R031` | ```step` cannot be 0`` |
| `R032` | `this value cannot be looped over` |
| `R040` | `no more input to read` |
| `R041` | ```read` is not available in this environment`` <br> `cannot read input: {m}` <br> ```read_all` is not available in this environment`` |
| `R050` | `panic: <boom> & "quotes"` <br> `panic: {msg}` |
| `R051` | `assertion failed` <br> `assertion failed: {message}` |
| `R060` | `waiting is not available in this environment` <br> `cannot wait: {m}` <br> `the time is not available in this environment` |
| `R061` | `a ulid can only hold a time between 0 and 281 474 976 710 655 milliseconds` |
| `R062` | `random.int needs min <= max, found {lo} and {hi}` |
| `R063` | `randomness is not available in this environment` <br> `cannot get random numbers: {m}` |
| `R064` | `a datetime must be between the years 1 and 9999` <br> `this date does not exist: {}` |
| `R065` | `the result is out of the years 1 to 9999` |
| `R070` | ``a program that serves requests needs `fun handle(req: Request) -> Response``` <br> ```handle` must be `fun handle(req: Request) -> Response``` <br> ``there is no test `{name}``` |
| `R071` | ``the function of `Response.stream` must give a string, or `none` to end`` |
| `R080` | ```{name}` is not available in this environment`` |
| `R081` | ```{name}` failed: {m}`` |
| `R082` | ``this value is nested too deeply for `Json.of` (does it contain itself?)`` <br> `this Json is nested too deeply` <br> `this value is nested too deeply to leave the program (does it contain itself?)` |
| `R083` | ``the host returned a `Json` object with a key that is not a string`` <br> `the host returned {} where {what} was expected` <br> ``the host returned `{name}`, which is not a variant of `{}``` |
| `R084` | `a function or a range cannot be turned into Json` <br> `the host returned a collection that is too big` |
| `R090` | ```exit` cannot be used in a program that serves requests (it would stop the server)`` |
| `R091` | ``the test called `exit({code})``` |
| `R092` | `these values are nested too deeply to be compared` |
| `R301` | ```Reflect.struct_info` needs a struct type, found `{shown}``` |
| `R900` | `the program ran for too long (step limit reached: infinite loop?)` |
| `R901` | `too many nested calls (infinite recursion?)` |
| `R902` | `the program printed too much (output limit reached)` |
| `R903` | `a value became too large (memory limit reached)` <br> `this value is too large to print (memory limit reached)` |
| `R904` | `the program uses too much memory (memory limit reached)` |
| `R905` | `the request took more than {} s (--request-timeout)` |
| `R906` | `the task was cancelled` |
| `R907` | `the debugging session ended` |

## Wxxx : Avertissements

| Code | Message |
| --- | --- |
| `W001` | ```{name}` is deprecated: {why}`` |

## Xxxx : Erreurs internes

| Code | Message |
| --- | --- |
| `X999` | `internal error: {detail} (this is a bug in qlang, please report it)` <br> `internal error` |

## Jxxx : Requêtes JSON

| Code | Message |
| --- | --- |
| `J001` | `message` |

## Qxxx : Règles de `qlang lint`

| Code | Signalé |
| --- | --- |
| `Q001` | une variable jamais utilisée |
| `Q002` | une fonction privée que rien n'appelle |
| `Q003` | un nom importé qui n'est jamais écrit |
| `Q004` | du code après `return`, `break` ou `continue` |
| `Q005` | une fonction avec trop de chemins (`--max-complexity`) |
