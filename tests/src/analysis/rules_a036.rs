/// Integration tests for analysis rule A036.
///
/// - A036: StackDepthExceeded — the cumulative shadow-stack usage along a
///   root-to-leaf call chain must not exceed the stack budget
///   (`STACK_BUDGET_BYTES = 65_536`). Only array/struct frames consume the
///   shadow stack; scalars live in WASM locals. Because A035 forbids recursion
///   the call graph is a DAG, so A036 reports the maximum-weight call chain.
///
/// These tests are the cross-crate guard for two properties that the rule's
/// in-crate unit tests cannot exercise: (1) the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, and (2) the
/// rule closes a gap that codegen alone leaves open — codegen bounds each
/// *individual* frame but not the *cumulative* depth across a chain.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, codegen_output_no_analysis};
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

    /// Returns true if any analysis error is a `StackDepthExceeded` (A036).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules.
    fn has_stack_depth_exceeded(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StackDepthExceeded { .. })),
        }
    }

    fn stack_depth_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::StackDepthExceeded { .. }))
            .expect("expected a StackDepthExceeded diagnostic")
            .clone()
    }

    /// A chain `a -> b -> c` where each frame (~24 KB for `[i32; 6000]`) is well
    /// under the 64 KB budget but their sum (~72 KB) exceeds it. The arrays are
    /// initialized via uzumaki inside a `forall` block so they count as compound
    /// locals without needing a 6000-element literal (which would also trip A025
    /// if left uninitialized).
    #[test]
    fn a036_over_budget_chain_rejected() {
        let source = r#"
            fn a() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return b();
            }
            fn b() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return c();
            }
            fn c() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return 0;
            }
        "#;
        assert!(
            has_stack_depth_exceeded(source),
            "expected StackDepthExceeded for the cumulative chain a -> b -> c"
        );
    }

    #[test]
    fn a036_over_budget_chain_names_the_chain() {
        let source = r#"
            fn a() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return b();
            }
            fn b() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return c();
            }
            fn c() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return 0;
            }
        "#;
        let diag = stack_depth_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("a -> b -> c"),
            "diagnostic should name the chain `a -> b -> c`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// Same call shape as the over-budget chain but with tiny arrays
    /// (`[i32; 16]` ~ 64 bytes/frame), so the cumulative depth stays far below
    /// the budget and A036 must not fire.
    #[test]
    fn a036_under_budget_chain_accepted() {
        let source = r#"
            fn a() -> i32 {
                forall {
                    let arr: [i32; 16] = @;
                    let x: i32 = arr[0];
                }
                return b();
            }
            fn b() -> i32 {
                forall {
                    let arr: [i32; 16] = @;
                    let x: i32 = arr[0];
                }
                return c();
            }
            fn c() -> i32 {
                forall {
                    let arr: [i32; 16] = @;
                    let x: i32 = arr[0];
                }
                return 0;
            }
        "#;
        assert!(
            !has_stack_depth_exceeded(source),
            "small-array call chain must not trip A036"
        );
    }

    /// A single function whose lone array local (`[i32; 20000]` ~ 80 KB) exceeds
    /// the budget on its own. The point is that A036 catches this at analysis
    /// time rather than letting codegen emit a frame that traps at runtime.
    #[test]
    fn a036_single_oversized_frame_rejected() {
        let source = r#"
            fn big() -> i32 {
                forall {
                    let arr: [i32; 20000] = @;
                    let x: i32 = arr[0];
                }
                return 0;
            }
        "#;
        assert!(
            has_stack_depth_exceeded(source),
            "single oversized frame must trip A036 at analysis time"
        );
    }

    /// A recursive (cyclic) program with array frames. A036's traversal must be
    /// cycle-safe: it must terminate (no hang) and not panic, while A035 owns the
    /// recursion diagnostic. Arrays are kept small — the point is cycle safety,
    /// not budget overflow.
    #[test]
    fn a036_recursive_program_is_cycle_safe_and_a035_owns_recursion() {
        let source = r#"
            fn a() -> i32 {
                forall {
                    let arr: [i32; 8] = @;
                    let x: i32 = arr[0];
                }
                return b();
            }
            fn b() -> i32 {
                forall {
                    let arr: [i32; 8] = @;
                    let x: i32 = arr[0];
                }
                return a();
            }
        "#;
        let result = analyze(source);
        let errors = result.expect_err("expected analysis errors for the recursive program");
        let has_recursion = errors
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. }));
        assert!(
            has_recursion,
            "recursion must be reported by A035 (RecursionDetected)"
        );
    }

    /// Behavioral parity test — the gap A036 closes.
    ///
    /// Each individual frame here (~24 KB for `[i32; 6000]`) is under the 64 KB
    /// budget, so codegen's per-frame bound is satisfied and `codegen` (run with
    /// analysis SKIPPED) succeeds — proving codegen alone does NOT catch the
    /// cumulative overflow. Running the analysis pass on the same source yields
    /// an A036 error, proving the rule closes that gap.
    ///
    /// Exact numeric per-function parity against codegen's private
    /// `FrameLayout.total_size` is validated by-construction rather than
    /// asserted here: that field is not exposed across crates, and the
    /// estimator's documented soundness argument (it charges worst-case padding,
    /// `size + 7` per slot rounded up to 16, so its per-function estimate is
    /// always >= codegen's real layout) is what guarantees A036 never
    /// under-approximates. This cross-crate behavioral test is the guard that the
    /// over-approximation is still tight enough to flag a real cumulative
    /// overflow that codegen would have emitted as a runtime trap.
    #[test]
    fn a036_closes_gap_codegen_leaves_open() {
        let source = r#"
            fn a() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return b();
            }
            fn b() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return c();
            }
            fn c() -> i32 {
                forall {
                    let arr: [i32; 6000] = @;
                    let x: i32 = arr[0];
                }
                return 0;
            }
        "#;
        let _output = codegen_output_no_analysis(source);

        assert!(
            has_stack_depth_exceeded(source),
            "A036 must reject the cumulative overflow that codegen alone accepts"
        );
    }

    // --- Part A: compound-type frame coverage ---------------------------------
    //
    // The tests above exercise only flat `[i32; N]` arrays. A036's per-function
    // frame estimate must also be sound for structs, mixed-alignment fields,
    // nested structs, arrays of structs, and a mutable `self` slot — the shapes
    // where the estimate's padding/alignment model could diverge from codegen's
    // real `FrameLayout`. The fixtures below drive each shape over (or, where a
    // property is the point, deliberately under) the 64 KB budget. Sizes are
    // chosen with comfortable margin from the budget; the exact estimate/real
    // numbers are pinned by the cross-crate parity test in Part B.

    /// A struct with two large scalar-array fields, one struct local per
    /// function, chained `p -> q`. Each frame (~56 KB) is under budget alone but
    /// their sum (~112 KB) exceeds it. Guards that a struct binding contributes a
    /// frame slot sized by the sum of its fields.
    #[test]
    fn a036_struct_local_over_budget_chain_rejected() {
        let source = r#"
            struct Blk { a: [i32; 7000]; b: [i32; 7000]; }
            fn p() -> i32 {
                forall { let x: Blk = @; }
                return q();
            }
            fn q() -> i32 {
                forall { let x: Blk = @; }
                return 0;
            }
        "#;
        assert!(
            has_stack_depth_exceeded(source),
            "struct-local chain p -> q must trip A036"
        );
    }

    /// A struct with interleaved small/large fields (`i8, i64, i8, i16` ahead of
    /// a large array). The per-field padding model must charge enough that the
    /// estimate stays >= codegen's aligned layout; a chain `p -> q` pushes the
    /// cumulative depth over budget. Guards the mixed-alignment field path.
    #[test]
    fn a036_mixed_alignment_struct_over_budget_chain_rejected() {
        let source = r#"
            struct M { a: i8; b: i64; c: i8; d: i16; pad: [i32; 9000]; }
            fn p() -> i32 {
                forall { let x: M = @; }
                return q();
            }
            fn q() -> i32 {
                forall { let x: M = @; }
                return 0;
            }
        "#;
        assert!(
            has_stack_depth_exceeded(source),
            "mixed-alignment struct chain p -> q must trip A036"
        );
    }

    /// One level of struct nesting (A026's limit): `Outer { inner: Inner; .. }`
    /// where `Inner` holds a large scalar array. `Inner` is built via uzumaki (a
    /// scalar-array struct, allowed by A027), then wrapped in an `Outer` literal
    /// in the same block; both the `Inner` and `Outer` bindings get frame slots.
    /// The chain `p -> q` exceeds budget. Guards the nested-struct size walk.
    #[test]
    fn a036_nested_struct_over_budget_chain_rejected() {
        let source = r#"
            struct Inner { big: [i32; 4500]; }
            struct Outer { inner: Inner; tag: i32; }
            fn p() -> i32 {
                forall {
                    let i: Inner = @;
                    let o: Outer = Outer { inner: i, tag: 1 };
                    let t: i32 = o.tag;
                }
                return q();
            }
            fn q() -> i32 {
                forall {
                    let i: Inner = @;
                    let o: Outer = Outer { inner: i, tag: 1 };
                    let t: i32 = o.tag;
                }
                return 0;
            }
        "#;
        assert!(
            has_stack_depth_exceeded(source),
            "nested-struct chain p -> q must trip A036"
        );
    }

    /// Array-of-structs frame slot. The over-budget direction is unreachable
    /// through a valid program: a `[S; N]` local can only be initialized by a
    /// struct-element array *literal* (uzumaki on an array of structs is rejected
    /// by A028, and codegen's `element_size` panics on a struct element via the
    /// uzumaki fill path), and a literal large enough to cross 64 KB is not
    /// practical. This test therefore covers the *sizing* of an array-of-structs
    /// slot with a small literal and asserts A036 stays silent under budget; the
    /// estimate >= real soundness for this shape is pinned by Part B's corpus.
    #[test]
    fn a036_array_of_structs_local_under_budget_accepted() {
        let source = r#"
            struct Pt { x: i32; y: i32; }
            fn aos() -> i32 {
                let a: [Pt; 3] = [Pt { x: 1, y: 2 }, Pt { x: 3, y: 4 }, Pt { x: 5, y: 6 }];
                return a[0].x;
            }
        "#;
        assert!(
            !has_stack_depth_exceeded(source),
            "small array-of-structs literal must not trip A036"
        );
    }

    /// Builds a `[Pt; n]` array literal of `Pt { x: i, y: i }` elements as a
    /// single source line. Array-of-structs locals have no repeat syntax and
    /// uzumaki on an array of structs is rejected by A028, so a large
    /// array-of-structs frame must be written as an explicit literal of `n`
    /// struct literals (mirroring how `stack_overflow_traps_at_runtime` in
    /// `codegen::wasm::base` generates large literals via `format!`).
    fn array_of_pt_literal(n: usize) -> String {
        let elems = (0..n)
            .map(|i| format!("Pt {{ x: {i}, y: {i} }}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{elems}]")
    }

    /// Genuine over-budget array-of-structs call chain. Each function holds a
    /// `[Pt; 5000]` literal local: `Pt { x: i32; y: i32; }` is 8 bytes, so the
    /// real frame is `8 * 5000 = 40000` bytes (< 64 KB, so codegen accepts each
    /// frame), and the estimate is `align_to(40000 + 7, 16) = 40016`. `p` calls
    /// `q`, so the chain estimate `40016 + 40016 = 80032` exceeds the 64 KB
    /// budget and A036 must fire and name `p -> q`. This is the AoS analogue of
    /// `a036_struct_local_over_budget_chain_rejected`.
    #[test]
    fn a036_array_of_structs_over_budget_chain_rejected() {
        let lit = array_of_pt_literal(5000);
        let source = format!(
            r#"
            struct Pt {{ x: i32; y: i32; }}
            fn p() -> i32 {{
                let a: [Pt; 5000] = {lit};
                return q() + a[0].x;
            }}
            fn q() -> i32 {{
                let b: [Pt; 5000] = {lit};
                return b[0].y;
            }}
            "#
        );
        let diag = stack_depth_diag(&source);
        let msg = diag.to_string();
        assert!(
            has_stack_depth_exceeded(&source),
            "array-of-structs chain p -> q (~40 KB each) must trip A036"
        );
        assert_eq!(diag.rule_id(), "A036");
        assert!(
            msg.contains("p -> q"),
            "diagnostic should name the chain `p -> q`, got: {msg}"
        );
    }

    /// Regression guard — before the estimator fix, A036 over-approximated
    /// array-of-structs ~3x and would have falsely rejected this valid ~24 KB
    /// frame. A single function holds a `[Pt; 3000]` literal local:
    /// `Pt { x: i32; y: i32; }` is 8 bytes, so the real frame is
    /// `8 * 3000 = 24000` bytes and the (fixed) estimate is
    /// `align_to(24000 + 7, 16) = 24016` — both comfortably under the 64 KB
    /// budget, and the function is not chained, so A036 must stay silent. The
    /// old estimator inflated each element by `+7` per field before multiplying
    /// by the length (`(4 + 7) + (4 + 7) = 22` bytes/element, `22 * 3000 =
    /// 66000`, or ~3x the real 24000 = ~72000), either of which crosses 65536
    /// and would have falsely fired here.
    #[test]
    fn a036_large_array_of_structs_that_fits_is_accepted() {
        let lit = array_of_pt_literal(3000);
        let source = format!(
            r#"
            struct Pt {{ x: i32; y: i32; }}
            fn fits() -> i32 {{
                let a: [Pt; 3000] = {lit};
                return a[1].x + a[2999].y;
            }}
            "#
        );
        assert!(
            !has_stack_depth_exceeded(&source),
            "a valid ~24 KB array-of-structs frame must not trip A036 (regression guard)"
        );
    }

    /// A `mut self` method allocates a frame slot for the (by-value) `self` copy.
    /// `caller` (a large array frame) calls `C.bump` (a large array frame); the
    /// chain `caller -> C.bump` exceeds budget. This is the only path that
    /// exercises the mutable-self frame slot, and the diagnostic must name the
    /// method by its canonical key `C.bump`.
    #[test]
    fn a036_mutable_self_method_over_budget_chain_rejected() {
        let source = r#"
            struct C { a: i32;
                fn bump(mut self) -> i32 {
                    forall { let b: [i32; 10000] = @; let t: i32 = b[0]; }
                    return 0;
                }
            }
            fn caller() -> i32 {
                forall { let d: [i32; 10000] = @; let t: i32 = d[0]; }
                let c: C = C { a: 1 };
                return c.bump();
            }
        "#;
        let diag = stack_depth_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("caller -> C.bump"),
            "diagnostic should name the chain `caller -> C.bump`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// `if`/`else` branches must be *maxed*, not *summed*. Each branch declares a
    /// `[i32; 9000]` array (~36 KB). Summing the two branches (~72 KB) would
    /// exceed the 64 KB budget; taking the per-branch maximum (~36 KB) stays
    /// under it. The function has no callees, so its whole frame is the branch
    /// contribution. A036 must therefore stay silent — proving the estimator
    /// (and codegen) reuse the offset across arms rather than summing them.
    #[test]
    fn a036_if_else_branches_are_maxed_not_summed_accepted() {
        let source = r#"
            fn branchy(n: i32) -> i32 {
                if n > 0 {
                    forall { let x: [i32; 9000] = @; let tx: i32 = x[0]; }
                } else {
                    forall { let y: [i32; 9000] = @; let ty: i32 = y[0]; }
                }
                return 0;
            }
        "#;
        assert!(
            !has_stack_depth_exceeded(source),
            "if/else branches must be maxed not summed: max(branch) is under budget"
        );
    }

    // --- Part B: cross-crate frame-size parity --------------------------------

    /// Enforced cross-crate soundness guard for A036.
    ///
    /// A036 may *over*-approximate codegen's real per-function frame (it charges
    /// worst-case 7-byte padding per slot and rounds up to 16), but it must never
    /// *under*-approximate: if the analysis estimate were below codegen's real
    /// `FrameLayout.total_size` for any function, A036 could accept a program
    /// that codegen lays out larger and overflows the shadow stack at runtime.
    ///
    /// This test replaces the prior "sound by construction" prose claim with a
    /// machine-checked invariant. For a corpus spanning every frame-bearing
    /// shape — flat arrays, a plain struct, a mixed-alignment struct, a nested
    /// struct, an array of structs (literal init), and a `mut self` method — it
    /// asserts `estimate[key] >= real[key]` for every function codegen emits a
    /// frame for. The corpus is built with analysis skipped so over-budget
    /// sources still produce a `CodegenOutput` whose real frame sizes are
    /// readable. A non-vacuity check asserts the corpus exercises at least one
    /// non-zero real frame.
    ///
    /// If this test ever fails with `est < real`, that is a real A036 soundness
    /// bug — fix the estimator, do not weaken the assertion.
    #[test]
    fn a036_estimate_is_sound_upper_bound_of_codegen_frame() {
        let corpus: &[&str] = &[
            // Flat scalar arrays across a chain.
            r#"
                fn a() -> i32 { forall { let arr: [i32; 6000] = @; let x: i32 = arr[0]; } return b(); }
                fn b() -> i32 { forall { let arr: [i32; 6000] = @; let x: i32 = arr[0]; } return 0; }
            "#,
            // Mixed scalar widths in one array-bearing struct.
            r#"
                struct Blk { a: [i32; 1000]; b: [i64; 500]; }
                fn s() -> i32 { forall { let x: Blk = @; } return 0; }
            "#,
            // Mixed-alignment struct (small fields ahead of a large array).
            r#"
                struct M { a: i8; b: i64; c: i8; d: i16; pad: [i32; 2000]; }
                fn s() -> i32 { forall { let x: M = @; } return 0; }
            "#,
            // One level of struct nesting.
            r#"
                struct Inner { big: [i32; 1500]; }
                struct Outer { inner: Inner; tag: i32; }
                fn s() -> i32 {
                    forall {
                        let i: Inner = @;
                        let o: Outer = Outer { inner: i, tag: 1 };
                        let t: i32 = o.tag;
                    }
                    return 0;
                }
            "#,
            // Array of structs via literal init.
            r#"
                struct Pt { x: i32; y: i32; }
                fn s() -> i32 {
                    let a: [Pt; 4] = [Pt { x: 1, y: 2 }, Pt { x: 3, y: 4 }, Pt { x: 5, y: 6 }, Pt { x: 7, y: 8 }];
                    return a[2].y;
                }
            "#,
            // Mutable-self method plus its caller.
            r#"
                struct C { a: i32;
                    fn bump(mut self) -> i32 {
                        forall { let b: [i32; 2000] = @; let t: i32 = b[0]; }
                        return 0;
                    }
                }
                fn caller() -> i32 {
                    forall { let d: [i32; 2000] = @; let t: i32 = d[0]; }
                    let c: C = C { a: 1 };
                    return c.bump();
                }
            "#,
            // if/else branches (codegen and estimate both max the arms).
            r#"
                fn branchy(n: i32) -> i32 {
                    if n > 0 {
                        forall { let x: [i32; 1000] = @; let tx: i32 = x[0]; }
                    } else {
                        forall { let y: [i32; 1200] = @; let ty: i32 = y[0]; }
                    }
                    return 0;
                }
            "#,
        ];

        // A non-trivial array-of-structs frame (`[Pt; 3000]` ~ 24 KB, built as
        // an explicit literal since AoS has no repeat syntax and uzumaki on it
        // is rejected by A028), so the estimate >= real invariant is exercised
        // on a meaningful AoS frame rather than only the tiny 3/4-element
        // literals above.
        let aos_lit = array_of_pt_literal(3000);
        let aos_src = format!(
            r#"
                struct Pt {{ x: i32; y: i32; }}
                fn aos() -> i32 {{
                    let a: [Pt; 3000] = {aos_lit};
                    return a[2999].x;
                }}
            "#
        );
        let sources: Vec<&str> = corpus
            .iter()
            .copied()
            .chain(std::iter::once(aos_src.as_str()))
            .collect();

        let mut saw_nonzero_real_frame = false;
        for src in sources {
            let ctx = type_check(src);
            let estimate = inference_analysis::estimate_frame_sizes(&ctx);
            let output = codegen_output_no_analysis(src);
            let real = output.frame_sizes();
            for (key, &real_bytes) in real {
                if real_bytes > 0 {
                    saw_nonzero_real_frame = true;
                }
                let est = estimate.get(key).copied().unwrap_or(0);
                assert!(
                    est >= real_bytes,
                    "A036 estimate {est} < codegen real frame {real_bytes} for fn `{key}` in source:\n{src}"
                );
            }
        }
        assert!(
            saw_nonzero_real_frame,
            "parity corpus must exercise at least one non-zero real frame (non-vacuity)"
        );
    }
}
