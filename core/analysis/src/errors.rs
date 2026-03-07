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
//! - [`AnalysisDiagnostic::BreakOutsideLoop`] - `break` used outside a loop body
//! - [`AnalysisDiagnostic::BreakInsideNonDetBlock`] - `break` used inside a non-deterministic block
//! - [`AnalysisDiagnostic::ReturnInsideLoop`] - `return` used inside a loop body
//! - [`AnalysisDiagnostic::InfiniteLoopWithoutBreak`] - Infinite loop missing a `break` statement
//! - [`AnalysisDiagnostic::ReturnInsideNonDetBlock`] - `return` used inside a non-deterministic block

use std::fmt::{self, Display, Formatter};

use inference_ast::nodes::Location;
use thiserror::Error;

/// Severity level for analysis findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisDiagnostic {
    #[error("break statement is only valid inside a loop body; if you intended to exit the function, use 'return'")]
    BreakOutsideLoop { location: Location },

    #[error("break statement is not allowed inside a '{block_kind}' block; break would interfere with the path exploration required for formal verification; move the break outside the '{block_kind}' block")]
    BreakInsideNonDetBlock {
        location: Location,
        block_kind: &'static str,
    },

    #[error(
        "return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
    )]
    ReturnInsideLoop { location: Location },

    #[error("infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)")]
    InfiniteLoopWithoutBreak { location: Location },

    #[error("return statement is not allowed inside a '{block_kind}' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the '{block_kind}' block")]
    ReturnInsideNonDetBlock {
        location: Location,
        block_kind: &'static str,
    },
}

impl AnalysisDiagnostic {
    /// Returns the source location associated with this error.
    #[must_use = "returns the source location without modifying the error"]
    pub fn location(&self) -> &Location {
        match self {
            AnalysisDiagnostic::BreakOutsideLoop { location }
            | AnalysisDiagnostic::BreakInsideNonDetBlock { location, .. }
            | AnalysisDiagnostic::ReturnInsideLoop { location }
            | AnalysisDiagnostic::InfiniteLoopWithoutBreak { location }
            | AnalysisDiagnostic::ReturnInsideNonDetBlock { location, .. } => location,
        }
    }

    /// Returns the analysis rule identifier (e.g. "A001") for this diagnostic.
    #[must_use = "returns the rule identifier without modifying the diagnostic"]
    pub fn rule_id(&self) -> &'static str {
        match self {
            AnalysisDiagnostic::BreakOutsideLoop { .. } => "A001",
            AnalysisDiagnostic::BreakInsideNonDetBlock { .. } => "A002",
            AnalysisDiagnostic::ReturnInsideLoop { .. } => "A003",
            AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. } => "A004",
            AnalysisDiagnostic::ReturnInsideNonDetBlock { .. } => "A005",
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
    errors: Vec<AnalysisDiagnostic>,
    warnings: Vec<AnalysisDiagnostic>,
    infos: Vec<AnalysisDiagnostic>,
}

impl AnalysisErrors {
    pub(crate) fn new(
        errors: Vec<AnalysisDiagnostic>,
        warnings: Vec<AnalysisDiagnostic>,
        infos: Vec<AnalysisDiagnostic>,
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
    pub fn errors(&self) -> &[AnalysisDiagnostic] {
        &self.errors
    }

    /// Returns the list of analysis warnings.
    #[must_use = "returns the list of analysis warnings"]
    pub fn warnings(&self) -> &[AnalysisDiagnostic] {
        &self.warnings
    }

    /// Returns the list of informational findings.
    #[must_use = "returns the list of informational findings"]
    pub fn infos(&self) -> &[AnalysisDiagnostic] {
        &self.infos
    }
}

impl Display for AnalysisErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut all: Vec<(&AnalysisDiagnostic, Severity)> = Vec::new();
        for d in &self.infos {
            all.push((d, Severity::Info));
        }
        for d in &self.warnings {
            all.push((d, Severity::Warning));
        }
        for d in &self.errors {
            all.push((d, Severity::Error));
        }
        all.sort_by(|a, b| {
            let la = a.0.location();
            let lb = b.0.location();
            (la.start_line, la.start_column).cmp(&(lb.start_line, lb.start_column))
        });
        let mut first = true;
        for (d, sev) in &all {
            if !first {
                writeln!(f)?;
            }
            write!(f, "{}: {sev}[{}]: {d}", d.location(), d.rule_id())?;
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
    pub(crate) warnings: Vec<AnalysisDiagnostic>,
    pub(crate) infos: Vec<AnalysisDiagnostic>,
}

impl AnalysisResult {
    /// Returns the list of analysis warnings.
    #[must_use = "returns the list of analysis warnings"]
    pub fn warnings(&self) -> &[AnalysisDiagnostic] {
        &self.warnings
    }

