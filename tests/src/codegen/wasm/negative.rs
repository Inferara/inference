//! Negative codegen tests.
//!
//! Each test verifies that an input which passes parsing and type-checking is
//! correctly *rejected* during WebAssembly code generation, and that the
//! resulting message contains the expected diagnostic substring. A refusal and
//! a crash are different outcomes and the tests here must tell them apart, so a
//! backstop row matches on [`crate::utils::CodegenAttempt`] rather than on a
//! `Result` whose `is_err()` is satisfied by either.
//!
//! Most rows drive code generation with analysis skipped: the shapes are ones an
//! analysis rule rejects first, and the point of the row is that the backend
//! refuses them too, for a caller that goes straight from type checking to code
//! generation.

use crate::utils::{AnalysisMode, CodegenAttempt, codegen_attempt};

mod uninitialized_variables {
    use crate::utils::build_ast;
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_analyze(source: &str) -> Result<(), Vec<AnalysisDiagnostic>> {
        let arena = build_ast(source.to_string());
        let ctx = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for uninitialized variable tests")
            .typed_context();
        match inference_analysis::analyze(&ctx) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.errors().to_vec()),
        }
    }

    #[test]
    fn uninitialized_i32() {
        let errors = try_analyze("pub fn test() { let x: i32; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized i32 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_i64() {
        let errors = try_analyze("pub fn test() { let x: i64; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized i64 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_u32() {
        let errors = try_analyze("pub fn test() { let x: u32; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized u32 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_bool() {
        let errors = try_analyze("pub fn test() { let x: bool; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized bool should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_struct() {
        let errors = try_analyze("struct P { x: i32; }\npub fn test() { let p: P; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized struct should fail analysis: {errors:?}"
        );
    }
}

/// Drives code generation past the analysis pass and asserts it *refused* with a
/// message containing `needle`.
///
/// Every backstop row goes through here rather than through a `Result`, because
/// the whole point of the row is that the backend produced a diagnostic instead
/// of aborting, and `is_err()` cannot tell those apart.
#[track_caller]
fn assert_codegen_rejects(source: &str, needle: &str) {
    match codegen_attempt(source, AnalysisMode::Skip) {
        CodegenAttempt::Ok(_) => {
            panic!("code generation must refuse this program, it produced a module")
        }
        CodegenAttempt::Panicked(payload) => {
            panic!("code generation must refuse, not crash: {payload}")
        }
        CodegenAttempt::Rejected(message) => assert!(
            message.contains(needle),
            "expected a message containing {needle:?}, got: {message}"
        ),
    }
}

/// Drives code generation with the analysis pass *running* and asserts it
/// refused with a message containing `needle`.
///
/// For the shapes no analysis rule owns. Running analysis is the assertion:
/// the message has to be the backend's own, so a rule that started rejecting
/// the fixture would report it in its own words and turn the row red rather
/// than silently taking the refusal over.
#[track_caller]
fn assert_codegen_rejects_with_analysis(source: &str, needle: &str) {
    match codegen_attempt(source, AnalysisMode::Run) {
        CodegenAttempt::Ok(_) => {
            panic!("code generation must refuse this program, it produced a module")
        }
        CodegenAttempt::Panicked(payload) => {
            panic!("code generation must refuse, not crash: {payload}")
        }
        CodegenAttempt::Rejected(message) => assert!(
            message.contains(needle),
            "expected a message containing {needle:?}, got: {message}"
        ),
    }
}

/// Drives code generation from the *partial* typed context the lossless
/// type-check entry point returns, and asserts it refused with a message
/// containing `needle`.
///
/// The shapes this serves are ones the type checker rejects, so the fatal entry
/// point never reaches the backend with them. A library consumer that ignores
/// the diagnostics it was handed does reach it, and that consumer is exactly who
/// the backstop exists for.
#[track_caller]
fn assert_codegen_rejects_ignoring_diagnostics(source: &str, needle: &str) {
    let arena = crate::utils::build_ast(source.to_string());
    let outcome = inference_type_checker::check_with_diagnostics(arena);
    assert!(
        !outcome.errors.is_empty(),
        "this fixture exists because the type checker rejects it; it no longer does"
    );
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inference_wasm_codegen::codegen(
            &outcome.typed_context,
            "output",
            inference_wasm_codegen::CodegenOptions::default(),
        )
    }));
    match attempt {
        Ok(Ok(_)) => panic!("code generation must refuse this program, it produced a module"),
        Ok(Err(error)) => {
            let message = error.to_string();
            assert!(
                message.contains(needle),
                "expected a message containing {needle:?}, got: {message}"
            );
        }
        Err(payload) => panic!(
            "code generation must refuse, not crash: {}",
            crate::utils::panic_message(&*payload)
        ),
    }
}

/// A parameter declared by its type alone is rejected by A050 with a source
/// location, and `_: T` is the spelling the rule leaves standing. These rows pin
/// the backend's own refusal of the bare form for a caller that skips analysis;
/// the accepting half lives with the `unnamed_params` goldens.
mod bare_type_parameter {
    use super::assert_codegen_rejects;

    #[test]
    fn bare_type_parameter_is_refused() {
        cov_mark::check!(wasm_codegen_bare_type_param_rejected);
        assert_codegen_rejects(
            "fn t(a: i32, i32) -> i32 { return a; } pub fn main() -> i32 { return t(1, 2); }",
            "a parameter declared by its type alone (`i32`)",
        );
    }

