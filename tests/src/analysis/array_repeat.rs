//! Analysis of the repeated array literal `[value; N]` (#218).
//!
//! A repeat is an array literal, so every rule that holds a list to where it
//! may appear holds a repeat to the same places, and every rule that looks at a
//! list's elements looks at the repeat's value — the one element it has.
#[cfg(test)]
mod array_repeat_analysis_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    /// The analysis diagnostics of `source`, empty when it passes.
    fn diagnostics(source: &str) -> Vec<AnalysisDiagnostic> {
        match inference_analysis::analyze(&type_check(source)) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.errors().to_vec(),
        }
    }

    /// A repeat is accepted wherever a list is: a `let` or `const` initializer,
    /// an assignment's right-hand side, a return value, a struct field, and an
    /// element of another array.
    #[test]
    fn a_repeat_is_accepted_where_a_list_is() {
        for source in [
            "pub fn f() -> i32 { let a: [i32; 4] = [0; 4]; return a[3]; }",
            "pub fn f() -> i32 { const A: [i32; 4] = [7; 4]; return A[1]; }",
            "pub fn f() -> i32 { let mut a: [i32; 4] = [1, 2, 3, 4]; a = [9; 4]; return a[0]; }",
            "pub fn f() -> [i32; 4] { return [5; 4]; }",
            "struct S { xs: [i32; 3]; }\n\
             pub fn f() -> i32 { let s: S = S { xs: [2; 3] }; return s.xs[2]; }",
            "pub fn f() -> i32 { let g: [[i32; 3]; 2] = [[1; 3]; 2]; return g[1][2]; }",
            "pub fn f() -> i32 { let g: [[i32; 2]; 2] = [[1; 2], [2; 2]]; return g[1][0]; }",
        ] {
            let errors = diagnostics(source);
            assert!(errors.is_empty(), "`{source}` must pass analysis, got: {errors:?}");
        }
    }

    /// Whether a diagnostic is the one a case expects.
    type Expects = fn(&AnalysisDiagnostic) -> bool;

    /// The positions a list is refused in refuse a repeat with the same
    /// diagnostic: a call argument (A012), an operand (A015), and a field or
    /// element assignment's right-hand side (A029).
    #[test]
    fn a_repeat_is_refused_where_a_list_is() {
        let cases: [(&str, Expects); 3] = [
            (
                "fn sum(a: [i32; 3]) -> i32 { return a[0]; }\n\
                 pub fn f() -> i32 { return sum([1; 3]); }",
                |e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", .. }),
            ),
            (
                "pub fn f() -> i32 { return [1; 3][0]; }",
                |e| {
                    matches!(
                        e,
                        AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { kind: "array", .. }
                    )
                },
            ),
            (
                "struct S { xs: [i32; 3]; }\n\
                 pub fn f() -> i32 { let mut s: S = S { xs: [0, 0, 0] }; s.xs = [1; 3]; \
                 return s.xs[0]; }",
                |e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }),
            ),
        ];
        for (source, expected) in cases {
            let errors = diagnostics(source);
            assert!(
                errors.iter().any(expected),
                "`{source}` must be refused, got: {errors:?}"
            );
        }
    }

    /// The value is checked against the element type, as a list's elements
    /// are: a value the element type cannot hold is A022.
    #[test]
    fn a_value_out_of_the_element_range_is_a022() {
        let errors = diagnostics("pub fn f() -> u8 { let a: [u8; 2] = [256; 2]; return a[0]; }");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. })),
            "{errors:?}"
        );
    }

    /// A036's frame estimate charges a repeat that reads the array it is
    /// assigned to the scratch region code generation stages it in, as it does
    /// a list: a chain that fits only without that scratch must be rejected.
    #[test]
    fn a036_charges_the_scratch_a_self_referencing_repeat_needs() {
        // An 8 KB array, reassigned from a repeat of its own element: 16 KB of
        // frame with the scratch, 8 KB without. Eight nested calls of a
        // function this size fit a 64 KB stack only if the scratch is missed.
        let mut source = String::from(
            "fn leaf() -> i32 { let mut a: [i32; 2048] = [0; 2048]; a = [a[1]; 2048]; \
             return a[0]; }\n",
        );
        let mut callee = String::from("leaf");
        for depth in 0..4 {
            let name = format!("level{depth}");
            source.push_str(&format!(
                "fn {name}() -> i32 {{ let mut a: [i32; 2048] = [0; 2048]; a = [a[1]; 2048]; \
                 return a[0] + {callee}(); }}\n"
            ));
            callee = name;
        }
        source.push_str(&format!("pub fn f() -> i32 {{ return {callee}(); }}\n"));
        let errors = diagnostics(&source);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StackDepthExceeded { .. })),
            "five 16 KB frames exceed the 64 KB stack only with the scratch charged: {errors:?}"
        );
    }
}
