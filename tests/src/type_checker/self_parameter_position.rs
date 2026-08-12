//! `self` must be the first parameter of a method.
//!
//! An instance call lowers the receiver ahead of the arguments written at the
//! call site, while the callee keeps its parameters in declaration order. A
//! receiver declared in any later position therefore binds the value of an
//! argument: depending on the shapes involved the module either compiles and
//! silently returns the receiver pointer instead of the intended value, or fails
//! WebAssembly validation outright. The frontend rejects the declaration.
//!
//! These tests pin the rejection and its span, the receiver shapes that must
//! keep compiling and running, and the neighbouring `self` diagnostics the rule
//! must leave alone.

use crate::utils::{build_ast, try_codegen, try_type_check_multi_file};
use inference_ast::nodes::Location;
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::TypeCheckError;

/// Type-checks `source` through the lossless entry point and returns the
/// structured diagnostics. Reaching this return at all proves the checker did
/// not unwind on the input.
fn diagnostics(source: &str) -> Vec<TypeCheckError> {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena)
        .errors
        .into_iter()
        .map(|d| d.error)
        .collect()
}

/// Asserts `source` yields exactly one diagnostic, a
/// `SelfReferenceNotFirstParameter` naming `function_name`, and returns its
/// location so the caller can pin the span.
///
/// The "exactly one" half is load-bearing: a misplaced receiver still
/// classifies the function as an instance method for the rest of checking, so
/// the declaration-site diagnostic must not be joined by an
/// `AssociatedFunctionCalledAsMethod` at every call site.
fn single_misplaced_receiver(source: &str, function_name: &str) -> Location {
    let errors = diagnostics(source);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one diagnostic, got: {errors:?}"
    );
    match &errors[0] {
        TypeCheckError::SelfReferenceNotFirstParameter {
            function_name: got,
            location,
        } => {
            assert_eq!(got, function_name, "diagnostic names the method");
            *location
        }
        other => panic!("expected SelfReferenceNotFirstParameter, got {other:?}"),
    }
}

mod rejection {
    use super::*;

