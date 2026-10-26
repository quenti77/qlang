//! `Reflect.constructors(module)`: the structures of a module, each one a `Callable` that builds it.

use std::process::Command;

#[test]
fn a_structure_is_built_from_its_fields_private_ones_too() {
    let dir = std::env::temp_dir().join(format!("qlang-ctors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("models.q"),
        "#[annotation]\nstruct Service\nend\nstruct Db\n  public url: string\nend\n#[Service]\nstruct Users\n  db: Db\n  tag: string?\nend\nexport Service, Db, Users\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.q"),
        "import \"models.q\" as models\nlet db = models.Db { url: \"x\" }\nfor c in Reflect.constructors(models) do\n  let marked = c.annotation(\"Service\") != none\n  print(\"{c.name} {c.params.len()} {marked}\")\n  if c.name == \"Users\" then\n    print(c.call([Any.of(db), Any.of(none as string?)]).is_ok())\n    print(c.call([Any.of(db)]).is_err())\n    print(c.call([Any.of(1), Any.of(none as string?)]).is_err())\n  end\nend\n",
    )
    .unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(dir.join("main.q"))
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        out,
        "Db 1 false\nUsers 2 true\ntrue\ntrue\ntrue\n",
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}
