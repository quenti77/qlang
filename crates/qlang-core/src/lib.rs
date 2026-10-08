//! qlang core: lexer, parser, type checker and interpreter.
//!
//! This crate never prints, reads input or touches the disk. Everything that
//! reaches outside goes through a [`host::Host`].

pub mod ast;
pub mod check;
pub mod diag;
pub mod host;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod types;
pub mod value;
