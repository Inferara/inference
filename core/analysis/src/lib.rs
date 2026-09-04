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
//! - A011: Struct definition with no fields and no methods. A field-less struct
//!   that declares methods is deliberately not warned: that is the supported
//!   method-namespace idiom, and A045 governs its values.
//!
//! ### Dead Code (A020)
//!
//! - A020: Unreachable code after `return`, `break`, or infinite loop
//!
//! ### Variable Initialization (A025)
//!
//! - A025: Variable declarations must have an initializer
//!
//! ### Codegen Restrictions (A012–A019, A022–A023, A026–A032, A038–A040)
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
//! ### Syntactic Restrictions (A033, A046)
//!
//! - A033: Combined/adjacent prefix unary operators (`--x`, `-~x`, `!!x`, including parenthesized variants)
//! - A046: A unary minus applied to a numeric literal must be written glued to
//!   the digits. `-128` is one token whose text carries the sign; `- 128` is a
//!   negation of the bare literal `128`, which every later rule measures on its
//!   own — so the same value used to compile or fail depending on a space
//!   (`- 100` was accepted at `i8`, `- 128` was not). Rejecting the separated
//!   spelling leaves one canonical way to write a negative literal. A022 skips
//!   exactly the literals this rule claims, so the range check never reports a
//!   magnitude the author did not write; nothing is silently accepted, since
//!   every skipped literal is rejected here. Negating a non-literal (`- x`),
//!   `-(128)`, and binary subtraction are out of scope. See
//!   [`rules::spaced_negative_literal`].
//!
//! ### External Function Contracts (A024, A047)
//!
//! - A024: A call to an `external fn` that no `use … from` directive binds. A
//!   bound external lowers to a WASM import the static-merge linker satisfies, so
//!   calling one is supported everywhere — including from inside a `spec`. An
//!   unbound declaration names no module, so nothing supplies a body for the call
//!   to reach, in any mode. See [`rules::extern_function_call`].
//! - A047: A compound argument at a `mut` `external fn` parameter must be rooted
//!   at a `mut` binding. A linked external shares the caller's linear memory, so
//!   a struct or array argument reaches it as a raw pointer with no copy in
//!   between; `mut` on the declaration states that the foreign body may store
//!   through that pointer, and the linker checks the claim against the merged
//!   body. This is the one place a write to a binding is invisible in Inference
//!   source — the store lives in a `.wasm` the type checker never reads — so the
//!   call site must carry the statement instead. Scalars and enums are out of
//!   scope: neither passes a region. See [`rules::extern_mut_argument`].
//!
//! ### Recursion (A035)
//!
//! - A035: Direct or indirect (mutual) recursion is forbidden (Power of 10, Rule 1)
//!
//! ### Stack Depth (A036)
//!
//! - A036: Cumulative shadow-stack usage along a call chain must not exceed the
//!   configured stack budget (64 KB by default). Because A035 makes the call
//!   graph acyclic, the worst-case usage is the maximum-weight root-to-leaf path
//!   (node weight = that function's compound-frame size). The estimator
//!   over-approximates each frame to stay sound against codegen; see
//!   [`rules::stack_depth`].
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
//! ### Field-less Struct Values (A045)
//!
//! - A045: A struct with no fields occupies zero bytes, so it has no value
//!   representation: there is no memory region to hold, copy, or reason about one
//!   of its values. Such a type is rejected as a struct literal, as the declared
//!   type of a `let`/`const`, as a parameter, as a return type, as a struct field,
//!   and as a `self` receiver — with arrays of it looked through at any depth.
//!   Rejecting it as a *field* type collapses the transitive case (a struct all of
//!   whose fields are zero-sized is itself zero-sized) into the base case, so no
//!   value of a zero-sized type exists in an accepted program. *Declaring* a
//!   field-less struct stays legal: a field-less struct with associated functions
//!   is the supported method-namespace idiom (`E::helper()`), which needs no
//!   values. See [`rules::fieldless_struct_value`].
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

/// The facts about the artifact a program will be compiled into that some rules
/// must measure the program against.
///
/// A rule of this kind is not checking a property of the source alone: A036 asks
/// whether a call chain's frames fit the shadow stack the emitted module will
/// actually declare, which is a code generation setting. Carrying the setting
/// here is what lets the answer follow the build instead of a constant that has
/// to be kept in sync by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisOptions {
    /// The shadow-stack size in bytes A036 measures cumulative call-chain frame
    /// usage against. Must equal the stack region code generation emits for the
    /// same build, or the rule polices a budget the artifact does not have.
    pub stack_budget_bytes: u32,
}

/// Implemented by hand rather than derived: a derived `Default` would give a
/// zero-byte budget, under which every program that touches memory fails. The
/// value here is the stack region a default build emits.
impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            stack_budget_bytes: 65_536,
        }
    }
}

/// Performs static analysis on the typed AST under the default artifact
/// settings.
///
/// This is the *default-layout* entry point. A caller that configures the
/// memory layout code generation emits must call [`analyze_with_options`]
/// instead and pass the matching budget, or A036 polices a shadow stack the
/// artifact does not have — accepting a program that overflows a smaller stack,
/// or rejecting one a larger stack accommodates.
///
/// # Errors
///
/// See [`analyze_with_options`].
pub fn analyze(typed_context: &TypedContext) -> Result<AnalysisResult, AnalysisErrors> {
    analyze_with_options(typed_context, AnalysisOptions::default())
}

/// Performs static analysis on the typed AST under the given artifact settings.
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
pub fn analyze_with_options(
    typed_context: &TypedContext,
    options: AnalysisOptions,
) -> Result<AnalysisResult, AnalysisErrors> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut infos = Vec::new();
    for &r in rules::all_rules() {
        let findings = r.check(typed_context, options);
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
    use crate::errors::{AnalysisDiagnostic, NonDetBlockKind};
    use inference_ast::nodes::Location;

    fn dummy_location() -> Location {
        Location::default()
    }

    #[test]
    fn rule_ids_match_diagnostic_rule_ids() {
        let diagnostics: Vec<AnalysisDiagnostic> = vec![
            AnalysisDiagnostic::BreakOutsideLoop { location: dummy_location() },
            AnalysisDiagnostic::BreakInsideNonDetBlock { location: dummy_location(), block_kind: NonDetBlockKind::Forall },
            AnalysisDiagnostic::ReturnInsideLoop { location: dummy_location() },
            AnalysisDiagnostic::InfiniteLoopWithoutBreak { location: dummy_location() },
            AnalysisDiagnostic::ReturnInsideNonDetBlock { location: dummy_location(), block_kind: NonDetBlockKind::Forall },
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
            AnalysisDiagnostic::NonDetOutsideSpec { location: dummy_location(), block_kind: NonDetBlockKind::Forall },
            AnalysisDiagnostic::ReservedExportName { name: "memory".to_string(), location: dummy_location() },
            AnalysisDiagnostic::ShiftCountOutOfRange { value: "32".to_string(), type_name: "i32".to_string(), max: 31, location: dummy_location() },
            AnalysisDiagnostic::FieldLessStructValue { name: "E".to_string(), position: "a struct literal", location: dummy_location() },
            AnalysisDiagnostic::SpacedNegativeLiteral { value: "128".to_string(), location: dummy_location() },
            AnalysisDiagnostic::ExternWriteThroughImmutableArgument { arg: "arr".to_string(), param: "a".to_string(), callee: "sort_pair".to_string(), ty: "[i32; 2]".to_string(), root: errors::ImmutableArgumentRoot::Binding, location: dummy_location() },
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
