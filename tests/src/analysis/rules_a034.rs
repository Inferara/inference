/// Integration tests for analysis rule A034.
///
/// - A034: VisibilityInsideSpec -- `pub` on definitions inside a `spec { ... }`
///   body has no effect; the spec itself is the visibility unit.
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

    fn collect_a034(source: &str) -> Vec<AnalysisDiagnostic> {
        let findings = match analyze(source) {
            Ok(r) => r.warnings().to_vec(),
            Err(e) => e.warnings().to_vec(),
        };
        findings
            .into_iter()
            .filter(|d| matches!(d, AnalysisDiagnostic::VisibilityInsideSpec { .. }))
            .collect()
    }

    // --- A034: visibility modifier inside spec body ---

    #[test]
    fn a034_pub_fn_inside_spec_fires() {
        let source = r#"
            spec MySpec {
                pub fn helper() -> i32 { return 1; }
            }
        "#;
        let a034 = collect_a034(source);
        assert_eq!(
            a034.len(),
            1,
            "expected exactly one A034 for `pub fn` inside spec, got: {a034:?}"
        );
        if let AnalysisDiagnostic::VisibilityInsideSpec {
            spec_name,
            def_name,
            def_kind,
            ..
        } = &a034[0]
        {
            assert_eq!(spec_name, "MySpec");
            assert_eq!(def_name, "helper");
            assert_eq!(*def_kind, "fn");
        } else {
            unreachable!();
        }
        assert_eq!(a034[0].rule_id(), "A034");
    }

    #[test]
    fn a034_pub_struct_inside_spec_fires() {
        let source = r#"
            spec MySpec {
                pub struct Inner { x: i32; }
            }
        "#;
        let a034 = collect_a034(source);
        assert_eq!(
            a034.len(),
            1,
            "expected exactly one A034 for `pub struct` inside spec, got: {a034:?}"
        );
        if let AnalysisDiagnostic::VisibilityInsideSpec {
            spec_name,
            def_name,
            def_kind,
            ..
        } = &a034[0]
        {
            assert_eq!(spec_name, "MySpec");
            assert_eq!(def_name, "Inner");
            assert_eq!(*def_kind, "struct");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn a034_pub_enum_inside_spec_fires() {
        let source = r#"
            spec MySpec {
                pub enum Color { Red, Green }
            }
        "#;
        let a034 = collect_a034(source);
        assert_eq!(
            a034.len(),
            1,
            "expected exactly one A034 for `pub enum` inside spec, got: {a034:?}"
        );
        if let AnalysisDiagnostic::VisibilityInsideSpec {
            def_name, def_kind, ..
        } = &a034[0]
        {
            assert_eq!(def_name, "Color");
            assert_eq!(*def_kind, "enum");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn a034_pub_type_alias_inside_spec_fires() {
        let source = r#"
            spec MySpec {
                pub type Foo = i32;
            }
        "#;
        let a034 = collect_a034(source);
        assert_eq!(
            a034.len(),
            1,
            "expected exactly one A034 for `pub type` inside spec, got: {a034:?}"
        );
        if let AnalysisDiagnostic::VisibilityInsideSpec {
            def_name, def_kind, ..
        } = &a034[0]
        {
            assert_eq!(def_name, "Foo");
            assert_eq!(*def_kind, "type");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn a034_bare_fn_inside_spec_does_not_fire() {
        let source = r#"
            spec MySpec {
                fn helper() -> i32 { return 1; }
            }
        "#;
        let a034 = collect_a034(source);
        assert!(
            a034.is_empty(),
            "bare `fn` inside spec must not trigger A034, got: {a034:?}"
        );
    }

    #[test]
    fn a034_bare_struct_inside_spec_does_not_fire() {
        let source = r#"
            spec MySpec {
                struct Inner { x: i32; }
            }
        "#;
        let a034 = collect_a034(source);
        assert!(
            a034.is_empty(),
            "bare `struct` inside spec must not trigger A034, got: {a034:?}"
        );
    }

    #[test]
    fn a034_top_level_pub_fn_does_not_fire() {
        let source = r#"
            pub fn top_helper() -> i32 { return 1; }
        "#;
        let a034 = collect_a034(source);
        assert!(
            a034.is_empty(),
            "top-level `pub fn` must not trigger A034, got: {a034:?}"
        );
    }

    #[test]
    fn a034_top_level_pub_struct_does_not_fire() {
        let source = r#"
            pub struct Point { x: i32; y: i32; }
        "#;
        let a034 = collect_a034(source);
        assert!(
            a034.is_empty(),
            "top-level `pub struct` must not trigger A034, got: {a034:?}"
        );
    }

    #[test]
    fn a034_multiple_pub_defs_in_spec_emit_one_diagnostic_each() {
        let source = r#"
            spec MySpec {
                pub fn alpha() -> i32 { return 1; }
                pub struct Inner { x: i32; }
                pub enum Color { Red }
            }
        "#;
        let a034 = collect_a034(source);
        assert_eq!(
            a034.len(),
            3,
            "expected one A034 per `pub` inner def, got: {a034:?}"
        );
        let kinds: Vec<&str> = a034
            .iter()
            .filter_map(|d| match d {
                AnalysisDiagnostic::VisibilityInsideSpec { def_kind, .. } => Some(*def_kind),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&"fn"));
        assert!(kinds.contains(&"struct"));
        assert!(kinds.contains(&"enum"));
    }

    #[test]
    fn a034_diagnostic_message_mentions_spec_name_and_def_name() {
        let source = r#"
            spec MySpec {
                pub fn helper() -> i32 { return 1; }
            }
        "#;
        let a034 = collect_a034(source);
        let text = a034[0].to_string();
        assert!(text.contains("MySpec"), "A034 must mention spec name: {text}");
        assert!(text.contains("helper"), "A034 must mention inner def name: {text}");
        assert!(text.contains("`pub`"), "A034 must reference the `pub` modifier: {text}");
    }
}
