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
use inference_type_checker::errors::TypeMismatchContext;
use thiserror::Error;

/// The note line appended to a range error, explaining where the literal's type
/// came from, or nothing when the literal kept the `i32` default.
fn literal_type_source_note(type_name: &str, source: Option<&TypeMismatchContext>) -> String {
    source.map_or_else(String::new, |source| {
        format!(
            "\nnote: the literal is typed `{type_name}` {}",
            source.literal_typing_reason()
        )
    })
}

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

    #[error("infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop does not count)")]
    InfiniteLoopWithoutBreak { location: Location },

    #[error("return statement is not allowed inside a '{block_kind}' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the '{block_kind}' block")]
    ReturnInsideNonDetBlock {
        location: Location,
        block_kind: &'static str,
    },

    #[error("uzumaki (@) is only valid inside a non-deterministic block (forall, exists, unique, assume); move it inside a non-deterministic block")]
    UzumakiOutsideNonDetBlock { location: Location },

    #[error("function `{function_name}` has return type but not all code paths return a value")]
    MissingReturn {
        function_name: String,
        location: Location,
    },

    #[error("uzumaki (@) used as a standalone expression has no effect; assign it to a variable or use it in a return statement")]
    StandaloneUzumaki { location: Location },

    #[error("enum `{name}` has no variants")]
    EmptyEnumDefinition { name: String, location: Location },

    #[error("method `{struct_name}::{method_name}` declares `self` but never accesses it; consider making it an associated function")]
    MethodNeverAccessesSelf {
        struct_name: String,
        method_name: String,
        location: Location,
    },

    #[error("struct `{name}` has no fields and no methods")]
    EmptyStructDefinition { name: String, location: Location },

    #[error("{kind} literal cannot be used directly as a function argument; assign to a variable first")]
    CompoundLiteralAsArgument {
        kind: &'static str,
        location: Location,
    },

    #[error("array uzumaki (@) cannot be used as a function argument; assign to a variable first")]
    ArrayUzumakiAsArgument { location: Location },

    #[error("{kind} literals can only be used in variable declarations, const initializers, assignments, return statements, or as struct field values")]
    CompoundLiteralInUnsupportedPosition {
        kind: &'static str,
        location: Location,
    },

    #[error("compound-returning function calls can only appear in `let` bindings or `return` statements; assign to a variable first")]
    CompoundReturnCallInExpressionPosition { location: Location },

    #[error("cannot assign from a compound-returning function call; use a new variable binding instead")]
    CompoundReturnCallInAssignment { location: Location },

    #[error("cannot chain method calls on compound-returning functions; assign the intermediate result to a variable first")]
    MethodCallChainOnCompoundReturn { location: Location },

    #[error("unreachable code after `{terminator}`")]
    DeadCode {
        terminator: &'static str,
        location: Location,
    },

    #[error("array index must be a 32-bit integer type, found `{found}`")]
    ArrayIndex64Bit { found: String, location: Location },

    /// The literal's type is usually written somewhere the literal is not — the
    /// annotation, parameter or return type of the position it appears in — so
    /// the note names that position.
    #[error(
        "literal `{value}` is out of range for type `{type_name}` (valid range: {min}..={max}){}",
        literal_type_source_note(type_name, type_source.as_ref())
    )]
    LiteralOutOfRange {
        value: String,
        type_name: String,
        min: i128,
        max: i128,
        type_source: Option<TypeMismatchContext>,
        location: Location,
    },

    #[error("uzumaki (@) can only appear in variable declarations or as function arguments; reassignment with @ is not allowed")]
    UzumakiInReassignment { location: Location },

    #[error("call to external function `{name}` is not supported in codegen; external functions cannot be compiled to WebAssembly yet")]
    ExternFunctionCall { name: String, location: Location },

    #[error("variable `{name}` must be initialized at declaration; use `let {name}: <type> = <value>;`")]
    UninitializedVariable { name: String, location: Location },

    #[error("struct `{outer}` field `{field}` has type `{ty}` which contains nested compound types; only one level of nesting is supported")]
    NestedCompoundDepthExceeded {
        outer: String,
        field: String,
        ty: String,
        location: Location,
    },

    #[error("uzumaki (@) cannot be assigned to struct `{name}` because it contains compound fields; uzumaki is only supported for structs whose fields are all scalars or scalar arrays")]
    UzumakiOnNestedStruct { name: String, location: Location },

    #[error("uzumaki (@) cannot be assigned to array of structs; arrays of structs do not support uzumaki")]
    UzumakiOnStructInArray { location: Location },

    #[error("uzumaki (@) cannot initialize field `{field}` of type `{ty}` because it is a struct or array; in a struct literal, uzumaki is only supported for scalar fields — initialize a compound field with a literal whose scalar leaves use @ (e.g. `Inner {{ v: @ }}`)")]
    UzumakiOnCompoundField { field: String, ty: String, location: Location },

    #[error("struct uzumaki (@) cannot be used as a function argument; assign to a variable first")]
    StructUzumakiAsArgument { location: Location },

    #[error("uzumaki (@) cannot initialize an array element of type `{ty}` because it is a struct or array; only scalar array elements may use @ — bind the value to a variable first, then use the variable as the element")]
    UzumakiOnCompoundArrayElement { ty: String, location: Location },

    #[error("compound literal cannot be assigned directly to a compound element; assign to a temporary variable first")]
    CompoundLiteralInCompoundAssign { location: Location },

    #[error("return expression in compound-returning function must be a variable, literal, function call, or field/element access; assign the expression to a temporary variable first")]
    UnsupportedCompoundReturnExpression { location: Location },

    #[error("top-level `const` declarations are not yet supported; declare `{name}` inside a function body, or track progress at https://github.com/Inferara/inference/issues/171")]
    TopLevelConstNotSupported { name: String, location: Location },

    #[error("combined unary operators are prohibited: `{op_outer}{op_inner}`; combining unary operators reduces readability and risks misinterpretation, use parentheses with a temporary variable instead")]
    CombinedUnaryOperators {
        op_outer: &'static str,
        op_inner: &'static str,
        location: Location,
    },

    #[error("visibility modifier `pub` on {def_kind} `{def_name}` inside spec `{spec_name}` has no effect; `spec` is the visibility unit, remove `pub`")]
    VisibilityInsideSpec {
        spec_name: String,
        def_name: String,
        def_kind: &'static str,
        location: Location,
    },

    #[error("recursive function call is not allowed: {cycle}; Inference forbids direct and indirect recursion (Power of 10, Rule 1) so stack usage stays statically bounded; restructure into an explicit loop")]
    RecursionDetected { cycle: String, location: Location },

    #[error("maximum stack depth {chain} uses {depth_bytes} bytes, exceeding the {budget_bytes}-byte stack; reduce array/struct frame sizes along this call chain")]
    StackDepthExceeded {
        chain: String,
        depth_bytes: u32,
        budget_bytes: u32,
        location: Location,
    },

    #[error("array index `{index}` is out of bounds for array of length {length}; valid indices are 0..{length}")]
    ArrayIndexConstOutOfBounds {
        index: String,
        length: u32,
        location: Location,
    },

    #[error("local `{name}` is already declared in this function (first declaration at {first_location}); each local name is introduced once per function body — rename one of them or hoist a single declaration above the blocks")]
    DuplicateLocalName {
        name: String,
        location: Location,
        first_location: Location,
    },

    #[error("non-deterministic '{block_kind}' block is only valid inside a spec declaration; non-deterministic constructs (forall, exists, assume, unique) describe specifications and cannot appear in executable code; move this logic into a function inside a `spec` declaration")]
    NonDetOutsideSpec {
        location: Location,
        block_kind: &'static str,
    },

    #[error("entry-file `pub fn {name}` collides with the reserved export name `{name}`; the compiled module reserves `memory` for its linear memory export and `__stack_pointer` for its stack pointer global, which WebAssembly hosts rely on; rename the function, or remove `pub` if it does not need to be exported")]
    ReservedExportName { name: String, location: Location },

    #[error("shift count `{value}` is out of range for type `{type_name}` (valid counts: 0..={max})")]
    ShiftCountOutOfRange {
        value: String,
        type_name: String,
        max: u32,
        location: Location,
    },

    #[error(
        "`{name}` is a struct with no fields, so it has no value representation and cannot be used as {position}; a field-less struct occupies zero bytes — there is no memory region to hold, copy, or reason about one of its values, so code generation has nothing to lower and a proof has nothing to describe; declaring a field-less struct stays legal — give `{name}` at least one field if you need values of it, or keep it as a pure namespace and declare its functions without `self`, calling them as `{name}::function_name()`"
    )]
    FieldLessStructValue {
        name: String,
        position: &'static str,
        location: Location,
    },

    /// The message quotes the fix rather than the offending spelling, because
    /// the gap may be a space, several, a newline, or a comment, and the advice
    /// is the same for all of them.
    #[error(
        "the minus sign is separated from the numeric literal `{value}`; a unary minus applied to a numeric literal must be written against the digits, so write `-{value}`; the sign is part of the literal token, and separating it leaves a negation of the bare `{value}`, which is then measured against the target type on its own — that is why the same value would otherwise compile or fail depending on the whitespace"
    )]
    SpacedNegativeLiteral { value: String, location: Location },
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
            | AnalysisDiagnostic::ReturnInsideNonDetBlock { location, .. }
            | AnalysisDiagnostic::UzumakiOutsideNonDetBlock { location }
            | AnalysisDiagnostic::MissingReturn { location, .. }
            | AnalysisDiagnostic::StandaloneUzumaki { location }
            | AnalysisDiagnostic::EmptyEnumDefinition { location, .. }
            | AnalysisDiagnostic::MethodNeverAccessesSelf { location, .. }
            | AnalysisDiagnostic::EmptyStructDefinition { location, .. }
            | AnalysisDiagnostic::CompoundLiteralAsArgument { location, .. }
            | AnalysisDiagnostic::ArrayUzumakiAsArgument { location }
            | AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { location, .. }
            | AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { location }
            | AnalysisDiagnostic::CompoundReturnCallInAssignment { location }
            | AnalysisDiagnostic::MethodCallChainOnCompoundReturn { location }
            | AnalysisDiagnostic::DeadCode { location, .. }
            | AnalysisDiagnostic::ArrayIndex64Bit { location, .. }
            | AnalysisDiagnostic::LiteralOutOfRange { location, .. }
            | AnalysisDiagnostic::UzumakiInReassignment { location }
            | AnalysisDiagnostic::ExternFunctionCall { location, .. }
            | AnalysisDiagnostic::UninitializedVariable { location, .. }
            | AnalysisDiagnostic::NestedCompoundDepthExceeded { location, .. }
            | AnalysisDiagnostic::UzumakiOnNestedStruct { location, .. }
            | AnalysisDiagnostic::UzumakiOnStructInArray { location, .. }
            | AnalysisDiagnostic::UzumakiOnCompoundField { location, .. }
            | AnalysisDiagnostic::StructUzumakiAsArgument { location }
            | AnalysisDiagnostic::UzumakiOnCompoundArrayElement { location, .. }
            | AnalysisDiagnostic::CompoundLiteralInCompoundAssign { location }
            | AnalysisDiagnostic::UnsupportedCompoundReturnExpression { location }
            | AnalysisDiagnostic::TopLevelConstNotSupported { location, .. }
            | AnalysisDiagnostic::CombinedUnaryOperators { location, .. }
            | AnalysisDiagnostic::VisibilityInsideSpec { location, .. }
            | AnalysisDiagnostic::RecursionDetected { location, .. }
            | AnalysisDiagnostic::StackDepthExceeded { location, .. }
            | AnalysisDiagnostic::ArrayIndexConstOutOfBounds { location, .. }
            | AnalysisDiagnostic::DuplicateLocalName { location, .. }
            | AnalysisDiagnostic::NonDetOutsideSpec { location, .. }
            | AnalysisDiagnostic::ReservedExportName { location, .. }
            | AnalysisDiagnostic::ShiftCountOutOfRange { location, .. }
            | AnalysisDiagnostic::FieldLessStructValue { location, .. }
            | AnalysisDiagnostic::SpacedNegativeLiteral { location, .. } => location,
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
            AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. } => "A006",
            AnalysisDiagnostic::MissingReturn { .. } => "A007",
            AnalysisDiagnostic::StandaloneUzumaki { .. } => "A008",
            AnalysisDiagnostic::EmptyEnumDefinition { .. } => "A009",
            AnalysisDiagnostic::MethodNeverAccessesSelf { .. } => "A010",
            AnalysisDiagnostic::EmptyStructDefinition { .. } => "A011",
            AnalysisDiagnostic::CompoundLiteralAsArgument { .. } => "A012",
            // A013: merged into A012 (CompoundLiteralAsArgument)
            AnalysisDiagnostic::ArrayUzumakiAsArgument { .. } => "A014",
            AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. } => "A015",
            AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. } => "A016",
            AnalysisDiagnostic::CompoundReturnCallInAssignment { .. } => "A017",
            AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. } => "A018",
            AnalysisDiagnostic::ArrayIndex64Bit { .. } => "A019",
            AnalysisDiagnostic::DeadCode { .. } => "A020",
            // A021: reserved for future use
            AnalysisDiagnostic::LiteralOutOfRange { .. } => "A022",
            AnalysisDiagnostic::UzumakiInReassignment { .. } => "A023",
            AnalysisDiagnostic::ExternFunctionCall { .. } => "A024",
            AnalysisDiagnostic::UninitializedVariable { .. } => "A025",
            AnalysisDiagnostic::NestedCompoundDepthExceeded { .. } => "A026",
            AnalysisDiagnostic::UzumakiOnNestedStruct { .. } => "A027",
            AnalysisDiagnostic::UzumakiOnStructInArray { .. } => "A028",
            AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. } => "A029",
            // A030: removed (multidimensional scalar array uzumaki is now supported at any depth)
            AnalysisDiagnostic::UnsupportedCompoundReturnExpression { .. } => "A031",
            AnalysisDiagnostic::TopLevelConstNotSupported { .. } => "A032",
            AnalysisDiagnostic::CombinedUnaryOperators { .. } => "A033",
            AnalysisDiagnostic::VisibilityInsideSpec { .. } => "A034",
            AnalysisDiagnostic::RecursionDetected { .. } => "A035",
            AnalysisDiagnostic::StackDepthExceeded { .. } => "A036",
            AnalysisDiagnostic::ArrayIndexConstOutOfBounds { .. } => "A037",
            AnalysisDiagnostic::UzumakiOnCompoundField { .. } => "A038",
            AnalysisDiagnostic::StructUzumakiAsArgument { .. } => "A039",
            AnalysisDiagnostic::UzumakiOnCompoundArrayElement { .. } => "A040",
            AnalysisDiagnostic::DuplicateLocalName { .. } => "A041",
            AnalysisDiagnostic::NonDetOutsideSpec { .. } => "A042",
            AnalysisDiagnostic::ReservedExportName { .. } => "A043",
            AnalysisDiagnostic::ShiftCountOutOfRange { .. } => "A044",
            AnalysisDiagnostic::FieldLessStructValue { .. } => "A045",
            AnalysisDiagnostic::SpacedNegativeLiteral { .. } => "A046",
        }
    }
}

