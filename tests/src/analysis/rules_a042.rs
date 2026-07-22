/// Integration tests for analysis rule A042.
///
/// - A042: NonDetOutsideSpec — the non-deterministic block forms (inline
///   `forall`/`exists`/`assume`/`unique` statement blocks and the
///   function-body-modifier form `fn f() forall { … }`) describe formal
///   specifications and are valid only lexically inside a `spec` declaration.
///   Used in a top-level function, a top-level struct method, or a block nested
///   inside either, they are rejected here. Anything under a `spec` (free
///   functions and spec-inner struct methods) is allowed.
///
/// The rule is lexical, so these tests only assert which construct is *where*;
/// none depends on the compilation mode. Only the outermost non-det block on
/// each path is reported (no cascade). These are the cross-crate guard that the
/// rule fires through a real parse -> type-check -> analyze pipeline, complementing
/// the in-crate message/`rule_id` unit tests in `core/analysis`.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, get_test_data_path, try_type_check_multi_file};
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    /// The four non-deterministic block kinds, each with the source keyword that
    /// introduces it, so the position tests can sweep all four uniformly.
    const KINDS: [&str; 4] = ["forall", "exists", "assume", "unique"];

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

    /// Every `NonDetOutsideSpec` (A042) diagnostic for `source`. Filters by
    /// variant so unrelated rules the same surface may trip do not perturb the
    /// count.
    fn a042_diags(source: &str) -> Vec<AnalysisDiagnostic> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::NonDetOutsideSpec { .. }))
                .cloned()
                .collect(),
        }
    }

    fn count_a042(source: &str) -> usize {
        a042_diags(source).len()
    }

    fn has_a042(source: &str) -> bool {
        count_a042(source) > 0
    }

    fn has_a006(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. })),
        }
    }

    // =====================================================================
    // Fires: every kind × every non-det form × every outside-spec position
    // =====================================================================

    /// Inline non-det block directly in a top-level function body, for each of
    /// the four kinds.
    #[test]
    fn a042_inline_block_in_top_level_fn_fires_for_each_kind() {
        for kind in KINDS {
            let source = format!(
                r#"
                    fn f() {{
                        {kind} {{
                            let x: i32 = 1;
                        }}
                    }}
                "#
            );
            let diags = a042_diags(&source);
            assert_eq!(
                diags.len(),
                1,
                "inline `{kind}` block in a top-level fn must fire exactly one A042, got: {diags:?}"
            );
            assert!(
                matches!(&diags[0], AnalysisDiagnostic::NonDetOutsideSpec { block_kind, .. } if *block_kind == kind),
                "A042 must name the `{kind}` block kind, got: {:?}",
                diags[0]
            );
        }
    }

    /// Function-body-modifier form (`fn f() forall { … }`) at the top level, for
    /// each kind. The body block itself carries the non-det kind.
    #[test]
    fn a042_body_modifier_on_top_level_fn_fires_for_each_kind() {
        for kind in KINDS {
            let source = format!(
                r#"
                    fn f() -> () {kind} {{
                        let x: i32 = 1;
                    }}
                "#
            );
            let diags = a042_diags(&source);
            assert_eq!(
                diags.len(),
                1,
                "`{kind}` body modifier on a top-level fn must fire exactly one A042, got: {diags:?}"
            );
            assert!(
                matches!(&diags[0], AnalysisDiagnostic::NonDetOutsideSpec { block_kind, .. } if *block_kind == kind),
                "A042 must name the `{kind}` body-modifier kind, got: {:?}",
                diags[0]
            );
        }
    }

    /// Inline non-det block inside a top-level struct *method* body, for each
    /// kind. The method accesses `self` so it trips no unrelated method lint.
    #[test]
    fn a042_inline_block_in_struct_method_fires_for_each_kind() {
        for kind in KINDS {
            let source = format!(
                r#"
                    struct S {{
                        v: i32;
                        fn m(self) -> i32 {{
                            {kind} {{
                                let x: i32 = 1;
                            }}
                            return self.v;
                        }}
                    }}
                    fn main() -> i32 {{ return 0; }}
                "#
            );
            assert_eq!(
                count_a042(&source),
                1,
                "inline `{kind}` block in a top-level struct method must fire exactly one A042"
            );
        }
    }

    /// Function-body-modifier form on a top-level struct method, for each kind.
    #[test]
    fn a042_body_modifier_on_struct_method_fires_for_each_kind() {
        for kind in KINDS {
            let source = format!(
                r#"
                    struct S {{
                        v: i32;
                        fn m(self) -> () {kind} {{
                            let x: i32 = 1;
                        }}
                    }}
                    fn main() -> i32 {{ return 0; }}
                "#
            );
            assert_eq!(
                count_a042(&source),
                1,
                "`{kind}` body modifier on a top-level struct method must fire exactly one A042"
            );
        }
    }

    /// A non-det block nested inside regular structure (a bare block, an `if`
    /// arm, and a loop body) in a top-level function still fires — the scan
    /// descends through regular blocks to find the outermost non-det block.
    #[test]
    fn a042_nested_inside_regular_blocks_fires_for_each_kind() {
        for kind in KINDS {
            let bare = format!(
                r#"
                    fn f() {{
                        {{
                            {kind} {{ let x: i32 = 1; }}
                        }}
                    }}
                "#
            );
            assert_eq!(
                count_a042(&bare),
                1,
                "`{kind}` inside a bare nested block must fire A042"
            );

            let if_arm = format!(
                r#"
                    fn f(c: bool) {{
                        if c {{
                            {kind} {{ let x: i32 = 1; }}
                        }}
                    }}
                "#
            );
            assert_eq!(
                count_a042(&if_arm),
                1,
                "`{kind}` inside an if-arm must fire A042"
            );

            let loop_body = format!(
                r#"
                    fn f() {{
                        loop {{
                            {kind} {{ let x: i32 = 1; }}
                            break;
                        }}
                    }}
                "#
            );
            assert_eq!(
                count_a042(&loop_body),
                1,
                "`{kind}` inside a loop body must fire A042"
            );
        }
    }

    /// A non-det block in the `else` arm of an `if` outside a spec fires — the
    /// scan inspects both arms.
    #[test]
    fn a042_else_arm_nondet_outside_spec_fires() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let a: i32 = 1;
                } else exists {
                    let b: i32 = 2;
                }
            }
        "#;
        let diags = a042_diags(source);
        assert_eq!(diags.len(), 1, "an `else exists` arm outside a spec must fire A042");
        assert!(
            matches!(&diags[0], AnalysisDiagnostic::NonDetOutsideSpec { block_kind, .. } if *block_kind == "exists"),
            "A042 must name the `exists` else-arm kind, got: {:?}",
            diags[0]
        );
    }

    // =====================================================================
    // Nested non-det: outermost-only, no cascade
    // =====================================================================

    /// A non-det block nested directly inside another non-det block outside a
    /// spec reports ONLY the outermost block — the inner one is not cascaded.
    #[test]
    fn a042_nested_nondet_reports_only_outermost() {
        let source = r#"
            fn f() {
                forall {
                    let x: i32 = 1;
                    exists {
                        let y: i32 = 2;
                    }
                }
            }
        "#;
        let diags = a042_diags(source);
        assert_eq!(
            diags.len(),
            1,
            "nested non-det outside a spec must report only the outermost block, got: {diags:?}"
        );
        assert!(
            matches!(&diags[0], AnalysisDiagnostic::NonDetOutsideSpec { block_kind, .. } if *block_kind == "forall"),
            "the single diagnostic must be for the outermost `forall`, got: {:?}",
            diags[0]
        );
    }

    /// A non-det block nested inside a body-modifier non-det block reports only
    /// the (outer) body-modifier block.
    #[test]
    fn a042_body_modifier_with_nested_block_reports_only_outermost() {
        let source = r#"
            fn f() -> () forall {
                assume {
                    let x: i32 = 1;
                }
            }
        "#;
        assert_eq!(
            count_a042(source),
            1,
            "a body-modifier non-det block with a nested non-det block must report once"
        );
    }

    // =====================================================================
    // Multiple offenders: all collected in one pass
    // =====================================================================

    /// Two sibling non-det blocks in one function body are both outermost, so
    /// both are reported in a single analysis pass.
    #[test]
    fn a042_sibling_nondet_blocks_all_reported() {
        let source = r#"
            fn f() {
                forall {
                    let x: i32 = 1;
                }
                exists {
                    let y: i32 = 2;
                }
            }
        "#;
        let diags = a042_diags(source);
        assert_eq!(
            diags.len(),
            2,
            "two sibling non-det blocks must both be reported, got: {diags:?}"
        );
    }

    /// Offenders spread across several top-level definitions (a free function, a
    /// struct method, and a body modifier) are all collected in one pass.
    #[test]
    fn a042_offenders_across_definitions_all_reported() {
        let source = r#"
            fn a() {
                forall { let x: i32 = 1; }
            }
            struct S {
                v: i32;
                fn m(self) -> i32 {
                    exists { let y: i32 = 2; }
                    return self.v;
                }
            }
            fn b() -> () assume {
                let z: i32 = 3;
            }
        "#;
        assert_eq!(
            count_a042(source),
            3,
            "one offender in each of three definitions must yield three A042 diagnostics"
        );
    }

    // =====================================================================
    // Allowed: anything lexically inside a `spec`
    // =====================================================================

    /// A body-modifier non-det spec free function does not fire.
    #[test]
    fn a042_spec_free_fn_body_modifier_allowed() {
        for kind in KINDS {
            let source = format!(
                r#"
                    fn main() -> i32 {{ return 0; }}
                    spec S {{
                        fn prop() -> () {kind} {{
                            let x: i32 = 1;
                        }}
                    }}
                "#
            );
            assert!(
                !has_a042(&source),
                "a `{kind}` body modifier on a spec free function must not fire A042"
            );
        }
    }

    /// Inline non-det blocks inside a spec function do not fire.
    #[test]
    fn a042_spec_fn_inline_blocks_allowed() {
        for kind in KINDS {
            let source = format!(
                r#"
                    fn main() -> i32 {{ return 0; }}
                    spec S {{
                        fn prop() {{
                            {kind} {{
                                let x: i32 = 1;
                            }}
                        }}
                    }}
                "#
            );
            assert!(
                !has_a042(&source),
                "an inline `{kind}` block inside a spec function must not fire A042"
            );
        }
    }

    /// A non-det block nested inside a spec function (under a body modifier) does
    /// not fire.
    #[test]
    fn a042_nested_nondet_inside_spec_fn_allowed() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn prop() -> () forall {
                    exists {
                        let x: i32 = 1;
                    }
                }
            }
        "#;
        assert!(
            !has_a042(source),
            "non-det nested inside a spec function must not fire A042"
        );
    }

    /// A spec-inner struct method carrying a non-det block does not fire: the
    /// scan never descends into a `spec`.
    #[test]
    fn a042_spec_inner_struct_method_allowed() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec Geometry {
                struct Point {
                    x: i32;
                    y: i32;
                    fn check(self) -> i32 {
                        forall {
                            let v: i32 = 1;
                        }
                        return self.x;
                    }
                }
            }
        "#;
        assert!(
            !has_a042(source),
            "a non-det block in a spec-inner struct method must not fire A042"
        );
    }

    /// An `else exists` arm inside a spec function does not fire.
    #[test]
    fn a042_else_arm_nondet_inside_spec_allowed() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn prop(c: bool) {
                    if c {
                        let a: i32 = 1;
                    } else exists {
                        let b: i32 = 2;
                    }
                }
            }
        "#;
        assert!(
            !has_a042(source),
            "an `else exists` arm inside a spec function must not fire A042"
        );
    }

    /// A plain function with only regular control flow (no non-det) does not
    /// fire — the accepted-code shape.
    #[test]
    fn a042_regular_top_level_fn_accepted() {
        let source = r#"
            fn f(c: bool) -> i32 {
                let mut acc: i32 = 0;
                if c {
                    acc = 1;
                } else {
                    acc = 2;
                }
                loop {
                    acc = acc + 1;
                    break;
                }
                return acc;
            }
        "#;
        assert!(
            !has_a042(source),
            "a regular function with no non-det constructs must not fire A042"
        );
    }

    // =====================================================================
    // Interaction with A006 (uzumaki outside a non-det block)
    // =====================================================================

    /// A bare `@` outside a spec and outside any non-det block trips A006 (there
    /// is no block to reject), not A042.
    #[test]
    fn a042_bare_uzumaki_outside_block_is_a006_not_a042() {
        let source = r#"
            fn f() {
                let x: i32 = @;
            }
        "#;
        assert!(!has_a042(source), "a bare `@` introduces no non-det block, so A042 must not fire");
        assert!(has_a006(source), "a bare `@` outside a non-det block must trip A006");
    }

    /// A `@` inside a `forall` block outside a spec trips A042 on the block, and
    /// A006 stays silent (the `@` does sit inside a non-det block). Rejecting the
    /// enclosing block is how `@` outside a spec is covered — A042 does not
    /// duplicate the `@` check.
    #[test]
    fn a042_uzumaki_inside_forall_outside_spec_is_a042_not_a006() {
        let source = r#"
            fn f() {
                forall {
                    let x: i32 = @;
                }
            }
        "#;
        assert!(has_a042(source), "the `forall` block outside a spec must trip A042");
        assert!(
            !has_a006(source),
            "the `@` sits inside a non-det block, so A006 must stay silent"
        );
    }

    // =====================================================================
    // Diagnostic quality
    // =====================================================================

    /// The diagnostic names the offending kind, points at a spec declaration,
    /// enumerates the non-deterministic constructs, and reports rule id A042.
    #[test]
    fn a042_diagnostic_quality() {
        let source = r#"
            fn f() {
                forall {
                    let x: i32 = 1;
                }
            }
        "#;
        let diags = a042_diags(source);
        assert_eq!(diags.len(), 1);
        let msg = diags[0].to_string();
        assert!(
            msg.contains("'forall' block"),
            "A042 message must name the offending block kind, got: {msg}"
        );
        assert!(
            msg.contains("spec declaration"),
            "A042 message must point to a spec declaration, got: {msg}"
        );
        assert!(
            msg.contains("forall, exists, assume, unique"),
            "A042 message must enumerate the non-deterministic constructs, got: {msg}"
        );
        assert_eq!(diags[0].rule_id(), "A042");
    }

    // =====================================================================
    // Fixture-driven negative tests
    // =====================================================================

    /// Loads a fixture under `tests/test_data/inf/` and returns its A042
    /// diagnostics. The two `nondet_*` fixtures place non-det constructs in plain
    /// functions specifically so this rule rejects them.
    fn a042_diags_for_fixture(file: &str) -> Vec<AnalysisDiagnostic> {
        let path = get_test_data_path().join("inf").join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        a042_diags(&source)
    }

    /// `nondet_blocks.inf` places three inline non-det blocks in plain top-level
    /// functions; each must be rejected with A042.
    #[test]
    fn a042_fixture_nondet_blocks_rejected() {
        let diags = a042_diags_for_fixture("nondet_blocks.inf");
        assert_eq!(
            diags.len(),
            3,
            "nondet_blocks.inf must trip A042 on each of its three inline blocks, got: {diags:?}"
        );
    }

    /// `nondet_body_modifiers.inf` places three non-det body modifiers on plain
    /// top-level functions; each must be rejected with A042.
    #[test]
    fn a042_fixture_nondet_body_modifiers_rejected() {
        let diags = a042_diags_for_fixture("nondet_body_modifiers.inf");
        assert_eq!(
            diags.len(),
            3,
            "nondet_body_modifiers.inf must trip A042 on each of its three body modifiers, got: {diags:?}"
        );
    }

    // =====================================================================
    // Multi-file: findings are labeled with the offending file's module path
    // =====================================================================

    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    /// A non-det block in an imported file is rejected and its rendered
    /// diagnostic names the imported file by its `::`-joined module path, while a
    /// finding stays anchored to the file it came from.
    #[test]
    fn a042_multi_file_finding_is_labeled_with_module_path() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 {
                        return lib::helper();
                    }
                "#,
            ),
            (
                vec!["lib"],
                r#"
                    pub fn helper() -> i32 {
                        forall {
                            let x: i32 = 1;
                        }
                        return 0;
                    }
                "#,
            ),
        ];
        let errors = analyze_multi(files).expect_err("the imported non-det block must be rejected");
        let has_a042 = errors
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::NonDetOutsideSpec { .. }));
        assert!(has_a042, "the imported `forall` block must trip A042");
        let rendered = errors.to_string();
        assert!(
            rendered.contains("lib:"),
            "the A042 finding must be labeled with the `lib` module path, got: {rendered}"
        );
    }
}
