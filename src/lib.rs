#![allow(clippy::cognitive_complexity)]
#![warn(absolute_paths_not_starting_with_crate)]
#![warn(explicit_outlives_requirements)]
#![warn(unreachable_pub)]
#![warn(deprecated_in_future)]
#![deny(unsafe_code)]
#![deny(unused_extern_crates)]

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use cranelift_module::Backend;
use cranelift_object::ObjectBackend;

pub type Product = <ObjectBackend as Backend>::Product;

use data::prelude::CompileError;
pub use data::prelude::*;
pub use lex::PreProcessor;
pub use parse::Parser;

#[macro_use]
pub mod utils;
pub mod arch;
pub mod data;
mod fold;
pub mod intern;
mod ir;
mod lex;
mod parse;

#[derive(Debug)]
pub enum Error {
    Source(VecDeque<CompileError>),
    Platform(String),
    IO(io::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IO(err)
    }
}

impl From<CompileError> for Error {
    fn from(err: CompileError) -> Error {
        Error::Source(vec_deque![err])
    }
}

impl From<VecDeque<CompileError>> for Error {
    fn from(errs: VecDeque<CompileError>) -> Self {
        Error::Source(errs)
    }
}

#[derive(Debug)]
pub struct Opt {
    /// The file where the C source came from
    pub filename: PathBuf,

    /// If set, print all tokens found by the lexer in addition to compiling.
    pub debug_lex: bool,

    /// If set, print the parsed abstract syntax tree in addition to compiling
    pub debug_ast: bool,

    /// If set, print the intermediate representation of the program in addition to compiling
    pub debug_asm: bool,

    /// If set, compile and assemble but do not link. Object file is machine-dependent.
    pub no_link: bool,

    /// The maximum number of errors to allow before giving up.
    /// If None, allows an unlimited number of errors.
    pub max_errors: Option<std::num::NonZeroUsize>,
}

impl Default for Opt {
    fn default() -> Self {
        Opt {
            filename: "<default>".into(),
            debug_lex: false,
            debug_ast: false,
            debug_asm: false,
            no_link: false,
            max_errors: None,
        }
    }
}

/// Compile and return the declarations and warnings.
pub fn compile(buf: &str, opt: &Opt) -> (Result<Product, Error>, VecDeque<CompileWarning>) {
    let filename = opt.filename.to_string_lossy();
    let filename_ref = InternedStr::get_or_intern(filename.as_ref());
    let mut cpp = PreProcessor::new(filename, buf.chars(), opt.debug_lex);
    let (first, mut errs) = cpp.first_token();
    let eof = || Location {
        span: (buf.len() as u32..buf.len() as u32).into(),
        filename: filename_ref,
    };

    let first = match first {
        Some(token) => token,
        None => {
            if errs.is_empty() {
                errs.push_back(eof().error(SemanticError::EmptyProgram));
            }
            return (Err(Error::Source(errs)), cpp.warnings());
        }
    };

    let mut parser = Parser::new(first, &mut cpp, opt.debug_ast);
    let (hir, parse_errors) = parser.collect_results();
    errs.extend(parse_errors.into_iter());
    if hir.is_empty() && errs.is_empty() {
        errs.push_back(eof().error(SemanticError::EmptyProgram));
    }

    let mut warnings = parser.warnings();
    warnings.extend(cpp.warnings());
    if !errs.is_empty() {
        return (Err(Error::Source(errs)), warnings);
    }
    let (result, ir_warnings) = ir::compile(hir, opt.debug_asm);
    warnings.extend(ir_warnings);
    (result.map_err(Error::from), warnings)
}

pub fn assemble(product: Product, output: &Path) -> Result<(), Error> {
    let bytes = product.emit().map_err(Error::Platform)?;
    File::create(output)?
        .write_all(&bytes)
        .map_err(io::Error::into)
}

pub fn link(obj_file: &Path, output: &Path) -> Result<(), io::Error> {
    use std::io::{Error, ErrorKind};
    // link the .o file using host linker
    let status = Command::new("cc")
        .args(&[&obj_file, Path::new("-o"), output])
        .status()
        .map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                Error::new(
                    ErrorKind::NotFound,
                    "could not find host cc (for linking). Is it on your PATH?",
                )
            } else {
                err
            }
        })?;
    if !status.success() {
        Err(Error::new(ErrorKind::Other, "linking program failed"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compile(src: &str) -> Result<Product, Error> {
        let options = Opt {
            filename: "<test-suite>".into(),
            ..Default::default()
        };
        super::compile(src, &options).0
    }
    fn compile_err(src: &str) -> VecDeque<CompileError> {
        match compile(src).err().unwrap() {
            Error::Source(errs) => errs,
            _ => unreachable!(),
        }
    }
    #[test]
    fn empty() {
        let mut lex_errs = compile_err("`");
        assert!(lex_errs.pop_front().unwrap().data.is_lex_err());
        assert!(lex_errs.is_empty());

        let mut empty_errs = compile_err("");
        let err = empty_errs.pop_front().unwrap().data;
        assert_eq!(err, SemanticError::EmptyProgram.into());
        assert!(err.is_semantic_err());
        assert!(empty_errs.is_empty());

        let mut parse_err = compile_err("+++");
        let err = parse_err.pop_front();
        assert!(parse_err.is_empty());
        assert!(err.unwrap().data.is_syntax_err());
    }
}