    /// Returns the list of informational findings.
    #[must_use = "returns the list of informational findings"]
    pub fn infos(&self) -> &[AnalysisDiagnostic] {
        &self.infos
    }

    /// Returns true if there are any warnings or informational findings.
    #[must_use = "returns whether any warnings or informational findings exist"]
    pub fn has_findings(&self) -> bool {
        !self.warnings.is_empty() || !self.infos.is_empty()
    }
}

impl Display for AnalysisResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut all: Vec<(&AnalysisDiagnostic, Severity)> = Vec::new();
        for d in &self.infos {
            all.push((d, Severity::Info));
        }
        for d in &self.warnings {
            all.push((d, Severity::Warning));
        }
        all.sort_by(|a, b| {
            let la = a.0.location();
            let lb = b.0.location();
            (la.start_line, la.start_column).cmp(&(lb.start_line, lb.start_column))
        });
        let mut first = true;
        for (d, sev) in &all {
            if !first {
                writeln!(f)?;
            }
            write!(f, "{}: {sev}[{}]: {d}", d.location(), d.rule_id())?;
            first = false;
        }
        Ok(())
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
        let err = AnalysisDiagnostic::BreakOutsideLoop {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "break statement is only valid inside a loop body; if you intended to exit the function, use 'return'"
        );
    }

    #[test]
    fn display_break_inside_nondet_block() {
        let err = AnalysisDiagnostic::BreakInsideNonDetBlock {
            location: test_location(),
            block_kind: "forall",
        };
        assert_eq!(
            err.to_string(),
            "break statement is not allowed inside a 'forall' block; break would interfere with the path exploration required for formal verification; move the break outside the 'forall' block"
        );
    }

    #[test]
    fn display_return_inside_loop() {
        let err = AnalysisDiagnostic::ReturnInsideLoop {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn display_infinite_loop_without_break() {
        let err = AnalysisDiagnostic::InfiniteLoopWithoutBreak {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)"
        );
    }

    #[test]
    fn display_return_inside_nondet_block() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: "forall",
        };
        assert_eq!(
            err.to_string(),
            "return statement is not allowed inside a 'forall' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the 'forall' block"
        );
    }

    #[test]
    fn display_break_inside_exists_block() {
        let err = AnalysisDiagnostic::BreakInsideNonDetBlock {
            location: test_location(),
            block_kind: "exists",
        };
        assert!(err.to_string().contains("'exists' block"));
    }

    #[test]
    fn display_return_inside_unique_block() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: "unique",
        };
        assert!(err.to_string().contains("'unique' block"));
    }

    #[test]
    fn display_break_inside_assume_block() {
        let err = AnalysisDiagnostic::BreakInsideNonDetBlock {
            location: test_location(),
            block_kind: "assume",
        };
        assert!(err.to_string().contains("'assume' block"));
    }

    #[test]
    fn display_return_inside_exists_block() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: "exists",
        };
        assert!(err.to_string().contains("'exists' block"));
    }

    #[test]
    fn error_location_accessor() {
        let loc = test_location();
        let err = AnalysisDiagnostic::BreakOutsideLoop { location: loc };
        assert_eq!(err.location(), &loc);
    }

    #[test]
    fn rule_id_values() {
        assert_eq!(
            AnalysisDiagnostic::BreakOutsideLoop { location: test_location() }.rule_id(),
            "A001"
        );
        assert_eq!(
            AnalysisDiagnostic::BreakInsideNonDetBlock { location: test_location(), block_kind: "forall" }.rule_id(),
            "A002"
        );
        assert_eq!(
            AnalysisDiagnostic::ReturnInsideLoop { location: test_location() }.rule_id(),
            "A003"
        );
        assert_eq!(
            AnalysisDiagnostic::InfiniteLoopWithoutBreak { location: test_location() }.rule_id(),
            "A004"
        );
        assert_eq!(
            AnalysisDiagnostic::ReturnInsideNonDetBlock { location: test_location(), block_kind: "forall" }.rule_id(),
            "A005"
        );
    }

    #[test]
    fn display_analysis_errors_single() {
        let errors = AnalysisErrors::new(
            vec![AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            }],
            vec![],
            vec![],
        );
        assert_eq!(
            errors.to_string(),
            "1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'"
        );
    }

    #[test]
    fn display_analysis_errors_multiple() {
        let errors = AnalysisErrors::new(
            vec![
                AnalysisDiagnostic::BreakOutsideLoop {
                    location: test_location(),
                },
                AnalysisDiagnostic::ReturnInsideLoop {
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
            "1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'\n3:10: error[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
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
        assert_eq!(result.to_string(), "");
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
    fn display_analysis_errors_with_warnings_sorted_by_location() {
        let errors = AnalysisErrors::new(
            vec![AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            }],
            vec![AnalysisDiagnostic::ReturnInsideLoop {
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
        assert_eq!(
            errors.to_string(),
            "1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'\n3:10: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn display_analysis_errors_with_all_severities_sorted_by_location() {
        let errors = AnalysisErrors::new(
            vec![AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            }],
            vec![AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            }],
            vec![AnalysisDiagnostic::InfiniteLoopWithoutBreak {
                location: test_location(),
            }],
        );
        // All at same location 1:5, so stable order within same location depends on push order:
        // infos first, then warnings, then errors
        assert_eq!(
            errors.to_string(),
            "1:5: info[A004]: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)\n1:5: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it\n1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'"
        );
    }

    #[test]
    fn display_analysis_result_with_warning() {
        let result = AnalysisResult {
            warnings: vec![AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            }],
            infos: vec![],
        };
        assert_eq!(
            result.to_string(),
            "1:5: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn has_findings_returns_false_when_empty() {
        let result = AnalysisResult {
            warnings: vec![],
            infos: vec![],
        };
        assert!(!result.has_findings());
    }

    #[test]
    fn has_findings_returns_true_with_warning() {
        let result = AnalysisResult {
            warnings: vec![AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            }],
            infos: vec![],
        };
        assert!(result.has_findings());
    }

    #[test]
    fn has_findings_returns_true_with_info() {
        let result = AnalysisResult {
            warnings: vec![],
            infos: vec![AnalysisDiagnostic::InfiniteLoopWithoutBreak {
                location: test_location(),
            }],
        };
        assert!(result.has_findings());
    }

    #[test]
    fn analysis_errors_new_panics_on_empty_errors() {
        let result = std::panic::catch_unwind(|| {
            AnalysisErrors::new(vec![], vec![], vec![]);
        });
        assert!(
            result.is_err(),
            "AnalysisErrors::new should panic when errors is empty"
        );
    }

    #[test]
    fn partial_eq_for_diagnostic() {
        let a = AnalysisDiagnostic::BreakOutsideLoop {
            location: test_location(),
        };
        let b = AnalysisDiagnostic::BreakOutsideLoop {
            location: test_location(),
        };
        assert_eq!(a, b);
    }
}
