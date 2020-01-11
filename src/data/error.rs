use std::collections::VecDeque;
use thiserror::Error;

use super::{Locatable, Location};

/// RecoverableResult is a type that represents a Result that can be recovered from.
///
/// See the [`Recover`] trait for more information.
///
/// [`Recover`]: trait.Recover.html
pub type RecoverableResult<T, E = CompileError> = Result<T, (E, T)>;
pub type CompileResult<T> = Result<T, CompileError>;
pub type CompileError = Locatable<Error>;
pub type CompileWarning = Locatable<Warning>;

/// ErrorHandler is a struct that hold errors generated by the compiler
///
/// An error handler is used because multiple errors may be generated by each
/// part of the compiler, this cannot be represented well with Rust's normal
/// `Result`.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ErrorHandler {
    errors: VecDeque<CompileError>,
    pub(crate) warnings: VecDeque<CompileWarning>,
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

    /// Stopgap to make it easier to transition to lazy warnings.
    ///
    /// TODO: Remove this method
    pub(crate) fn warn<W: Into<Warning>>(&mut self, warning: W, location: Location) {
        self.warnings
            .push_back(Locatable::new(warning.into(), location));
    }
    /// Add an iterator of errors to the error queue
    pub(crate) fn extend<E: Into<CompileError>>(&mut self, iter: impl Iterator<Item = E>) {
        self.errors.extend(iter.map(Into::into));
    }
}

impl Iterator for ErrorHandler {
    type Item = CompileError;

    fn next(&mut self) -> Option<CompileError> {
        self.pop_front()
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid program: {0}")]
    Semantic(#[from] SemanticError),

    #[error("invalid syntax: {0}")]
    Syntax(#[from] SyntaxError),

    #[error("invalid token: {0}")]
    Lex(#[from] LexError),
}

/// Semantic errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SemanticError {
    #[error("{0}")]
    Generic(String),
    #[error("cannot have empty program")]
    EmptyProgram,

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

/// Syntax errors are non-exhaustive and may have new variants added at any time
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SyntaxError {
    #[error("{0}")]
    Generic(String),

    #[error("expected {0}, got <end-of-file>")]
    EndOfFile(&'static str),

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

    #[doc(hidden)]
    #[error("internal error: do not construct nonexhaustive variants")]
    __Nonexhaustive,
}

#[derive(Debug, Error, PartialEq, Eq)]
/// errors are non-exhaustive and may have new variants added at any time
pub enum Warning {
    // for compatibility
    #[error("{0}")]
    Generic(String),

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

    lazy_static::lazy_static! {
        static ref DUMMY_ERROR: CompileError = CompileError::new(
            Error::Lex(LexError::UnterminatedComment),
            Default::default(),
        );
    }

    fn new_error(error: Error) -> CompileError {
        CompileError::new(error, Location::default())
    }

    #[test]
    fn test_error_handler_push_err() {
        let mut error_handler = ErrorHandler::new();
        error_handler.push_back(DUMMY_ERROR.clone());

        assert_eq!(
            error_handler,
            ErrorHandler {
                errors: vec_deque![DUMMY_ERROR.clone()],
                warnings: VecDeque::new(),
            }
        );
    }

    #[test]
    fn test_error_handler_into_iterator() {
        let mut error_handler = ErrorHandler::new();
        error_handler.push_back(DUMMY_ERROR.clone());
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_compile_error_semantic() {
        assert_eq!(
            CompileError::semantic(Locatable::new("".to_string(), Location::default())).data,
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
            DUMMY_ERROR.data.to_string(),
            "invalid token: unterminated /* comment"
        );

        assert_eq!(
            Error::Semantic(SemanticError::Generic("bad code".to_string())).to_string(),
            "invalid program: bad code"
        );
    }

    #[test]
    fn test_compile_error_from_locatable_string() {
        let _ = CompileError::from(Locatable::new("apples".to_string(), Location::default()));
    }

    #[test]
    fn test_compile_error_from_syntax_error() {
        let _ = CompileError::new(
            SyntaxError::from("oranges".to_string()).into(),
            Location::default(),
        );
    }

    #[test]
    fn test_recover_error() {
        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32> = Ok(1);
        assert_eq!(r.recover(&mut error_handler), 1);
        assert_eq!(error_handler.pop_front(), None);

        let mut error_handler = ErrorHandler::new();
        let r: RecoverableResult<i32> = Err((DUMMY_ERROR.clone(), 42));
        assert_eq!(r.recover(&mut error_handler), 42);
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(errors, vec![DUMMY_ERROR.clone()]);
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
                DUMMY_ERROR.clone(),
                new_error(Error::Semantic(SemanticError::Generic("pears".to_string()))),
            ],
            42,
        ));
        assert_eq!(r.recover(&mut error_handler), 42);
        let errors = error_handler.collect::<Vec<_>>();
        assert_eq!(
            errors,
            vec![
                DUMMY_ERROR.clone(),
                new_error(Error::Semantic(SemanticError::Generic("pears".to_string()))),
            ]
        );
    }
}