/// An analysis diagnostic paired with the file it was produced in.
///
/// Source locations are per-file-local in the merged arena of a multi-file
/// program, so a bare `line:col` from an imported file would be misread as the
/// entry file. A rule attaches the defining file's `module_path` (empty for the
/// entry file) to every finding it produces; the rendered diagnostic then names
/// the file. The entry file stays a bare `line:col`, so single-file programs are
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledDiagnostic {
    /// Source-root-relative module path of the file the finding belongs to;
    /// empty for the entry file.
    pub module_path: Vec<String>,
    pub diagnostic: AnalysisDiagnostic,
}

impl LabeledDiagnostic {
    /// Pairs `diagnostic` with the file named by `module_path`.
    #[must_use]
    pub fn new(module_path: Vec<String>, diagnostic: AnalysisDiagnostic) -> Self {
        Self {
            module_path,
            diagnostic,
        }
    }

    /// Pairs `diagnostic` with the entry file (no module path). Used by rules
    /// whose findings are always entry-local, and by tests.
    #[must_use]
    pub fn entry(diagnostic: AnalysisDiagnostic) -> Self {
        Self {
            module_path: Vec::new(),
            diagnostic,
        }
    }
}

/// Renders a single finding as `[label:]line:col: severity[rule]: message`,
/// prefixing the file label for non-entry files via the shared spelling.
fn write_finding(
    f: &mut Formatter<'_>,
    module_path: &[String],
    diagnostic: &AnalysisDiagnostic,
    severity: Severity,
) -> fmt::Result {
    let location = diagnostic.location();
    match inference_ast::nodes::file_label(module_path) {
        Some(label) => write!(
            f,
            "{label}:{location}: {severity}[{}]: {diagnostic}",
            diagnostic.rule_id()
        ),
        None => write!(
            f,
            "{location}: {severity}[{}]: {diagnostic}",
            diagnostic.rule_id()
        ),
    }
}

