/// Integration tests for analysis rule A037.
///
/// - A037: ArrayIndexConstOutOfBounds — when `arr[c]` has a constant integer
///   literal index `c` and the array's type is `[T; length]`, the access is
///   rejected at compile time if `c < 0` or `c >= length`. Dynamic (non-literal)
///   indices are out of scope and fall to the future runtime guard.
///
/// These tests are the cross-crate guard that the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, complementing the
/// in-crate message/`rule_id` unit tests in `core/analysis`.
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

    /// Returns true if any analysis error is an `ArrayIndexConstOutOfBounds`
    /// (A037). Filters by variant rather than asserting a total error count,
    /// since the surface may also trip unrelated rules.
    fn has_const_oob(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndexConstOutOfBounds { .. })),
        }
    }

    fn const_oob_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::ArrayIndexConstOutOfBounds { .. }))
            .expect("expected an ArrayIndexConstOutOfBounds diagnostic")
            .clone()
    }

    /// Counts how many `ArrayIndexConstOutOfBounds` (A037) diagnostics the
    /// analysis emits for `source`. Used by multi-violation tests that assert an
    /// exact A037 count; like the other helpers it filters by variant so that
    /// unrelated rules tripped by the same surface do not perturb the count.
    fn count_const_oob(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ArrayIndexConstOutOfBounds { .. }))
                .count(),
        }
    }

    /// `arr[3]` on `[i32; 3]` is the off-by-one: index equals the length, so the
    /// last valid index is 2. A037 must fire.
    #[test]
    fn a037_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[3];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected ArrayIndexConstOutOfBounds for arr[3] on [i32; 3]"
        );
    }

    /// A negative literal index `arr[-1]` lowers to a single `NumberLiteral`
    /// whose value keeps the leading `-`, so A037 catches it directly.
    #[test]
    fn a037_negative_index_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[-1];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected ArrayIndexConstOutOfBounds for arr[-1] on [i32; 3]"
        );
    }

    /// `arr[2]` is the last valid index of `[i32; 3]`; A037 must stay silent.
    #[test]
    fn a037_last_valid_index_accepted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[2];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "arr[2] on [i32; 3] is the last valid index and must not trip A037"
        );
    }

    /// `arr[0]` is in bounds; A037 must stay silent.
    #[test]
    fn a037_first_index_accepted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[0];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "arr[0] on [i32; 3] is in bounds and must not trip A037"
        );
    }

    /// A dynamic index (a variable, not a literal) is out of scope for A037 even
    /// when its value would be out of bounds at runtime; it falls to the future
    /// runtime guard. A037 must not fire here.
    #[test]
    fn a037_dynamic_index_not_flagged() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let i: i32 = 5;
                return arr[i];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "a dynamic (non-literal) index must not trip the constant-index rule A037"
        );
    }

    /// The diagnostic must name the offending index and the array length, and
    /// report rule id A037.
    #[test]
    fn a037_diagnostic_names_index_and_length() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[3];
            }
        "#;
        let diag = const_oob_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("out of bounds"),
            "A037 message must say the index is out of bounds, got: {msg}"
        );
        assert!(
            msg.contains('3'),
            "A037 message must include the index and length, got: {msg}"
        );
        assert!(
            msg.contains("length 3"),
            "A037 message must include the array length, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A037");
    }

    /// A negative literal index is reported verbatim in the diagnostic.
    #[test]
    fn a037_negative_index_named_in_diagnostic() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[-1];
            }
        "#;
        let diag = const_oob_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("-1"),
            "A037 message must include the negative index verbatim, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A037");
    }

    // ---------------------------------------------------------------------
    // Rejected: larger arrays and various lengths
    // ---------------------------------------------------------------------

    /// `arr[10]` on `[i32; 10]` is the off-by-one at a larger length: indices
    /// 0..9 are valid, so 10 is out of bounds.
    #[test]
    fn a037_len10_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
                return arr[10];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[10] on [i32; 10]"
        );
    }

    /// `arr[11]` on `[i32; 10]` is one past the off-by-one boundary.
    #[test]
    fn a037_len10_index_past_length_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
                return arr[11];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[11] on [i32; 10]"
        );
    }

    /// A single-element array `[i32; 1]` has only index 0 valid; `arr[1]` is the
    /// off-by-one and must fire.
    #[test]
    fn a037_len1_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 1] = [42];
                return arr[1];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[1] on [i32; 1]"
        );
    }

    /// A constant index far beyond the array length must be rejected.
    #[test]
    fn a037_index_far_beyond_length_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[1000];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[1000] on [i32; 3]"
        );
    }

    /// A literal index too large to fit in `i128` cannot be a valid index for
    /// any array; the rule mirrors `literal_out_of_range` and treats the
    /// parse failure as out of bounds.
    #[test]
    fn a037_index_overflowing_i128_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[999999999999999999999999999999999999999];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an index literal that overflows i128"
        );
    }

    /// A large negative literal index is rejected the same way `arr[-1]` is.
    #[test]
    fn a037_large_negative_index_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[-100];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[-100] on [i32; 3]"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: non-i32 element types — the array length drives the check
    // ---------------------------------------------------------------------

    /// A037 fires on `[u8; 4]` indexed at 4 regardless of the `u8` element type:
    /// the length, not the element type, drives the check.
    #[test]
    fn a037_u8_element_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> u8 {
                let arr: [u8; 4] = [1, 2, 3, 4];
                return arr[4];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[4] on [u8; 4]"
        );
    }

    /// A037 fires on `[i64; 2]` indexed at 2; the 64-bit element type is
    /// irrelevant to the length-based bounds check.
    #[test]
    fn a037_i64_element_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> i64 {
                let arr: [i64; 2] = [1, 2];
                return arr[2];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[2] on [i64; 2]"
        );
    }

    /// A037 fires on `[bool; 2]` indexed at 2.
    #[test]
    fn a037_bool_element_index_equal_to_length_rejected() {
        let source = r#"
            fn test() -> bool {
                let arr: [bool; 2] = [true, false];
                return arr[2];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[2] on [bool; 2]"
        );
    }

    /// A037 fires on `[i16; 3]` indexed at 5 (well past the length).
    #[test]
    fn a037_i16_element_index_past_length_rejected() {
        let source = r#"
            fn test() -> i16 {
                let arr: [i16; 3] = [1, 2, 3];
                return arr[5];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for arr[5] on [i16; 3]"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: write position — the walker visits both sides of an assignment
    // ---------------------------------------------------------------------

    /// A constant-OOB *write* `a[3] = 1` is caught because the analysis walker
    /// visits both sides of `Stmt::Assign`, so the LHS `ArrayIndexAccess` is
    /// walked and its literal index checked.
    #[test]
    fn a037_const_oob_write_rejected() {
        let source = r#"
            fn test() {
                let mut a: [i32; 3] = [0, 0, 0];
                a[3] = 1;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 on the LHS of the OOB write a[3] = 1"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: multi-dimensional arrays
    // ---------------------------------------------------------------------

    /// The outer index of a `[[i32; 3]; 2]` matrix selects a row; `m[2]` is out
    /// of bounds because the outer length is 2 (valid rows are 0 and 1).
    #[test]
    fn a037_multidim_outer_index_out_of_bounds_rejected() {
        let source = r#"
            fn test() -> i32 {
                let m: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
                return m[2][0];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for the outer index m[2] on [[i32; 3]; 2]"
        );
    }

    /// The inner index of a `[[i32; 3]; 2]` matrix selects a column; `m[0][3]`
    /// is out of bounds because the inner length is 3 (valid columns 0..2).
    #[test]
    fn a037_multidim_inner_index_out_of_bounds_rejected() {
        let source = r#"
            fn test() -> i32 {
                let m: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
                return m[0][3];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for the inner index m[0][3] on [[i32; 3]; 2]"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: array of structs
    // ---------------------------------------------------------------------

    /// Indexing an array of structs past its length fires A037; the subsequent
    /// `.x` member access does not change the index check on `p[2]`.
    #[test]
    fn a037_array_of_structs_index_out_of_bounds_rejected() {
        let source = r#"
            struct Pt { x: i32; y: i32; }
            fn test() -> i32 {
                let p: [Pt; 2] = [Pt { x: 1, y: 2 }, Pt { x: 3, y: 4 }];
                return p[2].x;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for p[2] on [Pt; 2]"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: const-OOB in various syntactic positions
    // ---------------------------------------------------------------------

    /// A constant-OOB access as a `let` initializer fires.
    #[test]
    fn a037_oob_in_let_initializer_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let x: i32 = arr[5];
                return x;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access in a let initializer"
        );
    }

    /// A constant-OOB access in a `return` expression fires.
    #[test]
    fn a037_oob_in_return_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[5];
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access in a return expression"
        );
    }

    /// A constant-OOB access on the RHS of an assignment fires; the walker
    /// visits the RHS expression of `Stmt::Assign`.
    #[test]
    fn a037_oob_on_assignment_rhs_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let mut x: i32 = 0;
                x = arr[5];
                return x;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access on the RHS of an assignment"
        );
    }

    /// Two distinct OOB accesses inside one binary expression each produce a
    /// diagnostic; the walker recurses into both operands of `Expr::Binary`.
    #[test]
    fn a037_two_oob_in_binary_expression_counted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[3] + arr[4];
            }
        "#;
        assert_eq!(
            count_const_oob(source),
            2,
            "expected exactly two A037 diagnostics for arr[3] + arr[4]"
        );
    }

    /// A constant-OOB access inside an `if` condition fires; the walker visits
    /// the condition expression of `Stmt::If`.
    #[test]
    fn a037_oob_in_if_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                if arr[5] > 0 {
                    return 1;
                }
                return 0;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access in an if condition"
        );
    }

    /// A constant-OOB access inside a conditional `loop` condition fires; the
    /// walker visits the condition expression of `Stmt::Loop`.
    #[test]
    fn a037_oob_in_loop_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                loop arr[5] > 0 {
                    break;
                }
                return 0;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access in a loop condition"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: method and spec bodies (the walker covers both)
    // ---------------------------------------------------------------------

    /// A constant-OOB access inside a struct *method* body fires; the walker's
    /// `for_each_function_body` descends into struct methods.
    #[test]
    fn a037_oob_in_struct_method_rejected() {
        let source = r#"
            struct Holder { n: i32;
                fn get(self) -> i32 {
                    let arr: [i32; 3] = [1, 2, 3];
                    return arr[3];
                }
            }
            fn main() -> i32 {
                return 0;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access inside a struct method body"
        );
    }

    /// A constant-OOB access inside a *spec* function body fires; the walker's
    /// `for_each_function_body` recurses into spec definitions.
    #[test]
    fn a037_oob_in_spec_function_rejected() {
        let source = r#"
            fn main() -> i32 {
                return 0;
            }
            spec S {
                fn check() -> i32 {
                    let arr: [i32; 3] = [1, 2, 3];
                    return arr[5];
                }
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access inside a spec function body"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: inside a non-det block
    // ---------------------------------------------------------------------

    /// A constant-OOB access on an uzumaki-initialized array inside a `forall`
    /// block fires; non-det block bodies are walked like any other.
    #[test]
    fn a037_oob_inside_forall_block_rejected() {
        let source = r#"
            fn test() -> i32 {
                forall {
                    let a: [i32; 3] = @;
                    let x: i32 = a[3];
                }
                return 0;
            }
        "#;
        assert!(
            has_const_oob(source),
            "expected A037 for an OOB access inside a forall block"
        );
    }

    // ---------------------------------------------------------------------
    // Rejected: multiple distinct OOB accesses in one function
    // ---------------------------------------------------------------------

    /// Three distinct constant-OOB accesses in one function produce three
    /// diagnostics, one per offending access.
    #[test]
    fn a037_three_distinct_oob_accesses_counted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let a: i32 = arr[3];
                let b: i32 = arr[4];
                let c: i32 = arr[100];
                return a + b + c;
            }
        "#;
        assert_eq!(
            count_const_oob(source),
            3,
            "expected exactly three A037 diagnostics for three distinct OOB accesses"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: in-bounds boundaries across several lengths
    // ---------------------------------------------------------------------

    /// The first, a middle, and the last index of `[i32; 5]` are all in bounds;
    /// none of them may trip A037.
    #[test]
    fn a037_in_bounds_boundaries_len5_accepted() {
        for index in ["0", "2", "4"] {
            let source = format!(
                r#"
                fn test() -> i32 {{
                    let arr: [i32; 5] = [10, 20, 30, 40, 50];
                    return arr[{index}];
                }}
            "#
            );
            assert!(
                !has_const_oob(&source),
                "arr[{index}] on [i32; 5] is in bounds and must not trip A037"
            );
        }
    }

    /// The last valid index of a larger `[i32; 10]` array is in bounds.
    #[test]
    fn a037_last_valid_index_len10_accepted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
                return arr[9];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "arr[9] on [i32; 10] is the last valid index and must not trip A037"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: dynamic / shallow-scope indices
    // ---------------------------------------------------------------------

    /// A parameter used as an index is dynamic, not a literal, so A037 never
    /// fires even though its runtime value could be out of bounds.
    #[test]
    fn a037_parameter_index_not_flagged() {
        let source = r#"
            fn test(i: i32) -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                return arr[i];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "a parameter index is dynamic and must not trip A037"
        );
    }

    /// A037 is intentionally shallow: an index that is a `const NAME` resolving
    /// to an out-of-range value is an `Identifier`, not a `NumberLiteral`, so
    /// the rule does not fire. This documents the deliberate scope boundary —
    /// such accesses fall to the future runtime guard.
    #[test]
    fn a037_const_name_index_not_flagged() {
        let source = r#"
            fn test() -> i32 {
                const C: i32 = 5;
                let arr: [i32; 3] = [1, 2, 3];
                return arr[C];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "a `const NAME` index is an identifier, not a literal, so A037 (shallow by design) must not fire"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: valid multi-dimensional and array-of-structs accesses
    // ---------------------------------------------------------------------

    /// A fully in-bounds multi-dimensional access `m[1][2]` on `[[i32; 3]; 2]`
    /// must not trip A037 at either dimension.
    #[test]
    fn a037_valid_multidim_access_accepted() {
        let source = r#"
            fn test() -> i32 {
                let m: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
                return m[1][2];
            }
        "#;
        assert!(
            !has_const_oob(source),
            "m[1][2] on [[i32; 3]; 2] is in bounds at both dimensions and must not trip A037"
        );
    }

    /// A fully in-bounds array-of-structs access `p[1].y` on `[Pt; 2]` must not
    /// trip A037.
    #[test]
    fn a037_valid_array_of_structs_access_accepted() {
        let source = r#"
            struct Pt { x: i32; y: i32; }
            fn test() -> i32 {
                let p: [Pt; 2] = [Pt { x: 1, y: 2 }, Pt { x: 3, y: 4 }];
                return p[1].y;
            }
        "#;
        assert!(
            !has_const_oob(source),
            "p[1].y on [Pt; 2] is in bounds and must not trip A037"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: zero-length arrays are not legal Inference
    // ---------------------------------------------------------------------

    /// A zero-length array `[i32; 0]` is rejected by the type checker
    /// ("invalid array size `0`; must be a positive integer that fits in 32
    /// bits"), so there is no legal Inference surface on which `arr[0]` on a
    /// length-0 array could reach A037. The `0 >= 0` OOB case is therefore
    /// unreachable in practice; this test documents that the type checker, not
    /// A037, is the guard here. (Per task guidance the scenario is noted rather
    /// than asserted as an A037 firing.)
    #[test]
    fn a037_zero_length_array_rejected_by_type_checker() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 0] = [];
                return arr[0];
            }
        "#;
        let arena = build_ast(source.to_string());
        let result = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "a zero-length array `[i32; 0]` must be rejected by the type checker, making the 0 >= 0 A037 case unreachable"
        );
    }

    // ---------------------------------------------------------------------
    // Mixed: one invalid access among valid ones
    // ---------------------------------------------------------------------

    /// A function that mixes several valid accesses with exactly one OOB access
    /// produces exactly one A037 diagnostic — the valid accesses add no noise.
    #[test]
    fn a037_mixed_valid_and_one_invalid_counts_exactly_one() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let a: i32 = arr[0];
                let b: i32 = arr[2];
                let c: i32 = arr[3];
                return a + b + c;
            }
        "#;
        assert_eq!(
            count_const_oob(source),
            1,
            "expected exactly one A037 diagnostic when only one of several accesses is OOB"
        );
    }

    // ---------------------------------------------------------------------
    // Diagnostic quality
    // ---------------------------------------------------------------------

    /// On a larger array the diagnostic still names the offending index and the
    /// array length verbatim and reports rule id A037.
    #[test]
    fn a037_large_array_diagnostic_names_index_and_length() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
                return arr[11];
            }
        "#;
        let diag = const_oob_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("11"),
            "A037 message must include the offending index 11, got: {msg}"
        );
        assert!(
            msg.contains("length 10"),
            "A037 message must include the array length 10, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A037");
    }

    /// For a multi-dimensional outer-index violation the diagnostic names the
    /// outer index and the outer length (2) and reports rule id A037.
    #[test]
    fn a037_multidim_diagnostic_names_outer_index_and_length() {
        let source = r#"
            fn test() -> i32 {
                let m: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
                return m[2][0];
            }
        "#;
        let diag = const_oob_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains('2'),
            "A037 message must include the outer index 2, got: {msg}"
        );
        assert!(
            msg.contains("length 2"),
            "A037 message must include the outer array length 2, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A037");
    }
}
