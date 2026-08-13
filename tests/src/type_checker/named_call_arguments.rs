//! An argument label must name the parameter it is written opposite.
//!
//! Arguments bind by position the whole way down — code generation, the proof
//! obligations and the emitted Rocq module all consume them in written order —
//! and until this rule was named nothing downstream read a label at all. A call
//! could therefore assert a binding it did not perform: `subtract(right: 3,
//! left: 10)` reads as `10 - 3` and computed `3 - 10`, and a proof discharged
//! against the emitted module was a proof about a program the source does not
//! describe. A misspelled or stale label compiled just as quietly, so renaming a
//! parameter left every call site of the old name silently valid. A partly
//! labelled list is a third shape the specification already forbids: when any
//! argument is named, all of them must be.
//!
//! These tests pin the three rejections, the token each caret sits on, the five
//! callee branches a label can reach the check through, the calls that must keep
//! compiling and running with each value in the slot the call site named, and
//! the neighbouring diagnostics the rule must leave alone.

use crate::utils::{build_ast, try_codegen, try_type_check_multi_file};
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::TypeCheckError;
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

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

/// The diagnostics `source` produces, asserted to number exactly `count`.
///
/// Pinning the total rather than filtering for the interesting variant is
/// load-bearing in both directions. Too few and a label slipped through; too
/// many and either the check fired twice — a call expression is re-inferred when
/// it fills a generic parameter slot — or it fired beside a diagnostic that
/// already covers the same mistake.
fn diagnostics_numbering(source: &str, count: usize) -> Vec<TypeCheckError> {
    let errors = diagnostics(source);
    assert_eq!(
        errors.len(),
        count,
        "expected exactly {count} diagnostic(s), got: {errors:?}"
    );
    errors
}

/// The `(line, column)` the caret of `error` sits on.
fn caret(error: &TypeCheckError) -> (u32, u32) {
    let location = error.location();
    (location.start_line, location.start_column)
}