/// Orders findings for display: by file (canonical arena order — entry first,
/// then lexicographic module path), then by line and column within a file.
///
/// Sorting by location alone is wrong across files, because per-file-local
/// offsets collide; the file key disambiguates so a multi-file report reads
/// file-by-file rather than interleaving same-numbered lines from different
/// files.
fn finding_sort_key(module_path: &[String], diagnostic: &AnalysisDiagnostic) -> (Vec<String>, u32, u32) {
    let location = diagnostic.location();
    (
        module_path.to_vec(),
        location.start_line,
        location.start_column,
    )
}

/// The file (module path) each finding in a severity-bucketed collection belongs
/// to, index-aligned with the collection's diagnostic vectors.
///
/// Stored behind a single [`Box`] on the owning collection so the diagnostics
/// stay directly sliceable for the bare-diagnostic accessors, while the file
/// labels add only one pointer to the owning struct (keeping the error type
/// small enough to return by value).
#[derive(Debug, Clone, Default)]
struct DiagnosticFiles {
    errors: Vec<Vec<String>>,
    warnings: Vec<Vec<String>>,
    infos: Vec<Vec<String>>,
}

/// Splits labeled findings into a diagnostic vector and an index-aligned
/// module-path vector. The two stay aligned so the bare-diagnostic accessors and
/// the file-named rendering describe the same findings.
fn split_labeled(labeled: Vec<LabeledDiagnostic>) -> (Vec<AnalysisDiagnostic>, Vec<Vec<String>>) {
    let mut diagnostics = Vec::with_capacity(labeled.len());
    let mut module_paths = Vec::with_capacity(labeled.len());
    for item in labeled {
        diagnostics.push(item.diagnostic);
        module_paths.push(item.module_path);
    }
    (diagnostics, module_paths)
}

