use qlang_core::{diag::Diagnostic, parser::parse_module};

#[test]
fn parses_syntax_tour() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/syntax.q")).unwrap();
    let mut diags: Vec<Diagnostic> = Vec::new();
    let m = parse_module(&src, 0, &mut diags);
    for d in &diags {
        eprintln!("{} {} {:?}", d.code, d.message, d.span);
    }
    assert!(diags.is_empty(), "{} diagnostics", diags.len());
    eprintln!("{} items", m.items.len());
}
