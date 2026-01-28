use std::fmt;

use inference_ast::nodes::Location;

/// Severity level for semantic analysis diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A diagnostic produced by semantic analysis.
#[derive(Debug, Clone)]
pub struct SemanticDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub location: Location,
}

impl fmt::Display for SemanticDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: [{}] {}", self.location, self.severity, self.message)
    }
}

/// Collection of diagnostics from a semantic analysis pass.
#[derive(Debug, Default)]
pub struct SemanticResult {
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl SemanticResult {
    /// Returns `true` if any diagnostic has `Error` severity.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Returns only the error-level diagnostics.
    #[must_use]
    pub fn errors(&self) -> Vec<&SemanticDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect()
    }

    /// Formats all diagnostics into a single error message string.
    #[must_use]
    pub fn format_errors(&self) -> String {
        self.errors()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns `true` if any diagnostic has `Warning` severity.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    }

    /// Returns only the warning-level diagnostics.
    #[must_use]
    pub fn warnings(&self) -> Vec<&SemanticDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect()
    }

    /// Formats all warning diagnostics into a single message string.
    #[must_use]
    pub fn format_warnings(&self) -> String {
        self.warnings()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