/// Compiles `source`, validates the emitted module, and returns what its
/// exported `main` evaluates to.
///
/// Executing is the load-bearing part rather than merely compiling: labels are
/// discarded before code generation, so nothing in the emitted module records
/// which name a call site wrote. Only running it shows which value reached which
/// parameter slot.
fn run_main(source: &str) -> i32 {
    let wasm_bytes = match try_codegen(source) {
        Ok(output) => output.wasm().to_vec(),
        Err(error) => panic!("a well-labelled call must still compile: {error}"),
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

mod rejection {
    use super::*;

    #[test]
    fn leading_label_mixed_with_a_positional_argument_rejected() {
        // The issue's first shape: one name present, the rest positional. The
        // caret sits on the argument that departs from the labelling the first
        // argument set — here the bare `3`, since there is no label token to
        // point at and a `Location` covers one token, never a span stitched from
        // the label to its colon.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(left: 10, 3); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::MixedNamedAndPositionalArguments { .. } = &errors[0] else {
            panic!("expected MixedNamedAndPositionalArguments, got {errors:?}");
        };
        assert_eq!(
            caret(&errors[0]),
            (1, 117),
            "caret sits on the unlabelled argument"
        );
    }

    #[test]
    fn trailing_label_mixed_with_a_positional_argument_rejected() {
        // The other order, where the departing argument is the labelled one and
        // the caret moves to its label token.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(10, right: 3); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::MixedNamedAndPositionalArguments { .. } = &errors[0] else {
            panic!("expected MixedNamedAndPositionalArguments, got {errors:?}");
        };
        assert_eq!(caret(&errors[0]), (1, 111), "caret sits on the label token");
    }

    #[test]
    fn mixing_is_reported_once_per_call_not_once_per_argument() {
        // Two labels then a bare argument is one malformed list, not two: the
        // report names the first departure and stops.
        let source = r#"fn three(a: i32, b: i32, c: i32) -> i32 { return a + b + c; } pub fn main() -> i32 { return three(a: 1, b: 2, 3); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::MixedNamedAndPositionalArguments { .. } = &errors[0] else {
            panic!("expected MixedNamedAndPositionalArguments, got {errors:?}");
        };
        assert_eq!(
            caret(&errors[0]),
            (1, 111),
            "caret sits on the first argument that departs from the first one's shape"
        );
    }

    #[test]
    fn a_trailing_label_after_two_positional_arguments_is_reported_once() {
        // The mirror of the previous case: the shape is set by the unlabelled
        // first argument, so the single label at the end is the departure.
        let source = r#"fn three(a: i32, b: i32, c: i32) -> i32 { return a + b + c; } pub fn main() -> i32 { return three(1, 2, c: 3); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::MixedNamedAndPositionalArguments { .. } = &errors[0] else {
            panic!("expected MixedNamedAndPositionalArguments, got {errors:?}");
        };
        assert_eq!(caret(&errors[0]), (1, 105), "caret sits on the label token");
    }

    #[test]
    fn unknown_labels_are_reported_one_per_argument() {
        // Neither name is declared anywhere in the signature, so each is its own
        // mistake with its own caret.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(wrong: 10, nonexistent: 3); }"#;
        let errors = diagnostics_numbering(source, 2);
        let reported: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::UnknownArgumentLabel {
                    kind, name, label, ..
                } => Some((*kind, name.as_str(), label.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![
                ("function", "subtract", "wrong"),
                ("function", "subtract", "nonexistent")
            ],
            "each unknown label is reported against the callee it was written for: {errors:?}"
        );
        assert_eq!(
            (caret(&errors[0]), caret(&errors[1])),
            ((1, 107), (1, 118)),
            "each caret sits on its own label token"
        );
    }

    #[test]
    fn out_of_order_labels_are_reported_one_per_argument() {
        // The dangerous shape. Both names are declared, so neither is unknown;
        // each report says where the name is declared and where it was written,
        // which is the pair a reader needs to put the list back in order.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(right: 3, left: 10); }"#;
        let errors = diagnostics_numbering(source, 2);
        let reported: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::ArgumentLabelOutOfOrder {
                    kind,
                    name,
                    label,
                    expected_position,
                    found_position,
                    ..
                } => Some((
                    *kind,
                    name.as_str(),
                    label.as_str(),
                    *expected_position,
                    *found_position,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![
                ("function", "subtract", "right", 2, 1),
                ("function", "subtract", "left", 1, 2)
            ],
            "positions are 1-based and name the declared slot and the written one: {errors:?}"
        );
        assert_eq!(
            (caret(&errors[0]), caret(&errors[1])),
            ((1, 107), (1, 117)),
            "each caret sits on its own label token"
        );
    }

    #[test]
    fn a_repeated_label_is_reported_as_out_of_order() {
        // A label is only ever compared with the parameter it faces, so the
        // second `left` fails that comparison and is reported for the position it
        // is in. That subsumes duplicate labels without a variant of their own:
        // the first `left` is where `left` belongs, and saying so is the same
        // advice a separate duplicate diagnostic would give.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(left: 10, left: 3); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::ArgumentLabelOutOfOrder {
            kind,
            name,
            label,
            expected_position,
            found_position,
            ..
        } = &errors[0]
        else {
            panic!("expected ArgumentLabelOutOfOrder, got {errors:?}");
        };
        assert_eq!(
            (
                *kind,
                name.as_str(),
                label.as_str(),
                *expected_position,
                *found_position
            ),
            ("function", "subtract", "left", 1, 2),
            "the repeat is reported at the position it occupies"
        );
        assert_eq!(
            caret(&errors[0]),
            (1, 117),
            "caret sits on the repeated label, not the first one"
        );
    }

    #[test]
    fn a_label_aimed_at_an_anonymous_parameter_is_unknown() {
        // `_: i32` binds no name, so no label can name it. Were the anonymous
        // slot carried as an empty name instead of as no name at all, a label
        // could "match" it and the mistake would compile.
        let source = r#"fn takes(_: i32, b: i32) -> i32 { return b; } pub fn main() -> i32 { return takes(a: 1, b: 2); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::UnknownArgumentLabel {
            kind, name, label, ..
        } = &errors[0]
        else {
            panic!("expected UnknownArgumentLabel, got {errors:?}");
        };
        assert_eq!(
            (*kind, name.as_str(), label.as_str()),
            ("function", "takes", "a"),
            "the label names no parameter of the callee"
        );
        assert_eq!(caret(&errors[0]), (1, 83), "caret sits on the label token");
    }

    #[test]
    fn instance_method_unknown_label_rejected() {
        // The instance-method branch, reached through a member-access callee.
        // The receiver is not one of the written arguments, so `d` is the only
        // labelable position and `bogus` names nothing.
        let source = r#"struct P { x: i32; y: i32; fn addto(self, d: i32) -> i32 { return self.x + d; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; return p.addto(bogus: 5); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::UnknownArgumentLabel {
            kind, name, label, ..
        } = &errors[0]
        else {
            panic!("expected UnknownArgumentLabel, got {errors:?}");
        };
        assert_eq!(
            (*kind, name.as_str(), label.as_str()),
            ("method", "P::addto", "bogus"),
            "a method is named with its owning type, as the arity diagnostic names it"
        );
        assert_eq!(caret(&errors[0]), (1, 150), "caret sits on the label token");
    }

    #[test]
    fn self_written_as_a_label_is_unknown() {
        // `self` is an ident-like token, so it is legal to *write* as a label and
        // reaches the check like any other name. It still names nothing: the
        // receiver is the member-access callee, never an entry in the argument
        // list, and it is dropped from the parameter names for exactly that
        // reason. Were it kept, this call would be judged against a two-name
        // signature and the well-formed `p.addto(d: 5)` would be rejected.
        let source = r#"struct P { x: i32; y: i32; fn addto(self, d: i32) -> i32 { return self.x + d; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; return p.addto(self: 5); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::UnknownArgumentLabel {
            kind, name, label, ..
        } = &errors[0]
        else {
            panic!("expected UnknownArgumentLabel, got {errors:?}");
        };
        assert_eq!(
            (*kind, name.as_str(), label.as_str()),
            ("method", "P::addto", "self"),
            "the receiver is not a labelable argument"
        );
        assert_eq!(caret(&errors[0]), (1, 150), "caret sits on the label token");
    }

    #[test]
    fn associated_function_out_of_order_labels_rejected() {
        // The `Type::assoc()` branch, which resolves through a type-member-access
        // callee rather than a plain identifier and so reaches the check by its
        // own route.
        let source = r#"struct P { x: i32; y: i32; fn make(a: i32, b: i32) -> i32 { return a - b; } } pub fn main() -> i32 { return P::make(b: 3, a: 10); }"#;
        let errors = diagnostics_numbering(source, 2);
        let reported: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::ArgumentLabelOutOfOrder {
                    kind,
                    name,
                    label,
                    expected_position,
                    found_position,
                    ..
                } => Some((
                    *kind,
                    name.as_str(),
                    label.as_str(),
                    *expected_position,
                    *found_position,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![
                ("method", "P::make", "b", 2, 1),
                ("method", "P::make", "a", 1, 2)
            ],
            "an associated function is reported under the method kind: {errors:?}"
        );
        assert_eq!(
            (caret(&errors[0]), caret(&errors[1])),
            ((1, 117), (1, 123)),
            "each caret sits on its own label token"
        );
    }

    #[test]
    fn external_function_out_of_order_labels_rejected() {
        // An extern's parameters never enter a scope and its body is elsewhere,
        // but its declared names are still the contract a call site labels
        // against.
        let source = r#"external fn sub(left: i32, right: i32) -> i32; pub fn main() -> i32 { return sub(right: 3, left: 10); }"#;
        let errors = diagnostics_numbering(source, 2);
        let reported: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::ArgumentLabelOutOfOrder {
                    kind,
                    name,
                    label,
                    expected_position,
                    found_position,
                    ..
                } => Some((
                    *kind,
                    name.as_str(),
                    label.as_str(),
                    *expected_position,
                    *found_position,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![
                ("function", "sub", "right", 2, 1),
                ("function", "sub", "left", 1, 2)
            ],
            "an extern is reported under the function kind: {errors:?}"
        );
        assert_eq!(
            (caret(&errors[0]), caret(&errors[1])),
            ((1, 82), (1, 92)),
            "each caret sits on its own label token"
        );
    }

    #[test]
    fn labels_on_an_external_function_declared_by_type_alone_are_unknown() {
        // `external fn sub(i32, i32)` declares two parameters and names neither,
        // so every label aimed at it names nothing — the extern analogue of the
        // `_: i32` case, and the form most likely to be mistaken for one that
        // simply has no parameter list to check against.
        let source = r#"external fn sub(i32, i32) -> i32; pub fn main() -> i32 { return sub(left: 10, right: 3); }"#;
        let errors = diagnostics_numbering(source, 2);
        let reported: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::UnknownArgumentLabel {
                    kind, name, label, ..
                } => Some((*kind, name.as_str(), label.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![("function", "sub", "left"), ("function", "sub", "right")],
            "bare positional types bind no name for a label to match: {errors:?}"
        );
        assert_eq!(
            (caret(&errors[0]), caret(&errors[1])),
            ((1, 69), (1, 79)),
            "each caret sits on its own label token"
        );
    }

    #[test]
    fn mixed_call_to_an_undefined_function_is_still_reported() {
        // The shape check is purely syntactic and runs before the callee is
        // resolved, so a partly labelled list is reported even when there is no
        // signature to check the names against — and the undefined-callee
        // diagnostic still stands beside it.
        let source = r#"pub fn main() -> i32 { return nosuch(a: 1, 2); }"#;
        let errors = diagnostics(source);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::MixedNamedAndPositionalArguments { .. })),
            "a malformed argument list is reported whatever the callee turns out to be: {errors:?}"
        );
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::UndefinedFunction { name, .. } if name == "nosuch"
            )),
            "the undefined callee is still reported: {errors:?}"
        );
        let mixed = errors
            .iter()
            .find(|e| matches!(e, TypeCheckError::MixedNamedAndPositionalArguments { .. }))
            .expect("the mixed diagnostic was asserted above");
        assert_eq!(
            caret(mixed),
            (1, 44),
            "caret sits on the unlabelled argument"
        );
    }

    #[test]
    fn cross_file_qualified_call_labels_checked_at_the_call_site() {
        // The file-qualified branch (`math::add(...)`), which a single-file
        // program never reaches. Locations in the merged multi-file arena are
        // per-file-local, and the call site here is neither the entry file nor
        // the callee's, so `1:55` is only readable if the diagnostic carries the
        // label of the file the *call* was written in.
        let files = [
            (
                vec![],
                "use caller; pub fn main() -> i32 { return caller::run(); }",
            ),
            (
                vec!["caller"],
                "use lib::math; pub fn run() -> i32 { return math::add(b: 2, a: 1); }",
            ),
            (
                vec!["lib", "math"],
                "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
            ),
        ];
        let Err(err) = try_type_check_multi_file(&files) else {
            panic!("reordered labels on a cross-file call must be rejected");
        };
        // Equality rather than a prefix: the whole aggregate is these two
        // diagnostics, so a cascade appended after them would fail here.
        assert_eq!(
            err.to_string(),
            "caller:1:55: argument label `b` is out of order: it names parameter 2 of function \
             `math::add` but appears at position 1; named arguments must be given in declaration \
             order; caller:1:61: argument label `a` is out of order: it names parameter 1 of \
             function `math::add` but appears at position 2; named arguments must be given in \
             declaration order",
            "the diagnostic is attributed to the calling file and names the written path"
        );
    }

    #[test]
    fn namespace_qualified_associated_call_labels_checked() {
        // The last of the five branches: an associated function reached through
        // an imported file's namespace (`geo::Point::new(...)`). The call sits in
        // the entry file, whose label is absent, so the bare `1:92` also shows
        // the report was not attributed to `lib::geo` where the callee lives.
        let files = [
            (
                vec![],
                "use lib::geo; use lib::geo::{Point}; \
                 pub fn main() -> i32 { let p: Point = geo::Point::new(b: 4, a: 3); return p.sum(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn new(a: i32, b: i32) -> Point { return Point { x: a, y: b }; } \
                 pub fn sum(self) -> i32 { return self.x + self.y; } }",
            ),
        ];
        let Err(err) = try_type_check_multi_file(&files) else {
            panic!("reordered labels on a namespace-qualified associated call must be rejected");
        };
        assert_eq!(
            err.to_string(),
            "1:92: argument label `b` is out of order: it names parameter 2 of method \
             `Point::new` but appears at position 1; named arguments must be given in declaration \
             order; 1:98: argument label `a` is out of order: it names parameter 1 of method \
             `Point::new` but appears at position 2; named arguments must be given in declaration \
             order",
            "the diagnostic carries the call site's file label and stands alone"
        );
    }
}

