//! A parameter name may be bound only once per function-like declaration.
//!
//! A repeat is not shadowing: a body reference resolves to the first binding, so
//! the value passed for the later parameter cannot be named, and code generation
//! keys parameter slots by name and asserts the collision away instead of
//! emitting a module. The frontend rejects the declaration, naming the repeated
//! parameter and the declaration it repeats in.
//!
//! These tests pin the rejection and its span across every declaration form that
//! carries parameters — free functions, methods, spec-inner functions and
//! `external fn` — the receiver spelling, the parameter forms that bind no name
//! and so may legally repeat, and the neighbouring duplicate-binding diagnostics
//! the rule must leave alone.

use crate::utils::{build_ast, try_codegen, try_type_check_multi_file};
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

/// The `(function_name, parameter_name, location)` of every duplicate-parameter
/// diagnostic in `source`, in report order.
fn duplicate_parameters(source: &str) -> Vec<(String, String, Location)> {
    diagnostics(source)
        .into_iter()
        .filter_map(|e| match e {
            TypeCheckError::DuplicateParameterName {
                function_name,
                parameter_name,
                location,
            } => Some((function_name, parameter_name, location)),
            _ => None,
        })
        .collect()
}

/// Asserts `source` yields exactly one diagnostic, a duplicate-parameter report
/// naming `function_name` and `parameter_name`, and returns its location.
///
/// The "exactly one" half is load-bearing in both directions: the declaration is
/// reported once per repeat rather than once per pass, and the body pass must not
/// add a second report of the same collision under its own wording.
fn single_duplicate_parameter(source: &str, function_name: &str, parameter_name: &str) -> Location {
    let errors = diagnostics(source);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one diagnostic, got: {errors:?}"
    );
    match &errors[0] {
        TypeCheckError::DuplicateParameterName {
            function_name: got_function,
            parameter_name: got_parameter,
            location,
        } => {
            assert_eq!(got_function, function_name, "diagnostic names the function");
            assert_eq!(
                got_parameter, parameter_name,
                "diagnostic names the repeated parameter"
            );
            *location
        }
        other => panic!("expected DuplicateParameterName, got {other:?}"),
    }
}

mod rejection {
    use super::*;