    /// The message names A050, so a reader who reaches this through a library
    /// call is pointed at the rule that owns the located diagnostic.
    #[test]
    fn bare_type_parameter_names_the_owning_rule() {
        assert_codegen_rejects(
            "pub fn f(i32) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "A050",
        );
    }

    /// An array spells its element type the way the source does, not the way the
    /// type checker's `Display` capitalizes it, because the fix the message
    /// suggests is a type the reader has to be able to write down.
    #[test]
    fn bare_array_type_parameter_renders_the_source_spelling() {
        assert_codegen_rejects(
            "pub fn f([i32; 2]) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "a parameter declared by its type alone (`[i32; 2]`)",
        );
    }

    /// A unit-typed `_` is the one shape the unnamed-parameter lowering has to
    /// answer for itself: it reaches the `_` arm before any unit check, and a
    /// parameter with no value type has no slot to occupy.
    #[test]
    fn unit_typed_ignored_parameter_is_refused() {
        cov_mark::check!(wasm_codegen_unit_ignored_parameter_rejected);
        assert_codegen_rejects(
            "pub fn f(_: ()) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "a unit-typed parameter written `_`",
        );
    }
}

/// A `string`-typed declaration or a string literal is rejected by A048 with a
/// source location. These rows drive code generation past that rule and pin that
/// the backend refuses each of the three distinct routes a `string` takes into
/// it: the literal in expression position, the layout of a value that contains
/// one, and a signature that names the type. The three produce three different
/// messages, and each row pins its own.
mod string_values {
    use super::assert_codegen_rejects;

    #[test]
    fn string_literal_is_refused() {
        cov_mark::check!(wasm_codegen_string_literal_rejected);
        assert_codegen_rejects(
            r#"pub fn main() -> i32 { let s: string = "hi"; return 1; }"#,
            "a string literal",
        );
    }

    #[test]
    fn string_array_element_is_refused_at_the_layout() {
        assert_codegen_rejects(
            r#"pub fn main() -> i32 { let a: [string; 2] = ["a", "b"]; return 1; }"#,
            "a `string` value in memory",
        );
    }

    #[test]
    fn string_struct_field_is_refused_at_the_layout() {
        assert_codegen_rejects(
            r#"struct S { s: string; } pub fn main() -> i32 { let v: S = S { s: "x" }; return 1; }"#,
            "a `string` value in memory",
        );
    }

    /// The signature route has its own message — the type never reaches a
    /// layout, so "in memory" would be the wrong thing to say — and it carries a
    /// source position because the parameter knows where it was written.
    #[test]
    fn string_parameter_is_refused_at_the_signature() {
        assert_codegen_rejects(
            "pub fn f(s: string) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "a `string` value has no WebAssembly lowering",
        );
    }
}

/// A unit value in a position that would have to hold one is rejected by A049
/// with a source location. These rows pin the backend's own refusal at each of
/// the three routes: the parameter slot, the binding's local, and the layout of
/// a value that contains one.
mod unit_values {
    use super::assert_codegen_rejects;

    #[test]
    fn unit_parameter_is_refused() {
        cov_mark::check!(wasm_codegen_unit_parameter_rejected);
        assert_codegen_rejects(
            "pub fn f(u: ()) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "the unit-typed parameter `u`",
        );
    }

    #[test]
    fn unit_binding_is_refused() {
        cov_mark::check!(wasm_codegen_unit_binding_rejected);
        assert_codegen_rejects(
            "pub fn main() -> i32 { let u: () = (); return 1; }",
            "the unit-typed binding `u`",
        );
    }

    #[test]
    fn unit_array_element_is_refused_at_the_layout() {
        assert_codegen_rejects(
            "pub fn main() -> i32 { let a: [(); 2] = [(), ()]; return 1; }",
            "a unit value in memory",
        );
    }

    #[test]
    fn unit_struct_field_is_refused_at_the_layout() {
        assert_codegen_rejects(
            "struct S { u: (); } pub fn main() -> i32 { let v: S = S { u: () }; return 1; }",
            "a unit value in memory",
        );
    }
}

/// A binding with no initializer is rejected by A025. Code generation reserves a
/// local for it in the pre-scan, so without a backstop the statement would emit
/// nothing and leave that local holding whatever the frame started with.
mod uninitialized_binding_backstop {
    use super::assert_codegen_rejects;

    #[test]
    fn uninitialized_binding_is_refused() {
        cov_mark::check!(wasm_codegen_uninitialized_binding_rejected);
        assert_codegen_rejects(
            "pub fn main() -> i32 { let x: i32; return 1; }",
            "the uninitialized binding `x`",
        );
    }

    #[test]
    fn uninitialized_binding_names_the_owning_rule() {
        assert_codegen_rejects("pub fn main() -> i32 { let x: i32; return 1; }", "A025");
    }
}

/// A number literal whose text does not fit the width the type checker recorded
/// on it. A022 reports it with a source location; without the backstop the
/// lowering would abort on a failed `parse`.
mod number_literal_backstop {
    use super::assert_codegen_rejects;

    #[test]
    fn out_of_range_unsigned_literal_is_refused() {
        cov_mark::check!(wasm_codegen_number_literal_rejected);
        assert_codegen_rejects(
            "pub fn main() -> u8 { return 300; }",
            "the number literal `300`, whose text does not fit the unsigned 8-bit width",
        );
    }

    #[test]
    fn out_of_range_literal_names_the_owning_rule() {
        assert_codegen_rejects("pub fn main() -> u8 { return 300; }", "A022");
    }
}

