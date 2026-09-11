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

use inference_ast::nodes::{ArithMode, BlockKind, GuardedOp, Location};
use inference_type_checker::errors::TypeMismatchContext;
use inference_type_checker::type_info::NumberType;
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

/// How the binding an A047 argument is rooted at was declared, which is what
/// decides the repair the message can offer.
///
/// The two forms are both immutable, and only one of them can be made mutable
/// where it stands. Collapsing them would leave the message naming `const mut`,
/// a declaration the grammar rejects, with no second sentence to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImmutableArgumentRoot {
    /// A `let` binding, a parameter, or a `self` receiver — or an argument with
    /// no binding behind it at all, which another rule already rejects and whose
    /// applicable advice is the second half of the same sentence. The
    /// declaration has a `mut` slot, so asking for it is the direct repair.
    Binding,
    /// A `const`. Immutable in a form no `mut` can relax, so the only repair is
    /// a separate `mut` binding initialized from it.
    Constant,
}

/// The repair clause of [`AnalysisDiagnostic::ExternWriteThroughImmutableArgument`],
/// which differs by how the argument's root is declared.
fn extern_mut_argument_fix(arg: &str, root: ImmutableArgumentRoot) -> String {
    match root {
        ImmutableArgumentRoot::Binding => format!(
            "`{arg}` is not declared `mut`, so its value must not change across the call; declare \
             it `mut {arg}` where it is bound, or copy it into a `mut` binding and pass that"
        ),
        ImmutableArgumentRoot::Constant => format!(
            "`{arg}` is a `const`, so its value must not change across the call, and `const mut` \
             is not a declaration the grammar accepts; copy `{arg}` into a `mut` binding and pass \
             that instead"
        ),
    }
}

/// The repair clause of [`AnalysisDiagnostic::UnitAsValue`], which differs by
/// the position the unit reached.
///
/// A declaration is repaired by editing or deleting the declaration; an
/// expression standing where a value was required has no declaration to edit,
/// so the same advice would send the author looking for one that is not there.
/// Reading the position rather than a yes/no keeps the two apart, and ties the
/// choice to the one position constant that names an expression.
fn unit_as_value_fix(position: &str) -> &'static str {
    if position == crate::rules::position::VALUE {
        "drop the `()`, or write a value the surrounding expression can use"
    } else {
        "remove the declaration, or give it a type that carries a value"
    }
}

/// Which generic surface an [`AnalysisDiagnostic::GenericNotSupported`] finding
/// stands on, which is what decides the fact the message states.
///
/// The two sites are refused for different reasons and must never share a
/// rationale sentence. A declaration is unlowerable because the compiler does
/// not monomorphize, which is a missing implementation; an application of type
/// arguments is unlowerable because no type declaration in the language accepts
/// any, which is a property of the language and stays true whatever
/// monomorphization does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericSite {
    /// A function, method, or `spec` function declaring type parameters.
    /// `function` arrives pre-rendered (`Struct::method` for a method) because
    /// the diagnostic has no arena to resolve a receiver against; `params` is
    /// the binder list as the source spells it, `T' U'`.
    Declaration { function: String, params: String },
    /// A type application (`Q i32'`) written where a type is expected, or
    /// standing on its own in expression position. `rendered` is the
    /// application as the source spells it.
    TypeApplication {
        rendered: String,
        position: &'static str,
    },
}

/// Which mode an [`AnalysisDiagnostic::ArithModeChangesNothing`] finding is
/// redundant against.
///
/// The two need different sentences and there is no shared one to parameterize.
/// A redundancy against the language's default is about a rule the author cannot
/// see in the source, so the message states it and names the other spelling as
/// the way to opt out; a redundancy against an enclosing annotation is about
/// text three characters away, and any sentence about the default would be false
/// of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedundantArithMode {
    /// No annotation encloses the finding: it names the mode unannotated
    /// arithmetic already has.
    AgainstTheDefault,
    /// An annotation of the same mode encloses the finding.
    InsideTheSameAnnotation,
}

/// The constants the operands of an [`AnalysisDiagnostic::ConstantArithmeticOverflow`]
/// operation folded to.
///
/// Which arm it is decides whether the message speaks of one constant or two,
/// and that is not cosmetic: the reader has to be told that each operand fits
/// the type and the operation does not, and a sentence about two operands
/// written over a negation describes an expression nobody wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldedOperands {
    /// A unary `-`, whose single operand folded to this value.
    Unary(i128),
    /// A binary `+`, `-` or `*`, whose operands folded to these values.
    Binary(i128, i128),
}

/// The exact result of an [`AnalysisDiagnostic::ConstantArithmeticOverflow`]
/// operation, in the mathematical integers.
///
/// `i128` holds every result the language's widths can produce but one: two
/// `u64` operands, each inside `0..=18446744073709551615`, can have a product as
/// large as `(2^64 - 1)^2`, which is above `i128::MAX`. Nothing else comes near
/// it — a product that large needs both magnitudes above `2^63`, and `u64` is
/// the only width that reaches there, while every sum, difference and negation
/// of two values from any width stays below `2^65` — so one wide arm completes
/// the type, and a `u64` value being non-negative is what makes that arm a
/// `u128` rather than something signed.
///
/// The wide case is a value rather than an absence because of what it proves: a
/// product too large for the fold's own arithmetic is an overflow of every width
/// there is, so the message names it instead of the rule falling silent on the
/// largest overflow it can be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactValue {
    /// A result `i128` holds, which is every result but the product below.
    Narrow(i128),
    /// A product of two non-negative operands above `i128::MAX`.
    Wide(u128),
}

impl Display for ExactValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExactValue::Narrow(value) => Display::fmt(value, f),
            ExactValue::Wide(value) => Display::fmt(value, f),
        }
    }
}

/// The message of [`AnalysisDiagnostic::ConstantArithmeticOverflow`].
///
/// Everything it states is derived from the fold, so the message cannot claim
/// something the rule did not establish. The clause naming the operand values
/// and asserting that each is in range is the one that keeps this readable as
/// what it is rather than as a complaint about an operand: a reader of
/// `let x: u8 = 200 + 100;` is told, in the message, that `200` and `100` are
/// both `u8`s and that the addition is not. A literal that is itself out of
/// range never reaches here — that finding belongs to A022, which owns it — so
/// the claim holds by construction.
///
/// The wrapped value is quoted only in the remedy. Under a default that traps,
/// stating it as what the type "holds" would be false: nothing holds anything,
/// the program stops.
fn constant_arithmetic_overflow_message(
    expression: &str,
    operands: FoldedOperands,
    op: GuardedOp,
    number: NumberType,
    exact: ExactValue,
    wrapped: i128,
) -> String {
    let range = number.range();
    let (min, max) = (range.start(), range.end());
    let ty = number.as_str();
    let spelling = op.spelling();
    let (folds, repair) = match operands {
        FoldedOperands::Binary(lhs, rhs) => (
            format!(
                "its operands fold to the constants `{lhs}` and `{rhs}`, each of which is itself \
                 within `{min}..={max}`, and it is their `{spelling}` that is not"
            ),
            "the operands or the declared type have to change",
        ),
        FoldedOperands::Unary(value) => (
            format!(
                "its operand folds to the constant `{value}`, which is itself within \
                 `{min}..={max}`, and it is the `{spelling}` applied to it that is not"
            ),
            "the operand or the declared type has to change",
        ),
    };
    format!(
        "`{expression}` overflows `{ty}` before the program runs; {folds} — the true result \
         `{exact}` is outside that range — so this is not a value the program computes but a \
         trap it takes on every run that reaches it, and no `assume`, envelope or specification \
         can recover a result the type cannot hold: if the wrap is what you meant, write \
         `wrapping({expression})`, which computes `{wrapped}` and does not trap, and otherwise \
         {repair}"
    )
}

