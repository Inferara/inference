#![warn(clippy::pedantic)]
//! Static Analysis Pass for the Inference Compiler
//!
//! This crate provides semantic analysis that validates invariants beyond what
//! the type checker covers. It operates on the fully-typed AST and runs after
//! type checking but before code generation.
//!
//! The type checker focuses on type correctness only — blocking dead-end type
//! errors that would prevent further analysis. Everything else (control flow,
//! codegen restrictions, lint warnings) is handled here.
//!
//! ## Current Analyses
//!
//! ### Control Flow (A001–A008)
//!
//! - A001: `break` must appear inside a loop body
//! - A002: `break` must not appear inside a non-deterministic block
//! - A003: `return` must not appear inside a loop body
//! - A004: Infinite loops (`loop { ... }`) must contain a `break` statement
//! - A005: `return` must not appear inside a non-deterministic block
//! - A006: Uzumaki (`@`) must not appear outside a non-deterministic block
//! - A007: Non-void functions must have a return on every execution path
//! - A008: Standalone uzumaki expression (not assigned to a variable)
//!
//! ### Lint Warnings (A009–A011)
//!
//! - A009: Enum definition with no variants
//! - A010: Method declares `self` but never accesses it
//! - A011: Struct definition with no fields and no methods
//!
//! ### Dead Code (A020)
//!
//! - A020: Unreachable code after `return`, `break`, or infinite loop
//!
//! ### Variable Initialization (A025)
//!
//! - A025: Variable declarations must have an initializer
//!
//! ### Codegen Restrictions (A012–A019, A022–A031, A038–A040)
//!
//! These rules describe constructs that are valid in the type system but cannot
//! be lowered by the current code generator.
//!
//! - A012: Compound literal (array or struct) passed directly as a function argument
//! - A014: Array uzumaki passed directly as a function argument
//! - A015: Compound literal (array or struct) in an unsupported expression position
//! - A016: Compound-returning function call in a general expression position
//! - A017: Compound-returning function call on the RHS of an assignment statement
//! - A018: Method call chained on the result of a compound-returning function
//! - A019: 64-bit integer used as an array index
//! - A022: Numeric literal out of range for its declared type
//! - A023: Uzumaki used in a variable reassignment (only `let` initializers allowed)
//! - A024: Call to an external (`extern`) function
//! - A026: Nested compound type depth exceeds one level
//! - A027: Uzumaki on struct with compound fields (nested struct)
//! - A028: Uzumaki on array of structs
//! - A029: Compound literal in compound element assignment
//! - A030: (removed — uzumaki on scalar arrays now supported at any depth)
//! - A031: Unsupported expression form in compound-returning function return
//! - A032: Top-level (module-scope) `const` declaration (not yet implemented)
//! - A038: Uzumaki (`@`) on a struct- or array-typed struct-literal field
//! - A039: Struct uzumaki (`@`) passed directly as a function argument
//! - A040: Uzumaki (`@`) on a struct- or array-typed array-literal element
//!
//! ### Syntactic Restrictions (A033)
//!
//! - A033: Combined/adjacent prefix unary operators (`--x`, `-~x`, `!!x`, including parenthesized variants)
//!
//! ### Recursion (A035)
//!
//! - A035: Direct or indirect (mutual) recursion is forbidden (Power of 10, Rule 1)
//!
//! ### Stack Depth (A036)
//!
//! - A036: Cumulative shadow-stack usage along a call chain must not exceed the
//!   64 KB stack budget. Because A035 makes the call graph acyclic, the
//!   worst-case usage is the maximum-weight root-to-leaf path (node weight =
//!   that function's compound-frame size). The estimator over-approximates each
//!   frame to stay sound against codegen; see [`rules::stack_depth`].
//!
//! ### Array Bounds (A037)
//!
//! - A037: A constant array index must be in bounds. When `arr[c]` has a literal
//!   index `c` and the array's type is `[T; length]`, the access is rejected if
//!   `c < 0` or `c >= length`. This is a compile-time check with zero runtime
//!   cost; dynamic (non-literal) indices are out of scope for this rule.
//!
//! ### Duplicate Local Names (A041)
//!
//! - A041: A function-local name (`let` or `const`) may be declared at most once
//!   per function body. Reusing a name across disjoint sibling blocks is well
//!   typed but collides in the flat WebAssembly local namespace, so it is
//!   rejected with a rename-or-hoist hint. This is a simplicity and auditability
//!   rule — it preserves a 1:1 source-name to local to proof-index mapping per
//!   function — not a proof-soundness requirement.
//!
//! ### Non-Deterministic Constructs Outside `spec` (A042)
//!
//! - A042: The non-deterministic block forms (inline `forall`/`exists`/`assume`/
//!   `unique` blocks and the function-body-modifier form `fn f() forall { … }`)
//!   describe formal specifications and are valid only lexically inside a `spec`
//!   declaration. Used in a top-level function, a top-level struct method, or a
//!   block nested inside either, they are rejected. The check is lexical, hence
//!   independent of the compilation mode. Only the outermost non-det block on
//!   each path is reported.
//!
//! ### Reserved Export Names (A043)
//!
//! - A043: An entry-file top-level `pub fn` may not be named `memory` or
//!   `__stack_pointer`. Codegen exports such a function under its plain source
//!   name and separately reserves those names for the module's synthetic linear
//!   memory and stack-pointer exports; a user function with either name would
//!   produce a duplicate export name (invalid wasm) or hijack the standard
//!   `memory` export with a Function. The check is unconditional so the ABI
//!   surface does not depend on whether the program happens to use memory.
//!
//! ## Pipeline Position
//!
//! ```text
//! parse -> type_check -> analyze -> codegen
//! ```
//!
//! The `analyze()` function is called by the orchestration layer in
//! `core/inference/src/lib.rs` after type checking succeeds.
//!
//! ## Rule Architecture
//!
//! Each analysis check is an independent struct implementing the [`rule::Rule`]
//! trait. Rules are registered in [`rules::all_rules()`] and executed
//! sequentially. The [`rule!`] macro reduces boilerplate for rule definitions.
//!
//! ## Design
//!
//! This crate depends on `inference-ast` and `inference-type-checker`.
//! The entry point accepts `&TypedContext` from the type checker.

