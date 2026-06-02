//! Error types for the parser crate.
//!
//! Two distinct error shapes live here:
//!
//! - [`ParseError`] is the per-site, structured diagnostic the resilient parser
//!   collects as it recovers. Parsing never fails as control flow (see the
//!   master plan, AD-4); it always returns a tree plus a `Vec<ParseError>`.
//! - [`ParserError`] is the consolidated crate error enum required of every
//!   `core/` crate, used when adapting parse results to `anyhow::Result` at the
//!   crate boundary.

use inference_ast::nodes::Location;
use thiserror::Error;

/// A single structured parse diagnostic, scoped to the smallest construct that
/// could not be recognized.
///
/// The parser produces a `Vec<ParseError>` alongside the AST instead of failing
/// fast, so that one syntax error does not abort the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Source span the diagnostic refers to.
    pub span: Location,
    /// Human-readable description of the problem.
    pub message: String,
}

/// Consolidated error enum for the parser crate.
#[derive(Debug, Error)]
#[must_use = "errors must not be silently ignored"]
pub enum ParserError {
    /// One or more syntax errors were collected while parsing a source file.
    #[error("syntax error at {span}: {message}")]
    Syntax { span: Location, message: String },

    /// General fallback for errors that do not fit a specific category.
    #[error("parser error: {0}")]
    General(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_is_clone_eq_and_debug() {
        let span = Location::new(0, 3, 1, 1, 1, 4);
        let error = ParseError {
            span,
            message: "boom".to_string(),
        };
        let clone = error.clone();
        assert_eq!(error, clone);

        let other = ParseError {
            span,
            message: "different".to_string(),
        };
        assert_ne!(error, other);

        // `Debug` must render the message field.
        let debug = format!("{error:?}");
        assert!(debug.contains("boom"), "debug was {debug:?}");
    }

    #[test]
    fn parser_error_display_per_variant() {
        let span = Location::new(5, 9, 2, 3, 2, 7);
        let syntax = ParserError::Syntax {
            span,
            message: "unexpected token".to_string(),
        };
        assert_eq!(syntax.to_string(), "syntax error at 2:3: unexpected token");

        let general = ParserError::General("something failed".to_string());
        assert_eq!(general.to_string(), "parser error: something failed");
    }
}
