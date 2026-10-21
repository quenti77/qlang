//! qlang core: lexer, parser, type checker and interpreter.
//!
//! This crate never prints, reads input or touches the disk. Everything that
//! reaches outside goes through a [`host::Host`].

pub mod ast;
pub mod check;
pub mod codec;
pub mod datetime;
pub mod diag;
pub mod fx;
pub mod host;
pub mod ids;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod protocol;
pub mod smallmap;
pub mod span;
pub mod types;
pub mod value;
