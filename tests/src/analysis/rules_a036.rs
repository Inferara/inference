/// Integration tests for analysis rule A036.
///
/// - A036: StackDepthExceeded — the cumulative shadow-stack usage along a
///   root-to-leaf call chain must not exceed the stack budget carried by
///   `AnalysisOptions::stack_budget_bytes` (65_536 by default). Only
///   array/struct frames consume the shadow stack; scalars live in WASM locals.
///   Because A035 forbids recursion the call graph is a DAG, so A036 reports the
///   maximum-weight call chain.
///
/// These tests are the cross-crate guard for four properties that the rule's
/// in-crate unit tests cannot exercise: (1) the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, (2) the
/// rule closes a gap that codegen alone leaves open — codegen bounds each
/// *individual* frame but not the *cumulative* depth across a chain, (3) the
/// budget is a setting the verdict follows in both directions rather than a
/// constant of the rule's own, and (4) the default budget is the stack codegen
/// actually emits — see [`budget_matches_emitted_stack`] at the end of this
/// file.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{
        build_ast, codegen_output_multi_file_no_analysis, codegen_output_no_analysis,
        try_type_check_multi_file,
    };
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

    /// The budget A036 measures against is the one it is given, not a constant
    /// of its own. A single ~40 KB frame fits the default 64 KB shadow stack and
    /// does not fit a 32 KB one, so the same program must be accepted under one
    /// budget and rejected under the other. A rule that ignored its options
    /// could not satisfy both halves at once, and the reported `budget_bytes`
    /// pins that the configured number reached the diagnostic rather than only
    /// the comparison.
    #[test]
    fn the_verdict_follows_the_configured_budget() {
        const SOURCE: &str = r#"
            spec S {
                fn heavy() -> i32 {
                    forall {
                        let arr: [i64; 5000] = @;
                        let x: i64 = arr[0];
                    }
                    return 0;
                }
            }
        "#;
        let ctx = type_check(SOURCE);

        let under = inference_analysis::analyze_with_options(
            &ctx,
            inference_analysis::AnalysisOptions::default(),
        );
        assert!(
            stack_depth_errors(&under).next().is_none(),
            "a 40 KB frame fits the default 64 KB shadow stack"
        );

        let over = inference_analysis::analyze_with_options(
            &ctx,
            inference_analysis::AnalysisOptions {
                stack_budget_bytes: 32_768,
            },
        );
        let reported = stack_depth_errors(&over)
            .next()
            .expect("a 40 KB frame must not fit a 32 KB shadow stack");
        assert_eq!(
            reported, 32_768,
            "the diagnostic must report the configured budget"
        );
    }

    /// The other direction of the same claim: a budget can also *clear* a
    /// finding, not only produce one.
    ///
    /// A chain of three ~24 KB frames overflows the default 64 KB shadow stack
    /// and fits a 128 KB one — the stack a two-page all-stack layout emits, so
    /// the larger budget is one a real build can actually be configured with.
    /// Without this half, a rule that ignored its options and always reported
    /// the default would still satisfy the firing direction, because firing more
    /// readily than configured is indistinguishable from a hard-coded budget
    /// that happens to be smaller.
    #[test]
    fn a_larger_budget_clears_a_chain_the_default_rejects() {
        const SOURCE: &str = r#"
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
        let ctx = type_check(SOURCE);

        let under = inference_analysis::analyze_with_options(
            &ctx,
            inference_analysis::AnalysisOptions::default(),
        );
        assert_eq!(
            stack_depth_errors(&under).next(),
            Some(65_536),
            "a ~72 KB chain must not fit the default 64 KB shadow stack"
        );

        let over = inference_analysis::analyze_with_options(
            &ctx,
            inference_analysis::AnalysisOptions {
                stack_budget_bytes: 131_072,
            },
        );
        assert!(
            stack_depth_errors(&over).next().is_none(),
            "a ~72 KB chain fits a 128 KB shadow stack"
        );
    }

    /// The `budget_bytes` of every A036 error in `result`.
    fn stack_depth_errors(
        result: &Result<AnalysisResult, AnalysisErrors>,
    ) -> impl Iterator<Item = u32> + '_ {
        result
            .as_ref()
            .err()
            .into_iter()
            .flat_map(|errors| errors.errors().iter())
            .filter_map(|e| match e {
                AnalysisDiagnostic::StackDepthExceeded { budget_bytes, .. } => Some(*budget_bytes),
                _ => None,
            })
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

    // Part A: compound-type frame coverage
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

    /// An `n`-element array literal of `i32` zeros, e.g. `[0, 0, 0]`.
    fn repeat_zeros(n: usize) -> String {
        let elems = std::iter::repeat_n("0", n).collect::<Vec<_>>().join(", ");
        format!("[{elems}]")
    }

    /// An `n`-element array literal whose first element reads `a[0]` (making the
    /// literal self-referential) and whose remaining elements are zeros.
    fn array_selfref_literal(n: usize) -> String {
        array_selfref_reading("a", n)
    }

    /// An `n`-element array literal whose first element reads `{var}[0]` (making
    /// the literal self-referential in `var`) and whose remaining elements are
    /// zeros. Lets a test place two self-referential reassignments of distinct
    /// destinations in one function.
    fn array_selfref_reading(var: &str, n: usize) -> String {
        let mut elems = vec![format!("{var}[0]")];
        elems.extend(std::iter::repeat_n("0".to_string(), n.saturating_sub(1)));
        format!("[{}]", elems.join(", "))
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

    // Part B: cross-crate frame-size parity

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

        // Self-referential compound reassignments: codegen reserves a scratch
        // frame region the size of the destination so the literal can be built
        // against the pre-assignment state and copied over. The estimator must
        // charge that scratch or `est < real` here. A large array-bearing struct
        // and a large array analogue exercise the invariant on real ~24/20 KB
        // scratch regions.
        let big_zeros = repeat_zeros(6000);
        let big_selfref_src = format!(
            r#"
                struct Big {{ data: [i32; 6000]; tag: i32; }}
                fn bigs() -> i32 {{
                    let mut b: Big = Big {{ data: {big_zeros}, tag: 0 }};
                    b = Big {{ data: b.data, tag: b.tag }};
                    return b.tag;
                }}
            "#
        );
        let arr_zeros = repeat_zeros(5000);
        let arr_selfref = array_selfref_literal(5000);
        let arr_selfref_src = format!(
            r#"
                fn arrs() -> i32 {{
                    let mut a: [i32; 5000] = {arr_zeros};
                    a = {arr_selfref};
                    return a[0];
                }}
            "#
        );

        let sources: Vec<&str> = corpus
            .iter()
            .copied()
            .chain(std::iter::once(aos_src.as_str()))
            .chain(std::iter::once(big_selfref_src.as_str()))
            .chain(std::iter::once(arr_selfref_src.as_str()))
            .collect();

        let mut saw_nonzero_real_frame = false;
        for src in sources {
            let ctx = type_check(src);
            let estimate = inference_analysis::estimate_frame_sizes(&ctx);
            let output = codegen_output_no_analysis(src);
            let real = output.frame_sizes();
            // Both maps are keyed by the structured `FnKey`, so two functions whose
            // keys render to the same `Display` string stay distinct here; the test
            // therefore compares each function's estimate against its own real frame.
            for (key, &real_bytes) in real {
                if real_bytes > 0 {
                    saw_nonzero_real_frame = true;
                }
                assert!(
                    estimate.contains_key(key),
                    "analysis estimate is missing a frame entry for fn `{key}` that codegen emitted in source:\n{src}"
                );
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

    /// A single function whose destination struct (~40 KB) fits under the 64 KB
    /// budget on its own, but the *self-referential* reassignment forces codegen
    /// to stage the literal in a scratch region of the same size before copying it
    /// to the destination. Charging that scratch pushes the estimate (~80 KB) over
    /// budget, so A036 must reject. Paired with
    /// `a036_non_self_ref_reassign_of_same_dest_accepted`, this proves the estimator
    /// counts the scratch — the only difference between the two is the self-reference.
    #[test]
    fn a036_self_ref_reassign_scratch_pushes_over_budget_rejected() {
        let zeros = repeat_zeros(10000);
        let source = format!(
            r#"
                struct Big {{ data: [i32; 10000]; }}
                fn s() -> i32 {{
                    let mut b: Big = Big {{ data: {zeros} }};
                    b = Big {{ data: b.data }};
                    return 0;
                }}
            "#
        );
        assert!(
            has_stack_depth_exceeded(&source),
            "self-referential reassign must charge scratch: dest (~40 KB) + scratch (~40 KB) \
             exceeds the 64 KB budget"
        );
    }

    /// The control for `a036_self_ref_reassign_scratch_pushes_over_budget_rejected`:
    /// the identical ~40 KB destination reassigned from a *non*-self-referential
    /// literal needs no scratch, so the estimate stays under budget and A036
    /// accepts. This isolates the self-reference as the cause of the rejection.
    #[test]
    fn a036_non_self_ref_reassign_of_same_dest_accepted() {
        let zeros = repeat_zeros(10000);
        let source = format!(
            r#"
                struct Big {{ data: [i32; 10000]; }}
                fn s() -> i32 {{
                    let mut b: Big = Big {{ data: {zeros} }};
                    b = Big {{ data: {zeros} }};
                    return 0;
                }}
            "#
        );
        assert!(
            !has_stack_depth_exceeded(&source),
            "a non-self-referential reassign reserves no scratch, so the ~40 KB frame fits"
        );
    }

    /// Two sequential self-referential reassignments of distinct large arrays in
    /// one function. Codegen reserves a single shared scratch region sized to the
    /// *larger* destination (~18 KB), reused for both build-then-copy sequences,
    /// so the real frame is the two destination slots plus that one region.
    ///
    /// The estimator must mirror that shared region with a single MAX charge, not
    /// a per-assignment SUM. With the two ~18 KB / ~17.6 KB destinations plus a
    /// single ~18 KB scratch charge the estimate (~53.6 KB) fits under the 64 KB
    /// budget, so A036 must accept. A sum-based scratch charge (two ~18 KB / ~17.6
    /// KB regions on top of the destinations, ~71 KB) would cross the budget and
    /// falsely reject this valid frame. Guards against re-introducing the
    /// sum-based scratch over-counting.
    #[test]
    fn a036_two_self_ref_reassigns_share_one_scratch_accepted() {
        let zeros_a = repeat_zeros(4500);
        let zeros_b = repeat_zeros(4400);
        let selfref_a = array_selfref_reading("a", 4500);
        let selfref_b = array_selfref_reading("b", 4400);
        let source = format!(
            r#"
                fn s() -> i32 {{
                    let mut a: [i32; 4500] = {zeros_a};
                    let mut b: [i32; 4400] = {zeros_b};
                    a = {selfref_a};
                    b = {selfref_b};
                    return a[0] + b[0];
                }}
            "#
        );
        assert!(
            !has_stack_depth_exceeded(&source),
            "two self-referential reassigns share one scratch region (max, not sum), \
             so the frame fits and A036 must accept"
        );
    }

    // Part C: cross-file frames
    //
    // A036's soundness assumes the call graph is whole-program: a cross-file
    // chain's cumulative depth must be summed, and an imported struct's frame
    // must be sized from its defining file (not looked up by bare name, which
    // misses the canonical-key index and would size the frame 0). These tests
    // guard both — a regression let cross-file frames under-count and overflow
    // the shadow stack at runtime with no diagnostic.

    /// Type-checks a multi-file program (entry first, empty module path) and runs
    /// the analysis pass.
    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    fn has_stack_depth_exceeded_multi(files: &[(Vec<&str>, &str)]) -> bool {
        match analyze_multi(files) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StackDepthExceeded { .. })),
        }
    }

    fn stack_depth_diag_multi(files: &[(Vec<&str>, &str)]) -> AnalysisDiagnostic {
        analyze_multi(files)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::StackDepthExceeded { .. }))
            .expect("expected a StackDepthExceeded diagnostic")
            .clone()
    }

    /// A cross-file chain `main -> lib::b::big` where each frame (~40 KB for an
    /// `[i64; 5000]` array) is under budget alone but their sum (~80 KB) exceeds
    /// it. The qualified call edge must be recorded for the cumulative depth to be
    /// summed, and the diagnostic must name the chain across files.
    #[test]
    fn a036_cross_file_over_budget_chain_rejected_and_names_chain() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b;
                    pub fn main() -> i32 {
                        forall {
                            let arr: [i64; 5000] = @;
                            let x: i64 = arr[0];
                        }
                        return lib::b::big();
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    pub fn big() -> i32 {
                        forall {
                            let arr: [i64; 5000] = @;
                            let x: i64 = arr[0];
                        }
                        return 0;
                    }
                "#,
            ),
        ];
        let diag = stack_depth_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("main -> lib.b.big"),
            "diagnostic should name the cross-file chain `main -> lib.b.big`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// The same cross-file call shape with tiny arrays stays far under budget, so
    /// A036 must not fire — the qualified edge is recorded but the cumulative
    /// depth is small.
    #[test]
    fn a036_cross_file_under_budget_chain_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b;
                    pub fn main() -> i32 {
                        forall {
                            let arr: [i32; 16] = @;
                            let x: i32 = arr[0];
                        }
                        return lib::b::small();
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    pub fn small() -> i32 {
                        forall {
                            let arr: [i32; 16] = @;
                            let x: i32 = arr[0];
                        }
                        return 0;
                    }
                "#,
            ),
        ];
        assert!(
            !has_stack_depth_exceeded_multi(files),
            "a small cross-file call chain must not trip A036"
        );
    }

    /// An imported struct used by value as a local must be sized from its defining
    /// file. A single `lib::big::Big` local (`[i64; 9000]` field ~ 72 KB) exceeds
    /// the budget on its own; before the fix the bare-name lookup missed the
    /// canonical-key index, sized the frame 0, and A036 stayed silent.
    #[test]
    fn a036_imported_struct_frame_sized_from_defining_file_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::big;
                    pub fn main() -> i32 {
                        forall {
                            let b: lib::big::Big = @;
                            let x: i32 = b.tag;
                        }
                        return 0;
                    }
                "#,
            ),
            (
                vec!["lib", "big"],
                r#"
                    pub struct Big { data: [i64; 9000]; tag: i32; }
                "#,
            ),
        ];
        assert!(
            has_stack_depth_exceeded_multi(files),
            "an imported over-budget struct frame must be sized (not 0) and trip A036"
        );
    }

    // Part D: qualified-typed by-value parameter frames
    //
    // A by-value parameter whose type is written as a `::`-qualified path
    // (`fn consume(big: lib::big::Big)`) reaches the frame estimator as an
    // unresolved `Qualified` carrier — the estimator derives a parameter type from
    // the raw AST, bypassing the canonicalization the type checker applies to a
    // stored signature. The carrier must be resolved to its struct and sized; a
    // regression sized it 0, so an oversized cross-file struct passed by value
    // slipped past the budget and overflowed the shadow stack at runtime.
    //
    // The chain uses two consumers (`consume -> consume2`), each taking the struct
    // by value, so the two qualified-parameter frames sum over the budget while the
    // *constructor* (`mk`) and each individual frame stay under it — isolating the
    // parameter-sizing fix from the (already-sound) constructor/local paths.

    /// `Big { data: [i32; 10000]; tag: i32; }` is ~40 KB. Two chained consumers
    /// each take it by value via a qualified parameter type, so the chain
    /// `make_one -> consume -> consume2` sums two ~40 KB parameter frames over the
    /// 64 KB budget. Without sizing the qualified parameter, those frames count 0
    /// and A036 stays silent.
    #[test]
    fn a036_qualified_param_by_value_over_budget_chain_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::big;
                    pub fn consume2(big: lib::big::Big) -> i32 { return big.tag; }
                    pub fn consume(big: lib::big::Big) -> i32 {
                        return consume2(big) + big.tag;
                    }
                    pub fn make_one() -> i32 {
                        let b: lib::big::Big = lib::big::mk();
                        return consume(b);
                    }
                    pub fn main() -> i32 { return make_one(); }
                "#,
            ),
            (
                vec!["lib", "big"],
                r#"
                    pub struct Big { data: [i32; 10000]; tag: i32; }
                    pub fn mk() -> Big {
                        forall {
                            let d: Big = @;
                            return d;
                        }
                    }
                "#,
            ),
        ];
        let diag = stack_depth_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("consume") && msg.contains("consume2"),
            "qualified-parameter chain should name the consumer chain, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// A method whose receiver-equivalent is reached by value through a qualified
    /// parameter type must be sized identically to the free-function form: the
    /// `mut self` slot path already resolves through the defining file, so this
    /// guards that a qualified *parameter* on a method (in addition to its `self`)
    /// is counted. The method `take` holds an over-budget qualified parameter and
    /// must trip A036 on its own.
    #[test]
    fn a036_qualified_param_on_method_over_budget_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::big;
                    pub struct Holder {
                        v: i32;
                        pub fn take(self, big: lib::big::Big) -> i32 { return big.tag + self.v; }
                    }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (
                vec!["lib", "big"],
                r#"
                    pub struct Big { data: [i32; 20000]; tag: i32; }
                "#,
            ),
        ];
        let diag = stack_depth_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("Holder.take"),
            "an over-budget qualified parameter on a method must trip A036 naming `Holder.take`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// Parity: the item-imported / bare form of the same by-value parameter behaves
    /// identically to the qualified form. `consume(big: Big)` (with `Big`
    /// item-imported) takes the struct by value; two chained consumers exceed
    /// budget exactly as the qualified-path version does, proving the fix did not
    /// special-case the qualified spelling but resolves it to the same struct.
    #[test]
    fn a036_item_imported_param_by_value_over_budget_chain_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::big::{Big, mk};
                    pub fn consume2(big: Big) -> i32 { return big.tag; }
                    pub fn consume(big: Big) -> i32 { return consume2(big) + big.tag; }
                    pub fn make_one() -> i32 {
                        let b: Big = mk();
                        return consume(b);
                    }
                    pub fn main() -> i32 { return make_one(); }
                "#,
            ),
            (
                vec!["lib", "big"],
                r#"
                    pub struct Big { data: [i32; 10000]; tag: i32; }
                    pub fn mk() -> Big {
                        forall {
                            let d: Big = @;
                            return d;
                        }
                    }
                "#,
            ),
        ];
        assert!(
            has_stack_depth_exceeded_multi(files),
            "the item-imported by-value parameter form must trip A036 identically to the qualified form"
        );
    }

    /// An under-budget qualified-typed by-value parameter must compile: a single
    /// `consume(small: lib::small::Small)` whose `Small` is well under the budget,
    /// with no chain, must not trip A036. Guards that sizing the qualified
    /// parameter does not over-reject a legitimately small one.
    #[test]
    fn a036_qualified_param_under_budget_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::small;
                    pub fn consume(s: lib::small::Small) -> i32 { return s.tag; }
                    pub fn main() -> i32 {
                        let s: lib::small::Small = lib::small::mk();
                        return consume(s);
                    }
                "#,
            ),
            (
                vec!["lib", "small"],
                r#"
                    pub struct Small { data: [i32; 16]; tag: i32; }
                    pub fn mk() -> Small {
                        forall {
                            let d: Small = @;
                            return d;
                        }
                    }
                "#,
            ),
        ];
        assert!(
            !has_stack_depth_exceeded_multi(files),
            "a small qualified-typed by-value parameter must not trip A036"
        );
    }

    /// Cross-file frame-size parity for qualified-typed by-value parameters.
    ///
    /// A036's soundness rests on its estimate never *under*-counting codegen's
    /// real frame. The single-file parity corpus in Part B cannot exercise a
    /// qualified parameter type, which only arises across files. This asserts the
    /// analysis estimate remains an upper bound of that real frame on a
    /// cross-file program.
    ///
    /// `consume` assigns its parameter, which is what keeps the parity check
    /// non-vacuous: a compound parameter the callee never writes is passed by
    /// reference and gets no frame copy at all, so a read-only `consume` would
    /// have a real frame of zero and compare an estimate against nothing.
    #[test]
    fn a036_estimate_is_sound_upper_bound_for_qualified_param_frame() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::big;
                    pub fn consume(mut p: lib::big::Big) -> i32 { p.tag = p.tag + 1; return p.tag; }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (
                vec!["lib", "big"],
                r#"
                    pub struct Big { data: [i32; 4000]; tag: i32; }
                "#,
            ),
        ];
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for parity test input");
        let estimate = inference_analysis::estimate_frame_sizes(&ctx);

        let output = codegen_output_multi_file_no_analysis(files);
        let real = output.frame_sizes();

        // `real` is keyed by the structured `FnKey`; the entry-file free function
        // `consume` has an empty module path, so it renders to the bare `consume`.
        // Find it by its rendered name to avoid naming `FnKey` in the test crate.
        let consume_real = real
            .iter()
            .find(|(key, _)| key.to_string() == "consume")
            .map(|(_, &bytes)| bytes)
            .expect("codegen must emit a frame size for `consume`");
        assert!(
            consume_real > 0,
            "the qualified-typed by-value parameter must give `consume` a real frame copy"
        );
        for (key, &real_bytes) in real {
            let est = estimate.get(key).copied().unwrap_or(0);
            assert!(
                est >= real_bytes,
                "A036 estimate {est} < codegen real frame {real_bytes} for fn `{key}`"
            );
        }
    }

    // Part E (#63): cross-file struct-associated-function chains
    //
    // The structured `FnKey` that fixed the A035 sibling-file collision also keys
    // A036's frame map. A cross-file chain through struct associated functions
    // (rather than free functions) must have its cumulative depth summed: each
    // assoc-fn node is keyed `Method`, distinct from any same-named sibling-file
    // free fn, so the chain's edges resolve to the right nodes and the budget is
    // checked across files.

    /// A cross-file chain through struct associated functions
    /// `main -> A::make -> lib::b::B::make`, each holding a ~40 KB frame, summing
    /// past the budget. Must be rejected and name the chain across files.
    #[test]
    fn a036_cross_file_assoc_fn_over_budget_chain_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a;
                    pub fn main() -> i32 { return lib::a::A::make(); }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b;
                    pub struct A {
                        v: i32;
                        pub fn make() -> i32 {
                            forall {
                                let arr: [i64; 5000] = @;
                                let x: i64 = arr[0];
                            }
                            return lib::b::B::make();
                        }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    pub struct B {
                        v: i32;
                        pub fn make() -> i32 {
                            forall {
                                let arr: [i64; 5000] = @;
                                let x: i64 = arr[0];
                            }
                            return 0;
                        }
                    }
                "#,
            ),
        ];
        let diag = stack_depth_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("lib.a.A.make") && msg.contains("lib.b.B.make"),
            "diagnostic should name the cross-file assoc-fn chain, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }

    /// The same cross-file assoc-fn call shape with small frames stays under
    /// budget, so A036 must not fire — the chain is recorded but bounded.
    #[test]
    fn a036_cross_file_assoc_fn_under_budget_chain_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a;
                    pub fn main() -> i32 { return lib::a::A::make(); }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b;
                    pub struct A {
                        v: i32;
                        pub fn make() -> i32 {
                            forall {
                                let arr: [i32; 16] = @;
                                let x: i32 = arr[0];
                            }
                            return lib::b::B::make();
                        }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    pub struct B {
                        v: i32;
                        pub fn make() -> i32 {
                            forall {
                                let arr: [i32; 16] = @;
                                let x: i32 = arr[0];
                            }
                            return 0;
                        }
                    }
                "#,
            ),
        ];
        assert!(
            !has_stack_depth_exceeded_multi(files),
            "a small cross-file assoc-fn chain must not trip A036"
        );
    }

    // Part F (#63): injective spec key keeps a heavy spec node visible
    //
    // A036 keys its frame map by `FnKey` through the shared call graph. Before the
    // spec key became injective, a heavy spec function in `lib/checks::S` and a
    // tiny same-folded sibling in `lib_checks::S` shared one key; the last-wins
    // index could keep the tiny node, under-counting the heavy chain so it slipped
    // under budget. The injective key keeps both nodes, so the heavy over-budget
    // chain is still detected.

    /// A heavy spec chain `S::heavy -> S::tail` (~80 KB summed) in `lib/checks`,
    /// shadowed by a tiny same-folded sibling `lib_checks::S`. The over-budget
    /// chain must still be rejected.
    #[test]
    fn a036_spec_over_budget_chain_with_folding_collision_sibling_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::checks;
                    use lib_checks;
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (
                vec!["lib", "checks"],
                r#"
                    spec S {
                        fn heavy() -> i32 {
                            forall {
                                let arr: [i64; 5000] = @;
                                let x: i64 = arr[0];
                            }
                            return tail();
                        }
                        fn tail() -> i32 {
                            forall {
                                let arr: [i64; 5000] = @;
                                let x: i64 = arr[0];
                            }
                            return 0;
                        }
                    }
                "#,
            ),
            (
                vec!["lib_checks"],
                r#"
                    spec S {
                        fn heavy() -> i32 { return 0; }
                        fn tail() -> i32 { return 0; }
                    }
                "#,
            ),
        ];
        let diag = stack_depth_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("lib_checks_S.heavy") && msg.contains("lib_checks_S.tail"),
            "the heavy spec chain must be detected despite the same-folded sibling, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A036");
    }
}

/// The cross-crate guard the two hand-synced constants behind A036 used to ask
/// for in prose and could not express.
///
/// A036's budget and the shadow stack codegen emits are two values in two crates
/// that neither depends on the other, so nothing brings them together at the
/// point of use: the rule compares call-chain depth against its own number and
/// codegen sizes the stack from its own. Changing one alone leaves the rule
/// policing a stack the artifact does not have — rejecting programs a larger
/// stack accommodates, or, in the dangerous direction, accepting programs that
/// overflow a smaller one. This test crate depends on both, which makes it the
/// only place the equality can be stated.
#[cfg(test)]
mod budget_matches_emitted_stack {
    #[test]
    fn default_stack_budget_equals_default_emitted_stack_size() {
        assert_eq!(
            inference_analysis::AnalysisOptions::default().stack_budget_bytes,
            inference_wasm_codegen::MemoryLayout::default().stack_size,
            "A036's default budget must be the stack region a default build emits"
        );
    }
}
