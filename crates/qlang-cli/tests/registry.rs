//! Version constraints, yanked versions, signatures, `verify`, `publish` and `info`, with git repositories
//! made in a temporary folder (no network).

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-registry-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    git_with(dir, args, "/dev/null");
}

/// `global` is the git configuration to use (the one of the machine would sign with its own tools).
fn git_with(dir: &Path, args: &[&str], global: &str) {
    let o = Command::new("git")
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// A git repository with a package `name`: one commit and one tag per version, the file `lib.q` saying
/// which version it is.
fn package(root: &Path, name: &str, versions: &[&str], deps: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    for v in versions {
        std::fs::write(
            dir.join("lib.q"),
            format!("export version\nfun version() -> string\n  \"{v}\"\nend\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join("qlang.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"{}\"\nmain = \"lib.q\"\n{deps}",
                v.trim_start_matches('v')
            ),
        )
        .unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", v]);
        git(&dir, &["tag", v]);
    }
    dir
}

fn project(root: &Path, deps: &str) -> PathBuf {
    let dir = root.join("app");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("qlang.toml"),
        format!("[package]\nname = \"app\"\n\n[dependencies]\n{deps}"),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.q"),
        "import version from \"@lib\"\nprint(version())\n",
    )
    .unwrap();
    dir
}

