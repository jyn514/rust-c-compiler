use std::collections::VecDeque;
use thiserror::Error;

use super::{
    lex::{Keyword, Token},
    prelude::{Location, LocationTrait},
    Expr, Locatable,
    ast,
};
use crate::intern::InternedStr;

/// RecoverableResult is a type that represents a Result that can be recovered from.
///
/// See the [`Recover`] trait for more information.
///
/// [`Recover`]: trait.Recover.html
pub type RecoverableResult<T, E = CompileError> = Result<T, (E, T)>;
pub type CompileResult<T, L = Location> = Result<T, CompileError<L>>;
pub type CompileError<L = Location> = Locatable<Error, L>;
pub type CompileWarning<L = Location> = Locatable<Warning, L>;

/// ErrorHandler is a struct that hold errors generated by the compiler
///
/// An error handler is used because multiple errors may be generated by each
/// part of the compiler, this cannot be represented well with Rust's normal
/// `Result`.
#[derive(Debug)]
pub(crate) struct ErrorHandler<L: LocationTrait = Location> {
    errors: VecDeque<CompileError<L>>,
    pub(crate) warnings: VecDeque<CompileWarning<L>>,
}

impl<L: LocationTrait> Default for ErrorHandler<L> {
    fn default() -> Self {
        Self {
            errors: VecDeque::default(),
            warnings: VecDeque::default(),
        }
    }
}

impl ErrorHandler {
    /// Construct a new error handler.
    pub(crate) fn new() -> ErrorHandler {
        Default::default()
    }

    /// Add an error to the error handler.
    pub(crate) fn push_back<E: Into<CompileError>>(&mut self, error: E) {
        self.errors.push_back(error.into());
    }

    /// Remove the first error from the queue
    pub(crate) fn pop_front(&mut self) -> Option<CompileError> {
        self.errors.pop_front()
    }

    /// Shortcut for adding a warning
    pub(crate) fn warn<W: Into<Warning>>(&mut self, warning: W, location: Location) {
        self.warnings.push_back(location.with(warning.into()));
    }

    /// Shortcut for adding an error
    pub(crate) fn error<E: Into<Error>>(&mut self, error: E, location: Location) {
        self.errors.push_back(location.with(error.into()));
    }

    /// Add an iterator of errors to the error queue
    pub(crate) fn extend<E: Into<CompileError>>(&mut self, iter: impl Iterator<Item = E>) {
        self.errors.extend(iter.map(Into::into));
    }

    /// Move another `ErrorHandler`'s errors and warnings into this one.
    pub(crate) fn append(&mut self, other: &mut Self) {
        self.errors.append(&mut other.errors);
        self.warnings.append(&mut other.warnings);
    }
}

impl Iterator for ErrorHandler {
    type Item = CompileError;

