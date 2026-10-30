//! `qlang init [dir]`: the files of a new project; `qlang init --web [dir]`: a web application
//! ready to run, over the `std` package.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAIN: &str = "print(\"Hello, qlang!\")\n";
const GITIGNORE: &str = ".qlang/\n";

const WEB_MAIN: &str = r#"-- Serve it:  qlang serve main.q --watch --dev
-- Test it:   qlang test
import "@std/web/middleware.q" as mw
import "app.q" as site

let app = site.build()
app.use(mw.logging())

fun handle(req: Request) -> Response
  app.handle(req)
end
"#;

const WEB_APP: &str = r#"-- Puts the application together. `main.q` serves it, the tests send requests to it.
--
-- `App.boot` reads everything these modules declare: the functions of providers.q build the
-- services (what a handler asks for by type), the functions of controllers.q with a route
-- annotation are the routes, and so is each struct marked `#[web.Controller]`. A struct marked
-- `#[di.Service]` is built by the container from its fields. Nothing to register by hand.
import "@std/web/app.q" as web
import "providers.q" as providers
import "controllers.q" as controllers

fun build() -> web.App
  web.App.boot(Reflect.declared(providers).concat(Reflect.declared(controllers)))
end

export build
"#;

const WEB_PROVIDERS: &str = r#"-- Services: each function gives the service of its return type, built once (`#[di.Scoped]`:
-- once per request, `#[di.Transient]`: each time). A handler or another provider asks for one by
-- adding a parameter of that type.
struct Config
  public greeting: string
end

fun make_config() -> Config
  Config { greeting: env.get("GREETING") ?? "Hello" }
end

export Config
"#;

const WEB_CONTROLLERS: &str = r#"-- The routes: a function with a route annotation. Its parameters are filled from the request
-- (`#[web.Path]`, `#[web.Query]`, `#[web.Body]`, `#[web.Header]`) or from the services.
-- What it returns becomes the response: text, a value (as JSON), a `Response`.
import "@std/web/app.q" as web
import Config from "providers.q"

#[web.Get("/")]
fun home(config: Config) -> string
  "{config.greeting}, qlang!"
end

#[web.Get("/hello/:name")]
fun hello(#[web.Path] name: string, config: Config) -> string
  "{config.greeting}, {name}!"
end
"#;

const WEB_TEST: &str = r#"-- qlang test
import "app.q" as site
import "@std/web/testing.q" as testing

fun test_the_home_page_answers()
  let client = testing.Client.new(site.build())
  client.get("/").expect_status(200).expect_body_contains("qlang")
end

fun test_a_name_in_the_path()
  let client = testing.Client.new(site.build())
  client.get("/hello/Ada").expect_body_contains("Ada")
end

fun test_an_unknown_route_is_a_404()
  testing.Client.new(site.build()).get("/nothing").expect_status(404)
end
"#;

/// A package name made of letters, digits, `_` and `-` from a directory name.
fn package_name(dir: &Path) -> String {
    let raw = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() {
        "app".to_string()
    } else {
        name
    }
}

pub fn init(args: &[&str]) -> ExitCode {
    let mut web = false;
    let mut install = true;
    let mut dirs: Vec<&str> = Vec::new();
    let mut unknown = false;
    for a in args {
        match *a {
            "--web" => web = true,
            "--no-install" => install = false,
            other if other.starts_with('-') => unknown = true,
            other => dirs.push(other),
        }
    }
    let dir = match dirs.as_slice() {
        _ if unknown => {
            eprintln!("usage: qlang init [--web] [--no-install] [dir]");
            return ExitCode::from(64);
        }
        [] => PathBuf::from("."),
        [dir] => PathBuf::from(dir),
        _ => {
            eprintln!("usage: qlang init [--web] [--no-install] [dir]");
            return ExitCode::from(64);
        }
    };
    match create(&dir, web) {
        Ok(created) => {
            for f in created {
                println!("created {f}");
            }
            if !web {
                println!("run it with: qlang run");
                return ExitCode::SUCCESS;
            }
            if install && !add_std(&dir) {
                println!("the `std` package was not installed: run `qlang add std` in the project");
            }
            println!("serve it with: qlang serve main.q --watch --dev   (tests: qlang test)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

/// `qlang add std` in the new project (it needs the package index, so the network).
fn add_std(dir: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .args(["add", "std"])
        .current_dir(dir)
        .status()
        .is_ok_and(|s| s.success())
}

/// Writes the project files into `dir` (made if missing) and returns what it created. An
/// existing `qlang.toml` means there already is a project: nothing is touched.
fn create(dir: &Path, web: bool) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let manifest = dir.join("qlang.toml");
    if manifest.exists() {
        return Err(format!(
            "{} already exists: this is already a project",
            manifest.display()
        ));
    }
    let name = package_name(&dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
    let mut created = Vec::new();
    let mut write = |file: &str, text: &str| -> Result<(), String> {
        let path = dir.join(file);
        if path.exists() {
            return Ok(()); // never overwrite what the user already has
        }
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        created.push(path.display().to_string());
        Ok(())
    };
    write(
        "qlang.toml",
        &format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"main.q\"\nqlang = \"{}.{}\"\n",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR")
        ),
    )?;
    if web {
        write("main.q", WEB_MAIN)?;
        write("app.q", WEB_APP)?;
        write("providers.q", WEB_PROVIDERS)?;
        write("controllers.q", WEB_CONTROLLERS)?;
        write("app_test.q", WEB_TEST)?;
    } else {
        write("main.q", MAIN)?;
    }
    write(".gitignore", GITIGNORE)?;
    Ok(created)
}