    #[test]
    fn repeated_parameter_in_free_function_rejected() {
        let source = r#"pub fn f(x: i32, x: i32) -> i32 { return 0; }"#;
        let location = single_duplicate_parameter(source, "f", "x");
        // The caret sits on the repeat, not the first declaration: the repeat is
        // the parameter to rename or remove.
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 18),
            "span points at the repeated parameter"
        );
    }

    #[test]
    fn every_repeat_of_one_parameter_is_reported() {
        // Three declarations of `x` are two repeats, each with its own span, so
        // renaming one still leaves a reported mistake behind.
        let source = r#"pub fn f(x: i32, x: i32, x: i32) -> i32 { return 0; }"#;
        let reports = duplicate_parameters(source);
        let spans: Vec<_> = reports
            .iter()
            .map(|(_, _, l)| (l.start_line, l.start_column))
            .collect();
        assert_eq!(
            spans,
            vec![(1, 18), (1, 26)],
            "each repeat is reported at its own declaration: {reports:?}"
        );
    }

    #[test]
    fn repeated_parameter_of_differing_types_in_free_function_rejected() {
        // Two declarations of one name are not an overload set: the second is
        // unreachable whatever type it is given, so the differing type neither
        // excuses the repeat nor splits it into two bindings the body could pick
        // between.
        let source = r#"pub fn f(x: i32, x: i64) -> i32 { return x; }"#;
        let location = single_duplicate_parameter(source, "f", "x");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 18),
            "span points at the repeated parameter"
        );
    }

    #[test]
    fn repeated_parameter_of_differing_types_rejected() {
        // The repeat is a repeat whether or not the two declarations agree on a
        // type; only the first binding is in scope, so the body types against it.
        let source =
            r#"struct S { v: i32; fn m(self, k: i32, k: i64) -> i32 { return self.v + k; } }"#;
        let reports = duplicate_parameters(source);
        assert_eq!(
            reports.len(),
            1,
            "one repeat, one report: {:?}",
            diagnostics(source)
        );
        assert_eq!(
            (reports[0].0.as_str(), reports[0].1.as_str()),
            ("S::m", "k"),
            "diagnostic names the method and the repeated parameter"
        );
    }

    #[test]
    fn repeated_parameter_in_method_names_the_owning_struct() {
        // A method is named the way the misplaced-receiver diagnostic names it, so
        // the two reports a reader can hit on one declaration agree on its name.
        let source = r#"struct N { value: i32; fn m(self, x: i32, x: i32) -> i32 { return self.value + x; } }"#;
        let location = single_duplicate_parameter(source, "N::m", "x");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 43),
            "span points at the repeated parameter"
        );
    }

    #[test]
    fn repeated_receiver_rejected_under_the_name_self() {
        // The receiver has no written name, so it is reported under the name it
        // binds — the one a body reference and a parameter slot both use.
        let source = r#"struct N { value: i32; fn m(self, self) -> i32 { return self.value; } }"#;
        let location = single_duplicate_parameter(source, "N::m", "self");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 35),
            "span points at the repeated receiver"
        );
    }

    #[test]
    fn every_repeat_of_the_receiver_is_reported() {
        // Three receivers are two repeats, exactly as three named parameters are.
        let source =
            r#"struct N { value: i32; fn m(self, self, self) -> i32 { return self.value; } }"#;
        let reports = duplicate_parameters(source);
        let spans: Vec<_> = reports
            .iter()
            .map(|(_, _, l)| (l.start_line, l.start_column))
            .collect();
        assert_eq!(
            spans,
            vec![(1, 35), (1, 41)],
            "each repeated receiver is reported at its own declaration: {reports:?}"
        );
    }

    #[test]
    fn mut_receiver_repeated_by_a_plain_one_rejected() {
        // `mut self` and `self` are two spellings of the same binding, so they
        // repeat each other.
        let source =
            r#"struct N { value: i32; fn m(mut self, self) -> i32 { return self.value; } }"#;
        let location = single_duplicate_parameter(source, "N::m", "self");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 39),
            "span points at the repeated receiver"
        );
    }

    #[test]
    fn plain_receiver_repeated_by_a_mut_one_rejected() {
        // The other order, where the span must cover the `mut` as well since the
        // whole `mut self` is what is repeated.
        let source =
            r#"struct N { value: i32; fn m(self, mut self) -> i32 { return self.value; } }"#;
        let location = single_duplicate_parameter(source, "N::m", "self");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 35),
            "span starts at the `mut` of the repeated `mut self`"
        );
        assert_eq!(
            (location.end_line, location.end_column),
            (1, 43),
            "span ends after `mut self`"
        );
    }

    #[test]
    fn mut_receiver_repeated_by_another_mut_one_rejected() {
        // The third of the three receiver-spelling pairs, so no combination of
        // `self` and `mut self` is left where a repeat could slip through.
        let source =
            r#"struct N { value: i32; fn m(mut self, mut self) -> i32 { return self.value; } }"#;
        let location = single_duplicate_parameter(source, "N::m", "self");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 39),
            "span starts at the `mut` of the repeated `mut self`"
        );
    }

    #[test]
    fn repeated_parameter_in_external_function_rejected() {
        // An extern's parameters never enter a scope, so before the rule was named
        // this declaration compiled and produced a module.
        let source = r#"external fn e(x: i32, x: i32) -> i32;"#;
        let location = single_duplicate_parameter(source, "e", "x");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 23),
            "span points at the repeated parameter"
        );
    }

    #[test]
    fn repeated_parameter_in_spec_inner_function_rejected() {
        // Definitions inside a `spec` body are reached by recursing back through
        // the same arm, so the rule holds for a function declared there too.
        let source = r#"spec Sp { fn g(x: i32, x: i32) -> i32 { return 0; } }"#;
        single_duplicate_parameter(source, "g", "x");
    }

    #[test]
    fn repeated_parameter_in_spec_inner_external_function_rejected() {
        let source = r#"spec Sp { external fn e(x: i32, x: i32) -> i32; }"#;
        single_duplicate_parameter(source, "e", "x");
    }

    #[test]
    fn repeated_parameter_in_collided_spec_struct_method_rejected() {
        // A spec-inner struct whose name collides with a top-level one is refused
        // registration, so its methods never register either. The declaration's own
        // mistakes are reported regardless, beside the collision.
        let source = r#"struct Helper { value: i32; } spec Sp { struct Helper { value: i32; fn m(self, k: i32, k: i32) -> i32 { return k; } } }"#;
        let reports = duplicate_parameters(source);
        assert_eq!(
            reports.len(),
            1,
            "the collided method's repeat is reported once: {:?}",
            diagnostics(source)
        );
        assert_eq!(
            (reports[0].0.as_str(), reports[0].1.as_str()),
            ("Helper::m", "k"),
            "diagnostic names the spec-inner method and its repeated parameter"
        );
    }

    #[test]
    fn repeated_parameter_in_imported_file_rejected() {
        // Signature validation walks the merged multi-file arena, where source
        // locations are per-file-local; the diagnostic must therefore carry the
        // defining file's label so `1:18` is not misread as an entry-file span.
        let files = [
            (
                vec![],
                "use lib::num::{f}; pub fn main() -> i32 { return f(1, 2); }",
            ),
            (
                vec!["lib", "num"],
                "pub fn f(x: i32, x: i32) -> i32 { return x; }",
            ),
        ];
        let Err(err) = try_type_check_multi_file(&files) else {
            panic!("duplicate parameters in an imported file must be rejected");
        };
        // Equality rather than a prefix: the whole aggregate is this one
        // diagnostic, so a cascade appended after it would fail here.
        assert_eq!(
            err.to_string(),
            "lib::num:1:18: parameter `x` is declared more than once in `f`",
            "diagnostic is attributed to the defining file and stands alone"
        );
    }
}