mod acceptance {
    use super::*;

    #[test]
    fn labels_in_declaration_order_compile_and_run() {
        // The rule's whole point, executed: the labels agree with the parameters
        // they face, and `10 - 3` is what the source reads as. Before the fix the
        // reordered spelling of this call compiled to `3 - 10`.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(left: 10, right: 3); }"#;
        assert_eq!(
            run_main(source),
            7,
            "subtract(left: 10, right: 3) subtracts the argument it names `right`"
        );
    }

    #[test]
    fn all_positional_arguments_compile_and_run() {
        // The unlabelled spelling of the same call: the check must not fire on a
        // list that carries no labels at all, which is every call in the corpus.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(10, 3); }"#;
        assert_eq!(
            run_main(source),
            7,
            "subtract(10, 3) binds by position exactly as before"
        );
    }

    #[test]
    fn three_labels_in_declaration_order_reach_their_own_slots() {
        // Three distinguishable values through three labelled slots: the digits
        // of the result say which argument landed where, so any permutation of
        // the binding would show up as a different number rather than as a
        // module that merely validates.
        let source = r#"fn mix(a: i32, b: i32, c: i32) -> i32 { return a * 100 + b * 10 + c; } pub fn main() -> i32 { return mix(a: 1, b: 2, c: 3); }"#;
        assert_eq!(
            run_main(source),
            123,
            "each labelled argument reaches the slot its label names"
        );
    }

