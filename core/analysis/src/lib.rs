#![warn(clippy::pedantic)]
//! Control Flow Analysis Pass for the Inference Compiler
//!
//! This crate provides semantic analysis that validates control flow invariants
//! beyond what the type checker covers. It operates on the fully-typed AST and
//! runs after type checking but before code generation.
//!
//! ## Current Analyses
//!
//! ### Loop Control Flow Validation
//!
//! - `break` must appear inside a loop body
//! - `break` must not appear inside a non-deterministic block
//! - `return` must not appear inside a loop body
//! - Infinite loops (`loop { ... }`) must contain a `break` statement
//! - `return` must not appear inside a non-deterministic block
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

pub mod errors;
pub mod rule;
pub mod rules;
mod walker;

use errors::{AnalysisErrors, AnalysisResult, Severity};

/// Performs control flow analysis on the typed AST.
///
/// Runs all registered analysis rules and collects errors. Currently includes:
/// - Loop control flow validation (break placement, return inside loop,
///   infinite loop detection)
///
/// # Errors
///
/// Returns `AnalysisErrors` if any control flow violations are found.
/// All errors are collected before returning, allowing the user to see
/// all issues at once.
pub fn analyze(typed_context: &TypedContext) -> Result<AnalysisResult, AnalysisErrors> {
    let all_rules = rules::all_rules();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut infos = Vec::new();
    for r in &all_rules {
        let findings = r.check(typed_context);
        match r.severity() {
            Severity::Error => errors.extend(findings),
            Severity::Warning => warnings.extend(findings),
            Severity::Info => infos.extend(findings),
        }
    }
    if errors.is_empty() {
        Ok(AnalysisResult { warnings, infos })
    } else {
        Err(AnalysisErrors::new(errors, warnings, infos))
    }
}