mod acceptance {
    use super::*;
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// Compiles `source`, validates the emitted module, and returns what its
    /// exported `main` evaluates to.
    ///
    /// Executing is the load-bearing part rather than merely compiling: the check
    /// is a name comparison over the parameter list, and an over-firing one would
    /// reject a well-formed declaration, while a mis-keyed parameter map would put
    /// the call site's arguments in the wrong slots without troubling the
    /// validator. Only running the module shows both are intact.
    fn run_main(source: &str) -> i32 {
        let wasm_bytes = match try_codegen(source) {
            Ok(output) => output.wasm().to_vec(),
            Err(error) => panic!("distinct parameter names must still compile: {error}"),
        };
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("generated Wasm module is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("failed to instantiate Wasm module: {e}"));
        let main: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "main")
            .expect("failed to get 'main'");
        main.call(&mut store, ()).expect("call to 'main' failed")
    }

    #[test]
    fn distinct_parameters_compile_and_run() {
        // Two differently named parameters, called with two different values: the
        // returned sum only comes out right if each argument reached the slot the
        // call site meant it for.
        let source = r#"pub fn add(a: i32, b: i32) -> i32 { return a - b; } pub fn main() -> i32 { return add(44, 2); }"#;
        assert_eq!(
            run_main(source),
            42,
            "add(44, 2) subtracts the second argument from the first"
        );
    }

    #[test]
    fn distinct_method_parameters_compile_and_run() {
        // The same for a method, where the receiver shares the parameter list with
        // the named parameters and is keyed in it under `self`.
        let source = r#"struct N { value: i32; fn spread(self, low: i32, high: i32) -> i32 { return self.value + high - low; } } pub fn main() -> i32 { let n: N = N { value: 40 }; return n.spread(1, 3); }"#;
        assert_eq!(
            run_main(source),
            42,
            "n.spread(1, 3) reads the receiver and both arguments from their own slots"
        );
    }