/// Shapes the *type checker* rejects, put to the backend anyway through the
/// lossless entry point. Each is a construct with no lowering that would
/// otherwise emit a malformed body: a `return` on an empty stack, an enum tag
/// for a type that is not an enum, a store with no destination, an
/// uninstantiated generic.
mod type_checker_backstops {
    use super::assert_codegen_rejects_ignoring_diagnostics;

    /// A unit expression produces nothing, so returning one from a function that
    /// declares a result would emit `return` with an empty operand stack.
    #[test]
    fn unit_returned_from_a_value_function_is_refused() {
        cov_mark::check!(wasm_codegen_unit_return_from_value_fn_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn f() -> i32 { return (); } pub fn main() -> i32 { return 1; }",
            "a unit value returned from a function that declares a result",
        );
    }

    /// The parenthesized spelling denotes the same value and must be refused the
    /// same way; a check that keyed on the bare literal would let this one
    /// through into a malformed body.
    #[test]
    fn parenthesized_unit_returned_from_a_value_function_is_refused() {
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn f() -> i32 { return (()); } pub fn main() -> i32 { return 1; }",
            "a unit value returned from a function that declares a result",
        );
    }

    #[test]
    fn a_variant_path_on_a_non_enum_type_is_refused() {
        cov_mark::check!(wasm_codegen_non_enum_type_member_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "struct P { x: i32; } pub fn main() -> i32 { return P::Missing; }",
            "on a non-enum type",
        );
    }

    #[test]
    fn an_assignment_to_a_literal_is_refused() {
        cov_mark::check!(wasm_codegen_unsupported_assign_target_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn main() -> i32 { 1 = 2; return 1; }",
            "an assignment to a target that is not a variable, an array element or a field",
        );
    }

    /// A qualified path to a proof-only specification function resolves to no
    /// executable index, so the callee matches none of the lowerable call forms.
    #[test]
    fn a_qualified_call_to_a_spec_function_is_refused() {
        cov_mark::check!(wasm_codegen_unresolved_callee_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "spec S { fn h() -> i32 { return 1; } } pub fn main() -> i32 { return S::h(); }",
            "a call whose callee resolves to no lowerable form",
        );
    }

    /// Exponentiation has no WebAssembly instruction and no expansion, so the
    /// operator never had a lowering at all.
    #[test]
    fn the_power_operator_is_refused() {
        cov_mark::check!(wasm_codegen_pow_operator_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn main() -> i32 { return 2 ** 3; }",
            "the `**` operator",
        );
    }

    /// A field access resolves a byte offset inside a struct layout, so an
    /// accessed expression of any other type has no layout to resolve one in.
    #[test]
    fn a_field_access_on_a_scalar_is_refused() {
        cov_mark::check!(wasm_codegen_member_access_on_non_struct_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn main() -> i32 { let x: i32 = 1; return x.f; }",
            "a field access on `i32`, a type with no fields",
        );
    }

    /// A type application in expression position names no definition to emit a
    /// call or a value for, so code generation refuses it.
    #[test]
    fn a_generic_type_in_expression_position_is_refused() {
        cov_mark::check!(wasm_codegen_generic_type_expression_rejected);
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn main() -> i32 { let x: i32 = t i32'; return x; }",
            "a generic type in expression position",
        );
    }

    /// The message names A051, so a reader who reaches this through a library
    /// call is pointed at the rule that owns the located diagnostic. Expression
    /// position is the one generic route whose refusal is raised here rather
    /// than beside its siblings, so it needs its own row: nothing else in the
    /// suite reads the rule this site reports.
    #[test]
    fn a_generic_type_in_expression_position_names_the_owning_rule() {
        assert_codegen_rejects_ignoring_diagnostics(
            "pub fn main() -> i32 { let x: i32 = t i32'; return x; }",
            "A051",
        );
    }
}

/// Generic code is rejected by A051 with a source location. These rows drive
/// code generation past that rule and pin the backend's own refusal of each
/// route a generic takes into it: the declaration that binds a type parameter,
/// the type application in a signature, the same application buried in an array
/// element, and the one that reaches a layout helper rather than a lowering arm.
///
/// Each route matters on its own, because two of them produce a *module* when
/// the backend does not refuse: a declaration whose binder shadows a struct
/// emits a body WebAssembly validation rejects, and an array of a type
/// application emits an export that looks like an ordinary `(param i32)`.
mod generic_code {
    use super::assert_codegen_rejects;

    /// Nothing substitutes a type argument, so a type parameter reaches lowering
    /// standing for no type at all.
    #[test]
    fn a_generic_function_declaration_is_refused() {
        cov_mark::check!(wasm_codegen_generic_function_rejected);
        assert_codegen_rejects(
            "fn id T'(x: T) -> T { return x; } pub fn main() -> i32 { return 1; }",
            "a function declared with type parameters",
        );
    }

    /// The message names A051, so a reader who reaches this through a library
    /// call is pointed at the rule that owns the located diagnostic.
    #[test]
    fn a_generic_declaration_names_the_owning_rule() {
        assert_codegen_rejects(
            "fn id T'(x: T) -> T { return x; } pub fn main() -> i32 { return 1; }",
            "A051",
        );
    }

    /// A type application renders in the source spelling (`Base Type'`), which
    /// is what the reader wrote, and carries the position of the annotation.
    #[test]
    fn a_generic_signature_type_is_refused() {
        assert_codegen_rejects(
            "struct Q { x: i32; } pub fn f(v: Q i32') -> i32 { return 1; } \
             pub fn main() -> i32 { return 1; }",
            "the type `Q i32'`, which gives type arguments to a declaration that accepts none",
        );
    }

