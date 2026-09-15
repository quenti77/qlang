//! Minimal host for the qlang language core: asks for a `.q` file (as a CLI
//! argument, or interactively if none is given), runs it, and prints its
//! output. Module paths used by `inclure` are resolved relative to the
//! entry file's directory.

use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use qlang_rs::globals::make_global_env;
use qlang_rs::{FsModuleResolver, InputSource, Interpreter, Lexer, Parser, QError, Std};

struct StdinInput;

impl InputSource for StdinInput {
    fn read(&mut self, message: &str) -> Option<String> {
        print!("{message} ");
        io::stdout().flush().ok();

        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
            Err(_) => None,
        }
    }
}

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => match ask_for_path() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("Impossible de lire le chemin du fichier : {err}");
                return ExitCode::FAILURE;
            }
        },
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Impossible de lire '{}' : {err}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut lexer = Lexer::new();
    if let Err(err) = lexer.tokenize(&source) {
        print_error(&err);
        return ExitCode::FAILURE;
    }

    let mut parser = Parser::new();
    parser.set_tokens(lexer.tokens().to_vec(), source);
    let program = match parser.make_ast() {
        Ok(program) => program,
        Err(err) => {
            print_error(&err);
            return ExitCode::FAILURE;
        }
    };

    let mut interpreter = Interpreter::new(
        make_global_env(),
        Std::new(),
        Std::new(),
        Box::new(StdinInput),
        Box::new(FsModuleResolver::new(base_dir)),
    );

    let result = interpreter.evaluate(&program);
    for line in interpreter.stdout().log() {
        println!("{line}");
    }

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            print_error(&err);
            ExitCode::FAILURE
        }
    }
}

fn ask_for_path() -> io::Result<PathBuf> {
    print!("Fichier qlang à exécuter : ");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(PathBuf::from(line.trim()))
}

fn print_error(err: &QError) {
    for line in err.render() {
        eprintln!("{line}");
    }
}
