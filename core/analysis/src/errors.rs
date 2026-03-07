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
//! - [`AnalysisError::ReturnInsideNonDetBlock`] - `return` used inside a non-deterministic block

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

impl Display for Severity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// Represents a control flow analysis error with source location.
#[derive(Debug, Clone, Error)]
pub enum AnalysisError {
    #[error("{location}: break statement is only valid inside a loop body")]
    BreakOutsideLoop { location: Location },

    #[error(
        "{location}: break statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and break would disrupt path exploration; move the break outside the non-deterministic block"
    )]
    BreakInsideNonDetBlock { location: Location },

    #[error(
        "{location}: return inside a loop is not allowed; use break to exit the loop, then return after it"
    )]
    ReturnInsideLoop { location: Location },

    #[error("{location}: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)")]
    InfiniteLoopWithoutBreak { location: Location },

    #[error("{location}: return statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and return would exit the enclosing function; move the return outside the non-deterministic block")]
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
/// Also carries any warnings and infos found alongside the errors.
#[derive(Debug, Clone)]
pub struct AnalysisErrors {
    errors: Vec<AnalysisError>,
    warnings: Vec<AnalysisError>,
    infos: Vec<AnalysisError>,
}

impl AnalysisErrors {
    pub(crate) fn new(
        errors: Vec<AnalysisError>,
        warnings: Vec<AnalysisError>,
        infos: Vec<AnalysisError>,
    ) -> Self {
        assert!(!errors.is_empty(), "AnalysisErrors must contain at least one error");
        Self {
            errors,
            warnings,
            infos,
        }
    }

    /// Returns the list of analysis errors.
    #[must_use = "returns the list of analysis errors"]
    pub fn errors(&self) -> &[AnalysisError] {
        &self.errors
    }

    /// Returns the list of analysis warnings.
    #[must_use = "returns the list of analysis warnings"]
    pub fn warnings(&self) -> &[AnalysisError] {
        &self.warnings
    }

    /// Returns the list of informational findings.
    #[must_use = "returns the list of informational findings"]
    pub fn infos(&self) -> &[AnalysisError] {
        &self.infos
    }
}

impl Display for AnalysisErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for i in &self.infos {
            if !first { writeln!(f)?; }
            write!(f, "{}: {i}", Severity::Info)?;
            first = false;
        }
        for w in &self.warnings {
            if !first { writeln!(f)?; }
            write!(f, "{}: {w}", Severity::Warning)?;
            first = false;
        }
        for e in &self.errors {
            if !first { writeln!(f)?; }
            write!(f, "{}: {e}", Severity::Error)?;
            first = false;
        }
        Ok(())
    }
}

impl std::error::Error for AnalysisErrors {}

/// Holds non-fatal analysis findings (warnings and informational messages).
///
/// Returned from `analyze()` when no hard errors are found, allowing the
/// compilation pipeline to continue while still reporting lesser findings.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub(crate) warnings: Vec<AnalysisError>,
    pub(crate) infos: Vec<AnalysisError>,
}

impl AnalysisResult {
    /// Returns the list of analysis warnings.
    #[must_use = "returns the list of analysis warnings"]
    pub fn warnings(&self) -> &[AnalysisError] {
        &self.warnings
    }

    /// Returns the list of informational findings.
    #[must_use = "returns the list of informational findings"]
    pub fn infos(&self) -> &[AnalysisError] {
        &self.infos
    }
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
            "1:5: break statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and break would disrupt path exploration; move the break outside the non-deterministic block"
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
            "1:5: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)"
        );
    }

    #[test]
    fn display_return_inside_nondet_block() {
        let err = AnalysisError::ReturnInsideNonDetBlock {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: return statement is not allowed inside a non-deterministic block; non-deterministic blocks must explore all execution paths, and return would exit the enclosing function; move the return outside the non-deterministic block"
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
        let errors = AnalysisErrors::new(
            vec![AnalysisError::BreakOutsideLoop {
                location: test_location(),
            }],
            vec![],
            vec![],
        );
        assert_eq!(
            errors.to_string(),
            "error: 1:5: break statement is only valid inside a loop body"
        );
    }

    #[test]
    fn display_analysis_errors_multiple() {
        let errors = AnalysisErrors::new(
            vec![
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
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            errors.to_string(),
            "error: 1:5: break statement is only valid inside a loop body\nerror: 3:10: return inside a loop is not allowed; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn display_analysis_result_empty() {
        let result = AnalysisResult {
            warnings: vec![],
            infos: vec![],
        };
        assert!(result.warnings().is_empty());
        assert!(result.infos().is_empty());
    }

    #[test]
    fn severity_variants() {
        assert_ne!(Severity::Error, Severity::Warning);
        assert_ne!(Severity::Warning, Severity::Info);
        assert_ne!(Severity::Error, Severity::Info);
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Info.to_string(), "info");
    }

    #[test]
    fn display_analysis_errors_with_warnings() {
        let errors = AnalysisErrors::new(
            vec![AnalysisError::BreakOutsideLoop {
                location: test_location(),
            }],
            vec![AnalysisError::ReturnInsideLoop {
                location: Location {
                    offset_start: 20,
                    offset_end: 30,
                    start_line: 3,
                    start_column: 10,
                    end_line: 3,
                    end_column: 20,
                },
            }],
            vec![],
        );
        let output = errors.to_string();
        assert!(output.contains("warning:"), "should contain warning prefix");
        assert!(output.contains("error:"), "should contain error prefix");
    }
}