    /// An array is a pointer to bytes, so an element with no layout leaves the
    /// signature describing a pointer into nothing. Both depths are asserted:
    /// a descent that stopped at the first element type would still refuse the
    /// flat spelling.
    #[test]
    fn a_generic_array_element_is_refused_at_every_depth() {
        let needle =
            "the type `Q i32'`, which gives type arguments to a declaration that accepts none";
        assert_codegen_rejects(
            "struct Q { x: i32; } pub fn g(p: [Q i32'; 2]) -> i32 { return 1; } \
             pub fn main() -> i32 { return 1; }",
            needle,
        );
        assert_codegen_rejects(
            "struct Q { x: i32; } pub fn g(p: [[Q i32'; 2]; 3]) -> i32 { return 1; } \
             pub fn main() -> i32 { return 1; }",
            needle,
        );
    }

    /// A struct field reaches the layout helpers rather than a lowering arm, so
    /// it is refused against a type with no source position to report.
    #[test]
    fn a_generic_typed_field_is_refused_in_memory() {
        assert_codegen_rejects(
            "struct Q { x: i32; } struct P { y: Q i32'; } pub fn f(p: P) -> i32 { return 1; } \
             pub fn main() -> i32 { return 1; }",
            "a value of a generic type in memory",
        );
    }

    /// A type parameter that shadows a declared struct is the shape that fails
    /// silently without this refusal: the type checker binds the name to the
    /// binder and the backend binds it to the struct, so every signature lowers,
    /// the call site does not match the body, and code generation returns a
    /// module WebAssembly validation rejects.
    #[test]
    fn a_shadowing_generic_does_not_reach_a_module() {
        assert_codegen_rejects(
            "struct T { x: i32; } fn g T'(a: T) -> T { return a; } \
             pub fn f(n: i32) -> i32 { return g(n); } pub fn main() -> i32 { return 1; }",
            "a function declared with type parameters",
        );
    }
}

/// Types a signature may name that no WebAssembly value type stands for.
///
/// Both rows reach one helper: an array asks its innermost element whether it
/// has a value type at all.
///
/// The array-of-a-function-type row runs the analysis pass, because nothing
/// before code generation refuses that shape and the backend is the only place
/// it can be caught — running analysis is what keeps that claim honest, since a
/// rule that started owning it would report it in its own words and turn the row
/// red. The unit-element row is an ordinary backstop instead: A049 owns it, so
/// it skips analysis like every other backstop here.
mod signature_types_with_no_value {
    use super::{assert_codegen_rejects, assert_codegen_rejects_with_analysis};

    /// An array is lowered as a pointer to its first element, so an element with
    /// no value type leaves the export describing a pointer into bytes nothing
    /// can size — and it does so while looking like an ordinary `(param i32)`.
    /// Restoring an unconditional `Ok(Some(ValType::I32))` for every array turns
    /// this red by producing a module.
    #[test]
    fn an_array_of_a_function_type_is_refused() {
        assert_codegen_rejects_with_analysis(
            "pub fn g(p: [fn(i32) -> i32; 2]) -> i32 { return 1; } \
             pub fn main() -> i32 { return 1; }",
            "unsupported type in WASM codegen: a function type",
        );
    }

    /// A unit element occupies no bytes, so the array it sits in has no stride.
    /// It is the one element kind that answers "no value type" without an error
    /// of its own — `-> ()` is how a function says it returns nothing — so
    /// letting that answer fall through to `Ok(Some(ValType::I32))` instead of
    /// minting a refusal turns this red by producing a module.
    #[test]
    fn an_array_of_the_unit_type_is_refused() {
        assert_codegen_rejects(
            "pub fn g(p: [(); 2]) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            "an array whose element type is the unit type",
        );
    }
}

/// A type with no value written as the type of a **struct field**.
///
/// A field is not a signature: the type checker erases it to a name and a shape
/// before code generation sees it, so the layout that meets it holds neither the
/// position it was written at nor the scope its bare names were read in. A
/// `spec` block's name is such a name — nothing canonicalizes it, because it
/// denotes neither a struct nor an enum — so it survives to the layout
/// unresolved, and reporting it as a name the layout failed to find blames the
/// type checker for a program the type checker accepted. The rows here assert
/// the field earns the refusal the same type earns in a signature; answering
/// from the layout's own erased view instead turns them red.
mod struct_field_types_with_no_value {
    use super::assert_codegen_rejects_with_analysis;
    use crate::utils::{AnalysisMode, CodegenAttempt, codegen_attempt, codegen_attempt_with_mode};

    /// A struct whose field is a struct whose field is a `spec` name. The
    /// refusal must be the one that names the type with no lowering, and must be
    /// the only thing reported: the outer struct nests one level, which is
    /// supported.
    ///
    /// Reading the inner struct's unresolved field name as a *compound* type is
    /// what turns this red — A026 then fires first, reporting nesting the program
    /// does not have. The identical program with an `i32` field compiles, so the
    /// nesting claim cannot be about the nesting; that control is the row below.
    #[test]
    fn a_nested_struct_reports_the_field_that_has_no_lowering_not_the_nesting() {
        let source = "spec Q { fn q() { } } struct Inner { f: Q; } struct Outer { i: Inner; } \
                      pub fn g(v: Outer) -> i32 { return 1; } pub fn main() -> i32 { return 1; }";
        match codegen_attempt(source, AnalysisMode::Run) {
            CodegenAttempt::Ok(_) => {
                panic!("code generation must refuse this program, it produced a module")
            }
            CodegenAttempt::Panicked(payload) => {
                panic!("code generation must refuse, not crash: {payload}")
            }
            CodegenAttempt::Rejected(message) => {
                assert!(
                    message.contains("unsupported type in WASM codegen: Q"),
                    "the field with no lowering must be what is reported, got: {message}"
                );
                assert!(
                    !message.contains("A026"),
                    "one supported level of nesting must not be reported as nesting, got: {message}"
                );
            }
        }
    }

