//! A method name may be defined only once per struct, as a function name may be
//! defined only once per scope.
//!
//! Two definitions of one method are not an overload set: a call names the
//! method by its name alone, so the second definition is unreachable whatever
//! its signature. Before the frontend rejected the repeat, a struct that
//! defined a method twice type-checked and passed analysis, and code generation
//! then panicked when it assigned the second definition the function index the
//! first already held (found while reducing a fuzzing crash, #544). The frontend
//! now refuses the repeat with the diagnostic a repeated free function gets,
//! naming the method and pointing at the repeat.
//!
//! These tests pin the rejection across the receiver forms (instance method,
//! associated function, one of each), differing signatures, a spec-inner
//! struct and a third definition, together with the same-named methods that
//! remain legal: on two different structs, and beside a free function.

use crate::utils::{build_ast, try_codegen};
use inference_ast::nodes::Location;
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::{RegistrationKind, TypeCheckError};

fn diagnostics(source: &str) -> Vec<TypeCheckError> {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena)
        .errors
        .into_iter()
        .map(|d| d.error)
        .collect()
}

/// The `(qualified_name, location)` of every method-registration failure in
/// `source`, in report order.
fn method_registration_failures(source: &str) -> Vec<(String, Location)> {
    diagnostics(source)
        .into_iter()
        .filter_map(|e| match e {
            TypeCheckError::RegistrationFailed {
                kind: RegistrationKind::Method,
                name,
                location,
                ..
            } => Some((name, location)),
            _ => None,
        })
        .collect()
}

/// Asserts `source` yields exactly one diagnostic, a method-registration failure
/// naming `qualified_name`, and returns its location.
fn single_duplicate_method(source: &str, qualified_name: &str) -> Location {
    let errors = diagnostics(source);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one diagnostic, got: {errors:?}"
    );
    match &errors[0] {
        TypeCheckError::RegistrationFailed {
            kind: RegistrationKind::Method,
            name,
            location,
            ..
        } => {
            assert_eq!(name, qualified_name, "diagnostic names the method");
            *location
        }
        other => panic!("expected a method RegistrationFailed, got {other:?}"),
    }
}

mod rejection {
    use super::*;

    #[test]
    fn repeated_instance_method_rejected() {
        let source =
            "struct P { x: i32; fn m(self) -> i32 { return 1; } fn m(self) -> i32 { return 2; } }";
        let location = single_duplicate_method(source, "P::m");
        // The caret sits on the repeat, not the first definition: the repeat is
        // the method to rename or remove.
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 52),
            "span points at the repeated method"
        );
    }

    #[test]
    fn repeated_associated_function_rejected() {
        let source = "struct P { x: i32; fn m() -> i32 { return 1; } fn m() -> i32 { return 2; } }";
        single_duplicate_method(source, "P::m");
    }

    #[test]
    fn instance_method_and_associated_function_of_one_name_rejected() {
        // Whether a definition takes `self` does not put it in another namespace:
        // `p.m()` and `P::m()` name the same method.
        let source =
            "struct P { x: i32; fn m(self) -> i32 { return 1; } fn m() -> i32 { return 2; } }";
        single_duplicate_method(source, "P::m");
    }

    #[test]
    fn repeated_method_of_differing_signatures_rejected() {
        // A differing signature neither excuses the repeat nor splits it into an
        // overload a call could pick between.
        let source = "struct P { x: i32; fn m(self) -> i32 { return 1; } fn m(self, k: i64) -> i64 { return k; } }";
        let failures = method_registration_failures(source);
        assert_eq!(
            failures.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["P::m"],
            "one repeat, one report: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn every_repeat_of_one_method_is_reported() {
        // Three definitions of `m` are two repeats, each reported at its own
        // definition, so removing one still leaves a reported mistake behind.
        let source = "struct P {\n  x: i32;\n  fn m(self) {}\n  fn m(self) {}\n  fn m(self) {}\n}";
        let failures = method_registration_failures(source);
        let lines: Vec<u32> = failures.iter().map(|(_, l)| l.start_line).collect();
        assert_eq!(
            lines,
            vec![4, 5],
            "each repeat is reported at its own definition: {failures:?}"
        );
    }

    #[test]
    fn repeated_method_of_a_spec_inner_struct_rejected() {
        let source = "spec S { struct P { x: i32; fn m(self) {} fn m(self) {} } }";
        let failures = method_registration_failures(source);
        assert_eq!(
            failures.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["P::m"],
            "the repeat inside a spec is reported too: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn repeated_method_reported_like_repeated_function() {
        // The method diagnostic reuses the wording a repeated free function gets,
        // so the two read as one rule.
        let errors = diagnostics("struct P { x: i32; fn m(self) {} fn m(self) {} }");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(rendered.len(), 1, "one diagnostic: {rendered:?}");
        assert!(
            rendered[0].contains("error registering method `P::m`")
                && rendered[0].contains("already exists"),
            "unexpected wording: {}",
            rendered[0]
        );
    }

    #[test]
    fn program_with_repeated_method_stops_at_the_type_checker() {
        // Before the frontend rejected the repeat, this program reached code
        // generation and panicked there.
        let source = "struct P { x: i32; fn m(self) -> i32 { return 1; } fn m(self) -> i32 { return 2; } }\n\
                      pub fn main() -> i32 { let p: P = P { x: 0 }; return p.m(); }";
        let arena = inference::parse(source).expect("the program parses");
        let error = inference::type_check(arena)
            .err()
            .expect("a struct defining `m` twice must not type-check");
        assert!(
            error
                .to_string()
                .contains("error registering method `P::m`"),
            "the build must fail at the type checker, got: {error}"
        );
    }
}

mod acceptance {
    use super::*;

    #[test]
    fn same_named_methods_on_different_structs_accepted() {
        let source = "struct P { x: i32; fn m(self) -> i32 { return self.x; } }\n\
                      struct Q { y: i32; fn m(self) -> i32 { return self.y; } }\n\
                      pub fn main() -> i32 { let p: P = P { x: 1 }; let q: Q = Q { y: 2 }; return p.m() + q.m(); }";
        let errors = diagnostics(source);
        assert!(errors.is_empty(), "no diagnostic expected, got: {errors:?}");
        try_codegen(source).expect("same-named methods on two structs compile");
    }

    #[test]
    fn method_named_like_a_free_function_accepted() {
        let source = "fn m() -> i32 { return 1; }\n\
                      struct P { x: i32; fn m(self) -> i32 { return self.x; } }\n\
                      pub fn main() -> i32 { let p: P = P { x: 2 }; return p.m() + m(); }";
        let errors = diagnostics(source);
        assert!(errors.is_empty(), "no diagnostic expected, got: {errors:?}");
        try_codegen(source).expect("a method and a free function of one name compile");
    }

    #[test]
    fn distinct_methods_on_one_struct_accepted() {
        let source = "struct P { x: i32; fn a(self) -> i32 { return self.x; } fn b(self) -> i32 { return self.x; } }";
        let errors = diagnostics(source);
        assert!(errors.is_empty(), "no diagnostic expected, got: {errors:?}");
    }
}
