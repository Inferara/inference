/// Integration tests for analysis rule A041.
///
/// - A041: DuplicateLocalName — each function-local name (`let`/`const`) may be
///   introduced at most once per function body. Two declarations of the same
///   name in disjoint sibling blocks — both arms of an `if`, two sequential
///   `if`s, a loop body and a later block, or two non-deterministic blocks —
///   are rejected here, because codegen flattens every body local into one
///   name-keyed map (one WebAssembly local per source name) where a repeat would
///   collide. Ancestor (nested) and parameter collisions are the type checker's
///   `VariableShadowed` territory and never reach analysis, so this file does not
///   exercise those shapes.
///
/// These tests are the cross-crate guard that the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, complementing the
/// in-crate message/`rule_id` unit tests in `core/analysis`.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_codegen, try_type_check_multi_file};
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

    /// Returns true if any analysis error is a `DuplicateLocalName` (A041).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules.
    fn has_a041(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::DuplicateLocalName { .. })),
        }
    }

    fn a041_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::DuplicateLocalName { .. }))
            .expect("expected a DuplicateLocalName diagnostic")
            .clone()
    }

    /// Counts how many `DuplicateLocalName` (A041) diagnostics the analysis emits
    /// for `source`. Used by the triple-duplicate test that asserts an exact A041
    /// count; like the other helpers it filters by variant so unrelated rules
    /// tripped by the same surface do not perturb the count.
    fn count_a041(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::DuplicateLocalName { .. }))
                .count(),
        }
    }

    /// Collects every `DuplicateLocalName` (A041) diagnostic for `source`, so a
    /// multi-violation test can inspect each diagnostic's two locations.
    fn a041_diags(source: &str) -> Vec<AnalysisDiagnostic> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::DuplicateLocalName { .. }))
                .cloned()
                .collect(),
        }
    }

    // ---------------------------------------------------------------------
    // Fires: duplicates across disjoint sibling blocks
    // ---------------------------------------------------------------------

    /// The two arms of one `if`/`else` each declare `x`. The arms are disjoint
    /// sibling scopes (no ancestor `x`), so the type checker accepts them, but
    /// they collapse to one WebAssembly local — A041 must fire on `x`.
    #[test]
    fn a041_if_else_arms_same_name_rejected() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let x: i32 = 1;
                } else {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 to flag the duplicated `x` across the if/else arms, got: {diag}"
        );
    }

    /// The issue's exact repro: two sequential `if`s, each declaring `x` in its
    /// then-arm, followed by a trailing `let z` so every path returns. The two
    /// `x`s live in sibling blocks at the same depth under different parents —
    /// the shape codegen panicked on.
    #[test]
    fn a041_sequential_sibling_ifs_rejected() {
        let source = r#"pub fn f(c: bool) -> i32 { if c { let x: i32 = 1; return x; } if !c { let x: i32 = 2; return x; } let z: i32 = 0; return z; }"#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 to flag `x` across the two sequential ifs, got: {diag}"
        );
    }

    /// Sibling reuse at the SAME nesting depth under DIFFERENT parents: `x` in the
    /// then-arm of one `if` and in the then-arm of a second `if`, isolated with
    /// no returns to obscure it.
    #[test]
    fn a041_two_then_arms_same_depth_rejected() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let x: i32 = 1;
                }
                if c {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for `x` reused in two sibling then-arms, got: {diag}"
        );
    }

    /// A `loop` body declares `x` and a later plain `{ }` block declares `x`
    /// again. The two are disjoint siblings; A041 descends into both the loop
    /// body and the bare block exactly as codegen's `pre_scan_locals` does.
    #[test]
    fn a041_loop_body_then_block_rejected() {
        let source = r#"
            fn f() {
                loop {
                    let x: i32 = 1;
                    break;
                }
                {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for `x` in a loop body and a later block, got: {diag}"
        );
    }

    /// Two sibling non-deterministic blocks — a `forall` then an `exists` — each
    /// declare `x`. The descent is BlockKind-agnostic: non-det blocks are walked
    /// with the same per-body accumulator as any other block, so the reuse is
    /// caught. (Neither block needs an uzumaki to be legal.)
    #[test]
    fn a041_sibling_nondet_blocks_rejected() {
        let source = r#"
            fn f() {
                forall {
                    let x: i32 = 1;
                }
                exists {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for `x` across a forall and an exists block, got: {diag}"
        );
    }

    /// A `const x` in one sibling arm and a `let x` in the other collide: the rule
    /// treats `ConstDef` and `VarDef` as the same flat namespace, so the
    /// cross-kind duplicate is rejected.
    #[test]
    fn a041_const_then_let_cross_collision_rejected() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    const x: i32 = 1;
                }
                if !c {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for a `const x` then `let x` cross-collision, got: {diag}"
        );
    }

    /// The reverse direction of the cross-kind collision: a `let x` first, then a
    /// `const x`. `local_declaration` must recognise both kinds whether they land
    /// first or second in the walk.
    #[test]
    fn a041_let_then_const_cross_collision_rejected() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let x: i32 = 1;
                }
                if !c {
                    const x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for a `let x` then `const x` cross-collision, got: {diag}"
        );
    }

    /// Three sibling blocks each declare `x`. A041 emits one diagnostic per
    /// *repeat*, so three declarations yield exactly two diagnostics, and both
    /// cite the SAME first declaration. The source has no leading newline so line
    /// numbers are unambiguous: the first `x` is on line 2, the repeats on lines
    /// 3 and 4.
    #[test]
    fn a041_triple_duplicate_emits_two_diagnostics_citing_first() {
        let source = r#"fn f(c: bool) {
    if c { let x: i32 = 1; }
    if c { let x: i32 = 2; }
    if c { let x: i32 = 3; }
}"#;
        assert_eq!(
            count_a041(source),
            2,
            "three sibling declarations of `x` must yield exactly two A041 diagnostics"
        );
        let diags = a041_diags(source);
        assert_eq!(diags.len(), 2, "expected exactly two A041 diagnostics");

        let mut first_lines = Vec::new();
        let mut first_locations = Vec::new();
        let mut repeat_locations = Vec::new();
        for diag in &diags {
            let AnalysisDiagnostic::DuplicateLocalName {
                name,
                location,
                first_location,
            } = diag
            else {
                panic!("filtered diagnostic was not a DuplicateLocalName: {diag}");
            };
            assert_eq!(name, "x", "each diagnostic must name the duplicated local `x`");
            first_lines.push(first_location.start_line);
            first_locations.push(*first_location);
            repeat_locations.push(*location);
        }

        assert_eq!(
            first_locations[0], first_locations[1],
            "both A041 diagnostics must cite the same first declaration"
        );
        assert_eq!(
            first_lines[0], 2,
            "the cited first declaration must be the line-2 `let x`, got line {}",
            first_lines[0]
        );
        assert_ne!(
            repeat_locations[0], repeat_locations[1],
            "the two diagnostics must be anchored at the two distinct repeat sites"
        );
    }

    /// Two sibling blocks each declare a compound (`Point`) local `p`. This is the
    /// frame-slot hazard shape: two compound `p`s would otherwise map to one
    /// name-keyed frame slot. A041 fires on `p` before that can happen.
    #[test]
    fn a041_compound_struct_duplicate_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn f(c: bool) {
                if c {
                    let p: Point = Point { x: 1, y: 2 };
                }
                if !c {
                    let p: Point = Point { x: 3, y: 4 };
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "p"),
            "expected A041 for the duplicated compound local `p`, got: {diag}"
        );
    }

    /// A duplicate inside a struct *method* body is caught: the walker's
    /// `for_each_function_body` descends into methods, and each method body gets
    /// its own fresh accumulator. The method accesses `self` so it trips no
    /// unrelated rule.
    #[test]
    fn a041_duplicate_in_method_body_rejected() {
        let source = r#"
            struct S {
                v: i32;
                fn m(self, c: bool) -> i32 {
                    if c {
                        let x: i32 = 1;
                    }
                    if !c {
                        let x: i32 = 2;
                    }
                    return self.v;
                }
            }
            fn main() -> i32 { return 0; }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for `x` duplicated inside a method body, got: {diag}"
        );
    }

    /// A duplicate inside a *spec function* body is caught: `for_each_function_body`
    /// recurses into spec definitions, so proof-obligation bodies are walked like
    /// any other.
    #[test]
    fn a041_duplicate_in_spec_function_rejected() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn check(c: bool) -> i32 {
                    if c {
                        let x: i32 = 1;
                    }
                    if !c {
                        let x: i32 = 2;
                    }
                    return 0;
                }
            }
        "#;
        let diag = a041_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::DuplicateLocalName { name, .. } if name == "x"),
            "expected A041 for `x` duplicated inside a spec function body, got: {diag}"
        );
    }

    // ---------------------------------------------------------------------
    // Diagnostic quality
    // ---------------------------------------------------------------------

    /// The diagnostic names the duplicated local, cites the first declaration,
    /// gives the rename-or-hoist guidance, never uses shadowing terminology, and
    /// reports rule id A041.
    #[test]
    fn a041_diagnostic_quality() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let x: i32 = 1;
                }
                if !c {
                    let x: i32 = 2;
                }
            }
        "#;
        let diag = a041_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("is already declared in this function"),
            "A041 message must state the local is already declared, got: {msg}"
        );
        assert!(
            msg.contains("first declaration at"),
            "A041 message must cite the first declaration, got: {msg}"
        );
        assert!(
            msg.contains("rename one of them or hoist"),
            "A041 message must give the rename-or-hoist guidance, got: {msg}"
        );
        assert!(
            !msg.contains("shadow"),
            "A041 message must not use shadowing terminology (nothing is shadowed here), got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A041");
    }

    // ---------------------------------------------------------------------
    // Does not fire
    // ---------------------------------------------------------------------

    /// Distinct names in the two arms (`a` in then, `b` in else) share no local
    /// name, so A041 must stay silent — the accepted-code shape.
    #[test]
    fn a041_unique_names_in_sibling_arms_accepted() {
        let source = r#"
            fn f(c: bool) {
                if c {
                    let a: i32 = 1;
                } else {
                    let b: i32 = 2;
                }
            }
        "#;
        assert!(
            !has_a041(source),
            "distinct names `a` and `b` in sibling arms must not trip A041"
        );
    }

    /// The same local name in two different functions in one file is fine: each
    /// function body gets a fresh accumulator, so the names never interact.
    #[test]
    fn a041_same_name_in_two_functions_accepted() {
        let source = r#"
            fn f() {
                let x: i32 = 1;
            }
            fn g() {
                let x: i32 = 2;
            }
        "#;
        assert!(
            !has_a041(source),
            "the same local name in two different functions must not trip A041"
        );
    }

    /// The hoisted rewrite the diagnostic suggests — a single `let mut x` above
    /// the branches, assigned in each and used after — must compile cleanly all
    /// the way through parse -> type-check -> analyze -> codegen. `try_codegen`
    /// runs that whole pipeline and catches panics, so an `Ok` here pins that the
    /// suggested fix genuinely works (and trips no analysis error, since a stray
    /// diagnostic would make the pipeline fail rather than return `Ok`).
    #[test]
    fn a041_hoisted_single_declaration_compiles_end_to_end() {
        let source = r#"
            fn f(c: bool) -> i32 {
                let mut x: i32 = 0;
                if c {
                    x = 1;
                } else {
                    x = 2;
                }
                return x;
            }
        "#;
        assert!(
            !has_a041(source),
            "a single hoisted declaration must not trip A041"
        );
        assert!(
            try_codegen(source).is_ok(),
            "the hoisted rewrite must compile end-to-end through the analysis-inclusive pipeline"
        );
    }

    // ---------------------------------------------------------------------
    // Multi-file: per-function scoping across module boundaries
    // ---------------------------------------------------------------------

    /// Type-checks a multi-file program (entry first, empty module path) and runs
    /// the analysis pass, returning its result.
    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    fn has_a041_multi(files: &[(Vec<&str>, &str)]) -> bool {
        match analyze_multi(files) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::DuplicateLocalName { .. })),
        }
    }

    /// The same local name declared in functions in two *different files* must not
    /// trip A041: the fresh-per-body accumulator makes names in distinct function
    /// bodies non-interacting even across module boundaries. Both files must
    /// type-check as one project, so the entry `use`s and calls the module fn.
    #[test]
    fn a041_same_name_in_two_files_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 {
                        let x: i32 = lib::helper();
                        return x;
                    }
                "#,
            ),
            (
                vec!["lib"],
                r#"
                    pub fn helper() -> i32 {
                        let x: i32 = 1;
                        return x;
                    }
                "#,
            ),
        ];
        assert!(
            !has_a041_multi(files),
            "the same local `x` in functions in two different files must not trip A041"
        );
    }
}