/// The message of [`AnalysisDiagnostic::ArithModeGovernsNothing`].
///
/// Two wordings, because the two spellings fail differently: a `wrapping(...)`
/// with nothing inside it is a no-op, and a `checked(...)` is a guarantee a
/// later reader will believe. Both name the two shapes that produce almost every
/// instance — a call, whose body the annotation does not reach, and a glued
/// negative literal, which is one token rather than a negation — because neither
/// is visible from the site the rule fires on.
fn arith_mode_governs_nothing_message(mode: ArithMode) -> String {
    let common = "nothing written inside it is a `+`, `-`, `*` or unary `-` at a type that can \
                  overflow";
    let reach = "the annotation governs the operators written between its own parentheses and \
                 nothing beyond them, so it does not reach into the body of a function called \
                 inside it, and a negative number such as `-2147483648` is a single literal \
                 carrying its own sign rather than a negation applied to a value";
    match mode {
        ArithMode::Checked => format!(
            "this `checked(...)` has no arithmetic to check; {common}, so it emits no guard at \
             all and a reader who takes it as a guarantee about this expression would be wrong: \
             {reach}; move it to the expression whose arithmetic you meant, or delete it"
        ),
        ArithMode::Wrapping => format!(
            "this `wrapping(...)` has no arithmetic to change; {common}, so every operator that \
             could have wrapped is somewhere else: {reach}; move it to the expression whose \
             arithmetic you meant, marking that function's own operators if the wrapping belongs \
             there, or delete it"
        ),
    }
}

/// The message of [`AnalysisDiagnostic::ArithModeChangesNothing`].
///
/// The sentence about what unannotated arithmetic does is produced from the
/// mode the finding carries, which for a redundancy against the default *is*
/// the language's default: the rule fires exactly when an annotation names the
/// mode already in force where it is written. So the message describes the
/// compiler the reader is running rather than a policy it has not adopted, and
/// moving that default moves this message and nothing else — while the text
/// stays a function of the arguments it was handed, which is what its tests can
/// vary.
fn arith_mode_changes_nothing_message(mode: ArithMode, enclosure: RedundantArithMode) -> String {
    let spelling = mode.spelling();
    match enclosure {
        RedundantArithMode::InsideTheSameAnnotation => format!(
            "this `{spelling}(...)` changes nothing; the `{spelling}(...)` it is written inside \
             already puts every operator here in that mode, so removing it changes no \
             arithmetic: delete it, or, if these operators were meant to differ from the ones \
             around them, write the other spelling"
        ),
        RedundantArithMode::AgainstTheDefault => {
            let other = mode.other().spelling();
            let verb = match mode {
                ArithMode::Checked => "trap",
                ArithMode::Wrapping => "wrap",
            };
            format!(
                "this `{spelling}(...)` changes nothing; `+`, `-`, `*` and unary `-` already \
                 {verb} on overflow everywhere except inside a `{other}(...)`, and no \
                 `{other}(...)` encloses this expression, so the annotation restates the default \
                 rather than restoring it: delete it, or, if you meant to restore {spelling} \
                 arithmetic inside a region you had opted out of, move it inside the \
                 `{other}(...)` whose operators you want exempted"
            )
        }
    }
}

/// The message of [`AnalysisDiagnostic::GenericNotSupported`], which is built
/// per site rather than parameterized, because the two sites state different
/// facts and offer different repairs.
///
/// Only the declaration site cites the monomorphization issue: it is the half
/// waiting on that feature, so "should I wait for it?" is a real question there
/// and is not one for a type argument no declaration will ever accept.
fn generic_not_supported_message(site: &GenericSite) -> String {
    match site {
        GenericSite::Declaration { function, params } => format!(
            "`{function}` is generic over `{params}`, and a generic function has no code \
             generation; the type checker infers a type argument at each call site and checks the \
             body against it, but nothing carries that substitution any further — the compiler \
             does not monomorphize, so a type parameter reaches code generation still standing \
             for no type at all, with no layout to size a frame slot with, no WebAssembly value \
             type to pass a value in, and no term for a proof to describe one with; generics are \
             not implemented rather than forbidden, so write one non-generic copy of \
             `{function}` for each type it is used at, or drop the type parameters if it is only \
             ever used at one, and track progress at \
             https://github.com/Inferara/inference/issues/76"
        ),
        GenericSite::TypeApplication { rendered, position } => format!(
            "`{rendered}` gives type arguments to a declaration that accepts none, and cannot be \
             used as {position}; no type declaration in Inference takes type parameters — a \
             struct, an enum and an `external fn` all declare a bare name, and only a function \
             binds one — so `{rendered}` names no declaration the compiler can lay \
             out: there is no byte size for frame layout to compute, no WebAssembly value type \
             for a signature to carry, and no term for a proof to describe a value of it; write \
             the base type name on its own if that is the type you meant"
        ),
    }
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

/// A block kind that carries non-deterministic semantics, and so can be named by
/// a diagnostic. [`BlockKind::Regular`] has no counterpart here on purpose: a
/// regular block is never the subject of these findings, so no message can ask
/// for a spelling that does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NonDetBlockKind {
    Forall,
    Exists,
    Assume,
    Unique,
}

impl NonDetBlockKind {
    /// The non-deterministic counterpart of `kind`, or `None` for a regular
    /// block. Call sites use this in place of a separate `is_non_det` test, so
    /// the guard and the reported kind cannot disagree.
    #[must_use]
    pub fn from_block_kind(kind: BlockKind) -> Option<Self> {
        match kind {
            BlockKind::Forall => Some(NonDetBlockKind::Forall),
            BlockKind::Exists => Some(NonDetBlockKind::Exists),
            BlockKind::Assume => Some(NonDetBlockKind::Assume),
            BlockKind::Unique => Some(NonDetBlockKind::Unique),
            BlockKind::Regular => None,
        }
    }