    #[test]
    fn receiver_last_of_two_parameters_rejected() {
        // The issue's own declaration: pre-fix this compiled and returned the
        // stack pointer (65520) rather than 42, because `delta` bound the
        // receiver pointer and `self` bound the literal `2`.
        let source = r#"struct Number { value: i32; fn plus(delta: i32, self) -> i32 { return self.value + delta; } } pub fn main() -> i32 { let number: Number = Number { value: 40 }; return number.plus(2); }"#;
        let location = single_misplaced_receiver(source, "Number::plus");
        // The caret sits on the receiver token itself — the thing to move — at
        // the `s` of `self` in `fn plus(delta: i32, self)`.
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 49),
            "span points at the receiver token"
        );
    }

    #[test]
    fn mut_receiver_in_the_middle_rejected() {
        // A receiver between two parameters, declared `mut`: the span must cover
        // the `mut` as well, since the whole `mut self` is what moves.
        let source = r#"struct N { v: i32; fn addsub(a: i32, mut self, b: i32) -> i32 { return a - b + self.v; } }"#;
        let location = single_misplaced_receiver(source, "N::addsub");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 38),
            "span starts at the `mut` of `mut self`"
        );
        assert_eq!(
            (location.end_line, location.end_column),
            (1, 46),
            "span ends after `mut self`"
        );
    }

    #[test]
    fn receiver_last_after_several_parameters_rejected() {
        // Position is what matters, not arity: a receiver in the fourth slot is
        // rejected exactly like one in the second.
        let source = r#"struct S { v: i32; fn f(a: i32, b: i32, c: i32, self) -> i32 { return a + b + c + self.v; } }"#;
        let location = single_misplaced_receiver(source, "S::f");
        assert_eq!(
            (location.start_line, location.start_column),
            (1, 49),
            "span points at the receiver token"
        );
    }

    #[test]
    fn receiver_after_i64_parameter_rejected() {
        // The receiver pointer is an i32 and the argument it would swap with is
        // an i64, so pre-fix this shape did not even produce a well-typed module:
        // it compiled, then failed WebAssembly validation with "expected i32,
        // found i64". The rejection now happens in the frontend.
        let source = r#"struct Wide { a: i64; b: i64; fn shift(k: i64, self) -> i64 { return self.a + k; } } pub fn main() -> i64 { let w: Wide = Wide { a: 40, b: 1 }; return w.shift(2); }"#;
        single_misplaced_receiver(source, "Wide::shift");
    }

    #[test]
    fn receiver_after_parameter_in_struct_returning_method_rejected() {
        // A struct return adds an sret pointer ahead of every parameter, shifting
        // the slot the receiver is expected in. Pre-fix this shape compiled and
        // returned the stack pointer instead of the intended field value.
        let source = r#"struct P { x: i32; y: i32; fn moved(dx: i32, self) -> P { return P { x: self.x + dx, y: self.y }; } }"#;
        single_misplaced_receiver(source, "P::moved");
    }

    #[test]
    fn receiver_after_parameter_in_spec_inner_struct_rejected() {
        // Definitions inside a `spec` body are collected by recursing back through
        // the same arm, so the rule must hold for a struct declared there too.
        let source = r#"spec S { struct Number { value: i32; fn plus(delta: i32, self) -> i32 { return self.value + delta; } } }"#;
        single_misplaced_receiver(source, "Number::plus");
    }

    #[test]
    fn receiver_after_parameter_in_imported_file_rejected() {
        // Registration runs over the merged multi-file arena, where source
        // locations are per-file-local; the diagnostic must therefore carry the
        // importing-file label so `1:57` is not misread as an entry-file span.
        let files = [
            (
                vec![],
                "use lib::num::{Number}; pub fn main() -> i32 { let n: Number = Number { value: 40 }; return n.plus(2); }",
            ),
            (
                vec!["lib", "num"],
                "pub struct Number { value: i32; pub fn plus(delta: i32, self) -> i32 { return self.value + delta; } }",
            ),
        ];
        let Err(err) = try_type_check_multi_file(&files) else {
            panic!("a misplaced receiver in an imported file must be rejected");
        };
        let message = err.to_string();
        assert!(
            message.starts_with(
                "lib::num:1:57: `self` must be the first parameter of method `Number::plus`"
            ),
            "diagnostic is attributed to the defining file, got: {message}"
        );
        // A cascade would append `; <next error>` after the note, so ending on
        // the note's last words is what proves there is only the one diagnostic.
        assert!(
            message.ends_with("to the front of the parameter list"),
            "the imported declaration produces one diagnostic, not a cascade: {message}"
        );
    }
}

