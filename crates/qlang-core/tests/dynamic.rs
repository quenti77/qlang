//! `type` aliases, `Any`, `Json.of` / `Json.to<T>` and the `Reflect` calls that read, build
//! and call things by name.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run_with(files: &[(&str, &str)], src: &str) -> String {
    let mut host = MemoryHost::default();
    for (name, text) in files {
        host.files.insert(name.to_string(), text.to_string());
    }
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => host.output,
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            codes.join(",")
        }
    }
}

fn run(src: &str) -> String {
    run_with(&[], src)
}

#[test]
fn aliases_name_a_type_and_are_not_keywords() {
    let out = run("type Handler = fun(int) -> int
type Names = array<string>
type Pair = (int, Names)

fun twice(f: Handler, n: int) -> int
  f(f(n))
end

let type = 5
let p: Pair = (1, [\"a\"])
print(twice(fun(x: int) -> int x + type end, 1))
print(p)
");
    assert_eq!(out, "11\n(1, [\"a\"])\n");
}

#[test]
fn aliases_cannot_loop_or_name_nothing() {
    assert_eq!(run("type A = B\ntype B = A\n"), "T122");
    assert_eq!(run("type C = nope\n"), "T001");
    assert_eq!(run("type D = array<int>\nlet x: D<int> = []\n"), "T121");
}

#[test]
fn an_alias_can_be_imported() {
    let out = run_with(
        &[(
            "kinds.q",
            "type Id = int\nexport Id\nfun next(i: Id) -> Id\n  i + 1\nend\nexport next\n",
        )],
        "import Id, next from \"kinds.q\"\nlet a: Id = 1\nprint(next(a))\n",
    );
    assert_eq!(out, "2\n");
}

const DOMAIN: &str = "struct Address
  public city: string
  public zip: string? = none
end

struct User
  public id: int
  public name: string
  public tags: array<string> = []
  public address: Address? = none
  #[JsonName(\"e_mail\")]
  public email: string = \"none@x\"
  #[JsonSkip]
  public cache: int = 0
  secret: string = \"hidden\"
end

enum Shape
  Circle(radius: float)
  Square(side: float)
  Empty
end

fun double(n: int) -> int
  n * 2
end

fun greet(name: string, user: User) -> string
  \"hello {name} {user.name}\"
end

fun text(a: Any?) -> string
  if a == none then
    return \"none\"
  end
  let t = a.as<string>()
  if t == none then \"?\" else t end
end

";

fn with(body: &str) -> String {
    run(&format!("{DOMAIN}{body}"))
}

#[test]
fn any_remembers_the_type_of_a_value() {
    let out = with(
        "let a = Any.of(42)
print(a.as<int>())
print(a.as<string>())
print(a.type_name())
let u = User { id: 1, name: \"Ada\" }
let au = Any.of(u)
print(au.type_name())
print(au.as<User>() != none)
print(au.as<Address>() == none)
print(Reflect.type_name<array<User>>())
let xs: array<int> = []
print(Any.of(xs).as<array<string>>() == none)
print(Any.of(xs).as<array<int>>() != none)
",
    );
    assert_eq!(
        out,
        "42\nnone\nint\nUser\ntrue\ntrue\narray<User>\ntrue\ntrue\n"
    );
}

#[test]
fn any_sees_a_struct_through_its_parent_and_its_traits() {
    let out = run("trait Named
  fun name(self) -> string
end
struct Animal
  public legs: int = 4
end
struct Dog extends Animal
  public label: string = \"rex\"
end
impl Named for Dog
  public fun name(self) -> string
    self.label
  end
end
let a: Animal = Dog {}
let boxed = Any.of(a)
print(boxed.type_name())
print(boxed.as<Dog>() != none)
print(boxed.as<Animal>() != none)
print(boxed.as<Named>() != none)
");
    assert_eq!(out, "Dog\ntrue\ntrue\ntrue\n");
}

#[test]
fn any_mistakes_are_reported() {
    assert_eq!(run("let a = Any.of([])\n"), "T303");
    assert_eq!(run("let a = Any.of(1)\nlet b = a.as<int?>()\n"), "T304");
    assert_eq!(run("let a = Any.of(1)\nlet b = a.nope()\n"), "T304");
    assert_eq!(run("let a = Any.of(1)\nprint(a == a)\n"), "T005");
}

#[test]
fn json_of_writes_public_fields_only() {
    let out = with(
        "let u = User { id: 1, name: \"Ada\", tags: [\"x\", \"y\"], address: Address { city: \"Paris\" } }
print(Json.of(u).to_text())
print(Json.of(Shape.Circle(2.5)).to_text())
print(Json.of(Shape.Empty).to_text())
print(Json.of([1, 2, 3]).to_text())
print(Json.of({ \"a\": 1.5 }).to_text())
print(Json.of(\"a\\\"b\").to_text())
",
    );
    assert_eq!(
        out,
        "{\"id\":1,\"name\":\"Ada\",\"tags\":[\"x\",\"y\"],\"address\":{\"city\":\"Paris\",\"zip\":null},\"e_mail\":\"none@x\"}\n\
         {\"type\":\"Circle\",\"radius\":2.5}\n\
         \"Empty\"\n\
         [1,2,3]\n\
         {\"a\":1.5}\n\
         \"a\\\"b\"\n"
    );
}

/// A qlang string literal holding `json` (braces and quotes escaped).
fn lit(json: &str) -> String {
    format!(
        "\"{}\"",
        json.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('{', "\\{")
    )
}

fn parse(json: &str) -> String {
    format!("Json.parse({}).unwrap()", lit(json))
}

#[test]
fn json_to_reads_a_type_and_says_where_it_fails() {
    let user = parse(
        r#"{"id": 7, "name": "Bob", "e_mail": "b@x", "secret": "x", "address": {"city": "Rome"}}"#,
    );
    let out = with(&format!(
        "let u = Json.to<User>({user}).unwrap()
print(u.email)
print(text(Reflect.get(u, \"secret\")))
print(u.address != none)
print(Json.to<User>({a}).unwrap_err().message)
print(Json.to<User>({b}).unwrap_err().message)
print(Json.to<User>({c}).unwrap_err().message)
print(Json.to<User>({d}).unwrap_err().message)
print(Json.to<array<int>>({e}).unwrap_err().message)
print(Json.to<array<Shape>>({f}).unwrap())
",
        a = parse(r#"{"id": "7", "name": "B"}"#),
        b = parse(r#"{"id": 7}"#),
        c = parse(r#"{"id": 7.5, "name": "B"}"#),
        d = parse(r#"{"id": 7, "name": "B", "address": {"zip": "1"}}"#),
        e = parse(r#"[1, 2, "x"]"#),
        f = parse(r#"["Empty", {"type": "Square", "side": 3}]"#),
    ));
    assert_eq!(
        out,
        "b@x\nhidden\ntrue\n\
         id: expected an int, found string\n\
         name: missing\n\
         id: expected an int, found a number with decimals\n\
         address.city: missing\n\
         [2]: expected an int, found string\n\
         [Shape.Empty, Shape.Square(3.0)]\n"
    );
}

#[test]
fn json_to_does_not_fill_a_private_field() {
    let j = parse(r#"{"id": 1, "name": "B", "secret": "pwned"}"#);
    let out = with(&format!(
        "let j = {j}
print(text(Reflect.get(Json.to<User>(j).unwrap(), \"secret\")))
"
    ));
    assert_eq!(out, "hidden\n");
}

#[test]
fn callables_are_found_by_name_and_checked_when_called() {
    let out = with(
        "let u = User { id: 1, name: \"Ada\" }
let c = Reflect.callable(\"double\")
if c != none then
  print(c.params[0].type_name)
  print(c.return_type)
  print(c.call([Any.of(21)]).unwrap().as<int>())
  print(c.call([Any.of(\"x\")]).unwrap_err().message)
  print(c.call([]).unwrap_err().message)
end
let g = Reflect.callable(\"greet\")
if g != none then
  print(text(g.call([Any.of(\"Mr\"), Any.of(u)]).unwrap()))
end
print(Reflect.callable(\"nope\") == none)
",
    );
    assert_eq!(
        out,
        "int\nint\n42\nargument 1: expected int, found string\nexpected 1 argument(s), found 0\nhello Mr Ada\ntrue\n"
    );
}

#[test]
fn reflection_reads_and_builds_structs_by_name() {
    let out = with(
        "let u = User { id: 1, name: \"Ada\" }
print(text(Reflect.get(u, \"name\")))
print(text(Reflect.get(u, \"secret\")))
print(text(Reflect.get(u, \"nope\")))
print(Reflect.construct<User>({ \"id\": Any.of(9), \"name\": Any.of(\"Zed\") }).unwrap().name)
print(Reflect.construct<User>({ \"id\": Any.of(\"x\"), \"name\": Any.of(\"Zed\") }).unwrap_err().message)
print(Reflect.construct<User>({ \"id\": Any.of(1) }).unwrap_err().message)
print(Reflect.construct<User>({ \"id\": Any.of(1), \"name\": Any.of(\"a\"), \"zzz\": Any.of(1) }).unwrap_err().message)
for s in Reflect.structs() do
  print(s.name)
end
",
    );
    assert_eq!(
        out,
        "Ada\nhidden\nnone\nZed\nfield `id`: expected int, found string\nmissing field `name`\n`User` has no field `zzz`\nAddress\nUser\n"
    );
}

#[test]
fn reflection_reaches_into_an_imported_module() {
    let out = run_with(
        &[(
            "providers.q",
            "struct Db
  public url: string
end

fun make_db() -> Db
  Db { url: \"sqlite://x\" }
end

fun not_a_provider(n: int) -> int
  n
end
",
        )],
        "import \"providers.q\" as p
for f in Reflect.functions(p) do
  print(\"{f.name} -> {f.return_type}\")
end
let c = Reflect.callable(p, \"make_db\")
if c != none then
  print(Reflect.get(c.call([]).unwrap(), \"url\") != none)
end
for s in Reflect.structs(p) do
  print(s.name)
end
",
    );
    assert_eq!(out, "make_db -> Db\nnot_a_provider -> int\ntrue\nDb\n");
}

// ---------------------------------------------------------------- generic functions

#[test]
fn a_generic_function_knows_its_type_arguments_when_it_asks() {
    let out = with(
        "fun name_of<T>() -> string
  Reflect.type_name<T>()
end

fun unbox<T>(a: Any) -> T?
  a.as<T>()
end

fun body<T>(j: Json) -> Result<T, Error>
  Json.to<T>(j)
end

fun through<T>() -> string
  name_of<array<T>>()
end

fun again<T>(n: int) -> string
  if n == 0 then
    return name_of<T>()
  end
  again<array<T>>(n - 1)
end

print(name_of<int>())
print(name_of<User>())
print(through<string>())
print(again<int>(2))
let a = Any.of(\"x\")
print(unbox<string>(a))
print(unbox<int>(a) == none)
let u = body<User>(Json.parse(\"\\{\\\"id\\\": 3, \\\"name\\\": \\\"C\\\"\\}\").unwrap()).unwrap()
print(u.name)
print(body<User>(Json.parse(\"1\").unwrap()).is_err())
",
    );
    assert_eq!(
        out,
        "int\nUser\narray<string>\narray<array<int>>\nx\ntrue\nC\ntrue\n"
    );
}

#[test]
fn a_method_with_its_own_type_parameter_knows_it_too() {
    let out = with(
        "struct Bag
  public items: map<string, Any> = {}
end

impl Bag
  public fun put<T>(self, value: T)
    self.items[Reflect.type_name<T>()] = Any.of(value)
  end

  public fun get<T>(self) -> T?
    let found = self.items[Reflect.type_name<T>()]
    if found == none then
      return none
    end
    found.as<T>()
  end
end

let bag = Bag {}
bag.put(5)
bag.put(\"five\")
print(bag.get<int>())
print(bag.get<string>())
print(bag.get<float>() == none)
",
    );
    assert_eq!(out, "5\nfive\ntrue\n");
}

#[test]
fn handlers_get_their_arguments_read_from_text_and_json() {
    let out = run("#[annotation]
struct Route
  path: string
  method: string = \"GET\"
end
#[annotation]
struct Path
end
#[annotation]
struct Body
end

struct CreateUser
  public name: string
  public age: int = 18
end

struct Repo
  public label: string = \"repo\"
end

#[Route(\"/users/:id\")]
fun show(#[Path] id: int, repo: Repo) -> string
  \"user {id} from {repo.label}\"
end

#[Route(\"/users\", method: \"POST\")]
fun create(#[Body] input: CreateUser) -> CreateUser
  input
end

fun parse(text: string) -> Json
  Json.parse(text).unwrap()
end

for c in Reflect.callables() do
  let route = c.annotation(\"Route\")
  if route == none then
    continue
  end
  let path = route.get_string(\"path\")
  let method = route.get_string(\"method\")
  print(\"{c.name} {path} {method} -> {c.return_type}\")
  let args: array<Any> = []
  for p in c.params do
    if p.annotation(\"Path\") != none then
      args.push(p.parse(\"42\").unwrap())
    else
      if p.annotation(\"Body\") != none then
        args.push(p.decode(parse(\"\\{\\\"name\\\": \\\"Ann\\\"\\}\")).unwrap())
      else
        args.push(Any.of(Repo {}))
      end
    end
  end
  print(Json.of(c.call(args).unwrap()).to_text())
end
print(Reflect.callables()[0].params[0].parse(\"x\").unwrap_err().message)
print(Reflect.callables()[1].params[0].decode(parse(\"[]\")).unwrap_err().message)
");
    assert_eq!(
        out,
        "show /users/:id GET -> string\n\"user 42 from repo\"\n\
         create /users POST -> CreateUser\n{\"name\":\"Ann\",\"age\":18}\n\
         `x` is not a valid int\nexpected an object for CreateUser, found array\n"
    );
}