/// Wrapper for multiple analysis errors, following the `TypeCheckErrors` pattern.
///
/// Collects all analysis errors found during a single pass, allowing the user
/// to see all issues at once rather than fixing one error at a time.
/// Also carries any warnings and infos found alongside the errors.
///
/// Each severity bucket stores the bare diagnostics directly (so the accessors
/// can hand out a slice) and the file each finding belongs to in a single boxed
/// [`DiagnosticFiles`] for file-named rendering.
#[derive(Debug, Clone)]
pub struct AnalysisErrors {
    errors: Vec<AnalysisDiagnostic>,
    warnings: Vec<AnalysisDiagnostic>,
    infos: Vec<AnalysisDiagnostic>,
    files: Box<DiagnosticFiles>,
}

impl AnalysisErrors {
    pub(crate) fn new(
        errors: Vec<LabeledDiagnostic>,
        warnings: Vec<LabeledDiagnostic>,
        infos: Vec<LabeledDiagnostic>,
    ) -> Self {
        assert!(!errors.is_empty(), "AnalysisErrors must contain at least one error");
        let (errors, error_files) = split_labeled(errors);
        let (warnings, warning_files) = split_labeled(warnings);
        let (infos, info_files) = split_labeled(infos);
        Self {
            errors,
            warnings,
            infos,
            files: Box::new(DiagnosticFiles {
                errors: error_files,
                warnings: warning_files,
                infos: info_files,
            }),
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
        render_findings(
            f,
            &[
                (&self.infos, &self.files.infos, Severity::Info),
                (&self.warnings, &self.files.warnings, Severity::Warning),
                (&self.errors, &self.files.errors, Severity::Error),
            ],
        )
    }
}

/// One severity bucket for rendering: the diagnostics, the index-aligned module
/// paths naming each finding's file, and the severity to print.
type FindingBucket<'a> = (&'a [AnalysisDiagnostic], &'a [Vec<String>], Severity);

/// Renders findings from severity buckets, sorted by file then line/column, each
/// prefixed with its file label. Shared by [`AnalysisErrors`] and
/// [`AnalysisResult`] so both channels format identically.
fn render_findings(f: &mut Formatter<'_>, buckets: &[FindingBucket]) -> fmt::Result {
    let mut all: Vec<(&[String], &AnalysisDiagnostic, Severity)> = Vec::new();
    for (diagnostics, files, severity) in buckets {
        // The diagnostic and file vectors are split index-aligned in `new`; if a
        // future change broke that, `zip` would silently drop the longer tail and
        // mislabel findings, so guard the invariant where they are consumed.
        debug_assert_eq!(
            diagnostics.len(),
            files.len(),
            "diagnostic and file-label vectors must stay index-aligned"
        );
        for (diagnostic, module_path) in diagnostics.iter().zip(files.iter()) {
            all.push((module_path, diagnostic, *severity));
        }
    }
    all.sort_by_key(|(module_path, diagnostic, _)| finding_sort_key(module_path, diagnostic));
    let mut first = true;
    for (module_path, diagnostic, severity) in &all {
        if !first {
            writeln!(f)?;
        }
        write_finding(f, module_path, diagnostic, *severity)?;
        first = false;
    }
    Ok(())
}

impl std::error::Error for AnalysisErrors {}

/// Holds non-fatal analysis findings (warnings and informational messages).
///
/// Returned from `analyze()` when no hard errors are found, allowing the
/// compilation pipeline to continue while still reporting lesser findings.
///
/// Like [`AnalysisErrors`], stores the bare diagnostics directly (for the
/// accessors) plus a single boxed [`DiagnosticFiles`] naming each finding's file
/// for file-named rendering.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    warnings: Vec<AnalysisDiagnostic>,
    infos: Vec<AnalysisDiagnostic>,
    files: Box<DiagnosticFiles>,
}