use inference_type_checker::typed_context::TypedContext;

mod call_graph;
pub mod errors;
pub mod rule;
pub mod rules;
mod walker;

use errors::{AnalysisErrors, AnalysisResult, Severity};

pub use rules::stack_depth::estimate_frame_sizes;

/// Re-exported because [`errors::AnalysisDiagnostic::LiteralOutOfRange`] carries
/// one: a consumer that destructures that diagnostic would otherwise need a
/// direct dependency on the type checker to name the field's type.
pub use inference_type_checker::errors::TypeMismatchContext;

/// Performs static analysis on the typed AST.
///
/// Runs all registered analysis rules and collects findings. Rules cover
/// control flow validation, lint warnings, and codegen restrictions. See the
/// module-level documentation for the full rule list.
///
/// # Errors
///
/// Returns `AnalysisErrors` if any `Error`-severity findings are produced.
/// All findings are collected before returning, allowing the user to see all
/// issues at once. `Warning`-severity findings are returned via `AnalysisResult`
/// on both success and error paths.
pub fn analyze(typed_context: &TypedContext) -> Result<AnalysisResult, AnalysisErrors> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut infos = Vec::new();
    for &r in rules::all_rules() {
        let findings = r.check(typed_context);
        match r.severity() {
            Severity::Error => errors.extend(findings),
            Severity::Warning => warnings.extend(findings),
            Severity::Info => infos.extend(findings),
        }
    }
    if errors.is_empty() {
        Ok(AnalysisResult::new(warnings, infos))
    } else {
        Err(AnalysisErrors::new(errors, warnings, infos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AnalysisDiagnostic;
    use inference_ast::nodes::Location;

    fn dummy_location() -> Location {
        Location::default()
    }

    #[test]
    fn rule_ids_match_diagnostic_rule_ids() {
        let diagnostics: Vec<AnalysisDiagnostic> = vec![
            AnalysisDiagnostic::BreakOutsideLoop { location: dummy_location() },
            AnalysisDiagnostic::BreakInsideNonDetBlock { location: dummy_location(), block_kind: "forall" },
            AnalysisDiagnostic::ReturnInsideLoop { location: dummy_location() },
            AnalysisDiagnostic::InfiniteLoopWithoutBreak { location: dummy_location() },
            AnalysisDiagnostic::ReturnInsideNonDetBlock { location: dummy_location(), block_kind: "forall" },
            AnalysisDiagnostic::UzumakiOutsideNonDetBlock { location: dummy_location() },
            AnalysisDiagnostic::MissingReturn { function_name: "f".to_string(), location: dummy_location() },
            AnalysisDiagnostic::StandaloneUzumaki { location: dummy_location() },
            AnalysisDiagnostic::EmptyEnumDefinition { name: "E".to_string(), location: dummy_location() },
            AnalysisDiagnostic::MethodNeverAccessesSelf { struct_name: "S".to_string(), method_name: "m".to_string(), location: dummy_location() },
            AnalysisDiagnostic::EmptyStructDefinition { name: "S".to_string(), location: dummy_location() },
            AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", location: dummy_location() },
            AnalysisDiagnostic::ArrayUzumakiAsArgument { location: dummy_location() },
            AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { kind: "struct", location: dummy_location() },
            AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { location: dummy_location() },
            AnalysisDiagnostic::CompoundReturnCallInAssignment { location: dummy_location() },
            AnalysisDiagnostic::MethodCallChainOnCompoundReturn { location: dummy_location() },
            AnalysisDiagnostic::ArrayIndex64Bit { found: "i64".to_string(), location: dummy_location() },
            AnalysisDiagnostic::DeadCode { terminator: "return", location: dummy_location() },
            AnalysisDiagnostic::LiteralOutOfRange { value: "256".to_string(), type_name: "u8".to_string(), min: 0, max: 255, type_source: None, location: dummy_location() },
            AnalysisDiagnostic::UzumakiInReassignment { location: dummy_location() },
            AnalysisDiagnostic::ExternFunctionCall { name: "print".to_string(), location: dummy_location() },
            AnalysisDiagnostic::UninitializedVariable { name: "x".to_string(), location: dummy_location() },
            AnalysisDiagnostic::NestedCompoundDepthExceeded { outer: "Outer".to_string(), field: "inner".to_string(), ty: "Inner".to_string(), location: dummy_location() },
            AnalysisDiagnostic::UzumakiOnNestedStruct { name: "Outer".to_string(), location: dummy_location() },
            AnalysisDiagnostic::UzumakiOnStructInArray { location: dummy_location() },
            AnalysisDiagnostic::CompoundLiteralInCompoundAssign { location: dummy_location() },
            AnalysisDiagnostic::UnsupportedCompoundReturnExpression { location: dummy_location() },
            AnalysisDiagnostic::TopLevelConstNotSupported { name: "X".to_string(), location: dummy_location() },
            AnalysisDiagnostic::CombinedUnaryOperators { op_outer: "-", op_inner: "~", location: dummy_location() },
            AnalysisDiagnostic::VisibilityInsideSpec { spec_name: "S".to_string(), def_name: "f".to_string(), def_kind: "fn", location: dummy_location() },
            AnalysisDiagnostic::RecursionDetected { cycle: "f -> f".to_string(), location: dummy_location() },
            AnalysisDiagnostic::StackDepthExceeded { chain: "a -> b".to_string(), depth_bytes: 80_000, budget_bytes: 65_536, location: dummy_location() },
            AnalysisDiagnostic::ArrayIndexConstOutOfBounds { index: "3".to_string(), length: 3, location: dummy_location() },
            AnalysisDiagnostic::UzumakiOnCompoundField { field: "i".to_string(), ty: "Inner".to_string(), location: dummy_location() },
            AnalysisDiagnostic::StructUzumakiAsArgument { location: dummy_location() },
            AnalysisDiagnostic::UzumakiOnCompoundArrayElement { ty: "Point".to_string(), location: dummy_location() },
            AnalysisDiagnostic::DuplicateLocalName { name: "x".to_string(), location: dummy_location(), first_location: dummy_location() },
            AnalysisDiagnostic::NonDetOutsideSpec { location: dummy_location(), block_kind: "forall" },
            AnalysisDiagnostic::ReservedExportName { name: "memory".to_string(), location: dummy_location() },
            AnalysisDiagnostic::ShiftCountOutOfRange { value: "32".to_string(), type_name: "i32".to_string(), max: 31, location: dummy_location() },
        ];

        let rules = rules::all_rules();
        assert_eq!(rules.len(), diagnostics.len(), "rule count must match diagnostic variant count");

        for (rule, diag) in rules.iter().zip(diagnostics.iter()) {
            assert_eq!(
                rule.id(),
                diag.rule_id(),
                "Rule '{}' has id '{}' but its diagnostic variant has rule_id '{}'",
                rule.name(),
                rule.id(),
                diag.rule_id()
            );
        }
    }
}
