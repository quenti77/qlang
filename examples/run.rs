use std::env;
use std::fs;

use qlang::lexer::Lexer;

fn main() -> () {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Merci d'indiquer le fichier à exécuter");
        return ();
    }

    let file_path = &args[1];
    let file_content = fs::read_to_string(file_path)
        .expect(&format!("Le fichier {} doit être accessible en lecture", file_path));

    let lex = Lexer::new();
    lex.tokenize(&file_content);
}
