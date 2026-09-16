//! Rust port of the qlang language core: lexer, parser, AST, environment
//! and tree-walking interpreter.
//!
//! This crate only covers the *language* itself. Anything host-specific
//! (a REPL, a GUI, file-watching, ...) is expected to live in a separate
//! crate/binary built on top of this one, wiring in its own
//! `module::InputSource` / `module::ModuleResolver` implementations.

pub mod ast;
pub mod callable;
pub mod environment;
pub mod error;
pub mod globals;
pub mod interpreter;
pub mod lexer;
pub mod module;
pub mod objects;
pub mod parser;
pub mod position;
pub mod stdio;
pub mod token;
pub mod values;

pub use ast::Program;
pub use environment::Environment;
pub use error::QError;
pub use interpreter::Interpreter;
pub use lexer::Lexer;
pub use module::{FsModuleResolver, InputSource, MapModuleResolver, ModuleResolver, NoopInput};
pub use parser::Parser;
pub use stdio::Std;
pub use values::Value;