    /// The identical shape with a lowerable inner field compiles, which is what
    /// makes the row above an assertion about the field's type rather than about
    /// the nesting. Without this control, a change that started rejecting every
    /// nested struct would keep that row green.
    #[test]
    fn the_same_nesting_with_a_lowerable_inner_field_compiles() {
        let source = "struct Inner { f: u64; } struct Outer { i: Inner; } \
                      pub fn g(v: Outer) -> i32 { return 1; } pub fn main() -> i32 { return 1; }";
        assert!(
            matches!(
                codegen_attempt(source, AnalysisMode::Run),
                CodegenAttempt::Ok(_)
            ),
            "one level of struct nesting is supported and must still produce a module"
        );
    }

    /// A `spec` block's name written as a field type. It reaches the layout as a
    /// name nothing resolves, and no rule owns it, so the refusal is the one the
    /// same name earns in a signature — and, either way, not an accusation
    /// against a phase that accepted the program.
    #[test]
    fn a_spec_name_in_a_struct_field_does_not_blame_an_earlier_phase() {
        let source = "spec Q { fn q() { } } struct S { f: Q; } pub fn g(v: S) -> i32 { return 1; } \
                      pub fn main() -> i32 { return 1; }";
        match codegen_attempt(source, AnalysisMode::Run) {
            CodegenAttempt::Ok(_) => {
                panic!("code generation must refuse this program, it produced a module")
            }
            CodegenAttempt::Panicked(payload) => {
                panic!("code generation must refuse, not crash: {payload}")
            }
            CodegenAttempt::Rejected(message) => {
                assert!(
                    message.contains("unsupported type in WASM codegen: Q"),
                    "a spec name must be refused as the type it is, got: {message}"
                );
                assert!(
                    !message.contains("should have caught this"),
                    "a program the pipeline accepted must not be blamed on an earlier phase, \
                     got: {message}"
                );
            }
        }
    }

    /// A struct declared **inside a `spec` block**, whose field is a spec name.
    ///
    /// Such a struct is not among its file's top-level definitions, so the walk
    /// that recovers the field's type node has to enter the `spec` scope the
    /// struct is declared in. Reading the file alone finds no declaration of
    /// `S` at all, the classification never happens, and the refusal degrades
    /// back to `struct 'Q' not found in type context -- the type checker should
    /// have caught this` — an accusation against a phase that accepted the
    /// program.
    ///
    /// Proof mode is what makes the row reachable: compile mode drops
    /// specification functions from the artifact, so nothing asks a spec-inner
    /// struct for a layout there.
    #[test]
    fn a_spec_inner_struct_is_read_in_the_spec_scope_that_declares_it() {
        let source = "spec Q { struct S { f: Q; } fn q(v: S) -> i32 { return 1; } } \
                      pub fn main() -> i32 { return 1; }";
        match codegen_attempt_with_mode(
            source,
            AnalysisMode::Run,
            inference_wasm_codegen::CompilationMode::Proof,
        ) {
            CodegenAttempt::Ok(_) => {
                panic!("code generation must refuse this program, it produced a module")
            }
            CodegenAttempt::Panicked(payload) => {
                panic!("code generation must refuse, not crash: {payload}")
            }
            CodegenAttempt::Rejected(message) => {
                assert!(
                    message.contains("unsupported type in WASM codegen: Q"),
                    "a spec name must be refused as the type it is, got: {message}"
                );
                assert!(
                    !message.contains("should have caught this"),
                    "a program the pipeline accepted must not be blamed on an earlier phase, \
                     got: {message}"
                );
            }
        }
    }

    /// A struct reached only through an array parameter gets no layout while its
    /// signature is lowered, so the first field access on one is what asks for
    /// the layout — and is therefore where a field with no lowering is first
    /// refused. The refusal has to travel out as a refusal: asserting the layout
    /// succeeded there turns a diagnostic the caller was about to print into a
    /// process abort, which `CodegenAttempt::Panicked` is what distinguishes.
    #[test]
    fn a_field_access_through_an_array_parameter_refuses_instead_of_aborting() {
        assert_codegen_rejects_with_analysis(
            "spec Q { fn q() { } } struct S { f: Q; g: i32; } \
             pub fn h(v: [S; 2]) -> i32 { return v[0].g; } pub fn main() -> i32 { return 1; }",
            "unsupported type in WASM codegen: Q",
        );
    }

