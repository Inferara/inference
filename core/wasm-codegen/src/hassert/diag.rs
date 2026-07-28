//! Diagnostics for the proof-mode specification-to-`hassert` translation.
//!
//! These are their own `P0xx` registry rather than analysis `A0xx` rules for a
//! structural reason: the analysis pass is mode-blind, and every construct these
//! codes reject is legal in proof-mode WASM emission (a `loop` inside a `forall`
//! body compiles fine). They lack only an *assertion* encoding, so they are
//! proof-mode code-generation errors, surfaced in the same
//! `[file_label:]line:col: error[P00x]: message` shape the user already reads
//! from analysis.
//!
//! Every diagnostic is collected — the translator visits all specification
//! functions and records every problem — so a spec with several mistakes
//! surfaces them all at once rather than one round-trip at a time. A diagnostic
//! keeps its function from producing an obligation but does not fail code
//! generation (see the module docs on why the pass stays additive).

use std::fmt;

use inference_ast::nodes::{Location, file_label};

/// A proof-mode specification-translation error code.
///
/// The numbering is stable and user-facing; the messages live at the call sites
/// (many carry a construct name or a reason that only the site knows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PCode {
    /// Spec function body is `exists`/`unique`/`assume`-quantified.
    P001,
    /// A construct with no assertion encoding (`loop`, `break`, `unique` block,
    /// `**`, a literal that is not a scalar, memory access).
    P002,
    /// Reassignment (`Stmt::Assign`) in a specification body.
    P003,
    /// A non-scalar type in a term, parameter, or `@` position.
    P004,
    /// A call that cannot be represented as a `T_app` term.
    P005,
    /// `@` outside a `let` right-hand side or a call-argument position.
    P006,
    /// A `forall` block nested inside an `exists` context.
    P007,
    /// `@` at a compound (array/struct) type.
    P008,
    /// A quantified specification *method* (never silently dropped).
    P009,
}

impl fmt::Display for PCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            PCode::P001 => "P001",
            PCode::P002 => "P002",
            PCode::P003 => "P003",
            PCode::P004 => "P004",
            PCode::P005 => "P005",
            PCode::P006 => "P006",
            PCode::P007 => "P007",
            PCode::P008 => "P008",
            PCode::P009 => "P009",
        };
        f.write_str(code)
    }
}

/// One proof-mode translation diagnostic, carrying the source location and the
/// defining file's module path so it renders with the right file label.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct HassertDiagnostic {
    code: PCode,
    location: Location,
    module_path: Vec<String>,
    message: String,
}

impl HassertDiagnostic {
    pub(crate) fn new(
        code: PCode,
        location: Location,
        module_path: Vec<String>,
        message: String,
    ) -> Self {
        Self {
            code,
            location,
            module_path,
            message,
        }
    }
}

impl fmt::Display for HassertDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(label) = file_label(&self.module_path) {
            write!(f, "{label}:")?;
        }
        write!(
            f,
            "{}:{}: error[{}]: {}",
            self.location.start_line, self.location.start_column, self.code, self.message
        )
    }
}