    #[test]
    fn instance_method_label_compiles_and_runs() {
        // The receiver is not one of the written arguments, so the single label
        // `d` faces the single parameter `d`, and running the module shows the
        // argument reached that slot rather than merely type-checking against it.
        //
        // A receiver counted among the callee's parameter names would not show up
        // here — the extra name makes the list longer than the arguments written,
        // and the length gate then skips the check silently. What catches that is
        // [`rejection::instance_method_unknown_label_rejected`], where the same
        // shift turns a rejection into an acceptance.
        let source = r#"struct P { x: i32; y: i32; fn addto(self, d: i32) -> i32 { return self.x + d; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; return p.addto(d: 5); }"#;
        assert_eq!(
            run_main(source),
            6,
            "p.addto(d: 5) adds the labelled argument to the receiver's field"
        );
    }

    #[test]
    fn associated_function_labels_in_order_compile_and_run() {
        // The `Type::assoc()` branch written correctly, executed: the same two
        // names that are rejected reordered are accepted in declaration order.
        let source = r#"struct P { x: i32; y: i32; fn make(a: i32, b: i32) -> i32 { return a - b; } } pub fn main() -> i32 { return P::make(a: 10, b: 3); }"#;
        assert_eq!(
            run_main(source),
            7,
            "P::make(a: 10, b: 3) subtracts the argument it names `b`"
        );
    }