mod acceptance {
    use super::*;
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// Compiles `source`, validates the emitted module, and returns what its
    /// exported `main` evaluates to.
    ///
    /// Executing is the load-bearing part rather than merely compiling: a
    /// receiver pointer and an i32 argument share a WebAssembly value type, so a
    /// swapped ABI still yields a module the validator accepts. Only running it
    /// shows which value landed in which parameter slot.
    fn run_main(source: &str) -> i32 {
        let wasm_bytes = match try_codegen(source) {
            Ok(output) => output.wasm().to_vec(),
            Err(error) => panic!("a leading receiver must still compile: {error}"),
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
    fn leading_receiver_alone_runs() {
        // The minimal method shape: a receiver and nothing else.
        let source = r#"struct N { v: i32; fn get(self) -> i32 { return self.v; } } pub fn main() -> i32 { let n: N = N { v: 7 }; return n.get(); }"#;
        assert_eq!(run_main(source), 7, "n.get() reads the receiver's field");
    }

    #[test]
    fn leading_receiver_with_parameter_runs() {
        // The shape the rule is about, written correctly: receiver first, then
        // the argument it was being confused with.
        let source = r#"struct N { v: i32; fn plus(self, delta: i32) -> i32 { return self.v + delta; } } pub fn main() -> i32 { let n: N = N { v: 40 }; return n.plus(2); }"#;
        assert_eq!(
            run_main(source),
            42,
            "n.plus(2) adds the argument to the receiver's field"
        );
    }

    #[test]
    fn leading_mut_receiver_with_parameter_runs() {
        // `mut self` is the other receiver spelling and must be accepted in the
        // first position just the same.
        let source = r#"struct N { v: i32; fn bump(mut self, delta: i32) -> i32 { self.v = self.v + delta; return self.v; } } pub fn main() -> i32 { let n: N = N { v: 40 }; return n.bump(2); }"#;
        assert_eq!(
            run_main(source),
            42,
            "n.bump(2) adds the argument to the receiver's field"
        );
    }

    #[test]
    fn associated_function_without_receiver_runs() {
        // No receiver at all: the positional check must fire on a `self` that is
        // not first, never on the absence of one.
        let source = r#"struct P { x: i32; fn new(x: i32) -> P { return P { x: x }; } } pub fn main() -> i32 { let p: P = P::new(5); return p.x; }"#;
        assert_eq!(
            run_main(source),
            5,
            "P::new(5) stores its argument in the field"
        );
    }

    #[test]
    fn leading_receiver_in_struct_returning_method_runs() {
        // The sret half: with a struct return the receiver occupies slot one,
        // behind the sret pointer, and that is the slot the callee must accept.
        let source = r#"struct P { x: i32; y: i32; fn moved(self, dx: i32) -> P { return P { x: self.x + dx, y: self.y }; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; let q: P = p.moved(10); return q.x; }"#;
        assert_eq!(
            run_main(source),
            11,
            "p.moved(10) offsets the receiver's x by the argument"
        );
    }

    #[test]
    fn issue_program_with_leading_receiver_returns_42() {
        // The issue's own program with `self` moved to the front, executing to
        // the value the issue says it should have produced all along. Before the
        // fix the unmoved version compiled and returned 65520, a stack pointer.
        let source = r#"struct Number { value: i32; fn plus(self, delta: i32) -> i32 { return self.value + delta; } } pub fn main() -> i32 { let number: Number = Number { value: 40 }; return number.plus(2); }"#;
        assert_eq!(
            run_main(source),
            42,
            "number.plus(2) is the issue's expected 42"
        );
    }
}

mod unchanged_diagnostics {
    use super::*;

    #[test]
    fn duplicate_receiver_keeps_registration_diagnostic() {
        // Two receivers means the second one is not first, but the pre-existing
        // duplicate-binding diagnostic is the useful one and must stay the only
        // report — the positional check reports the first receiver's position,
        // which here is zero.
        let source = r#"struct S { v: i32; fn twice(self, self) -> i32 { return 1; } }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::RegistrationFailed { name, reason: Some(reason), .. }
                    if name == "self" && reason.contains("already declared in this scope")
            )),
            "duplicate receiver keeps its registration diagnostic: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::SelfReferenceNotFirstParameter { .. })),
            "duplicate receiver must not be reported as a misplaced one: {errors:?}"
        );
    }

    #[test]
    fn standalone_function_receiver_keeps_its_diagnostics() {
        // A receiver outside a struct is not a method at all, so the standalone
        // diagnostics still own that case.
        let source = r#"pub fn free(x: i32, self) -> i32 { return x; }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::SelfReferenceInFunction { function_name, .. } if function_name == "free"
            )),
            "standalone receiver keeps SelfReferenceInFunction: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::SelfReferenceOutsideMethod { .. })),
            "standalone receiver keeps SelfReferenceOutsideMethod: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::SelfReferenceNotFirstParameter { .. })),
            "standalone receiver is not a misplaced method receiver: {errors:?}"
        );
    }

    #[test]
    fn external_function_receiver_keeps_its_diagnostic() {
        // An extern declaration has no struct to be a method of either, and its
        // receiver is rejected by the same standalone rule rather than the
        // positional one.
        let source = r#"external fn e(x: i32, self) -> i32;"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::SelfReferenceInFunction { function_name, .. } if function_name == "e"
            )),
            "extern receiver keeps SelfReferenceInFunction: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::SelfReferenceNotFirstParameter { .. })),
            "extern receiver is not a misplaced method receiver: {errors:?}"
        );
    }
}
