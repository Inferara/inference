/// Integration tests for analysis rule A056.
///
/// - A056: ArrayIndexNotProvenInBounds — every array access whose index is not
///   a bare literal must be proven, from the code the programmer wrote, to index
///   within `0..length` on every run that reaches it. The runtime guard stays,
///   but an out-of-range index has to be a path the program handles, never an
///   implicit trap.
///
/// Each test runs the real parse -> type-check -> analyze pipeline and compares
/// the A056 findings against the accesses expected to be unproven.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_ast::ids::NodeId;

    /// One A056 finding: the index's local name, when it is one, and the
    /// interval the analysis established for it.
    #[derive(Debug, PartialEq, Eq)]
    struct Unproven {
        index: Option<String>,
        lo: i128,
        hi: i128,
    }

    fn unproven(index: Option<&str>, lo: i128, hi: i128) -> Unproven {
        Unproven {
            index: index.map(str::to_string),
            lo,
            hi,
        }
    }

    const I32_MIN: i128 = i32::MIN as i128;
    const I32_MAX: i128 = i32::MAX as i128;
    const U32_MAX: i128 = u32::MAX as i128;

    fn a056_findings(source: &str) -> Vec<Unproven> {
        let arena = build_ast(source.to_string());
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context();
        let Err(errors) = inference_analysis::analyze(&ctx) else {
            return Vec::new();
        };
        errors
            .errors()
            .iter()
            .filter_map(|diagnostic| match diagnostic {
                AnalysisDiagnostic::ArrayIndexNotProvenInBounds { index, lo, hi, .. } => {
                    Some(Unproven {
                        index: index.clone(),
                        lo: *lo,
                        hi: *hi,
                    })
                }
                _ => None,
            })
            .collect()
    }

    fn assert_proven(source: &str) {
        let findings = a056_findings(source);
        assert!(
            findings.is_empty(),
            "expected every access proven, got {findings:?}"
        );
    }

    #[test]
    fn a056_unsigned_index_under_upper_guard_is_proven() {
        assert_proven(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if i < 4 { r = a[i]; } else { r = 0; } return r; }",
        );
    }

    /// A signed index can be negative, which the upper bound alone does not rule
    /// out; the runtime guard's unsigned compare would trap on it.
    #[test]
    fn a056_signed_index_under_upper_guard_alone_is_unproven() {
        let findings = a056_findings(
            "pub fn f(i: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if i < 4 { r = a[i]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), I32_MIN, 3)]);
    }

    #[test]
    fn a056_signed_index_under_both_bounds_is_proven() {
        assert_proven(
            "pub fn f(i: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if i >= 0 && i < 4 { r = a[i]; } return r; }",
        );
    }

    #[test]
    fn a056_negated_and_parenthesized_guard_is_proven() {
        assert_proven(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if !((i >= 4)) { r = a[i]; } return r; }",
        );
    }

    #[test]
    fn a056_early_return_proves_the_rest_of_the_body() {
        assert_proven(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             if i >= 4 { return 0; } return a[i]; }",
        );
    }

    /// The failing branch of a guard is where the index is out of range, so an
    /// access there is always out of bounds.
    #[test]
    fn a056_access_in_the_failing_branch_is_rejected() {
        let findings = a056_findings(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if i < 4 { r = 0; } else { r = a[i]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), 4, U32_MAX)]);
    }

    #[test]
    fn a056_unguarded_parameter_index_is_rejected() {
        let findings = a056_findings(
            "pub fn f(i: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; return a[i]; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), I32_MIN, I32_MAX)]);
    }

    /// The write half: an unguarded store is reported like a load.
    #[test]
    fn a056_unguarded_store_is_rejected() {
        let findings =
            a056_findings("pub fn f(i: u32) { let mut a: [i32; 4] = [1, 2, 3, 4]; a[i] = 7; }");
        assert_eq!(findings, vec![unproven(Some("i"), 0, U32_MAX)]);
    }

    /// The canonical counting loop: `i` starts at 0, only grows, and the loop
    /// condition caps it — no `i >= 0` needs to be written for a signed counter.
    #[test]
    fn a056_counting_loop_is_proven() {
        assert_proven(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut s: i32 = 0; \
             let mut i: i32 = 0; loop i < 4 { s = s + a[i]; i = i + 1; } return s; }",
        );
    }

    #[test]
    fn a056_loop_exited_by_a_guarded_break_is_proven() {
        assert_proven(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut s: i32 = 0; \
             let mut i: i32 = 0; loop { if i >= 4 { break; } s = s + a[i]; i = i + 1; } \
             return s; }",
        );
    }

    /// Advancing the counter before the access moves it past the bound the
    /// condition established.
    #[test]
    fn a056_counter_advanced_before_the_access_is_rejected() {
        let findings = a056_findings(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut s: i32 = 0; \
             let mut i: i32 = 0; loop i < 4 { i = i + 1; s = s + a[i]; } return s; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), 1, 4)]);
    }

    #[test]
    fn a056_reassignment_between_guard_and_use_is_rejected() {
        let findings = a056_findings(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut j: u32 = i; \
             let mut r: i32 = 0; if j < 4 { j = j + 1; r = a[j]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(Some("j"), 1, 4)]);
    }

    /// `&&` evaluates its right operand only where the left one held.
    #[test]
    fn a056_short_circuit_and_guards_its_right_operand() {
        assert_proven(
            "pub fn f(i: u32) -> bool { let a: [i32; 4] = [1, 2, 3, 4]; \
             return i < 4 && a[i] > 0; }",
        );
    }

    #[test]
    fn a056_access_before_its_guard_in_a_conjunction_is_rejected() {
        let findings = a056_findings(
            "pub fn f(i: u32) -> bool { let a: [i32; 4] = [1, 2, 3, 4]; \
             return a[i] > 0 && i < 4; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), 0, U32_MAX)]);
    }

    /// `||` evaluates its right operand only where the left one failed.
    #[test]
    fn a056_short_circuit_or_guards_its_right_operand() {
        assert_proven(
            "pub fn f(i: u32) -> bool { let a: [i32; 4] = [1, 2, 3, 4]; \
             return i >= 4 || a[i] > 0; }",
        );
    }

    /// An `assert` establishes its condition only by halting when it is false,
    /// which is not a handled path.
    #[test]
    fn a056_assert_is_not_a_guard() {
        let findings = a056_findings(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; assert(i < 4); \
             return a[i]; }",
        );
        assert_eq!(findings, vec![unproven(Some("i"), 0, U32_MAX)]);
    }

    #[test]
    fn a056_in_range_const_index_is_proven() {
        assert_proven(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; const K: i32 = 3; \
             return a[K]; }",
        );
    }

    /// A named constant out of range is caught here; A037 sees only a literal
    /// written directly under the access.
    #[test]
    fn a056_out_of_range_const_index_is_rejected() {
        let findings = a056_findings(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; const K: i32 = 4; \
             return a[K]; }",
        );
        assert_eq!(findings, vec![unproven(Some("K"), 4, 4)]);
    }

    #[test]
    fn a056_parenthesized_literal_index_out_of_range_is_rejected() {
        let findings =
            a056_findings("pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; return a[(4)]; }");
        assert_eq!(findings, vec![unproven(None, 4, 4)]);
    }

    /// A bare literal index is A037's, which already reports it.
    #[test]
    fn a056_bare_literal_index_is_left_to_a037() {
        assert_proven("pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; return a[4]; }");
    }

    /// Only a plain local is narrowed by a condition; a field read twice may not
    /// read the same value, so the guard proves nothing about the access.
    #[test]
    fn a056_guarded_field_index_is_rejected() {
        let findings = a056_findings(
            "struct S { i: u32; } pub fn f(s: S) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             let mut r: i32 = 0; if s.i < 4 { r = a[s.i]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(None, 0, U32_MAX)]);
    }

    #[test]
    fn a056_field_bound_to_a_local_then_guarded_is_proven() {
        assert_proven(
            "struct S { i: u32; } pub fn f(s: S) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             let k: u32 = s.i; let mut r: i32 = 0; if k < 4 { r = a[k]; } return r; }",
        );
    }

    #[test]
    fn a056_guard_on_an_offset_index_is_proven() {
        assert_proven(
            "pub fn f(j: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = 0; \
             if j + 1 < 4 { r = a[j + 1]; } return r; }",
        );
    }

    #[test]
    fn a056_remainder_and_mask_indices_are_proven() {
        assert_proven(
            "pub fn f(i: u32, k: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             return a[i % 4] + a[k & 3]; }",
        );
    }

    /// A wrapping subtraction that can leave the type can wrap to any value.
    #[test]
    fn a056_wrapping_arithmetic_that_can_wrap_is_rejected() {
        let findings = a056_findings(
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = 0; \
             if i < 4 { r = a[wrapping(i - 1)]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(None, 0, U32_MAX)]);
    }

    #[test]
    fn a056_index_type_that_cannot_leave_the_array_is_proven() {
        let elements = vec!["0"; 256].join(", ");
        assert_proven(&format!(
            "pub fn f(k: u8) -> i32 {{ let a: [i32; 256] = [{elements}]; return a[k]; }}"
        ));
    }

    #[test]
    fn a056_each_dimension_of_a_nested_access_is_checked() {
        let findings = a056_findings(
            "pub fn f(i: u32, j: u32) -> i32 { let m: [[i32; 2]; 3] = [[1, 2], [3, 4], [5, 6]]; \
             let mut r: i32 = 0; if i < 3 { r = m[i][j]; } return r; }",
        );
        assert_eq!(findings, vec![unproven(Some("j"), 0, U32_MAX)]);
    }

    #[test]
    fn a056_binary_search_is_proven() {
        assert_proven(
            "pub fn f(t: i32) -> i32 { let a: [i32; 8] = [2, 5, 8, 12, 16, 23, 38, 56]; \
             let mut r: i32 = 8; let mut lo: i32 = 0; let mut hi: i32 = 7; \
             loop lo <= hi { let mid: i32 = (lo + hi) / 2; let v: i32 = a[mid]; \
             if v == t { r = mid; break; } if v < t { lo = mid + 1; } else { hi = mid - 1; } } \
             return r; }",
        );
    }

    #[test]
    fn a056_nested_counting_loops_are_proven() {
        assert_proven(
            "pub fn f() -> i32 { let mut a: [i32; 6] = [5, 3, 8, 1, 9, 2]; let mut i: i32 = 0; \
             loop i < 6 { let mut j: i32 = 0; loop j < 5 { let k: i32 = j + 1; \
             if a[j] > a[k] { let t: i32 = a[j]; a[j] = a[k]; a[k] = t; } j = j + 1; } \
             i = i + 1; } return a[0]; }",
        );
    }

    #[test]
    fn a056_method_guarding_its_index_is_proven() {
        assert_proven(
            "struct T { a: [i32; 4]; fn get(self, i: u32) -> i32 { let mut r: i32 = 0; \
             if i < 4 { r = self.a[i]; } return r; } }",
        );
    }

    /// A `forall` body is never lowered, so there is no runtime access to prove.
    #[test]
    fn a056_forall_specification_body_is_not_examined() {
        assert_proven(
            "spec S { fn p() forall { let a: [i32; 4] = @; let i: i32 = @; \
             assert(a[i] == a[i]); } }",
        );
    }

    /// No run reaches a loop behind `if false`, so nothing in it is reported.
    #[test]
    fn a056_access_in_an_unreachable_loop_is_not_reported() {
        assert_proven(
            "pub fn f(i: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = 0; \
             if false { loop i < 100 { r = a[i]; } } return r; }",
        );
    }

    /// `depth` counting loops nested in one another, each over a counter of type
    /// `ty`, with the innermost counter indexing a `[i32; 4]`.
    fn nested_counting_loops(depth: usize, ty: &str) -> String {
        let mut source = String::from(
            "pub fn f() -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut s: i32 = 0; ",
        );
        for d in 0..depth {
            source.push_str(&format!("let mut c{d}: {ty} = 0; loop c{d} < 4 {{ "));
        }
        source.push_str(&format!("s = wrapping(s + a[c{}]); ", depth - 1));
        for d in (0..depth).rev() {
            source.push_str(&format!("c{d} = c{d} + 1; }} "));
        }
        source.push_str("return s; }");
        source
    }

    /// Re-analyzing a nested loop for every pass of the one around it multiplies
    /// with the depth, so past a statement budget each remaining loop is
    /// analyzed in one pass that forgets every local its body writes. The
    /// fallback keeps what a loop condition establishes: an unsigned counter
    /// is still proven by `c < 4` alone, even this deep.
    #[test]
    fn a056_budget_fallback_keeps_the_loop_condition() {
        assert_proven(&nested_counting_loops(16, "u32"));
    }

    /// What the fallback forgets is the lower bound a signed counter's
    /// initializer established, so the access is reported — never accepted on
    /// a range narrower than the counter can take.
    #[test]
    fn a056_budget_fallback_reports_conservatively() {
        assert_eq!(
            a056_findings(&nested_counting_loops(16, "i32")),
            vec![unproven(Some("c15"), I32_MIN, 3)]
        );
    }

    /// The source text of every access a passing analysis hands to code
    /// generation as proven in bounds, sorted.
    fn proven_in_bounds(source: &str) -> Vec<String> {
        let arena = build_ast(source.to_string());
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context();
        let result = inference_analysis::analyze(&ctx)
            .unwrap_or_else(|errors| panic!("analysis should pass, got:\n{errors}"));
        let arena = ctx.arena();
        let mut accesses: Vec<String> = result
            .proven_in_bounds()
            .accesses()
            .iter()
            .map(|&access| {
                arena
                    .get_node_source(NodeId::Expr(access))
                    .expect("a proven access has source text")
                    .to_string()
            })
            .collect();
        accesses.sort();
        accesses
    }

    /// Every access A056 proves is handed on, and a literal index — which
    /// code generation folds and never guards — is not one A056 covers.
    #[test]
    fn a056_hands_every_proven_access_to_codegen() {
        assert_eq!(
            proven_in_bounds(
                "pub fn f(i: u32, j: i32) -> i32 { let mut a: [i32; 4] = [1, 2, 3, 4]; \
                 let mut r: i32 = a[0]; if i < 4 { r = r + a[i]; } \
                 if j >= 0 && j < 4 { a[j] = r; } return r; }",
            ),
            vec!["a[i]", "a[j]"]
        );
    }

    /// A named or computed constant takes the guarded path in code generation,
    /// and the analysis folds it, so its guard is omitted as well.
    #[test]
    fn a056_hands_a_folded_constant_index_to_codegen() {
        assert_eq!(
            proven_in_bounds(
                "pub fn f() -> i32 { const K: i32 = 2; let a: [i32; 4] = [1, 2, 3, 4]; \
                 return a[K] + a[1 + 1]; }",
            ),
            vec!["a[1 + 1]", "a[K]"]
        );
    }

    /// Each level of a nested access is its own access, with its own length.
    #[test]
    fn a056_hands_both_levels_of_a_nested_access_to_codegen() {
        assert_eq!(
            proven_in_bounds(
                "pub fn f(i: u32, j: u32) -> i32 { let g: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]]; \
                 let mut r: i32 = 0; if i < 2 && j < 3 { r = g[i][j]; } return r; }",
            ),
            vec!["g[i]", "g[i][j]"]
        );
    }

    /// An access no run reaches passes A056 without an interval to stand on.
    /// It is not handed on: its guard costs nothing at run time, and keeping
    /// it means no elided guard rests on a contradiction alone.
    #[test]
    fn a056_does_not_hand_an_unreached_access_to_codegen() {
        assert_eq!(
            proven_in_bounds(
                "pub fn f(i: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = 0; \
                 if i < 0 && i > 3 { r = a[i]; } if i >= 0 && i < 4 { r = a[i]; } return r; }",
            )
            .len(),
            1
        );
    }
}
