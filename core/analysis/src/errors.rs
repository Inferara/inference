//! Analysis Error Types
//!
//! This module defines the error types produced by the analysis pass, providing
//! detailed context and location information for all control flow violations.
//!
//! ## Error Design
//!
//! All analysis errors:
//! - Include precise source location (line and column)
//! - Provide actionable error messages with guidance
//! - Use descriptive error messages via `thiserror`
//! - Are collected and reported together (error recovery)
//!
//! ## Error Categories
//!
//! **Loop Control Flow Errors**:
//! - [`AnalysisError::BreakOutsideLoop`] - `break` used outside a loop body
//! - [`AnalysisError::BreakInsideNonDetBlock`] - `break` used inside a non-deterministic block
//! - [`AnalysisError::ReturnInsideLoop`] - `return` used inside a loop body
//! - [`AnalysisError::InfiniteLoopWithoutBreak`] - Infinite loop missing a `break` statement

use std::fmt::{self, Display, Formatter};

use inference_ast::nodes::Location;
use thiserror::Error;

/// Severity level for analysis findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Represents a control flow analysis error with source location.
#[derive(Debug, Clone, Error)]
pub enum AnalysisError {
    #[error(
        "{location}: break statement is only valid inside a loop body"
    )]
    BreakOutsideLoop { location: Location },

    #[error(
        "{location}: break statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and break would exit the enclosing loop"
    )]
    BreakInsideNonDetBlock { location: Location },

    #[error(
        "{location}: return inside a loop is not allowed; use break to exit the loop, then return after it"
    )]
    ReturnInsideLoop { location: Location },

    #[error("{location}: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate")]
    InfiniteLoopWithoutBreak { location: Location },

    #[error("{location}: return statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and return would exit the enclosing function")]
    ReturnInsideNonDetBlock { location: Location },
}

impl AnalysisError {
    /// Returns the source location associated with this error.
    #[must_use = "returns the source location without modifying the error"]
    pub fn location(&self) -> &Location {
        match self {
            AnalysisError::BreakOutsideLoop { location }
            | AnalysisError::BreakInsideNonDetBlock { location }
            | AnalysisError::ReturnInsideLoop { location }
            | AnalysisError::InfiniteLoopWithoutBreak { location }
            | AnalysisError::ReturnInsideNonDetBlock { location } => location,
        }
    }
}

/// Wrapper for multiple analysis errors, following the `TypeCheckErrors` pattern.
///
/// Collects all analysis errors found during a single pass, allowing the user
/// to see all issues at once rather than fixing one error at a time.
#[derive(Debug, Clone)]
pub struct AnalysisErrors {
    errors: Vec<AnalysisError>,
}

impl AnalysisErrors {
    pub(crate) fn new(errors: Vec<AnalysisError>) -> Self {
        Self { errors }
    }

    /// Returns the list of analysis errors.
    #[must_use]
    pub fn errors(&self) -> &[AnalysisError] {
        &self.errors
    }
}

impl Display for AnalysisErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let messages: Vec<String> = self.errors.iter().map(ToString::to_string).collect();
        write!(f, "{}", messages.join("; "))
    }
}

impl std::error::Error for AnalysisErrors {}

/// Holds non-fatal analysis findings (warnings and informational messages).
///
/// Returned from `analyze()` when no hard errors are found, allowing the
/// compilation pipeline to continue while still reporting lesser findings.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub warnings: Vec<AnalysisError>,
    pub infos: Vec<AnalysisError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_location() -> Location {
        Location {
            offset_start: 4,
            offset_end: 9,
            start_line: 1,
            start_column: 5,
            end_line: 1,
            end_column: 10,
        }
    }

    #[test]
    fn display_break_outside_loop() {
        let err = AnalysisError::BreakOutsideLoop {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: break statement is only valid inside a loop body"
        );
    }

    #[test]
    fn display_break_inside_nondet_block() {
        let err = AnalysisError::BreakInsideNonDetBlock {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: break statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and break would exit the enclosing loop"
        );
    }

    #[test]
    fn display_return_inside_loop() {
        let err = AnalysisError::ReturnInsideLoop {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: return inside a loop is not allowed; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn display_infinite_loop_without_break() {
        let err = AnalysisError::InfiniteLoopWithoutBreak {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate"
        );
    }

    #[test]
    fn display_return_inside_nondet_block() {
        let err = AnalysisError::ReturnInsideNonDetBlock {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: return statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and return would exit the enclosing function"
        );
    }

    #[test]
    fn error_location_accessor() {
        let loc = test_location();
        let err = AnalysisError::BreakOutsideLoop { location: loc };
        assert_eq!(err.location(), &loc);
    }

    #[test]
    fn display_analysis_errors_single() {
        let errors = AnalysisErrors::new(vec![AnalysisError::BreakOutsideLoop {
            location: test_location(),
        }]);
        assert_eq!(
            errors.to_string(),
            "1:5: break statement is only valid inside a loop body"
        );
    }

    #[test]
    fn display_analysis_errors_multiple() {
        let errors = AnalysisErrors::new(vec![
            AnalysisError::BreakOutsideLoop {
                location: test_location(),
            },
            AnalysisError::ReturnInsideLoop {
                location: Location {
                    offset_start: 20,
                    offset_end: 30,
                    start_line: 3,
                    start_column: 10,
                    end_line: 3,
                    end_column: 20,
                },
            },
        ]);
        assert_eq!(
            errors.to_string(),
            "1:5: break statement is only valid inside a loop body; 3:10: return inside a loop is not allowed; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn display_analysis_errors_empty() {
        let errors = AnalysisErrors::new(vec![]);
        assert_eq!(errors.to_string(), "");
    }

    #[test]
    fn severity_variants() {
        assert_ne!(Severity::Error, Severity::Warning);
        assert_ne!(Severity::Warning, Severity::Info);
        assert_ne!(Severity::Error, Severity::Info);
    }
}