    #[test]
    fn zero_argument_call_compiles_and_runs() {
        // An empty argument list has no first argument to set a shape and no
        // label to compare, so both checks must return before they index into
        // anything.
        let source = r#"fn zero() -> i32 { return 7; } pub fn main() -> i32 { return zero(); }"#;
        assert_eq!(run_main(source), 7, "a zero-argument call is untouched");
    }

    #[test]
    fn anonymous_parameter_stays_passable_positionally() {
        // `_: i32` cannot be labelled, but it can still be passed: the rule that
        // makes a label aimed at it unknown must not make the positional call
        // unwritable. Code generation does not support ignored parameters yet,
        // which is why this asserts on the frontend rather than on a module.
        let source = r#"fn takes(_: i32, b: i32) -> i32 { return b; } pub fn main() -> i32 { return takes(1, 2); }"#;
        assert!(
            diagnostics(source).is_empty(),
            "an anonymous parameter is passed by position: {:?}",
            diagnostics(source)
        );
    }

    #[test]
    fn labelling_a_call_changes_nothing_it_emits() {
        // A label is checked against the declaration and then dropped; it never
        // selects a parameter. The two spellings of one call must therefore emit
        // the same module byte for byte, which is what makes the rejection of an
        // out-of-order label a frontend decision rather than a lowering one: were
        // labels ever to become selective, this equality is the first thing that
        // would break.
        let labelled = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(left: 10, right: 3); }"#;
        let positional = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(10, 3); }"#;

        let emitted = |source: &str| {
            try_codegen(source)
                .unwrap_or_else(|e| panic!("a well-labelled call must still compile: {e}"))
                .wasm()
                .to_vec()
        };
        assert_eq!(
            emitted(labelled),
            emitted(positional),
            "writing the labels a call already satisfies must not change the emitted module"
        );
    }
}