    /// The indefinite article preceding this kind's spelling: "an" before the
    /// vowel-initial `exists` and `assume`, "a" before `forall` and `unique`
    /// (which begins with a consonant sound). The match is exhaustive so that a
    /// kind added later cannot silently inherit the wrong article.
    fn article(self) -> &'static str {
        match self {
            NonDetBlockKind::Exists | NonDetBlockKind::Assume => "an",
            NonDetBlockKind::Forall | NonDetBlockKind::Unique => "a",
        }
    }
}

impl Display for NonDetBlockKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NonDetBlockKind::Forall => "forall",
            NonDetBlockKind::Exists => "exists",
            NonDetBlockKind::Assume => "assume",
            NonDetBlockKind::Unique => "unique",
        })
    }
}

/// Represents a control flow analysis error with source location.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisDiagnostic {
    #[error("break statement is only valid inside a loop body; if you intended to exit the function, use 'return'")]
    BreakOutsideLoop { location: Location },

    #[error(
        "break statement is not allowed inside {article} '{block_kind}' block; break would interfere with the path exploration required for formal verification; move the break outside the '{block_kind}' block",
        article = .block_kind.article()
    )]
    BreakInsideNonDetBlock {
        location: Location,
        block_kind: NonDetBlockKind,
    },

    #[error(
        "return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it"
    )]
    ReturnInsideLoop { location: Location },

    #[error("infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop does not count)")]
    InfiniteLoopWithoutBreak { location: Location },

    #[error(
        "return statement is not allowed inside {article} '{block_kind}' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the '{block_kind}' block",
        article = .block_kind.article()
    )]
    ReturnInsideNonDetBlock {
        location: Location,
        block_kind: NonDetBlockKind,
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

    #[error(
        "call to external function `{name}`, which no `use ... from` directive binds; an \
         `external fn` declaration states a signature and nothing else, so no module supplies \
         a body for this call. Add `use {{ {name} }} from <module>;` at the top level of the \
         file that declares it — a `use` clause reaches its own file's top-level declarations \
         only, so an `external fn` declared inside a `spec` must be moved out to be bound"
    )]
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
        block_kind: NonDetBlockKind,
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

    /// `arg` names the binding the argument is rooted at, which is the value
    /// the foreign store reaches — for `sort_pair(outer.inner)` that is
    /// `outer`, while the caret sits on the argument as written. An argument
    /// with no root binding supplies a short shape of itself instead; that
    /// program is already rejected by another rule, and the message's second
    /// fix is the applicable one. `ty` is the *parameter's* declared type, not
    /// the argument's: it says what the declaration asked for, and unlike the
    /// argument's own type it is spellable in every case, including a `self`
    /// receiver. `root` selects the repair, because a `const` root cannot take
    /// the `mut` the other form is asked for.
    #[error(
        "cannot pass `{arg}` to parameter `{param}: {ty}` of `external fn {callee}`, which is declared `mut` and may write through it; {fix}",
        fix = extern_mut_argument_fix(.arg, *.root)
    )]
    ExternWriteThroughImmutableArgument {
        arg: String,
        param: String,
        callee: String,
        ty: String,
        root: ImmutableArgumentRoot,
        location: Location,
    },

    /// One variant serves every position a string value can be introduced at,
    /// because the fact and the fix are the same at all of them: the message
    /// names the position and is otherwise fixed text.
    #[error(
        "`string` has no value representation in a compiled program and cannot be used as \
         {position}; the type checker accepts `string` and `String` as type names, but no phase \
         after it can lower one — there is no layout for a string in linear memory, no \
         WebAssembly type to pass one in, and no term for a proof to describe it with; string \
         support is not implemented, so model text as data that has a layout — a `[u8; N]` with \
         its bytes written as numbers, or an enum tag when the value is one of a fixed set"
    )]
    StringNotSupported {
        position: &'static str,
        location: Location,
    },

    /// The message has to say which spellings of unit stay legal, because the
    /// one it rejects and the ones it does not are written with the same two
    /// characters — a reader told only that `()` is unusable would conclude
    /// that a void function is too. `position` also selects the repair, because
    /// the value position has no declaration for the other clause to point at.
    #[error(
        "the unit type `()` has no value representation and cannot be used as {position}; a unit \
         value carries no information, so it occupies no bytes and has no WebAssembly type — a \
         parameter declared `()` is given no argument slot, a binding of it has nothing to store, \
         and an array of it has no element size; `()` stays legal as the way a function says it \
         returns nothing, and `return;`, `return ();` and a bare `();` statement are unaffected; \
         {fix}",
        fix = unit_as_value_fix(.position)
    )]
    UnitAsValue {
        position: &'static str,
        location: Location,
    },

    /// `index` is 0-based over the parameters the declaration writes, with a
    /// `self` receiver excluded — the same numbering the type checker's own
    /// argument messages use, since the parameter lists those messages index
    /// into are built with the receiver filtered out. Two user-facing messages must not number
    /// one slot differently. `function` arrives pre-rendered (`Struct::method`
    /// for a method) because the diagnostic has no arena to resolve a receiver
    /// against.
    #[error(
        "parameter {index} of `{function}` is declared by its type alone (`{ty}`); a parameter \
         with no name can be neither read in the body nor labeled at a call site, and Inference \
         already has one spelling for that — `_: {ty}` says the parameter is present and \
         deliberately unused, while a bare `{ty}` says nothing at all, leaving two spellings for \
         one thing; write `_: {ty}` if you do not need the value, or give the parameter a name"
    )]
    UnnamedParameter {
        function: String,
        index: usize,
        ty: String,
        location: Location,
    },

    /// One variant serves both generic surfaces, because one rule owns them: a
    /// declaration that binds type parameters and an application that gives type
    /// arguments. `site` selects the whole message rather than a clause of it —
    /// the two state different facts, cite different things, and offer different
    /// repairs, so there is no shared sentence for them to be parameterized
    /// around.
    #[error("{message}", message = generic_not_supported_message(.site))]
    GenericNotSupported {
        site: GenericSite,
        location: Location,
    },

    /// An operation whose operands are constants and whose result leaves the
    /// type it is performed at. Every field is what the fold established: the
    /// expression as written, the values its operands folded to, the operator,
    /// the type, and the two results the two spellings would give.
    #[error(
        "{message}",
        message = constant_arithmetic_overflow_message(
            .expression, *.operands, *.op, *.number, *.exact, *.wrapped
        )
    )]
    ConstantArithmeticOverflow {
        /// The operation as the source spells it. Named for what it is rather
        /// than `source`, which `thiserror` reserves for a wrapped error.
        expression: String,
        operands: FoldedOperands,
        op: GuardedOp,
        number: NumberType,
        exact: ExactValue,
        /// The result the `wrapping(...)` spelling computes, which every width
        /// holds by construction — including the one whose exact product does
        /// not fit `i128`, since reducing it into the range is what makes it a
        /// value of the type.
        wrapped: i128,
        location: Location,
    },

    /// An arithmetic-mode annotation with no operator inside it to govern. The
    /// mode selects the whole message: a `wrapping(...)` that governs nothing is
    /// inert, a `checked(...)` that governs nothing reads as a guarantee nothing
    /// backs, and the two have no shared sentence to be parameterized around.
    #[error("{message}", message = arith_mode_governs_nothing_message(*.mode))]
    ArithModeGovernsNothing {
        mode: ArithMode,
        location: Location,
    },

    /// An arithmetic-mode annotation naming the mode already in force where it
    /// is written. `enclosure` says what it is redundant against, which decides
    /// which of the two facts the message states.
    #[error(
        "{message}",
        message = arith_mode_changes_nothing_message(*.mode, *.enclosure)
    )]
    ArithModeChangesNothing {
        mode: ArithMode,
        enclosure: RedundantArithMode,
        location: Location,
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
            | AnalysisDiagnostic::SpacedNegativeLiteral { location, .. }
            | AnalysisDiagnostic::ExternWriteThroughImmutableArgument { location, .. }
            | AnalysisDiagnostic::StringNotSupported { location, .. }
            | AnalysisDiagnostic::UnitAsValue { location, .. }
            | AnalysisDiagnostic::UnnamedParameter { location, .. }
            | AnalysisDiagnostic::GenericNotSupported { location, .. }
            | AnalysisDiagnostic::ConstantArithmeticOverflow { location, .. }
            | AnalysisDiagnostic::ArithModeGovernsNothing { location, .. }
            | AnalysisDiagnostic::ArithModeChangesNothing { location, .. } => location,
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
            AnalysisDiagnostic::ExternWriteThroughImmutableArgument { .. } => "A047",
            AnalysisDiagnostic::StringNotSupported { .. } => "A048",
            AnalysisDiagnostic::UnitAsValue { .. } => "A049",
            AnalysisDiagnostic::UnnamedParameter { .. } => "A050",
            AnalysisDiagnostic::GenericNotSupported { .. } => "A051",
            AnalysisDiagnostic::ConstantArithmeticOverflow { .. } => "A052",
            AnalysisDiagnostic::ArithModeGovernsNothing { .. } => "A053",
            AnalysisDiagnostic::ArithModeChangesNothing { .. } => "A054",
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
            block_kind: NonDetBlockKind::Forall,
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
            block_kind: NonDetBlockKind::Forall,
        };
        assert_eq!(
            err.to_string(),
            "return statement is not allowed inside a 'forall' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the 'forall' block"
        );
    }

    /// The vowel-initial kinds take "an"; the message stays otherwise
    /// identical to the consonant form pinned above.
    #[test]
    fn display_return_inside_exists_block_uses_an() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: NonDetBlockKind::Exists,
        };
        assert_eq!(
            err.to_string(),
            "return statement is not allowed inside an 'exists' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the 'exists' block"
        );
    }

    #[test]
    fn display_return_inside_assume_block_uses_an() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: NonDetBlockKind::Assume,
        };
        assert_eq!(
            err.to_string(),
            "return statement is not allowed inside an 'assume' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the 'assume' block"
        );
    }

    /// "unique" begins with a consonant sound, so it keeps "a".
    #[test]
    fn display_return_inside_unique_block_uses_a() {
        let err = AnalysisDiagnostic::ReturnInsideNonDetBlock {
            location: test_location(),
            block_kind: NonDetBlockKind::Unique,
        };
        assert!(
            err.to_string()
                .starts_with("return statement is not allowed inside a 'unique' block"),
            "got: {err}"
        );
    }

    /// The vowel-initial kinds take "an" in the break diagnostic too; the
    /// consonant form is pinned by `display_break_inside_nondet_block`.
    #[test]
    fn display_break_inside_exists_block_uses_an() {
        let err = AnalysisDiagnostic::BreakInsideNonDetBlock {
            location: test_location(),
            block_kind: NonDetBlockKind::Exists,
        };
        assert_eq!(
            err.to_string(),
            "break statement is not allowed inside an 'exists' block; break would interfere with the path exploration required for formal verification; move the break outside the 'exists' block"
        );
    }

    #[test]
    fn display_break_inside_assume_block_uses_an() {
        let err = AnalysisDiagnostic::BreakInsideNonDetBlock {
            location: test_location(),
            block_kind: NonDetBlockKind::Assume,
        };
        assert_eq!(
            err.to_string(),
            "break statement is not allowed inside an 'assume' block; break would interfere with the path exploration required for formal verification; move the break outside the 'assume' block"
        );
    }

    /// The spelling a message interpolates is the source keyword, so each kind
    /// is pinned rather than left to the variant name's casing.
    #[test]
    fn non_det_block_kind_spells_the_source_keyword() {
        assert_eq!(NonDetBlockKind::Forall.to_string(), "forall");
        assert_eq!(NonDetBlockKind::Exists.to_string(), "exists");
        assert_eq!(NonDetBlockKind::Assume.to_string(), "assume");
        assert_eq!(NonDetBlockKind::Unique.to_string(), "unique");
    }

    /// A regular block has no counterpart, which is what lets the call sites
    /// use this conversion as their guard instead of testing `is_non_det` and
    /// then translating the kind separately.
    #[test]
    fn non_det_block_kind_maps_every_block_kind() {
        assert_eq!(
            NonDetBlockKind::from_block_kind(BlockKind::Forall),
            Some(NonDetBlockKind::Forall)
        );
        assert_eq!(
            NonDetBlockKind::from_block_kind(BlockKind::Exists),
            Some(NonDetBlockKind::Exists)
        );
        assert_eq!(
            NonDetBlockKind::from_block_kind(BlockKind::Assume),
            Some(NonDetBlockKind::Assume)
        );
        assert_eq!(
            NonDetBlockKind::from_block_kind(BlockKind::Unique),
            Some(NonDetBlockKind::Unique)
        );
        assert_eq!(NonDetBlockKind::from_block_kind(BlockKind::Regular), None);
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
            AnalysisDiagnostic::BreakInsideNonDetBlock { location: test_location(), block_kind: NonDetBlockKind::Forall }.rule_id(),
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
            AnalysisDiagnostic::ReturnInsideNonDetBlock { location: test_location(), block_kind: NonDetBlockKind::Forall }.rule_id(),
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
            block_kind: NonDetBlockKind::Forall,
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
        for kind in [
            NonDetBlockKind::Forall,
            NonDetBlockKind::Exists,
            NonDetBlockKind::Assume,
            NonDetBlockKind::Unique,
        ] {
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

    /// The A047 message has to carry four separate facts, because none of them
    /// is visible where the reader is standing: which binding is at risk, which
    /// parameter claimed the right to write, that the claim is what makes the
    /// call illegal, and what to write instead. The fix clause is asserted
    /// without a type annotation on purpose — `mut self` takes none, and the
    /// receiver is one of the shapes that reaches this message.
    #[test]
    fn display_extern_write_through_immutable_argument() {
        let err = AnalysisDiagnostic::ExternWriteThroughImmutableArgument {
            arg: "arr".to_string(),
            param: "a".to_string(),
            callee: "sort_pair".to_string(),
            ty: "[i32; 2]".to_string(),
            root: ImmutableArgumentRoot::Binding,
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("cannot pass `arr`"),
            "A047 diagnostic must name the argument's binding, got: {text}"
        );
        assert!(
            text.contains("parameter `a: [i32; 2]`"),
            "A047 diagnostic must name the parameter and its declared type, got: {text}"
        );
        assert!(
            text.contains("`external fn sort_pair`"),
            "A047 diagnostic must name the external function, got: {text}"
        );
        assert!(
            text.contains("declared `mut` and may write through it"),
            "A047 diagnostic must say why the parameter is dangerous, got: {text}"
        );
        assert!(
            text.contains("`arr` is not declared `mut`"),
            "A047 diagnostic must say what is wrong with the argument, got: {text}"
        );
        assert!(
            text.contains("its value must not change across the call"),
            "A047 diagnostic must state the rule being broken, got: {text}"
        );
        assert!(
            text.contains("declare it `mut arr` where it is bound"),
            "A047 diagnostic must spell the primary fix out, got: {text}"
        );
        assert!(
            text.contains("copy it into a `mut` binding and pass that"),
            "A047 diagnostic must offer the fix for an argument the caller does \
             not own, got: {text}"
        );
        assert_eq!(err.rule_id(), "A047");
    }

    /// A `self` receiver reaches this message, and `mut self: Pair` is not
    /// something anyone can write — so the fix clause must stay free of a type
    /// annotation for every subject it names.
    #[test]
    fn display_extern_write_through_immutable_argument_fix_is_spellable() {
        for (arg, ty) in [("arr", "[i32; 2]"), ("p", "Pair"), ("self", "Pair")] {
            let err = AnalysisDiagnostic::ExternWriteThroughImmutableArgument {
                arg: arg.to_string(),
                param: "a".to_string(),
                callee: "sort_pair".to_string(),
                ty: ty.to_string(),
                root: ImmutableArgumentRoot::Binding,
                location: test_location(),
            };
            let text = err.to_string();
            assert!(
                text.contains(&format!("declare it `mut {arg}` where it is bound")),
                "A047 must recommend a spelling that is legal for `{arg}`, got: {text}"
            );
            assert!(
                !text.contains(&format!("mut {arg}: {ty}")),
                "A047 must not annotate the fix with a type, got: {text}"
            );
        }
    }

    /// A `const` root reaches this message too, and the repair the other roots
    /// are given is one it cannot take: `const mut P` is a parse error. The two
    /// renderings are asserted against each other, so a change that collapses
    /// them back into one fails here rather than shipping advice the grammar
    /// rejects.
    #[test]
    fn display_extern_write_through_immutable_argument_const_root() {
        let of_root = |root| {
            AnalysisDiagnostic::ExternWriteThroughImmutableArgument {
                arg: "P".to_string(),
                param: "p".to_string(),
                callee: "sort_pair".to_string(),
                ty: "Pair".to_string(),
                root,
                location: test_location(),
            }
            .to_string()
        };
        let constant = of_root(ImmutableArgumentRoot::Constant);
        let binding = of_root(ImmutableArgumentRoot::Binding);

        assert!(
            constant.contains("`P` is a `const`"),
            "A047 must say the root is a `const`, got: {constant}"
        );
        assert!(
            constant.contains("`const mut` is not a declaration the grammar accepts"),
            "A047 must say why the usual repair is unavailable, got: {constant}"
        );
        assert!(
            constant.contains("copy `P` into a `mut` binding and pass that instead"),
            "A047 must offer the repair a `const` can take, got: {constant}"
        );
        assert!(
            !constant.contains("declare it `mut P`"),
            "A047 must never ask a `const` for `mut`, got: {constant}"
        );
        assert!(
            binding.contains("declare it `mut P` where it is bound"),
            "the other root keeps the direct repair, got: {binding}"
        );
        let shared = "cannot pass `P` to parameter `p: Pair` of `external fn sort_pair`, which \
                      is declared `mut` and may write through it; ";
        assert!(
            constant.starts_with(shared) && binding.starts_with(shared),
            "only the repair clause may differ between the two roots, got:\n{constant}\n{binding}"
        );
    }

    /// The A048 message is the only place a reader is told that a type the
    /// checker accepted cannot be used, so it has to carry the whole
    /// explanation: which name, that it has no value representation, why no
    /// later phase can supply one, and what to write instead.
    #[test]
    fn display_string_not_supported() {
        let err = AnalysisDiagnostic::StringNotSupported {
            position: "the type of a string literal",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("`string` has no value representation"),
            "A048 diagnostic must name the type and state what is wrong, got: {text}"
        );
        assert!(
            text.contains("accepts `string` and `String` as type names"),
            "A048 diagnostic must say why the annotation was accepted, got: {text}"
        );
        assert!(
            text.contains("no layout for a string in linear memory"),
            "A048 diagnostic must state the memory mechanism, got: {text}"
        );
        assert!(
            text.contains("no WebAssembly type to pass one in"),
            "A048 diagnostic must state the ABI mechanism, got: {text}"
        );
        assert!(
            text.contains("no term for a proof to describe it with"),
            "A048 diagnostic must state the verification consequence, got: {text}"
        );
        assert!(
            text.contains("string support is not implemented"),
            "A048 diagnostic must say the feature is missing rather than forbidden, got: {text}"
        );
        assert!(
            text.contains("a `[u8; N]` with its bytes written as numbers"),
            "A048 diagnostic must offer the byte-array fix, and say how to write the bytes, \
             got: {text}"
        );
        assert!(
            text.contains("an enum tag"),
            "A048 diagnostic must offer the fixed-set fix, got: {text}"
        );
        assert_eq!(err.rule_id(), "A048");
    }

    /// One variant serves every position the rule covers, so each position
    /// string must render into the same sentence.
    #[test]
    fn display_string_not_supported_names_each_position() {
        for position in [
            "the type of a string literal",
            "the declared type of a variable",
            "the type of a parameter",
            "the return type of a function",
            "the type of a struct field",
        ] {
            let err = AnalysisDiagnostic::StringNotSupported {
                position,
                location: test_location(),
            };
            assert!(
                err.to_string()
                    .contains(&format!("cannot be used as {position};")),
                "A048 diagnostic must name the `{position}` position"
            );
        }
    }

    /// `()` is written the same way in the position this rule rejects and in
    /// the positions it leaves alone, so the message has to name the legal ones
    /// explicitly or a reader will conclude that a void function is illegal
    /// too.
    #[test]
    fn display_unit_as_value() {
        let err = AnalysisDiagnostic::UnitAsValue {
            position: "a value",
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("the unit type `()` has no value representation"),
            "A049 diagnostic must name the type and state what is wrong, got: {text}"
        );
        assert!(
            text.contains("occupies no bytes and has no WebAssembly type"),
            "A049 diagnostic must explain the mechanism, got: {text}"
        );
        assert!(
            text.contains("is given no argument slot"),
            "A049 diagnostic must say what a unit parameter lacks, got: {text}"
        );
        assert!(
            text.contains("a binding of it has nothing to store"),
            "A049 diagnostic must say what a unit binding lacks, got: {text}"
        );
        assert!(
            text.contains("an array of it has no element size"),
            "A049 diagnostic must say what a unit array lacks, got: {text}"
        );
        assert!(
            text.contains("`()` stays legal as the way a function says it returns nothing"),
            "A049 diagnostic must say the return position is unaffected, got: {text}"
        );
        assert!(
            text.contains("`return;`, `return ();` and a bare `();` statement are unaffected"),
            "A049 diagnostic must name the exempt statement forms, got: {text}"
        );
        assert!(
            text.contains("drop the `()`, or write a value the surrounding expression can use"),
            "A049 diagnostic must offer the fix the value position can take, got: {text}"
        );
        assert_eq!(err.rule_id(), "A049");
    }

    /// The repair differs by position, and getting it wrong is not cosmetic: an
    /// expression standing where a value was required has no declaration to
    /// remove, so the declaration advice would send the author looking for one
    /// that is not there. The two clauses are asserted against each other, so a
    /// change that collapses them back into one fails here.
    #[test]
    fn display_unit_as_value_repair_follows_the_position() {
        let of_position = |position| {
            AnalysisDiagnostic::UnitAsValue {
                position,
                location: test_location(),
            }
            .to_string()
        };
        let declaration_fix = "remove the declaration, or give it a type that carries a value";
        let value_fix = "drop the `()`, or write a value the surrounding expression can use";

        for position in [
            "the declared type of a variable",
            "the type of a parameter",
            "the type of a struct field",
        ] {
            let text = of_position(position);
            assert!(
                text.contains(declaration_fix),
                "A049 must offer the declaration repair at `{position}`, got: {text}"
            );
            assert!(
                !text.contains(value_fix),
                "A049 must not offer the expression repair at `{position}`, got: {text}"
            );
        }

        let value = of_position("a value");
        assert!(
            value.contains(value_fix),
            "A049 must offer the expression repair at the value position, got: {value}"
        );
        assert!(
            !value.contains(declaration_fix),
            "A049 must not ask an expression to remove a declaration it does not have, \
             got: {value}"
        );
    }

    #[test]
    fn display_unit_as_value_names_each_position() {
        for position in [
            "a value",
            "the declared type of a variable",
            "the type of a parameter",
            "the type of a struct field",
        ] {
            let err = AnalysisDiagnostic::UnitAsValue {
                position,
                location: test_location(),
            };
            assert!(
                err.to_string()
                    .contains(&format!("cannot be used as {position};")),
                "A049 diagnostic must name the `{position}` position"
            );
        }
    }

    /// The A050 message names a parameter the source gives no name to, so the
    /// index and the type are the only two handles a reader has on it — and the
    /// fix has to be spelled, because the shorter of the two repairs is a
    /// single character the reader has to be told about.
    #[test]
    fn display_unnamed_parameter() {
        let err = AnalysisDiagnostic::UnnamedParameter {
            function: "P::m".to_string(),
            index: 1,
            ty: "i32".to_string(),
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("parameter 1 of `P::m`"),
            "A050 diagnostic must name the position and the function, got: {text}"
        );
        assert!(
            text.contains("declared by its type alone (`i32`)"),
            "A050 diagnostic must name the type that stands in for the parameter, got: {text}"
        );
        assert!(
            text.contains("neither read in the body nor labeled at a call site"),
            "A050 diagnostic must say what the missing name costs, got: {text}"
        );
        assert!(
            text.contains("`_: i32` says the parameter is present and deliberately unused"),
            "A050 diagnostic must contrast the two spellings, got: {text}"
        );
        assert!(
            text.contains("leaving two spellings for one thing"),
            "A050 diagnostic must say why the weaker spelling is removed, got: {text}"
        );
        assert!(
            text.contains("write `_: i32` if you do not need the value"),
            "A050 diagnostic must spell the `_` fix out, got: {text}"
        );
        assert!(
            text.contains("give the parameter a name"),
            "A050 diagnostic must offer the naming fix, got: {text}"
        );
        assert_eq!(err.rule_id(), "A050");
    }

    /// The fix is built from the parameter's own type, so it has to track it —
    /// an array type in particular, whose rendering carries brackets the advice
    /// must reproduce rather than paraphrase.
    #[test]
    fn display_unnamed_parameter_quotes_each_type() {
        for ty in ["i32", "[i32; 2]", "Point"] {
            let err = AnalysisDiagnostic::UnnamedParameter {
                function: "f".to_string(),
                index: 0,
                ty: ty.to_string(),
                location: test_location(),
            };
            let text = err.to_string();
            assert!(
                text.contains(&format!("write `_: {ty}` if you do not need the value")),
                "A050 must recommend the `_` spelling of `{ty}`, got: {text}"
            );
            assert!(
                text.contains(&format!("a bare `{ty}` says nothing at all")),
                "A050 must quote the offending spelling of `{ty}`, got: {text}"
            );
        }
    }

    /// `A052` speaks of one constant or two, and which it is comes from the
    /// fold rather than from the operator: a negation has one operand, and a
    /// sentence about two written over one describes an expression nobody
    /// wrote. The in-range claim about the operands is the load-bearing half —
    /// it is what separates this finding from `A022`, which owns an operand
    /// that is itself out of range — so each arm is held to it.
    #[test]
    fn display_constant_arithmetic_overflow() {
        let err = AnalysisDiagnostic::ConstantArithmeticOverflow {
            expression: "max + 1".to_string(),
            operands: FoldedOperands::Binary(2_147_483_647, 1),
            op: GuardedOp::Add,
            number: NumberType::I32,
            exact: ExactValue::Narrow(2_147_483_648),
            wrapped: -2_147_483_648,
            location: test_location(),
        };
        assert_eq!(err.rule_id(), "A052");
        let text = err.to_string();
        assert!(
            text.contains("`max + 1` overflows `i32` before the program runs"),
            "A052 must open on the expression the source wrote, got: {text}"
        );
        assert!(
            text.contains(
                "its operands fold to the constants `2147483647` and `1`, each of which is \
                 itself within `-2147483648..=2147483647`, and it is their `+` that is not"
            ),
            "the binary arm must name both constants and claim each is in range, got: {text}"
        );
        assert!(
            text.contains("the true result `2147483648` is outside that range"),
            "A052 must state the result that does not fit, got: {text}"
        );
        assert!(
            text.contains("write `wrapping(max + 1)`, which computes `-2147483648`"),
            "the remedy must quote the wrapped value, got: {text}"
        );
        assert!(
            text.contains("the operands or the declared type have to change"),
            "the binary arm's fallback repair must speak of operands, got: {text}"
        );
    }

    #[test]
    fn display_constant_arithmetic_overflow_unary_arm() {
        let err = AnalysisDiagnostic::ConstantArithmeticOverflow {
            expression: "-min".to_string(),
            operands: FoldedOperands::Unary(-128),
            op: GuardedOp::Neg,
            number: NumberType::I8,
            exact: ExactValue::Narrow(128),
            wrapped: -128,
            location: test_location(),
        };
        assert_eq!(err.rule_id(), "A052");
        let text = err.to_string();
        assert!(
            text.contains(
                "its operand folds to the constant `-128`, which is itself within \
                 `-128..=127`, and it is the `-` applied to it that is not"
            ),
            "the unary arm must speak of one operand, got: {text}"
        );
        assert!(
            !text.contains("its operands fold to the constants"),
            "the unary arm must not describe two operands, got: {text}"
        );
        assert!(
            text.contains("the operand or the declared type has to change"),
            "the unary arm's fallback repair must be singular, got: {text}"
        );
    }

    /// The one result `i128` does not hold still has to be stated, and stated as
    /// the number it is: a message that rendered `u64::MAX * u64::MAX` as
    /// anything but its true product would be claiming something the fold did
    /// not establish, which is the whole reason the field carries a value here
    /// rather than an absence.
    #[test]
    fn display_constant_arithmetic_overflow_wide_product() {
        let err = AnalysisDiagnostic::ConstantArithmeticOverflow {
            expression: "a * b".to_string(),
            operands: FoldedOperands::Binary(
                i128::from(u64::MAX),
                i128::from(u64::MAX),
            ),
            op: GuardedOp::Mul,
            number: NumberType::U64,
            exact: ExactValue::Wide(u128::from(u64::MAX) * u128::from(u64::MAX)),
            wrapped: 1,
            location: test_location(),
        };
        assert_eq!(err.rule_id(), "A052");
        let text = err.to_string();
        assert!(
            text.contains(
                "the true result `340282366920938463426481119284349108225` is outside that \
                 range"
            ),
            "A052 must state the product itself, got: {text}"
        );
        assert!(
            text.contains("write `wrapping(a * b)`, which computes `1`"),
            "the remedy must quote the value the wrap computes, got: {text}"
        );
    }

    /// The two `A053` wordings answer two different mistakes, so each is held to
    /// the sentence only it carries: a `wrapping(...)` with nothing to change is
    /// inert, and a `checked(...)` with nothing to check is a guarantee a later
    /// reader would believe. Both name the call and the glued-literal shapes,
    /// which is where nearly every instance comes from and neither of which is
    /// visible from the annotation itself.
    #[test]
    fn display_arith_mode_governs_nothing() {
        let wrapping = AnalysisDiagnostic::ArithModeGovernsNothing {
            mode: ArithMode::Wrapping,
            location: test_location(),
        };
        assert_eq!(wrapping.rule_id(), "A053");
        let text = wrapping.to_string();
        assert!(
            text.contains("this `wrapping(...)` has no arithmetic to change"),
            "A053 must open on the spelling the author wrote, got: {text}"
        );
        assert!(
            text.contains("it does not reach into the body of a function called inside it"),
            "A053 must name the annotated-call shape, got: {text}"
        );
        assert!(
            text.contains("`-2147483648` is a single literal carrying its own sign"),
            "A053 must name the glued-literal shape, got: {text}"
        );

        let checked = AnalysisDiagnostic::ArithModeGovernsNothing {
            mode: ArithMode::Checked,
            location: test_location(),
        }
        .to_string();
        assert!(
            checked.contains("this `checked(...)` has no arithmetic to check"),
            "got: {checked}"
        );
        assert!(
            checked.contains("a reader who takes it as a guarantee about this expression would \
                              be wrong"),
            "the `checked` wording states the stake the `wrapping` one does not, got: {checked}"
        );
    }

    /// The default-dependent sentence is produced from the mode the finding
    /// carries, which at the top level is the language's own default: exactly
    /// one of the two spellings can be redundant there, and the message
    /// describes the compiler the reader is running.
    #[test]
    fn display_arith_mode_changes_nothing() {
        let redundant = ArithMode::DEFAULT.spelling();
        let other = ArithMode::DEFAULT.other().spelling();
        let verb = if ArithMode::DEFAULT == ArithMode::Checked {
            "trap"
        } else {
            "wrap"
        };
        let err = AnalysisDiagnostic::ArithModeChangesNothing {
            mode: ArithMode::DEFAULT,
            enclosure: RedundantArithMode::AgainstTheDefault,
            location: test_location(),
        };
        assert_eq!(err.rule_id(), "A054");
        let text = err.to_string();
        assert!(
            text.contains(&format!("this `{redundant}(...)` changes nothing")),
            "got: {text}"
        );
        assert!(
            text.contains(&format!(
                "already {verb} on overflow everywhere except inside a `{other}(...)`"
            )),
            "the sentence about unannotated arithmetic must follow the language's default, \
             got: {text}"
        );
        assert!(
            text.contains(&format!(
                "move it inside the `{other}(...)` whose operators you want exempted"
            )),
            "got: {text}"
        );

        let nested = AnalysisDiagnostic::ArithModeChangesNothing {
            mode: ArithMode::Checked,
            enclosure: RedundantArithMode::InsideTheSameAnnotation,
            location: test_location(),
        }
        .to_string();
        assert!(
            nested.contains("the `checked(...)` it is written inside already puts every \
                             operator here in that mode"),
            "a nested redundancy states the annotation above it, not the default, got: {nested}"
        );
        assert!(
            !nested.contains("restates the default"),
            "a nested redundancy must say nothing about the default, got: {nested}"
        );

        // Every part of the sentence comes from the mode the finding carries,
        // which is what keeps the spelling, its opposite and the verb
        // describing one language rather than two. The rule raises this
        // enclosure only when that mode is the one already in force — the
        // default, at the top level — so rendering the other mode is asking a
        // question about the text alone: all three must move together.
        let other_mode = ArithMode::DEFAULT.other();
        let other_verb = if other_mode == ArithMode::Checked {
            "trap"
        } else {
            "wrap"
        };
        let inverted = AnalysisDiagnostic::ArithModeChangesNothing {
            mode: other_mode,
            enclosure: RedundantArithMode::AgainstTheDefault,
            location: test_location(),
        }
        .to_string();
        assert!(
            inverted.contains(&format!(
                "this `{}(...)` changes nothing; `+`, `-`, `*` and unary `-` already \
                 {other_verb} on overflow everywhere except inside a `{redundant}(...)`",
                other_mode.spelling()
            )),
            "the sentence follows the mode the finding carries, got: {inverted}"
        );
    }

    /// The A051 declaration message is the only place a reader is told that a
    /// declaration the type checker handled completely cannot be compiled, so it
    /// has to carry the whole explanation: which declaration, which binders,
    /// that the substitution stops at the type checker, what each missing piece
    /// costs, that the feature is missing rather than forbidden, what to write
    /// instead, and where to follow the feature.
    #[test]
    fn display_generic_declaration_not_supported() {
        let err = AnalysisDiagnostic::GenericNotSupported {
            site: GenericSite::Declaration {
                function: "P::m".to_string(),
                params: "T' U'".to_string(),
            },
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("`P::m` is generic over `T' U'`"),
            "A051 must name the declaration and every binder it declares, got: {text}"
        );
        assert!(
            text.contains("a generic function has no code generation"),
            "A051 must state what is wrong with the declaration, got: {text}"
        );
        assert!(
            text.contains("infers a type argument at each call site"),
            "A051 must say what the type checker did, so the reader knows why it was accepted, \
             got: {text}"
        );
        assert!(
            text.contains("the compiler does not monomorphize"),
            "A051 must name the missing mechanism, got: {text}"
        );
        assert!(
            text.contains("no layout to size a frame slot with"),
            "A051 must state the memory consequence, got: {text}"
        );
        assert!(
            text.contains("no WebAssembly value type to pass a value in"),
            "A051 must state the ABI consequence, got: {text}"
        );
        assert!(
            text.contains("no term for a proof to describe one with"),
            "A051 must state the verification consequence, got: {text}"
        );
        assert!(
            text.contains("generics are not implemented rather than forbidden"),
            "A051 must say the feature is missing rather than forbidden, got: {text}"
        );
        assert!(
            text.contains("write one non-generic copy of `P::m` for each type it is used at"),
            "A051 must spell the hand-written-copies fix out, got: {text}"
        );
        assert!(
            text.contains("drop the type parameters if it is only ever used at one"),
            "A051 must offer the single-instantiation fix, got: {text}"
        );
        assert!(
            text.contains("https://github.com/Inferara/inference/issues/76"),
            "A051 must cite the feature the declaration half waits on, got: {text}"
        );
        assert_eq!(err.rule_id(), "A051");
    }

    /// The application message states a different fact from the declaration one:
    /// no declaration in the language accepts type arguments at all, which is
    /// why it offers a different repair and cites nothing.
    #[test]
    fn display_generic_type_application_not_supported() {
        let err = AnalysisDiagnostic::GenericNotSupported {
            site: GenericSite::TypeApplication {
                rendered: "Q i32'".to_string(),
                position: "the type of a parameter",
            },
            location: test_location(),
        };
        let text = err.to_string();
        assert!(
            text.contains("`Q i32'` gives type arguments to a declaration that accepts none"),
            "A051 must quote the application as written and state what is wrong, got: {text}"
        );
        assert!(
            text.contains("no type declaration in Inference takes type parameters"),
            "A051 must state the language fact behind the refusal, got: {text}"
        );
        assert!(
            text.contains(
                "a struct, an enum and an `external fn` all declare a bare name"
            ),
            "A051 must enumerate the declarations that take no type parameters, got: {text}"
        );
        assert!(
            text.contains("only a function binds one"),
            "A051 must say which declaration does bind a type parameter, got: {text}"
        );
        assert!(
            text.contains("no byte size for frame layout to compute"),
            "A051 must state the memory consequence, got: {text}"
        );
        assert!(
            text.contains("no WebAssembly value type for a signature to carry"),
            "A051 must state the ABI consequence, got: {text}"
        );
        assert!(
            text.contains("no term for a proof to describe a value of it"),
            "A051 must state the verification consequence, got: {text}"
        );
        assert!(
            text.contains("write the base type name on its own if that is the type you meant"),
            "A051 must offer the repair, got: {text}"
        );
        assert_eq!(err.rule_id(), "A051");
    }

    /// One variant serves every position the application site covers, so each
    /// position string must render into the same sentence.
    #[test]
    fn display_generic_not_supported_names_each_position() {
        for position in [
            "a value",
            "the declared type of a variable",
            "the type of a parameter",
            "the return type of a function",
            "the type of a struct field",
        ] {
            let err = AnalysisDiagnostic::GenericNotSupported {
                site: GenericSite::TypeApplication {
                    rendered: "Q i32'".to_string(),
                    position,
                },
                location: test_location(),
            };
            assert!(
                err.to_string()
                    .contains(&format!("cannot be used as {position};")),
                "A051 diagnostic must name the `{position}` position"
            );
        }
    }

    /// The two sites are refused for different reasons, and neither rationale is
    /// true of the other: a declaration is waiting on monomorphization, while a
    /// type argument has no declaration to apply to whatever monomorphization
    /// does. Each clause is asserted against the other rendering, so a change
    /// that collapses the two messages into one fails here.
    #[test]
    fn display_generic_not_supported_states_one_fact_per_site() {
        let declaration = AnalysisDiagnostic::GenericNotSupported {
            site: GenericSite::Declaration {
                function: "id".to_string(),
                params: "T'".to_string(),
            },
            location: test_location(),
        }
        .to_string();
        let application = AnalysisDiagnostic::GenericNotSupported {
            site: GenericSite::TypeApplication {
                rendered: "Q i32'".to_string(),
                position: "the type of a parameter",
            },
            location: test_location(),
        }
        .to_string();

        let declaration_fix = "write one non-generic copy of `id` for each type it is used at";
        let application_fix = "write the base type name on its own if that is the type you meant";
        assert!(
            declaration.contains(declaration_fix),
            "A051 must offer the copies repair at a declaration, got: {declaration}"
        );
        assert!(
            !declaration.contains(application_fix),
            "A051 must not tell a declaration to drop its type arguments, got: {declaration}"
        );
        assert!(
            application.contains(application_fix),
            "A051 must offer the base-name repair at an application, got: {application}"
        );
        assert!(
            !application.contains(declaration_fix),
            "A051 must not ask an application to write non-generic copies, got: {application}"
        );
        assert!(
            !application.contains("https://github.com/Inferara/inference/issues/76"),
            "the application site is not waiting on monomorphization and must cite nothing, \
             got: {application}"
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