    /// A `spec` name below an **array-typed field**. The refusal is the same one,
    /// but the name the layout fails on is not the field's own type: the field is
    /// `[Q; 2]`, and it is the array's *element*, asked for a width so the array
    /// can be turned into a byte count, that has no lowering.
    ///
    /// That is what makes this row distinct from every other field here, all of
    /// which name the unlowerable type directly. The classification does not read
    /// the failing name at all — it recovers the field's whole declared type node
    /// and re-asks the signature helper — so the right message comes out only
    /// because that helper descends an array to its element. The two halves have
    /// to compose, and this is the only row where they must: excluding an
    /// array-typed field from the classification leaves this program reporting a
    /// name the layout failed to find and blaming a phase that accepted it, while
    /// every row below stays green.
    ///
    /// A `spec` name carries no source position, so there is no column to assert
    /// here: what the row pins is which message is produced, not where.
    #[test]
    fn an_array_typed_field_reports_the_element_that_has_no_lowering() {
        match codegen_attempt(
            "spec Q { fn q() { } } struct S { f: [Q; 2]; } \
             pub fn g(v: S) -> i32 { return 1; } pub fn main() -> i32 { return 1; }",
            AnalysisMode::Run,
        ) {
            CodegenAttempt::Ok(_) => {
                panic!("code generation must refuse this program, it produced a module")
            }
            CodegenAttempt::Panicked(payload) => {
                panic!("code generation must refuse, not crash: {payload}")
            }
            CodegenAttempt::Rejected(message) => {
                assert!(
                    message.contains("unsupported type in WASM codegen: Q"),
                    "the array's element is what has no lowering and what must be named, \
                     got: {message}"
                );
                assert!(
                    !message.contains("should have caught this"),
                    "a program the pipeline accepted must not be blamed on an earlier phase, \
                     got: {message}"
                );
            }
        }
    }

    /// A declaration nothing lays out, recording a gap rather than blessing it.
    ///
    /// Every refusal above is raised by the layout, and a struct no lowered
    /// signature names is never laid out — so this program is accepted and writes
    /// a module, even though the field it declares has no lowering. Nothing owns
    /// that: a `spec` name in a type position is refused by no analysis rule, and
    /// A051's own suite asserts the opposite policy for its carrier, that an
    /// unused struct with a field the layout has no size for must not compile.
    ///
    /// Until a declaration-anchored rule owns it, this is the behaviour, and it
    /// is asserted so that closing the gap is a deliberate edit to this row.
    #[test]
    fn a_struct_declaration_no_signature_names_is_accepted_today() {
        let source = "spec Q { fn q() { } } struct Inner { f: Q; } struct Outer { i: Inner; } \
                      pub fn main() -> i32 { return 1; }";
        match codegen_attempt(source, AnalysisMode::Run) {
            CodegenAttempt::Ok(_) => {}
            CodegenAttempt::Panicked(payload) => {
                panic!("an unused declaration must not crash the compiler: {payload}")
            }
            CodegenAttempt::Rejected(message) => {
                assert!(
                    !message.contains("A026"),
                    "a field whose name resolves to no struct is not a nested compound type, and \
                     reporting it as one blames nesting the program does not have: {message}"
                );
                panic!(
                    "this row records that nothing owns an unused declaration with an \
                     unlowerable field; a rule that now owns it must replace this row rather \
                     than change its expectation: {message}"
                );
            }
        }
    }
}

/// A `@` the lowering has no draw for: over a type with no value representation,
/// or in a compound position that binds no variable to fill.
///
/// A compound draw fills a frame slot leaf by leaf and the slot is keyed by the
/// binding that owns it, so an element position — which names no binding — has
/// nothing to fill. A014, A038, A039 and A040 reject those positions with a
/// source location; these rows pin the backend's own refusal.
mod uzumaki_backstops {
    use super::assert_codegen_rejects;

    #[test]
    fn a_draw_over_a_string_is_refused() {
        cov_mark::check!(wasm_codegen_undrawable_uzumaki_rejected);
        assert_codegen_rejects(
            "pub fn main() { forall { let s: string = @; } }",
            "an `@` over `string`, a type with no value representation",
        );
    }

    /// A struct `@` at an array-element position. The element names no binding,
    /// so there is no frame slot to fill leaf by leaf.
    #[test]
    fn a_struct_draw_at_an_array_element_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_struct_uzumaki_rejected);
        assert_codegen_rejects(
            "struct Point { x: i32; y: i32; } \
             pub fn main() { forall { let a: [Point; 2] = [Point { x: 0, y: 0 }, @]; } }",
            "a struct `@` in a position that binds no variable",
        );
    }

    /// A draw over a struct one of whose fields is itself a struct. A draw
    /// fills one leaf per store and a struct-typed field is not a leaf.
    #[test]
    fn a_draw_over_a_struct_with_a_struct_field_is_refused() {
        cov_mark::check!(wasm_codegen_nested_struct_field_uzumaki_rejected);
        assert_codegen_rejects(
            "struct Inner { x: i32; } struct Outer { i: Inner; } \
             pub fn main() { forall { let o: Outer = @; } }",
            "an `@` over a struct whose field `i` is itself a struct",
        );
    }

    /// The array analogue: the outer `@` of a two-dimensional array literal
    /// draws a whole `[i32; 2]` at a position with no binding of its own.
    #[test]
    fn an_array_draw_at_an_array_element_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_array_uzumaki_rejected);
        assert_codegen_rejects(
            "pub fn main() { forall { let a: [[i32; 2]; 2] = [@, [1, 2]]; } }",
            "an array `@` in a position that binds no variable",
        );
    }
}

/// A compound-returning call in a position that provides no destination to write
/// the result into. The `sret` convention needs one, and only a variable
/// initializer and a `return` supply it; A016, A017 and A018 reject the rest.
mod compound_return_position_backstops {
    use super::assert_codegen_rejects;