impl AnalysisResult {
    pub(crate) fn new(warnings: Vec<LabeledDiagnostic>, infos: Vec<LabeledDiagnostic>) -> Self {
        let (warnings, warning_files) = split_labeled(warnings);
        let (infos, info_files) = split_labeled(infos);
        Self {
            warnings,
            infos,
            files: Box::new(DiagnosticFiles {
                errors: Vec::new(),
                warnings: warning_files,
                infos: info_files,
            }),
        }
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

    /// Returns true if there are any warnings or informational findings.
    #[must_use = "returns whether any warnings or informational findings exist"]
    pub fn has_findings(&self) -> bool {
        !self.warnings.is_empty() || !self.infos.is_empty()
    }
}

impl Display for AnalysisResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        render_findings(
            f,
            &[
                (&self.infos, &self.files.infos, Severity::Info),
                (&self.warnings, &self.files.warnings, Severity::Warning),
            ],
        )
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

    /// A literal's type is written where the literal is not, so the range
    /// error carries the position that supplied it.
    #[test]
    fn display_literal_out_of_range_note() {
        let with_source = AnalysisDiagnostic::LiteralOutOfRange {
            value: "300".to_string(),
            type_name: "u8".to_string(),
            min: 0,
            max: 255,
            type_source: Some(TypeMismatchContext::Return),
            location: test_location(),
        };
        assert_eq!(
            with_source.to_string(),
            "literal `300` is out of range for type `u8` (valid range: 0..=255)\n\
             note: the literal is typed `u8` by the type expected in return statement"
        );
    }

    /// A literal nothing typed has no position to name, and the message is the
    /// one it has always been.
    #[test]
    fn display_literal_out_of_range_without_a_source() {
        let without_source = AnalysisDiagnostic::LiteralOutOfRange {
            value: "300".to_string(),
            type_name: "u8".to_string(),
            min: 0,
            max: 255,
            type_source: None,
            location: test_location(),
        };
        assert_eq!(
            without_source.to_string(),
            "literal `300` is out of range for type `u8` (valid range: 0..=255)"
        );
        assert!(!without_source.to_string().contains("note:"));
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
            "infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop does not count)"
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
        assert_eq!(
            AnalysisDiagnostic::StackDepthExceeded {
                chain: "a -> b".to_string(),
                depth_bytes: 80_000,
                budget_bytes: 65_536,
                location: test_location(),
            }
            .rule_id(),
            "A036"
        );
    }