    #[test]
    fn repeated_ignored_parameters_accepted() {
        // `_: T` binds no name, so a repeat names nothing twice. Code generation
        // does not support ignored parameters yet, which is why this asserts on the
        // frontend rather than on a module.
        let source = r#"pub fn f(_: i32, _: i32) -> i32 { return 1; }"#;
        assert!(
            duplicate_parameters(source).is_empty(),
            "ignored parameters bind no name: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn ignored_parameter_alongside_a_named_one_accepted() {
        let source =
            r#"struct S { v: i32; fn m(self, _: i32, k: i32) -> i32 { return self.v + k; } }"#;
        assert!(
            duplicate_parameters(source).is_empty(),
            "an ignored parameter never repeats a named one: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn repeated_positional_parameter_types_accepted() {
        // An `external fn` may write bare types with no parameter names at all, and
        // two of the same type name nothing twice. This is the form the rule would
        // most easily over-reach into, since the extern arm is the one it newly
        // reaches.
        let source = r#"external fn e(i32, i32) -> i32;"#;
        assert!(
            diagnostics(source).is_empty(),
            "bare positional types bind no name: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn distinct_parameters_accepted() {
        let source = r#"struct N { value: i32; fn m(self, x: i32, y: i32) -> i32 { return self.value + x + y; } }"#;
        assert!(
            diagnostics(source).is_empty(),
            "distinct parameter names are accepted: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn same_parameter_name_in_two_functions_accepted() {
        // The set is per declaration, not per file: two functions may each declare
        // an `x`.
        let source =
            r#"pub fn f(x: i32) -> i32 { return x; } pub fn g(x: i32) -> i32 { return x; }"#;
        assert!(
            diagnostics(source).is_empty(),
            "the name set is scoped to one declaration: {:?}",
            diagnostics(source)
        );
    }
}

mod neighbouring_diagnostics {
    use super::*;

    #[test]
    fn body_binding_colliding_with_a_parameter_keeps_its_diagnostic() {
        // Suppressing the parameter repeat in the body pass must not suppress this:
        // a `let` that reuses a parameter name is a different mistake, made in the
        // body, and the parameter is genuinely already in scope when it is reached.
        let source = r#"pub fn f(x: i32) -> i32 { let x: i32 = 2; return x; }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::RegistrationFailed { name, reason: Some(reason), .. }
                    if name == "x" && reason.contains("already declared in this scope")
            )),
            "a body binding over a parameter is still reported: {errors:?}"
        );
        assert!(
            duplicate_parameters(source).is_empty(),
            "the parameter list itself is well formed: {errors:?}"
        );
    }

    #[test]
    fn body_binding_colliding_with_a_receiver_keeps_its_diagnostic() {
        let source =
            r#"struct N { value: i32; fn m(self) -> i32 { let self: i32 = 2; return self; } }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::RegistrationFailed { name, reason: Some(reason), .. }
                    if name == "self" && reason.contains("already declared in this scope")
            )),
            "a body binding over the receiver is still reported: {errors:?}"
        );
    }

    #[test]
    fn repeated_receiver_outside_a_method_keeps_the_standalone_diagnostics() {
        // A receiver in a free function is two mistakes at once, and the repeat is
        // a third: the standalone diagnostics say the receiver does not belong
        // here, and the repeat says it is written twice.
        let source = r#"pub fn free(self, self) -> i32 { return 1; }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::SelfReferenceInFunction { function_name, .. } if function_name == "free"
            )),
            "the standalone receiver diagnostic is kept: {errors:?}"
        );
        let reports = duplicate_parameters(source);
        assert_eq!(
            reports.len(),
            1,
            "the repeat is reported once beside it: {errors:?}"
        );
        assert_eq!(
            (reports[0].0.as_str(), reports[0].1.as_str()),
            ("free", "self"),
            "a free function is named bare, without a `::` owner"
        );
    }
}