    #[test]
    fn a_compound_returning_method_call_in_expression_position_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_compound_method_call_rejected);
        assert_codegen_rejects(
            "struct P { x: i32; fn dup(self) -> P { return P { x: self.x }; } } \
             pub fn main() -> i32 { let p: P = P { x: 1 }; return p.dup().x; }",
            "no destination to write the result into",
        );
    }

    #[test]
    fn a_compound_returning_associated_call_in_expression_position_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_compound_associated_call_rejected);
        assert_codegen_rejects(
            "struct P { x: i32; fn make() -> P { return P { x: 1 }; } } \
             pub fn main() -> i32 { return P::make().x; }",
            "no destination to write the result into",
        );
    }
}

/// A compound literal in a position that binds no variable. The literal is
/// written into the frame slot of the binding that owns it, so an argument —
/// which names no binding — has nothing to write into. A012 rejects the
/// argument position and A015 every other unsupported one; these rows pin the
/// backend's own refusal.
mod compound_literal_position_backstops {
    use super::assert_codegen_rejects;

    #[test]
    fn a_struct_literal_passed_as_an_argument_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_struct_literal_rejected);
        assert_codegen_rejects(
            "struct P { x: i32; } \
             fn g(p: P) -> i32 { return p.x; } \
             pub fn main() -> i32 { return g(P { x: 1 }); }",
            "a struct literal in a position that binds no variable",
        );
    }

    #[test]
    fn an_array_literal_passed_as_an_argument_is_refused() {
        cov_mark::check!(wasm_codegen_slotless_array_literal_rejected);
        assert_codegen_rejects(
            "fn g(a: [i32; 2]) -> i32 { return a[0]; } \
             pub fn main() -> i32 { return g([1, 2]); }",
            "an array literal in a position that binds no variable",
        );
    }
}

/// The rule ids the crate's diagnostics name must be rules that exist.
///
/// A backstop message points the reader at the rule that owns the located
/// version of the same complaint. If that rule is renamed or retired, the
/// message sends the reader looking for a catalog entry that is not there —
/// a failure no message-needle test can see, because the needle still matches.
mod named_rules_exist {
    #[test]
    fn every_rule_a_codegen_message_names_is_a_registered_rule() {
        let registered: Vec<&str> = inference_analysis::rules::all_rules()
            .iter()
            .map(|r| r.id())
            .collect();
        assert!(
            registered.len() > 40,
            "the rule registry looks empty, so this check would pass vacuously: {registered:?}"
        );
        for named in inference_wasm_codegen::NAMED_ANALYSIS_RULES {
            assert!(
                registered.contains(named),
                "a code generation diagnostic names `{named}`, which is not a registered \
                 analysis rule; the message would point the reader at nothing"
            );
        }
    }
}

mod unsupported_compound_types {
    use crate::utils::try_codegen;