    #[test]
    fn display_analysis_errors_single() {
        let errors = AnalysisErrors::new(
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            })],
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
                LabeledDiagnostic::entry(AnalysisDiagnostic::BreakOutsideLoop {
                    location: test_location(),
                }),
                LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                    location: Location {
                        offset_start: 20,
                        offset_end: 30,
                        start_line: 3,
                        start_column: 10,
                        end_line: 3,
                        end_column: 20,
                    },
                }),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            errors.to_string(),
            "1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'\n3:10: error[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    /// A finding from a non-entry file is prefixed with the file's `::`-joined
    /// module path, while an entry-file finding stays a bare `line:col`. The
    /// sort places the entry file first, then by module path.
    #[test]
    fn display_analysis_errors_names_non_entry_file() {
        let errors = AnalysisErrors::new(
            vec![
                LabeledDiagnostic::new(
                    vec!["lib".to_string(), "geom".to_string()],
                    AnalysisDiagnostic::BreakOutsideLoop {
                        location: test_location(),
                    },
                ),
                LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                    location: test_location(),
                }),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            errors.to_string(),
            "1:5: error[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it\nlib::geom:1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'"
        );
    }

    /// Two findings at the same line:col in different imported files render
    /// distinguishably, each named by its own file.
    #[test]
    fn display_analysis_errors_distinguishes_same_location_in_two_files() {
        let errors = AnalysisErrors::new(
            vec![
                LabeledDiagnostic::new(
                    vec!["lib".to_string(), "a".to_string()],
                    AnalysisDiagnostic::BreakOutsideLoop {
                        location: test_location(),
                    },
                ),
                LabeledDiagnostic::new(
                    vec!["lib".to_string(), "b".to_string()],
                    AnalysisDiagnostic::BreakOutsideLoop {
                        location: test_location(),
                    },
                ),
            ],
            vec![],
            vec![],
        );
        let rendered = errors.to_string();
        assert!(
            rendered.contains("lib::a:1:5: error[A001]:"),
            "expected lib::a to be named, got: {rendered}"
        );
        assert!(
            rendered.contains("lib::b:1:5: error[A001]:"),
            "expected lib::b to be named, got: {rendered}"
        );
    }

    #[test]
    fn display_analysis_result_empty() {
        let result = AnalysisResult::new(vec![], vec![]);
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
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            })],
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                location: Location {
                    offset_start: 20,
                    offset_end: 30,
                    start_line: 3,
                    start_column: 10,
                    end_line: 3,
                    end_column: 20,
                },
            })],
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
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::BreakOutsideLoop {
                location: test_location(),
            })],
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            })],
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::InfiniteLoopWithoutBreak {
                location: test_location(),
            })],
        );
        // All at same location 1:5, so stable order within same location depends on push order:
        // infos first, then warnings, then errors
        assert_eq!(
            errors.to_string(),
            "1:5: info[A004]: infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop does not count)\n1:5: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it\n1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'"
        );
    }

    #[test]
    fn display_analysis_result_with_warning() {
        let result = AnalysisResult::new(
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            })],
            vec![],
        );
        assert_eq!(
            result.to_string(),
            "1:5: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    /// A warning from a non-entry file is named by the file in the
    /// `AnalysisResult` channel too, matching the error channel.
    #[test]
    fn display_analysis_result_names_non_entry_file() {
        let result = AnalysisResult::new(
            vec![LabeledDiagnostic::new(
                vec!["lib".to_string(), "geom".to_string()],
                AnalysisDiagnostic::ReturnInsideLoop {
                    location: test_location(),
                },
            )],
            vec![],
        );
        assert_eq!(
            result.to_string(),
            "lib::geom:1:5: warning[A003]: return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
        );
    }

    #[test]
    fn has_findings_returns_false_when_empty() {
        let result = AnalysisResult::new(vec![], vec![]);
        assert!(!result.has_findings());
    }

    #[test]
    fn has_findings_returns_true_with_warning() {
        let result = AnalysisResult::new(
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::ReturnInsideLoop {
                location: test_location(),
            })],
            vec![],
        );
        assert!(result.has_findings());
    }

    #[test]
    fn has_findings_returns_true_with_info() {
        let result = AnalysisResult::new(
            vec![],
            vec![LabeledDiagnostic::entry(AnalysisDiagnostic::InfiniteLoopWithoutBreak {
                location: test_location(),
            })],
        );
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
    fn display_compound_literal_in_unsupported_position_lists_const_initializer() {
        let err = AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition {
            kind: "array",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("const initializer"),
            "A015 diagnostic must mention `const initializers` among permitted positions, got: {text}"
        );
        assert!(
            text.contains("variable declarations"),
            "A015 diagnostic must mention variable declarations, got: {text}"
        );
    }

    #[test]
    fn display_top_level_const_not_supported() {
        let err = AnalysisDiagnostic::TopLevelConstNotSupported {
            name: "X".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("top-level `const`"),
            "A032 diagnostic must mention top-level const, got: {text}"
        );
        assert!(
            text.contains('X'),
            "A032 diagnostic must include the constant name, got: {text}"
        );
        assert!(
            text.contains("inside a function body"),
            "A032 diagnostic must suggest declaring inside a function body, got: {text}"
        );
    }

    #[test]
    fn display_combined_unary_operators() {
        let err = AnalysisDiagnostic::CombinedUnaryOperators {
            op_outer: "-",
            op_inner: "~",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("combined unary operators are prohibited"),
            "A033 diagnostic must explain the prohibition, got: {text}"
        );
        assert!(
            text.contains("-~"),
            "A033 diagnostic must include the combined operator glyphs, got: {text}"
        );
        assert!(
            text.contains("temporary variable"),
            "A033 diagnostic must suggest using a temporary variable, got: {text}"
        );
    }

    #[test]
    fn display_visibility_inside_spec() {
        let err = AnalysisDiagnostic::VisibilityInsideSpec {
            spec_name: "MySpec".to_string(),
            def_name: "do_thing".to_string(),
            def_kind: "fn",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("MySpec"),
            "A034 diagnostic must include the spec name, got: {text}"
        );
        assert!(
            text.contains("do_thing"),
            "A034 diagnostic must include the inner definition name, got: {text}"
        );
        assert!(
            text.contains("fn"),
            "A034 diagnostic must include the definition kind, got: {text}"
        );
        assert!(
            text.contains("`pub`"),
            "A034 diagnostic must reference the `pub` modifier, got: {text}"
        );
        assert_eq!(err.rule_id(), "A034");
    }

    #[test]
    fn display_recursion_detected() {
        let err = AnalysisDiagnostic::RecursionDetected {
            cycle: "fact -> fact".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("recursive function call is not allowed"),
            "A035 diagnostic must explain the prohibition, got: {text}"
        );
        assert!(
            text.contains("fact -> fact"),
            "A035 diagnostic must include the cycle chain, got: {text}"
        );
        assert!(
            text.contains("Power of 10"),
            "A035 diagnostic must cite Power of 10, got: {text}"
        );
        assert_eq!(err.rule_id(), "A035");
    }

    #[test]
    fn display_stack_depth_exceeded() {
        let err = AnalysisDiagnostic::StackDepthExceeded {
            chain: "main -> work -> alloc".to_string(),
            depth_bytes: 98_304,
            budget_bytes: 65_536,
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("main -> work -> alloc"),
            "A036 diagnostic must include the call chain, got: {text}"
        );
        assert!(
            text.contains("98304"),
            "A036 diagnostic must include the depth in bytes, got: {text}"
        );
        assert!(
            text.contains("65536"),
            "A036 diagnostic must include the budget in bytes, got: {text}"
        );
        assert_eq!(err.rule_id(), "A036");
    }

    #[test]
    fn display_array_index_const_out_of_bounds() {
        let err = AnalysisDiagnostic::ArrayIndexConstOutOfBounds {
            index: "3".to_string(),
            length: 3,
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("out of bounds"),
            "A037 diagnostic must say the index is out of bounds, got: {text}"
        );
        assert!(
            text.contains('3'),
            "A037 diagnostic must include the offending index and length, got: {text}"
        );
        assert!(
            text.contains("length 3"),
            "A037 diagnostic must include the array length, got: {text}"
        );
        assert_eq!(err.rule_id(), "A037");
    }

    #[test]
    fn display_array_index_const_out_of_bounds_negative() {
        let err = AnalysisDiagnostic::ArrayIndexConstOutOfBounds {
            index: "-1".to_string(),
            length: 5,
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("-1"),
            "A037 diagnostic must include a negative index verbatim, got: {text}"
        );
        assert!(
            text.contains("length 5"),
            "A037 diagnostic must include the array length, got: {text}"
        );
    }

    #[test]
    fn display_uzumaki_on_compound_field() {
        let err = AnalysisDiagnostic::UzumakiOnCompoundField {
            field: "i".to_string(),
            ty: "Inner".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("`i`"),
            "A038 diagnostic must name the offending field, got: {text}"
        );
        assert!(
            text.contains("Inner"),
            "A038 diagnostic must include the field type, got: {text}"
        );
        assert!(
            text.contains("scalar"),
            "A038 diagnostic must explain uzumaki is only for scalar fields, got: {text}"
        );
        assert_eq!(err.rule_id(), "A038");
    }

    #[test]
    fn display_struct_uzumaki_as_argument() {
        let err = AnalysisDiagnostic::StructUzumakiAsArgument {
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("function argument"),
            "A039 diagnostic must say it cannot be used as a function argument, got: {text}"
        );
        assert!(
            text.contains("assign to a variable"),
            "A039 diagnostic must suggest assigning to a variable first, got: {text}"
        );
        assert_eq!(err.rule_id(), "A039");
    }

    #[test]
    fn display_uzumaki_on_compound_array_element() {
        let err = AnalysisDiagnostic::UzumakiOnCompoundArrayElement {
            ty: "Point".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("Point"),
            "A040 diagnostic must include the element type, got: {text}"
        );
        assert!(
            text.contains("scalar"),
            "A040 diagnostic must explain only scalar array elements may use @, got: {text}"
        );
        assert_eq!(err.rule_id(), "A040");
    }

    #[test]
    fn display_duplicate_local_name() {
        let first_location = Location {
            offset_start: 0,
            offset_end: 5,
            start_line: 2,
            start_column: 9,
            end_line: 2,
            end_column: 14,
        };
        let err = AnalysisDiagnostic::DuplicateLocalName {
            name: "x".to_string(),
            location: test_location(),
            first_location,
        };
        let text = err.to_string();
        assert!(
            text.contains("local `x` is already declared"),
            "A041 diagnostic must name the duplicated local, got: {text}"
        );
        assert!(
            text.contains("first declaration at 2:9"),
            "A041 diagnostic must cite the first declaration's location, got: {text}"
        );
        assert!(
            text.contains("rename one of them or hoist"),
            "A041 diagnostic must give the rename-or-hoist guidance, got: {text}"
        );
        assert!(
            !text.contains("shadow"),
            "A041 diagnostic must not use shadowing terminology, got: {text}"
        );
        assert_eq!(err.rule_id(), "A041");
    }

    #[test]
    fn display_non_det_outside_spec() {
        let err = AnalysisDiagnostic::NonDetOutsideSpec {
            location: test_location(),
            block_kind: "forall",
        };
        let text = err.to_string();
        assert!(
            text.contains("'forall' block"),
            "A042 diagnostic must name the offending block kind, got: {text}"
        );
        assert!(
            text.contains("spec declaration"),
            "A042 diagnostic must point to a spec declaration, got: {text}"
        );
        assert!(
            text.contains("forall, exists, assume, unique"),
            "A042 diagnostic must enumerate the non-deterministic constructs, got: {text}"
        );
        assert_eq!(err.rule_id(), "A042");
    }

    #[test]
    fn display_non_det_outside_spec_names_each_kind() {
        for kind in ["forall", "exists", "assume", "unique"] {
            let err = AnalysisDiagnostic::NonDetOutsideSpec {
                location: test_location(),
                block_kind: kind,
            };
            assert!(
                err.to_string().contains(&format!("'{kind}' block")),
                "A042 diagnostic must name the `{kind}` block kind"
            );
        }
    }

    #[test]
    fn display_reserved_export_name() {
        let err = AnalysisDiagnostic::ReservedExportName {
            name: "memory".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        // The leading fragment pins the OFFENDING name; the bare word "memory"
        // also appears later in the why-clause, so match the full phrase.
        assert!(
            text.contains("entry-file `pub fn memory` collides"),
            "A043 diagnostic must name the offending function, got: {text}"
        );
        assert!(
            text.contains("reserved export name"),
            "A043 diagnostic must explain the name is reserved, got: {text}"
        );
        assert!(
            text.contains("`__stack_pointer`"),
            "A043 diagnostic must explain both reserved names, got: {text}"
        );
        assert!(
            text.contains("rename the function"),
            "A043 diagnostic must suggest renaming the function, got: {text}"
        );
        assert!(
            text.contains("remove `pub`"),
            "A043 diagnostic must suggest removing `pub`, got: {text}"
        );
        assert_eq!(err.rule_id(), "A043");
    }

    #[test]
    fn display_shift_count_out_of_range() {
        let err = AnalysisDiagnostic::ShiftCountOutOfRange {
            value: "32".to_string(),
            type_name: "i32".to_string(),
            max: 31,
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("shift count `32`"),
            "A044 diagnostic must name the offending count, got: {text}"
        );
        assert!(
            text.contains("type `i32`"),
            "A044 diagnostic must name the operand type, got: {text}"
        );
        assert!(
            text.contains("0..=31"),
            "A044 diagnostic must state the valid count range, got: {text}"
        );
        assert_eq!(err.rule_id(), "A044");
    }

    /// The A045 message must carry the whole teaching contract: what is wrong,
    /// why (both the memory mechanism and the verification consequence), that
    /// declarations are not banned, and the two fixes.
    #[test]
    fn display_fieldless_struct_value() {
        let err = AnalysisDiagnostic::FieldLessStructValue {
            name: "E".to_string(),
            position: "a struct literal",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("`E` is a struct with no fields"),
            "A045 diagnostic must name the offending struct, got: {text}"
        );
        assert!(
            text.contains("no value representation"),
            "A045 diagnostic must state what is wrong, got: {text}"
        );
        assert!(
            text.contains("zero bytes"),
            "A045 diagnostic must explain the mechanism, got: {text}"
        );
        assert!(
            text.contains("a proof has nothing to describe"),
            "A045 diagnostic must state the verification consequence, got: {text}"
        );
        assert!(
            text.contains("declaring a field-less struct stays legal"),
            "A045 diagnostic must say declarations are not banned, got: {text}"
        );
        assert!(
            text.contains("at least one field"),
            "A045 diagnostic must suggest giving the struct a field, got: {text}"
        );
        assert!(
            text.contains("without `self`"),
            "A045 diagnostic must suggest the namespace fix, got: {text}"
        );
        assert!(
            text.contains("`E::function_name()`"),
            "A045 diagnostic must show how a namespace function is called, got: {text}"
        );
        assert_eq!(err.rule_id(), "A045");
    }

    /// One variant serves every position, so each position string must render
    /// into the same sentence.
    #[test]
    fn display_fieldless_struct_value_names_each_position() {
        for position in [
            "a struct literal",
            "the declared type of a variable",
            "the type of a parameter",
            "the return type of a function",
            "the type of a struct field",
            "the type of a `self` receiver",
        ] {
            let err = AnalysisDiagnostic::FieldLessStructValue {
                name: "E".to_string(),
                position,
                location: test_location(),
            };
            assert!(
                err.to_string()
                    .contains(&format!("cannot be used as {position};")),
                "A045 diagnostic must name the `{position}` position"
            );
        }
    }

    /// The A046 message has one job the rule cannot do for the reader: spell the
    /// glued form out. It must also say why, so the requirement does not read as
    /// arbitrary style.
    #[test]
    fn display_spaced_negative_literal() {
        let err = AnalysisDiagnostic::SpacedNegativeLiteral {
            value: "128".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("separated from the numeric literal `128`"),
            "A046 diagnostic must name the literal, got: {text}"
        );
        assert!(
            text.contains("write `-128`"),
            "A046 diagnostic must spell the fix out, got: {text}"
        );
        assert!(
            text.contains("part of the literal token"),
            "A046 diagnostic must explain the mechanism, got: {text}"
        );
        assert!(
            text.contains("depending on the whitespace"),
            "A046 diagnostic must say why the separated spelling is rejected, got: {text}"
        );
        assert_eq!(err.rule_id(), "A046");
    }

    /// The fix is built from the literal's own text, so it has to track it.
    #[test]
    fn display_spaced_negative_literal_quotes_each_value() {
        for value in ["42", "128", "9223372036854775808"] {
            let err = AnalysisDiagnostic::SpacedNegativeLiteral {
                value: value.to_string(),
                location: test_location(),
            };
            assert!(
                err.to_string().contains(&format!("write `-{value}`")),
                "A046 diagnostic must recommend the glued form of `{value}`"
            );
        }
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