fn qlang(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (String, String, i32) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qlang"));
    c.args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0");
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

fn url(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

fn running(app: &Path) -> String {
    qlang(app, &["run", "main.q"], &[]).0.trim().to_string()
}

#[test]
fn a_version_constraint_picks_the_highest_tag_in_range_and_update_moves_it() {
    let root = temp("range");
    let lib = package(
        &root,
        "lib",
        &["v1.2.0", "v1.4.0", "v1.5.0-rc.1", "v2.0.0"],
        "",
    );
    let app = project(
        &root,
        &format!("lib = {{ git = \"{}\", version = \"^1.2\" }}\n", url(&lib)),
    );
    let (out, err, code) = qlang(&app, &["install"], &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("lib version:^1.2 -> v1.4.0"), "{out}");
    assert_eq!(running(&app), "v1.4.0");
    let lock = std::fs::read_to_string(app.join("qlang.lock")).unwrap();
    assert!(lock.contains("ref = \"version:^1.2\""), "{lock}");

    // a new release: `install` keeps the locked commit, `update` moves to it
    std::fs::write(
        lib.join("lib.q"),
        "export version\nfun version() -> string\n  \"v1.6.0\"\nend\n",
    )
    .unwrap();
    git(&lib, &["commit", "-qam", "v1.6.0"]);
    git(&lib, &["tag", "v1.6.0"]);
    qlang(&app, &["install"], &[]);
    assert_eq!(running(&app), "v1.4.0");
    let (out, _, code) = qlang(&app, &["update"], &[]);
    assert_eq!(code, 0);
    assert!(out.contains("-> v1.6.0"), "{out}");
    assert_eq!(running(&app), "v1.6.0");
}

#[test]
fn a_range_with_no_tag_says_so() {
    let root = temp("none");
    let lib = package(&root, "lib", &["v1.0.0"], "");
    let app = project(
        &root,
        &format!("lib = {{ git = \"{}\", version = \"^3\" }}\n", url(&lib)),
    );
    let (_, err, code) = qlang(&app, &["install"], &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no version tag satisfying `^3`"), "{err}");
}

#[test]
fn a_yanked_version_is_not_chosen_by_a_new_resolution() {
    let root = temp("yanked");
    let lib = package(&root, "lib", &["v1.2.0", "v1.3.0", "v1.4.0"], "");
    let index = root.join("index.json");
    std::fs::write(
        &index,
        format!(
            "{{\"lib\": {{\"git\": \"{}\", \"description\": \"d\", \"license\": \"MIT\", \"yanked\": [\"v1.4.0\"]}}}}",
            url(&lib)
        ),
    )
    .unwrap();
    let app = project(
        &root,
        &format!("lib = {{ git = \"{}\", version = \"^1\" }}\n", url(&lib)),
    );
    let (out, err, code) = qlang(
        &app,
        &["install"],
        &[("QLANG_INDEX_URL", index.to_str().unwrap())],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("-> v1.3.0"), "{out}");
}

#[test]
fn two_packages_asking_for_ranges_get_one_tag_that_suits_both_or_a_clear_error() {
    let root = temp("conflict");
    let lib = package(&root, "lib", &["v1.2.0", "v1.4.0", "v1.6.0"], "");
    let other_deps = format!(
        "\n[dependencies]\nlib = {{ git = \"{}\", version = \"<1.5\" }}\n",
        url(&lib)
    );
    let other = package(&root, "other", &["v0.1.0"], &other_deps);
    let app = project(
        &root,
        &format!(
            "lib = {{ git = \"{}\", version = \"^1.2\" }}\nother = {{ git = \"{}\", tag = \"v0.1.0\" }}\n",
            url(&lib),
            url(&other)
        ),
    );
    let (out, err, code) = qlang(&app, &["install"], &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(running(&app), "v1.4.0", "{out}");

    // an impossible pair
    let root = temp("clash");
    let lib = package(&root, "lib", &["v1.2.0", "v2.1.0"], "");
    let other_deps = format!(
        "\n[dependencies]\nlib = {{ git = \"{}\", version = \"^2\" }}\n",
        url(&lib)
    );
    let other = package(&root, "other", &["v0.1.0"], &other_deps);
    let app = project(
        &root,
        &format!(
            "lib = {{ git = \"{}\", version = \"^1\" }}\nother = {{ git = \"{}\", tag = \"v0.1.0\" }}\n",
            url(&lib),
            url(&other)
        ),
    );
    let (_, err, code) = qlang(&app, &["install"], &[]);
    assert_eq!(code, 1);
    assert!(
        err.contains("no version suits both") || err.contains("no version tag satisfying"),
        "{err}"
    );
}

#[test]
fn verify_catches_a_package_changed_after_the_install() {
    let root = temp("verify");
    let lib = package(&root, "lib", &["v1.0.0"], "");
    let app = project(
        &root,
        &format!("lib = {{ git = \"{}\", tag = \"v1.0.0\" }}\n", url(&lib)),
    );
    assert_eq!(qlang(&app, &["install"], &[]).2, 0);
    let (out, _, code) = qlang(&app, &["verify"], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok   lib"), "{out}");
    let file = app.join(".qlang/packages/lib/lib.q");
    std::fs::write(
        &file,
        "export version\nfun version() -> string\n  \"tampered\"\nend\n",
    )
    .unwrap();
    let (out, _, code) = qlang(&app, &["verify"], &[]);
    assert_eq!(code, 1);
    assert!(out.contains("FAIL lib: files were changed"), "{out}");
}

#[test]
fn a_token_is_only_for_https_and_must_be_set() {
    let root = temp("token");
    let lib = package(&root, "lib", &["v1.0.0"], "");
    let app = project(
        &root,
        &format!(
            "lib = {{ git = \"{}\", tag = \"v1.0.0\", token_env = \"LIB_TOKEN\" }}\n",
            url(&lib)
        ),
    );
    let (_, err, code) = qlang(&app, &["install"], &[("LIB_TOKEN", "abc")]);
    assert_eq!(code, 1);
    assert!(err.contains("only for https repositories"), "{err}");
    let app = project(
        &root,
        "lib = { git = \"https://example.invalid/x/lib\", tag = \"v1.0.0\", token_env = \"MISSING_TOKEN_VAR\" }\n",
    );
    let (_, err, code) = qlang(&app, &["install"], &[]);
    assert_eq!(code, 1);
    assert!(
        err.contains("MISSING_TOKEN_VAR") && err.contains("is not set"),
        "{err}"
    );
}

#[cfg(unix)]
#[test]
fn signed_true_needs_a_signature_git_can_verify() {
    let root = temp("signed");
    // an ssh key, and the file of keys git may trust
    let key = root.join("key");
    let k = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .status();
    if !k.is_ok_and(|s| s.success()) {
        eprintln!("no ssh-keygen: skipped");
        return;
    }
    let public = std::fs::read_to_string(root.join("key.pub")).unwrap();
    std::fs::write(root.join("allowed"), format!("t@example.com {public}")).unwrap();
    let config = root.join("gitconfig");
    std::fs::write(
        &config,
        format!(
            "[gpg]\n\tformat = ssh\n[gpg \"ssh\"]\n\tallowedSignersFile = {}\n",
            root.join("allowed").display()
        ),
    )
    .unwrap();
    let env = [("GIT_CONFIG_GLOBAL", config.to_str().unwrap())];

    // a repository whose only commit is signed
    let signed = root.join("signed");
    std::fs::create_dir_all(&signed).unwrap();
    let cfg = config.to_str().unwrap();
    git_with(&signed, &["init", "-q", "-b", "main"], cfg);
    git_with(
        &signed,
        &["config", "user.signingkey", key.to_str().unwrap()],
        cfg,
    );
    std::fs::write(
        signed.join("lib.q"),
        "export version\nfun version() -> string\n  \"signed\"\nend\n",
    )
    .unwrap();
    git_with(&signed, &["add", "-A"], cfg);
    git_with(&signed, &["commit", "-q", "-S", "-m", "one"], cfg);
    git_with(&signed, &["tag", "v1.0.0"], cfg);
    let unsigned = package(&root, "unsigned", &["v1.0.0"], "");

    let app = project(
        &root,
        &format!(
            "lib = {{ git = \"{}\", tag = \"v1.0.0\", signed = true }}\n",
            url(&signed)
        ),
    );
    let (out, err, code) = qlang(&app, &["install"], &env);
    assert_eq!(code, 0, "{out}{err}");

    let app = project(
        &root,
        &format!(
            "lib = {{ git = \"{}\", tag = \"v1.0.0\", signed = true }}\n",
            url(&unsigned)
        ),
    );
    let (_, err, code) = qlang(&app, &["install"], &env);
    assert_eq!(code, 1);
    assert!(err.contains("no signature git can verify"), "{err}");
}

#[test]
fn publish_lists_what_is_missing_and_prints_the_entry_when_ready() {
    let root = temp("publish");
    let dir = root.join("pkg");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("lib.q"), "export f\nfun f() -> int\n  1\nend\n").unwrap();
    std::fs::write(
        dir.join("qlang.toml"),
        "[package]\nname = \"pkg\"\nversion = \"1.0.0\"\nmain = \"lib.q\"\n",
    )
    .unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "x"]);
    let (out, err, code) = qlang(&dir, &["publish"], &[]);
    assert_eq!(code, 1, "{out}{err}");
    for missing in [
        "no `description`",
        "no `license`",
        "no README.md",
        "no tag v1.0.0",
        "no `origin` remote",
    ] {
        assert!(
            out.contains(missing) || out.contains(&missing.replace("no tag", "HEAD has no tag")),
            "{missing}: {out}"
        );
    }

    std::fs::write(
        dir.join("qlang.toml"),
        "[package]\nname = \"pkg\"\nversion = \"1.0.0\"\nmain = \"lib.q\"\ndescription = \"A package\"\nlicense = \"MIT\"\nkeywords = [\"demo\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("README.md"), "# pkg\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "ready"]);
    git(&dir, &["tag", "v1.0.0"]);
    git(
        &dir,
        &["remote", "add", "origin", "https://example.com/me/pkg.git"],
    );
    let (out, err, code) = qlang(&dir, &["publish"], &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("\"pkg\": {")
            && out.contains("\"git\": \"https://example.com/me/pkg\"")
            && out.contains("\"license\": \"MIT\""),
        "{out}"
    );
}

#[test]
fn info_shows_versions_and_a_removed_package_is_hidden_and_refused() {
    let root = temp("info");
    let lib = package(&root, "lib", &["v1.0.0", "v1.1.0"], "");
    let index = root.join("index.json");
    std::fs::write(
        &index,
        format!(
            "{{\"lib\": {{\"git\": \"{}\", \"description\": \"A lib\", \"license\": \"MIT\", \"keywords\": [\"x\"], \"yanked\": [\"v1.0.0\"]}}, \"gone\": {{\"git\": \"{}\", \"description\": \"old\", \"removed\": \"abandoned\"}}}}",
            url(&lib),
            url(&lib)
        ),
    )
    .unwrap();
    let env = [("QLANG_INDEX_URL", index.to_str().unwrap())];
    let (out, _, code) = qlang(&root, &["info", "lib"], &env);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("license:    MIT") && out.contains("v1.1.0, v1.0.0 (yanked)"),
        "{out}"
    );
    assert!(out.contains("--version \"^1.1.0\""), "{out}");
    let (out, _, _) = qlang(&root, &["search"], &env);
    assert!(
        out.contains("lib") && out.contains("[MIT]") && !out.contains("gone"),
        "{out}"
    );
    let (_, err, code) = qlang(&root, &["add", "gone"], &env);
    assert_eq!(code, 1);
    assert!(
        err.contains("was removed from the package index: abandoned"),
        "{err}"
    );
}