    #[test]
    fn array_of_arrays_succeeds() {
        let result = try_codegen(
            "pub fn test() -> i32 { let a: [[i32; 2]; 2] = [[1,2],[3,4]]; return a[0][0]; }",
        );
        assert!(
            result.is_ok(),
            "multi-dimensional array literal init should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn array_of_structs_succeeds() {
        let result = try_codegen(
            "struct P { x: i32; }\npub fn test() -> i32 { let a: [P; 2] = [P{x:1}, P{x:2}]; return 1; }",
        );
        assert!(
            result.is_ok(),
            "array of structs should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn struct_with_array_field_succeeds() {
        let result = try_codegen(
            "struct S { arr: [i32; 2]; }\npub fn test() -> i32 { let s: S = S { arr: [1, 2] }; return 1; }",
        );
        assert!(
            result.is_ok(),
            "struct with array field should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn nested_array_of_structs_succeeds() {
        let result = try_codegen(
            "struct P { x: i32; y: i32; }\npub fn test() -> i32 { let g: [[P; 2]; 2] = [[P{x:1,y:2}, P{x:3,y:4}], [P{x:5,y:6}, P{x:7,y:8}]]; return g[1][0].x; }",
        );
        assert!(
            result.is_ok(),
            "nested array-of-structs literal init should succeed codegen: {:?}",
            result.err()
        );
    }
}

mod uzumaki_compound_types {
    use crate::utils::wasm_codegen_no_analysis;

    #[test]
    fn uzumaki_struct_in_forall() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let wasm_bytes = wasm_codegen_no_analysis(
            "struct P { x: i32; }\npub fn test() { forall { let p: P = @; } }",
        );
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct uzumaki WASM is invalid: {e}"));
    }

    /// A `@` as the return expression of a compound-returning function. The sret
    /// lowering has no form for it, and the refusal it raises is now reported
    /// rather than folded into a panic, so these rows assert a rejection.
    #[test]
    fn uzumaki_struct_return() {
        super::assert_codegen_rejects(
            "struct P { x: i32; }\npub fn test() -> P { return @; }",
            "unsupported sret return expression",
        );
    }

    #[test]
    fn uzumaki_array_return() {
        super::assert_codegen_rejects(
            "pub fn test() -> [i32; 3] { return @; }",
            "unsupported sret return expression",
        );
    }
}

mod compound_reassignment {
    use crate::utils::try_codegen;

    #[test]
    fn array_literal_reassignment() {
        let result = try_codegen(
            "pub fn test() -> i32 { let mut a: [i32; 2] = [1, 2]; a = [3, 4]; return a[0]; }",
        );
        assert!(
            result.is_ok(),
            "array literal reassignment should succeed codegen"
        );
    }
}

mod extern_function_call {
    use crate::utils::build_ast;

    #[test]
    fn extern_function_call_rejected_before_codegen() {
        let source = "external fn print(val: i32) -> ();\npub fn main() { print(42); }";
        let arena = build_ast(source.to_string());
        let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should pass for extern function call")
            .typed_context();
        let analysis_result = inference_analysis::analyze(&typed_context);
        assert!(
            analysis_result.is_err(),
            "call to extern function should be rejected by analysis"
        );
        let err = analysis_result.unwrap_err().to_string();
        assert!(
            err.contains("external function") && err.contains("print"),
            "expected analysis error about external function call, got: {err}"
        );
    }
}

mod duplicate_local_name {
    use crate::utils::{AnalysisMode, CodegenAttempt, codegen_attempt};

    /// The issue repro — two sequential sibling `if`s each declaring `x` — is
    /// rejected by analysis rule A041, so it only reaches codegen on the
    /// no-analysis path. There, `pre_scan_locals`' flat `locals_map` still
    /// catches the duplicate as a defense-in-depth backstop. This pins that the
    /// backstop survives and that A041 and codegen agree on this shape.
    #[test]
    fn duplicate_local_backstop_assert_still_fires_without_analysis() {
        match codegen_attempt(
            r#"pub fn f(c: bool) -> i32 { if c { let x: i32 = 1; return x; } if !c { let x: i32 = 2; return x; } let z: i32 = 0; return z; }"#,
            AnalysisMode::Skip,
        ) {
            CodegenAttempt::Panicked(payload) => assert!(
                payload.contains("collides with an existing entry in locals_map"),
                "unexpected panic payload: {payload}"
            ),
            // Deliberately the one row that expects an abort rather than a
            // diagnostic: this backstop is an `assert!` on an invariant the
            // pre-scan owns, not a refusal of a construct with no lowering, and
            // turning it into one would report a compiler bug as a user error.
            other => panic!(
                "the duplicate-local assertion must fire: {}",
                match other {
                    CodegenAttempt::Ok(_) => "a module was produced".to_string(),
                    CodegenAttempt::Rejected(message) => format!("codegen refused with {message}"),
                    CodegenAttempt::Panicked(_) => unreachable!(),
                }
            ),
        }
    }
}

mod misplaced_self_parameter {
    use crate::utils::build_ast;
    use inference_type_checker::check_with_diagnostics;

    /// Generates WASM from the *partial* typed context the lossless type-check
    /// entry point returns, catching panics.
    ///
    /// The fatal entry point `try_codegen` uses aborts on the frontend's
    /// misplaced-receiver diagnostic, so codegen is never reached through it.
    /// Going through `check_with_diagnostics` keeps the recovered context and
    /// feeds it to codegen anyway, which is how a library consumer that ignores
    /// diagnostics would drive the backend.
    fn try_codegen_ignoring_diagnostics(source: &str) -> Result<(), String> {
        let arena = build_ast(source.to_string());
        let outcome = check_with_diagnostics(arena);
        assert!(
            !outcome.errors.is_empty(),
            "the frontend must have rejected this source before codegen"
        );
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inference_wasm_codegen::codegen(
                &outcome.typed_context,
                "output",
                inference_wasm_codegen::CodegenOptions::default(),
            )
        }))
        .map_err(|panic| crate::utils::panic_message(&*panic))?
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// A caller that ignores the frontend diagnostic must hit the backend's own
    /// guard rather than emit a module whose receiver slot holds an argument.
    #[test]
    fn receiver_in_second_slot_hits_the_codegen_guard() {
        let result = try_codegen_ignoring_diagnostics(
            r#"struct Number { value: i32; fn plus(delta: i32, self) -> i32 { return self.value + delta; } } pub fn main() -> i32 { let number: Number = Number { value: 40 }; return number.plus(2); }"#,
        );
        assert!(
            result.is_err(),
            "a misplaced receiver must not reach a generated module"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("parameter `self` does not occupy the first parameter slot"),
            "unexpected error message: {err}"
        );
    }

    /// Same guard with a struct return, where the receiver's reserved slot sits
    /// behind the sret pointer — the half of the assertion that would go unpinned
    /// by the plain-return case alone.
    #[test]
    fn receiver_behind_sret_pointer_hits_the_codegen_guard() {
        let result = try_codegen_ignoring_diagnostics(
            r#"struct P { x: i32; y: i32; fn moved(dx: i32, self) -> P { return P { x: self.x + dx, y: self.y }; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; let q: P = p.moved(10); return q.x; }"#,
        );
        assert!(
            result.is_err(),
            "a misplaced receiver behind an sret pointer must not reach a generated module"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("parameter `self` does not occupy the first parameter slot"),
            "unexpected error message: {err}"
        );
    }

    /// A repeated receiver lands in a later slot too, so both backstops describe
    /// it. The duplicate is the accurate diagnosis and must be the one reported.
    #[test]
    fn duplicated_receiver_reports_the_duplicate_not_the_misplacement() {
        let result = try_codegen_ignoring_diagnostics(
            r#"struct Number { value: i32; fn plus(self, delta: i32, self) -> i32 { return self.value + delta; } } pub fn main() -> i32 { let number: Number = Number { value: 40 }; return number.plus(2); }"#,
        );
        assert!(
            result.is_err(),
            "a duplicated receiver must not reach a generated module"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("collides with an existing entry in locals_map"),
            "unexpected error message: {err}"
        );
    }
}
