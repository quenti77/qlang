//! The JSON protocol on standard input and output (`qlang run --json`, `qlang check --json`).

pub use qlang_core::protocol::{Ceiling, handle};
use qlang_core::protocol::handle_with;
use std::io::Read;
use std::process::ExitCode;

pub fn run_stdin(force_check: bool) -> ExitCode {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("cannot read the request: {e}");
        return ExitCode::from(66);
    }
    println!("{}", handle_with(&input, &Ceiling::default(), force_check));
    ExitCode::SUCCESS
}
