/// Integration tests for analysis rule A039.
///
/// - A039: StructUzumakiAsArgument — a struct-typed (or custom non-enum) `@`
///   passed directly as a function argument is rejected. Codegen lowers a
///   struct-typed `@` by filling a named frame slot, and a call argument has no
///   such name, so it reaches codegen with no enclosing variable and panics. This
///   is the struct sibling of A014 (the array case in the same argument
///   position); arrays stay with A014, scalars and enums need no slot and are
///   allowed.
///
/// These tests are the cross-crate guard that the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, complementing the
/// in-crate message/`rule_id` unit test in `core/analysis`. The rejected inputs
/// are type-valid (only analysis rejects them), so `type_check` must succeed: the
/// type checker threads each argument-position `@` its parameter's declared type.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = type_check(source);
        inference_analysis::analyze(&ctx)
    }

    /// Returns true if any analysis error is a `StructUzumakiAsArgument` (A039).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules.
    fn has_a039(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StructUzumakiAsArgument { .. })),
        }
    }

    fn a039_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::StructUzumakiAsArgument { .. }))
            .expect("expected a StructUzumakiAsArgument diagnostic")
            .clone()
    }

    /// Counts how many `StructUzumakiAsArgument` (A039) diagnostics the analysis
    /// emits for `source`. Filters by variant so unrelated rules tripped by the
    /// same surface do not perturb the count.
    fn count_a039(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::StructUzumakiAsArgument { .. }))
                .count(),
        }
    }

    // ---------------------------------------------------------------------
    // Rejected: A039 fires
    // ---------------------------------------------------------------------

    /// THE regression guard: a struct-typed `@` passed as a free-function
    /// argument. This exact program panicked codegen
    /// ("Struct uzumaki ... has no enclosing variable name") before A039.
    #[test]
    fn a039_struct_uzumaki_as_argument_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            spec Check {
                fn obligation() {
                    forall {
                        let r: i32 = consume(@);
                    }
                }
            }
            pub fn main() {}
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct-typed `@` passed as a function argument"
        );
    }

    /// The same struct-typed `@` argument in a plain free function (no spec):
    /// the walker visits free-function bodies just as it does spec bodies.
    #[test]
    fn a039_struct_uzumaki_argument_in_free_function_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn f() {
                forall {
                    let r: i32 = consume(@);
                }
            }
            fn main() {}
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct-typed `@` argument in a free function"
        );
    }

    /// A struct `@` mixed with scalar arguments is rejected: only the struct
    /// parameter position needs a frame slot, and that is where `@` sits.
    #[test]
    fn a039_struct_uzumaki_argument_mixed_with_scalars_rejected() {
        let source = r#"
            struct Mid { v: i32; }
            fn consume(a: i32, m: Mid, b: i32) -> i32 { return a + m.v + b; }
            fn main() {
                forall {
                    let r: i32 = consume(1, @, 3);
                }
            }
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct `@` in the middle of scalar arguments"
        );
    }

    /// Two struct `@` arguments in one call produce exactly two A039
    /// diagnostics, one per offending argument position.
    #[test]
    fn a039_two_struct_uzumaki_arguments_counted() {
        let source = r#"
            struct A { v: i32; }
            struct B { w: i32; }
            fn consume(a: A, b: B) -> i32 { return a.v + b.w; }
            fn main() {
                forall {
                    let r: i32 = consume(@, @);
                }
            }
        "#;
        assert_eq!(
            count_a039(source),
            2,
            "expected exactly two A039 diagnostics for two struct `@` arguments"
        );
    }

    /// A struct `@` argument outside any non-det block is still rejected by A039:
    /// unlike A038, A039 has no `nondet_depth` guard (it checks all calls, like
    /// its sibling A014). The bare `@` also trips A006, but A039 owns the
    /// argument-position defect and must fire here too.
    #[test]
    fn a039_struct_uzumaki_argument_outside_nondet_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn main() {
                let r: i32 = consume(@);
            }
        "#;
        assert!(
            has_a039(source),
            "A039 has no nondet gate (like A014); a struct `@` argument is rejected outside a non-det block too"
        );
    }

    /// A struct `@` argument inside an `exists` block is rejected; A039 fires
    /// regardless of the enclosing non-det block kind.
    #[test]
    fn a039_struct_uzumaki_argument_in_exists_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn main() {
                exists {
                    let r: i32 = consume(@);
                }
            }
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct `@` argument inside an exists block"
        );
    }

    /// A struct `@` argument inside a struct *method* body is rejected: the
    /// walker descends into struct methods just as it does free functions.
    #[test]
    fn a039_struct_uzumaki_argument_in_method_body_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            struct Maker {
                n: i32;
                fn make(self) -> i32 {
                    forall {
                        let r: i32 = consume(@);
                    }
                    return self.n;
                }
            }
            fn main() {}
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct `@` argument inside a struct method body"
        );
    }

    /// A struct `@` passed to a *method* call (`obj.m(@)`) is rejected. A method
    /// call lowers to `Expr::FunctionCall` whose `function` is the receiver
    /// member-access and whose `args` hold `@`, so A039 sees the argument exactly
    /// as it sees a free-function argument -- A039 inherits A014's call-coverage
    /// model (all `Expr::FunctionCall` argument lists), and methods are not a
    /// special case.
    #[test]
    fn a039_struct_uzumaki_as_method_argument_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            struct Sink {
                n: i32;
                fn absorb(self, p: Point) -> i32 { return self.n + p.x; }
            }
            fn main() {
                let s: Sink = Sink { n: 0 };
                forall {
                    let r: i32 = s.absorb(@);
                }
            }
        "#;
        assert!(
            has_a039(source),
            "expected A039 for a struct `@` passed as a method-call argument"
        );
    }

    // ---------------------------------------------------------------------
    // Diagnostic quality
    // ---------------------------------------------------------------------

    /// The diagnostic explains the argument position is unsupported, suggests
    /// assigning to a variable first, and reports rule id A039.
    #[test]
    fn a039_diagnostic_message_and_rule_id() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn main() {
                forall {
                    let r: i32 = consume(@);
                }
            }
        "#;
        let diag = a039_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("function argument"),
            "A039 message must say `@` cannot be used as a function argument, got: {msg}"
        );
        assert!(
            msg.contains("assign to a variable"),
            "A039 message must suggest assigning to a variable first, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A039");
    }

    // ---------------------------------------------------------------------
    // Accepted: positions and types A039 does not own
    // ---------------------------------------------------------------------

    /// A scalar `@` argument is accepted: a scalar `@` lowers to a single uzumaki
    /// opcode and needs no enclosing variable. The program must also type-check
    /// and analyze cleanly (no A039, and no error at all).
    #[test]
    fn a039_scalar_uzumaki_argument_accepted() {
        let source = r#"
            fn consume(x: i32) -> i32 { return x; }
            fn main() {
                forall {
                    let r: i32 = consume(@);
                }
            }
        "#;
        assert!(
            !has_a039(source),
            "a scalar `@` argument must not trip A039"
        );
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "a scalar `@` argument must analyze cleanly: {:?}",
            result.err()
        );
    }

    /// An enum-typed `@` argument is accepted: an enum lowers to a single scalar
    /// uzumaki opcode like any other scalar, so it needs no frame slot.
    #[test]
    fn a039_enum_uzumaki_argument_accepted() {
        let source = r#"
            enum Color { Red, Green, Blue }
            fn consume(c: Color) -> i32 { return 0; }
            fn main() {
                forall {
                    let r: i32 = consume(@);
                }
            }
        "#;
        assert!(
            !has_a039(source),
            "an enum-typed `@` argument must not trip A039 (enums are scalar-like)"
        );
    }

    /// An array `@` argument is A014's, not A039's: assert A014 fires and A039
    /// does not. This guards against double-coverage between the two
    /// argument-position uzumaki rules.
    #[test]
    fn a039_array_uzumaki_argument_is_a014_not_a039() {
        let source = r#"
            fn consume(a: [i32; 3]) -> i32 { return a[0]; }
            fn main() {
                forall {
                    let r: i32 = consume(@);
                }
            }
        "#;
        let errors = analyze(source).expect_err("expected analysis errors but got Ok");
        let has_a014 = errors
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayUzumakiAsArgument { .. }));
        let has_a039 = errors
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::StructUzumakiAsArgument { .. }));
        assert!(has_a014, "expected A014 for an array `@` argument, got: {errors:?}");
        assert!(
            !has_a039,
            "an array `@` argument is A014's job; A039 must not also fire, got: {errors:?}"
        );
    }

    /// A struct *literal* (not `@`) passed as an argument is not A039's: the
    /// argument is a literal, so A039 stays silent. A012 (compound literal as
    /// argument) owns that defect; the inner scalar field `@` is allowed. This
    /// guards that A039 keys on the argument being `@`, not on the parameter type.
    #[test]
    fn a039_struct_literal_argument_not_flagged() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn main() {
                forall {
                    let r: i32 = consume(Point { x: @, y: 0 });
                }
            }
        "#;
        assert!(
            !has_a039(source),
            "a struct literal argument is A012's job, not A039's; A039 keys on the argument being `@`"
        );
    }

    /// Whole-struct `let p: Point = @;` is A027's position, not A039's: the `@`
    /// is a `let` initializer, not a call argument, so A039 must stay silent.
    #[test]
    fn a039_struct_uzumaki_let_binding_not_flagged() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let p: Point = @;
                }
            }
        "#;
        assert!(
            !has_a039(source),
            "a whole-struct `let p: Point = @;` is A027's job, not A039's"
        );
    }

    /// A call with no `@` argument at all (all arguments concrete) must not trip
    /// A039, even inside a non-det block.
    #[test]
    fn a039_struct_argument_without_uzumaki_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn consume(p: Point) -> i32 { return p.x; }
            fn main() {
                let p: Point = Point { x: 1, y: 2 };
                let r: i32 = consume(p);
            }
        "#;
        assert!(
            !has_a039(source),
            "a call passing a struct *variable* (not `@`) must not trip A039"
        );
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "passing a struct variable argument must analyze cleanly: {:?}",
            result.err()
        );
    }

    // The predicate's `TypeInfoKind::Custom(name)` arm (rejecting a custom
    // non-enum `@` argument) has no type-valid integration test here: a `type`
    // alias to a struct does not behave as the struct for member access — a
    // parameter typed by such an alias fails type-checking the moment its struct
    // fields are used (`member access requires a struct type, found `P``), so no
    // well-typed program reaches A039 with an unresolved `Custom` struct
    // argument. The arm is a defensive guard matching codegen (which treats a
    // `Custom` non-enum `@` as struct-like and panics on the missing slot) and
    // mirroring A038's predicate; its enum-exemption branch is covered by
    // `a039_enum_uzumaki_argument_accepted`.
}
