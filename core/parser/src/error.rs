use std::fmt;
use thiserror::Error;

/// Parser error types with location information
#[derive(Debug, Clone, Error)]
pub enum ParseError {
    #[error("Unexpected token at position {pos}: expected {expected}, found {found}")]
    UnexpectedToken {
        pos: usize,
        expected: String,
        found: String,
    },

    #[error("Unexpected end of file while parsing {context}")]
    UnexpectedEof { context: String },

    #[error("Invalid syntax at position {pos}: {reason}")]
    InvalidSyntax { pos: usize, reason: String },

    #[error("Failed to parse {context} at position {pos}")]
    FailedToParse { pos: usize, context: String },

    #[error("Duplicate definition: {name}")]
    DuplicateName { name: String },

    #[error("Invalid type annotation: {reason}")]
    InvalidTypeAnnotation { reason: String },

    #[error("Invalid generic parameters: {reason}")]
    InvalidGenerics { reason: String },
}

/// Error recovery mode allows the parser to continue after errors
#[derive(Debug, Clone)]
pub struct ParseErrorWithRecovery {
    pub error: ParseError,
    pub recovered: bool,
}

impl fmt::Display for ParseErrorWithRecovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.recovered {
            write!(f, "{} (recovered)", self.error)
        } else {
            write!(f, "{}", self.error)
        }
    }
}

/// Collects multiple errors during parsing for batch reporting
#[derive(Debug, Default, Clone)]
pub struct ParseErrorCollector {
    errors: Vec<ParseError>,
}

impl ParseErrorCollector {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn add_error(&mut self, error: ParseError) {
        self.errors.push(error);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    pub fn take_errors(self) -> Vec<ParseError> {
        self.errors
    }
}