mod unchanged_diagnostics {
    use super::*;

    /// Asserts `source` reports an arity mismatch against `name` and that no
    /// label diagnostic joins it.
    fn only_the_arity_is_reported(source: &str, name: &str, expected: usize, found: usize) {
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::ArgumentCountMismatch {
                    name: got, expected: got_expected, found: got_found, ..
                } if got == name && *got_expected == expected && *got_found == found
            )),
            "the arity mismatch is reported: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| matches!(
                e,
                TypeCheckError::UnknownArgumentLabel { .. }
                    | TypeCheckError::ArgumentLabelOutOfOrder { .. }
                    | TypeCheckError::MixedNamedAndPositionalArguments { .. }
            )),
            "no label diagnostic joins it: {errors:?}"
        );
    }

    #[test]
    fn arity_mismatch_with_a_bad_label_reports_only_the_arity() {
        // A wrong count leaves every label facing a parameter that may not be the
        // one the writer meant, so a label report on top of it would be advice
        // derived from a list that is already known to be the wrong length. The
        // count is the actionable diagnostic and stands alone. This branch
        // abandons the call as soon as it counts, which the next two do not.
        let source = r#"fn subtract(left: i32, right: i32) -> i32 { return left - right; } pub fn main() -> i32 { return subtract(wrong: 10); }"#;
        only_the_arity_is_reported(source, "subtract", 2, 1);
    }

    #[test]
    fn too_many_labelled_arguments_on_a_method_report_only_the_arity() {
        // The method branch counts, reports, and then binds its arguments anyway,
        // so here the label check is reached with more arguments written than the
        // callee has parameters. Only the length gate keeps it from reading past
        // the end of the parameter names.
        let source = r#"struct P { x: i32; y: i32; fn addto(self, d: i32) -> i32 { return self.x + d; } } pub fn main() -> i32 { let p: P = P { x: 1, y: 2 }; return p.addto(bogus: 1, extra: 2); }"#;
        only_the_arity_is_reported(source, "P::addto", 1, 2);
    }

    #[test]
    fn too_few_labelled_arguments_on_an_associated_function_report_only_the_arity() {
        // The same fall-through on the `Type::assoc()` branch, with the shortfall
        // in the other direction. `b` faces `a` here only because an argument is
        // missing, so calling it out of order would be an answer to a question the
        // count already asked — and the advice would be wrong, since `b` is
        // exactly where it belongs once the missing argument is written.
        let source = r#"struct P { x: i32; y: i32; fn make(a: i32, b: i32) -> i32 { return a - b; } } pub fn main() -> i32 { return P::make(b: 3); }"#;
        only_the_arity_is_reported(source, "P::make", 2, 1);
    }

    #[test]
    fn struct_literal_fields_may_still_be_written_in_any_order() {
        // A struct literal binds by name, not by position, and reordering its
        // fields is legal and always was. The call rule shares the `name: value`
        // spelling and must not have leaked into it — this compiles and runs, and
        // reading back `x` shows the reordered fields still landed correctly.
        let source = r#"struct P { x: i32; y: i32; } pub fn main() -> i32 { let p: P = P { y: 2, x: 1 }; return p.x; }"#;
        assert_eq!(
            run_main(source),
            1,
            "a reordered struct literal still binds each field by its name"
        );
    }

    #[test]
    fn call_to_an_undefined_function_keeps_its_diagnostic() {
        // An all-positional call to a callee that does not resolve reaches the
        // shape check, finds nothing to report, and leaves the undefined-function
        // diagnostic as the only word on the call.
        let source = r#"pub fn main() -> i32 { return nosuch(1, 2); }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::UndefinedFunction { name, .. } if name == "nosuch"
            )),
            "the undefined callee is still reported: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| matches!(
                e,
                TypeCheckError::UnknownArgumentLabel { .. }
                    | TypeCheckError::ArgumentLabelOutOfOrder { .. }
                    | TypeCheckError::MixedNamedAndPositionalArguments { .. }
            )),
            "an unlabelled call has nothing to report: {errors:?}"
        );
    }

    #[test]
    fn a_call_filling_a_generic_slot_is_inferred_twice() {
        // The premise the next three tests rest on, and the reason the label
        // checks need a guard of their own: inferring the type argument of `id`
        // re-infers the expression that fills its parameter slot, so `h`'s call
        // node is visited twice and its arity diagnostic — which has no such
        // guard — is written twice.
        //
        // FIXME: the doubled arity report is a pre-existing wart of that second
        // visit, not of argument labelling, and is out of scope for the labelling
        // rule; this asserts today's behaviour so the doubling is noticed if it
        // is ever fixed and this test's premise disappears with it.
        let source = r#"fn id T'(x: T) -> T { return x; } fn h(a: i32, b: i32) -> i32 { return a; } pub fn main() -> i32 { return id(h(1)); }"#;
        let arity_reports = diagnostics(source)
            .into_iter()
            .filter(|e| matches!(e, TypeCheckError::ArgumentCountMismatch { .. }))
            .count();
        assert_eq!(
            arity_reports, 2,
            "an unguarded diagnostic on the re-inferred call is written once per visit"
        );
    }

    #[test]
    fn unknown_label_in_a_generic_slot_is_reported_once() {
        // The same double visit with a guarded diagnostic: one mistake, one
        // report. Without the guard this would read 2, exactly as the arity
        // report above does.
        let source = r#"fn id T'(x: T) -> T { return x; } fn g(a: i32) -> i32 { return a; } pub fn main() -> i32 { return id(g(bogus: 1)); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::UnknownArgumentLabel { name, label, .. } = &errors[0] else {
            panic!("expected UnknownArgumentLabel, got {errors:?}");
        };
        assert_eq!(
            (name.as_str(), label.as_str()),
            ("g", "bogus"),
            "the inner call is reported, not the generic wrapper"
        );
        assert_eq!(caret(&errors[0]), (1, 104), "caret sits on the label token");
    }

    #[test]
    fn out_of_order_labels_in_a_generic_slot_are_reported_once_each() {
        // Two mistakes on one re-inferred call: the guard is per call site, not
        // per diagnostic, so it must still let both labels report — suppressing
        // the second visit is not the same as suppressing the second label.
        let source = r#"fn id T'(x: T) -> T { return x; } fn g(a: i32, b: i32) -> i32 { return a - b; } pub fn main() -> i32 { return id(g(b: 3, a: 10)); }"#;
        let errors = diagnostics_numbering(source, 2);
        let labels: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                TypeCheckError::ArgumentLabelOutOfOrder { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            vec!["b", "a"],
            "each label reports once, and neither reports twice: {errors:?}"
        );
    }

    #[test]
    fn mixed_arguments_in_a_generic_slot_are_reported_once() {
        // The shape check carries its own guard, separate from the one the label
        // check uses, and this is what pins it.
        let source = r#"fn id T'(x: T) -> T { return x; } fn g(a: i32, b: i32) -> i32 { return a - b; } pub fn main() -> i32 { return id(g(1, b: 2)); }"#;
        let errors = diagnostics_numbering(source, 1);
        let TypeCheckError::MixedNamedAndPositionalArguments { .. } = &errors[0] else {
            panic!("expected MixedNamedAndPositionalArguments, got {errors:?}");
        };
        assert_eq!(caret(&errors[0]), (1, 119), "caret sits on the label token");
    }
}
