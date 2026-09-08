//! A type-parameter name may be bound only once per function declaration.
//!
//! A repeated binder is not shadowing: an annotation resolves to the first one,
//! so the later binder names nothing while still raising the declared arity, and
//! every call site is then asked for a type argument that no parameter position
//! can supply. Without a diagnostic at the declaration the mistake surfaces only
//! as an uninferrable type parameter at each caller, or — for an uncalled
//! declaration — as the generic refusal, neither of which names the duplication.
//!
//! These tests pin the rejection and its span across the declaration forms that
//! can bind type parameters (free function, struct method, `spec` function, and
//! a method of a struct declared inside a `spec`), the uncalled declaration, and
//! the distinct-binder list the check must leave alone.
//!
//! Deleting the `report_duplicate_type_parameters` call from signature
//! validation turns every `rejection` test here red.

use crate::utils::build_ast;
use inference_ast::nodes::Location;
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::TypeCheckError;

fn diagnostics(source: &str) -> Vec<TypeCheckError> {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena)
        .errors
        .into_iter()
        .map(|d| d.error)
        .collect()
}

/// Asserts `source` yields exactly one diagnostic, a duplicate-type-parameter
/// report naming `function_name` and `parameter_name`, and returns its location.
///
/// The "exactly one" half is load-bearing in both directions: the repeat is
/// reported once rather than once per validation pass, and no caller-side
/// inference failure is left standing beside it.
fn single_duplicate_type_parameter(
    source: &str,
    function_name: &str,
    parameter_name: &str,
) -> Location {
    let errors = diagnostics(source);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one diagnostic, got: {errors:?}"
    );
    match &errors[0] {
        TypeCheckError::DuplicateTypeParameterName {
            function_name: got_function,
            parameter_name: got_parameter,
            location,
        } => {
            assert_eq!(got_function, function_name, "diagnostic names the function");
            assert_eq!(
                got_parameter, parameter_name,
                "diagnostic names the repeated binder"
            );
            *location
        }
        other => panic!("expected DuplicateTypeParameterName, got {other:?}"),
    }
}

mod rejection {
    use super::*;

    #[test]
    fn repeated_binder_in_free_function_rejected() {
        let source = r#"fn g T' T'(x: i32) -> i32 { return x; }"#;
        let location = single_duplicate_type_parameter(source, "g", "T");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 9),
            "the caret sits on the repeat, not on the binder it repeats"
        );
    }

    #[test]
    fn repeated_binder_in_struct_method_rejected() {
        let source = r#"struct S { x: i32; fn m T' T'(self) -> i32 { return self.x; } }"#;
        single_duplicate_type_parameter(source, "S::m", "T");
    }

    #[test]
    fn repeated_binder_in_spec_function_rejected() {
        let source = r#"spec Sp { fn q T' T'(x: i32) -> i32 { return x; } }"#;
        single_duplicate_type_parameter(source, "q", "T");
    }

    #[test]
    fn repeated_binder_in_spec_struct_method_rejected() {
        let source = r#"spec Sp { struct H { v: i32; fn m T' T'(self) -> i32 { return self.v; } } }"#;
        single_duplicate_type_parameter(source, "H::m", "T");
    }

    /// The defect is a property of the declaration. Filtering the check by the
    /// call graph, or moving it to the inference that runs per call site, turns
    /// this red while the called forms above stay green.
    #[test]
    fn uncalled_declaration_is_still_rejected() {
        let source = r#"
            fn g T' T'(x: i32) -> i32 { return x; }
            pub fn main() -> i32 { return 0; }
        "#;
        single_duplicate_type_parameter(source, "g", "T");
    }

    /// Three binders where two collide: the report names the repeated one, and
    /// the binder between them is not swept up.
    #[test]
    fn only_the_repeat_of_a_longer_binder_list_is_reported() {
        let source = r#"fn g T' U' T'(x: i32) -> i32 { return x; }"#;
        single_duplicate_type_parameter(source, "g", "T");
    }
}

mod acceptance {
    use super::*;

    /// Distinct binders are the shape the check must not touch. It reports
    /// nothing here; the declaration's remaining refusal is the generic one,
    /// which analysis owns and which does not run in this pipeline.
    #[test]
    fn distinct_binders_are_accepted() {
        let source = r#"fn pick T' U'(a: T, b: U) -> T { return a; }"#;
        assert!(
            !diagnostics(source).iter().any(|e| matches!(
                e,
                TypeCheckError::DuplicateTypeParameterName { .. }
            )),
            "distinct binders must not be reported: {:?}",
            diagnostics(source)
        );
    }

    /// A type parameter and an ordinary parameter of the same name occupy
    /// different lists, so neither check fires. Merging the two lists into one
    /// name set turns this red.
    #[test]
    fn a_binder_and_a_parameter_may_share_a_name() {
        let source = r#"fn g T'(T: i32) -> i32 { return T; }"#;
        assert!(
            diagnostics(source).is_empty(),
            "a binder and a parameter are separate namespaces: {:?}",
            diagnostics(source)
        );
    }
}