    fn next(&mut self) -> Option<CompileError> {
        self.pop_front()
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    #[error("invalid program: {0}")]
    Semantic(#[from] SemanticError),

    #[error("invalid syntax: {0}")]
    Syntax(#[from] SyntaxError),

    #[error("invalid macro: {0}")]
    PreProcessor(#[from] CppError),

    #[error("invalid token: {0}")]
    Lex(#[from] LexError),
}

/// Semantic errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SemanticError {
    #[error("{0}")]
    Generic(String),

    /*
    #[error("cannot have empty program")]
    EmptyProgram,

    #[error("use of undeclared identifier '{0}'")]
    UndeclaredVar(InternedStr),

    #[error("`{0}` is only allowed on function declarations")]
    InvalidFuncQualifiers(super::FunctionQualifiers),

    #[error("{} overflow in expresson", if *(.is_positive) { "positive" } else { "negative" })]
    ConstOverflow { is_positive: bool },

    // String is the reason it couldn't be assigned
    #[error("cannot assign to {0}")]
    NotAssignable(String),

    #[error("cannot take address of {0}")]
    InvalidAddressOf(&'static str),

    #[error("cannot divide by zero")]
    DivideByZero,

    #[error("cannot shift {} by a negative amount", if *(.is_left) { "left" } else { "right" })]
    NegativeShift { is_left: bool },

    #[error("unreachable statement")]
    UnreachableStatement,

    #[error("use of undeclared label {0}")]
    UndeclaredLabel(InternedStr),

    #[error("{}case outside of switch statement", if *(.is_default) { "default " } else { "" })]
    CaseOutsideSwitch { is_default: bool },

    #[error("cannot have multiple {}cases in a switch statement",
            if *(.is_default) { "default " } else { "" } )]
    DuplicateCase { is_default: bool },

    #[error("void must be the first and only parameter if specified")]
    InvalidVoidParameter,

    #[error("expected expression, got typedef")]
    TypedefInExpressionContext,

    #[error("overflow in enumeration constant")]
    EnumOverflow,

    #[error("initializers cannot be empty")]
    EmptyInitializer,
    */
    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

/// Syntax errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SyntaxError {
    #[error("{0}")]
    Generic(String),

    #[error("expected {0}, got <end-of-file>")]
    EndOfFile(&'static str),

    #[error("expected statement, got {0}")]
    NotAStatement(super::Keyword),

    // expected a primary expression, but got EOF or an invalid token
    #[error("expected variable, literal, or '('")]
    MissingPrimary,

    #[error("expected identifier, got '{}'",
        .0.as_ref().map_or("<end-of-file>".into(),
                           |t| std::borrow::Cow::Owned(t.to_string())))]
    ExpectedId(Option<Token>),

    #[error("expected declaration specifier, got keyword '{0}'")]
    ExpectedDeclSpecifier(Keyword),

    #[error("expected declarator in declaration")]
    ExpectedDeclarator,

    #[error("empty type name")]
    ExpectedType,

    #[error("expected '(', '*', or variable, got '{0}'")]
    ExpectedDeclaratorStart(Token),

    #[error("only functions can have a function body (got {0})")]
    NotAFunction(ast::InitDeclarator),

    #[error("functions cannot be initialized (got {0})")]
    FunctionInitializer(ast::Initializer),

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

/// Preprocessing errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq)]
pub enum CppError {
    /// A user-defined error (`#error`) was present.
    /// The `Vec<Token>` contains the tokens which followed the error.

    // TODO: this allocates a string for each token,
    // might be worth separating out into a function at some point
    #[error("#error {}", (.0).iter().map(|t| t.to_string()).collect::<Vec<_>>().join(" "))]
    User(Vec<Token>),

    /// An invalid directive was present, such as `#invalid`
    #[error("invalid preprocessing directive")]
    InvalidDirective,

    /// A valid token was present in an invalid position, such as `#if *`
    ///
    /// The `&str` describes the expected token;
    /// the `Token` is the actual token found.
    #[error("expected {0}, got {1}")]
    UnexpectedToken(&'static str, Token),

    /// The file ended unexpectedly.
    ///
    /// This error is separate from an unterminated `#if`:
    /// it occurs if the file ends in the middle of a directive,
    /// such as `#define`.
    ///
    /// The `&str` describes what token was expected.
    #[error("expected {0}, got <end-of-file>")]
    EndOfFile(&'static str),

    #[error("file '{0}' not found")]
    FileNotFound(String),

    #[error("wrong number of arguments: expected {0}, got {1}")]
    TooFewArguments(usize, usize),

    #[error("IO error: {0}")]
    // TODO: find a way to put io::Error in here (doesn't derive Clone or PartialEq)
    IO(String),

    /// The file ended before an `#if`, `#ifdef`, or `#ifndef` was closed.
    #[error("#if is never terminated")]
    UnterminatedIf,

    /// An `#if` occurred without an expression following.
    #[error("expected expression for #if")]
    EmptyExpression,

    /// A `#define` occured without an identifier following.
    #[error("macro name missing")]
    EmptyDefine,

    /// An `#include<>` or `#include""` was present.
    #[error("empty filename")]
    EmptyInclude,

    /// A `#endif` was present, but no `#if` was currently open
    #[error("#endif without #if")]
    UnexpectedEndIf,

    /// An `#else` was present, but either
    /// a) no `#if` was currently open, or
    /// b) an `#else` has already been seen.
    #[error("#else after #else or #else without #if")]
    UnexpectedElse,

    /// An `#elif` was present, but either
    /// a) no `#if` was currently open, or
    /// b) an `#else` has already been seen.
    #[error("{}", if *early { "#elif without #if" } else { "#elif after #else " })]
    UnexpectedElif { early: bool },

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

/// Lex errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LexError {
    #[error("{0}")]
    Generic(String),

    #[error("unterminated /* comment")]
    UnterminatedComment,

    #[error("no newline at end of file")]
    NoNewlineAtEOF,

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

#[derive(Debug, Error, PartialEq)]
/// errors are non-exhaustive and may have new variants added at any time
pub enum Warning {
    // for compatibility
    #[error("{0}")]
    Generic(String),

    /// A #warning directive was present, followed by the tokens in this variant.
    // TODO: this allocates a string for each token,
    // might be worth separating out into a function at some point
    #[error("#warning {}", (.0).iter().map(|t| t.to_string()).collect::<Vec<_>>().join(" "))]
    User(Vec<Token>),

    #[error("rcc does not support #pragma")]
    IgnoredPragma,

    #[error("variadic macros are not yet supported")]
    IgnoredVariadic,

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

impl<T: Into<String>> From<T> for Warning {
    fn from(msg: T) -> Warning {
        Warning::Generic(msg.into())
    }
}

impl CompileError {
    pub(crate) fn semantic(err: Locatable<String>) -> Self {
        Self::from(err)
    }
    pub fn location(&self) -> Location {
        self.location
    }
    pub fn is_lex_err(&self) -> bool {
        self.data.is_lex_err()
    }
    pub fn is_syntax_err(&self) -> bool {
        self.data.is_syntax_err()
    }
    pub fn is_semantic_err(&self) -> bool {
        self.data.is_semantic_err()
    }
}

impl Error {
    pub fn is_lex_err(&self) -> bool {
        if let Error::Lex(_) = self {
            true
        } else {
            false
        }
    }
    pub fn is_syntax_err(&self) -> bool {
        if let Error::Syntax(_) = self {
            true
        } else {
            false
        }
    }
    pub fn is_semantic_err(&self) -> bool {
        if let Error::Semantic(_) = self {
            true
        } else {
            false
        }
    }
}

impl From<Locatable<String>> for CompileError {
    fn from(err: Locatable<String>) -> Self {
        err.map(|s| SemanticError::Generic(s).into())
    }
}

impl From<Locatable<SemanticError>> for CompileError {
    fn from(err: Locatable<SemanticError>) -> Self {
        err.map(Error::Semantic)
    }
}

impl From<Locatable<SyntaxError>> for CompileError {
    fn from(err: Locatable<SyntaxError>) -> Self {
        err.map(Error::Syntax)
    }
}

impl From<Locatable<CppError>> for CompileError {
    fn from(err: Locatable<CppError>) -> Self {
        err.map(Error::PreProcessor)
    }
}

impl From<Locatable<String>> for Locatable<SemanticError> {
    fn from(err: Locatable<String>) -> Self {
        err.map(SemanticError::Generic)
    }
}

impl<S: Into<String>> From<S> for SemanticError {
    fn from(err: S) -> Self {
        SemanticError::Generic(err.into())
    }
}

impl<S: Into<String>> From<S> for SyntaxError {
    fn from(err: S) -> Self {
        SyntaxError::Generic(err.into())
    }
}

pub(crate) trait Recover {
    type Ok;
    fn recover(self, error_handler: &mut ErrorHandler) -> Self::Ok;
}

impl<T, E: Into<CompileError>> Recover for RecoverableResult<T, E> {
    type Ok = T;
    fn recover(self, error_handler: &mut ErrorHandler) -> T {
        self.unwrap_or_else(|(e, i)| {
            error_handler.push_back(e);
            i
        })
    }
}

impl<T, E: Into<CompileError>> Recover for RecoverableResult<T, Vec<E>> {
    type Ok = T;
    fn recover(self, error_handler: &mut ErrorHandler) -> T {
        self.unwrap_or_else(|(es, i)| {
            error_handler.extend(es.into_iter());
            i
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_error() -> CompileError {
        Location::default().with(Error::Lex(LexError::UnterminatedComment))
    }

    fn new_error(error: Error) -> CompileError {
        Location::default().with(error)
    }

    #[test]
    fn test_error_handler_into_iterator() {
        let mut error_handler = ErrorHandler::new();
        error_handler.push_back(dummy_error());
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_compile_error_semantic() {
        assert_eq!(
            CompileError::semantic(Location::default().with("".to_string())).data,
            Error::Semantic(SemanticError::Generic("".to_string())),
        );
    }

    #[test]
    fn test_compile_error_is_kind() {
        let e = Error::Lex(LexError::Generic("".to_string()));
        assert!(e.is_lex_err());
        assert!(!e.is_semantic_err());
        assert!(!e.is_syntax_err());

        let e = Error::Semantic(SemanticError::Generic("".to_string()));
        assert!(!e.is_lex_err());
        assert!(e.is_semantic_err());
        assert!(!e.is_syntax_err());

        let e = Error::Syntax(SyntaxError::Generic("".to_string()));
        assert!(!e.is_lex_err());
        assert!(!e.is_semantic_err());
        assert!(e.is_syntax_err());
    }

    #[test]
    fn test_compile_error_display() {
        assert_eq!(
            dummy_error().data.to_string(),
            "invalid token: unterminated /* comment"
        );

        assert_eq!(
            Error::Semantic(SemanticError::Generic("bad code".to_string())).to_string(),
            "invalid program: bad code"
        );
    }

    #[test]
    fn test_recover_error() {
        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32> = Ok(1);
        assert_eq!(r.recover(&mut error_handler), 1);
        assert_eq!(error_handler.pop_front(), None);

        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32> = Err((dummy_error(), 42));
        assert_eq!(r.recover(&mut error_handler), 42);
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(errors, vec![dummy_error()]);
    }

    #[test]
    fn test_recover_multiple_errors() {
        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32, Vec<CompileError>> = Ok(1);
        assert_eq!(r.recover(&mut error_handler), 1);
        assert_eq!(error_handler.pop_front(), None);

        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32, Vec<CompileError>> = Err((
            vec![
                dummy_error(),
                new_error(Error::Semantic(SemanticError::Generic("pears".to_string()))),
            ],
            42,
        ));
        assert_eq!(r.recover(&mut error_handler), 42);
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(
            errors,
            vec![
                dummy_error(),
                new_error(Error::Semantic(SemanticError::Generic("pears".to_string()))),
            ]
        );
    }
}
