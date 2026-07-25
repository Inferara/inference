/// Integration tests for analysis rule A043.
///
/// - A043: ReservedExportName — an entry-file top-level `pub fn` may not be named
///   `memory` or `__stack_pointer`. Codegen exports such a function under its
///   plain source name and separately reserves those two names for the module's
///   synthetic linear-memory and shadow-stack exports; a user function with
///   either name collides with that surface — producing a duplicate export name
///   (invalid wasm) when the program uses memory, or hijacking the standard
///   `memory` export with a Function when it does not. The rule is unconditional,
///   so the exported ABI never depends on that hidden codegen state.
///
/// The predicate is entry-file-only and top-level-only: methods (which nest in a
/// struct), spec-inner functions, imported-file `pub fn`s, locals, and struct
/// fields are all out of scope because none of them are exported under a plain
/// module-level name. These tests are the cross-crate guard that the rule fires
/// through a real parse -> type-check -> analyze pipeline, complementing the
/// in-crate message/`rule_id` unit tests in `core/analysis`.
///
/// Bare integer literals are `i32` and do not coerce in return position, so the
/// name-only test bodies return `i32`; the two tests that genuinely exercise an
/// `i64` value read it from a struct field, where the type is concrete.
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

    /// Returns true if any analysis error is a `ReservedExportName` (A043).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules (or warnings).
    fn has_a043(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ReservedExportName { .. })),
        }
    }

    fn a043_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::ReservedExportName { .. }))
            .expect("expected a ReservedExportName diagnostic")
            .clone()
    }

    /// Counts how many `ReservedExportName` (A043) diagnostics the analysis emits
    /// for `source`. Like the other helpers it filters by variant so unrelated
    /// rules tripped by the same surface do not perturb the count.
    fn count_a043(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ReservedExportName { .. }))
                .count(),
        }
    }

    /// Type-checks a multi-file program (entry first, empty module path) and runs
    /// the analysis pass, returning its result.
    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    fn has_a043_multi(files: &[(Vec<&str>, &str)]) -> bool {
        match analyze_multi(files) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ReservedExportName { .. })),
        }
    }

    // ---------------------------------------------------------------------
    // Fires: entry-file top-level `pub fn` claiming a reserved name
    // ---------------------------------------------------------------------

    /// A memory-FREE `pub fn memory` still fires. This is the ABI-landmine case:
    /// today it compiles to a valid module that exports a *Function* named
    /// `memory`, hijacking the name hosts resolve to linear memory. The rule is
    /// unconditional, so it rejects the name even though no memory is emitted.
    #[test]
    fn a043_entry_pub_fn_memory_rejected() {
        let source = r#"
            pub fn memory() -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        let diag = a043_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::ReservedExportName { name, .. } if name == "memory"),
            "expected A043 to flag the entry-file `pub fn memory`, got: {diag}"
        );
    }

    /// The twin case for the shadow-stack global: an entry-file `pub fn
    /// __stack_pointer` is rejected on the same unconditional grounds.
    #[test]
    fn a043_entry_pub_fn_stack_pointer_rejected() {
        let source = r#"
            pub fn __stack_pointer() -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        let diag = a043_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::ReservedExportName { name, .. } if name == "__stack_pointer"),
            "expected A043 to flag the entry-file `pub fn __stack_pointer`, got: {diag}"
        );
    }

    /// The invalid-wasm shape: a struct local makes codegen emit linear memory,
    /// so a `pub fn memory` would produce two exports named `memory`. A043 fires
    /// before that invalid module can be built. The returned `t.v` is a concrete
    /// `i64` field read, so the `i64` return type is genuine here.
    #[test]
    fn a043_memory_with_struct_local_rejected() {
        let source = r#"
            struct T { v: i64; }
            pub fn memory() -> i64 { let t: T = T { v: 1 }; return t.v; }
        "#;
        let diag = a043_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::ReservedExportName { name, .. } if name == "memory"),
            "expected A043 to flag the memory-using `pub fn memory`, got: {diag}"
        );
    }

    /// One file declaring both reserved names must yield exactly two A043
    /// diagnostics — one per offending function.
    #[test]
    fn a043_both_reserved_names_two_diagnostics() {
        let source = r#"
            pub fn memory() -> i32 { return 1; }
            pub fn __stack_pointer() -> i32 { return 2; }
        "#;
        assert_eq!(
            count_a043(source),
            2,
            "a file declaring both reserved names must yield exactly two A043 diagnostics"
        );
    }

    /// Diagnostic quality through the real pipeline: the finding names the
    /// offending function, gives both suggested fixes, and reports rule id A043.
    #[test]
    fn a043_diagnostic_quality() {
        let source = r#"
            pub fn memory() -> i32 { return 1; }
        "#;
        let diag = a043_diag(source);
        assert!(
            matches!(&diag, AnalysisDiagnostic::ReservedExportName { name, .. } if name == "memory"),
            "expected A043 to flag `memory`, got: {diag}"
        );
        let msg = diag.to_string();
        assert!(
            msg.contains("entry-file `pub fn memory` collides"),
            "A043 message must name the offending function, got: {msg}"
        );
        assert!(
            msg.contains("rename the function"),
            "A043 message must suggest renaming the function, got: {msg}"
        );
        assert!(
            msg.contains("remove `pub`"),
            "A043 message must suggest removing `pub`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A043");
    }

    /// The entry-file predicate must survive project-mode threading: an entry
    /// file with a `pub fn memory` fires A043 even when it imports and calls into
    /// a sibling module. The imported module carries an innocent `pub fn helper`.
    #[test]
    fn a043_entry_offense_fires_in_project_mode() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn memory() -> i32 { return 1; }
                    pub fn main() -> i32 {
                        let x: i32 = lib::helper();
                        return x;
                    }
                "#,
            ),
            (
                vec!["lib"],
                r#"
                    pub fn helper() -> i32 { return 1; }
                "#,
            ),
        ];
        assert!(
            has_a043_multi(files),
            "an entry-file `pub fn memory` must still fire A043 in project mode"
        );
    }

    // ---------------------------------------------------------------------
    // Does not fire
    // ---------------------------------------------------------------------

    /// A non-pub `fn memory` in the entry file is never exported (private
    /// functions are not part of the module surface), so A043 stays silent and
    /// the program compiles end-to-end.
    #[test]
    fn a043_private_fn_memory_accepted() {
        let source = r#"
            fn memory() -> i32 { return 1; }
        "#;
        assert!(
            !has_a043(source),
            "a private `fn memory` is never exported, so A043 must not fire"
        );
        assert!(
            try_codegen(source).is_ok(),
            "a private `fn memory` must compile end-to-end through the analysis-inclusive pipeline"
        );
    }

    /// An imported-file `pub fn memory` is intra-project visibility, never a
    /// module export, so A043 must not fire on it. The entry file calls
    /// `lib::memory()` so both files type-check as one project.
    #[test]
    fn a043_imported_file_pub_fn_memory_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 {
                        let x: i32 = lib::memory();
                        return x;
                    }
                "#,
            ),
            (
                vec!["lib"],
                r#"
                    pub fn memory() -> i32 { return 1; }
                "#,
            ),
        ];
        assert!(
            !has_a043_multi(files),
            "an imported-file `pub fn memory` is never exported, so A043 must not fire"
        );
    }

    /// A struct *method* named `memory` nests in `Def::Struct` and is never a
    /// module export, so A043 must not fire. This also guards that the rule does
    /// not recurse into struct definitions. `self.v` is a concrete `i64` field
    /// read, so the `i64` return type is genuine here.
    #[test]
    fn a043_method_named_memory_accepted() {
        let source = r#"
            struct S {
                v: i64;
                fn memory(self) -> i64 { return self.v; }
            }
        "#;
        assert!(
            !has_a043(source),
            "a struct method named `memory` is never exported, so A043 must not fire"
        );
    }

    /// A spec-inner function named `memory` nests in `Def::Spec` and is never a
    /// module export, so A043 must not fire. This is the guard that the rule
    /// iterates direct defs only and does NOT descend through a body walker into
    /// spec definitions.
    #[test]
    fn a043_spec_inner_fn_memory_accepted() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn memory() -> i32 { return 0; }
            }
        "#;
        assert!(
            !has_a043(source),
            "a spec-inner `fn memory` is never exported, so A043 must not fire"
        );
    }

    /// A function *local* named `memory` is not an export, so A043 must not fire.
    #[test]
    fn a043_local_named_memory_accepted() {
        let source = r#"
            fn f() {
                let memory: i32 = 0;
            }
        "#;
        assert!(
            !has_a043(source),
            "a local named `memory` is not an export, so A043 must not fire"
        );
    }

    /// A struct *field* named `memory` is not an export, so A043 must not fire.
    #[test]
    fn a043_struct_field_named_memory_accepted() {
        let source = r#"
            struct S { memory: i64; }
        "#;
        assert!(
            !has_a043(source),
            "a struct field named `memory` is not an export, so A043 must not fire"
        );
    }

    /// A plain `pub fn main` program is fine: `main` is not a reserved export
    /// name, and it exports under its own name.
    #[test]
    fn a043_pub_fn_main_accepted() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            !has_a043(source),
            "`pub fn main` exports as `main`, which is not reserved, so A043 must not fire"
        );
    }

    /// The suggested fix genuinely compiles: the rejected memory-using shape,
    /// renamed to `pub fn my_memory`, passes the full analysis-inclusive pipeline
    /// (`try_codegen` runs parse -> type-check -> analyze -> codegen and catches
    /// panics), so an `Ok` here pins the rename as a real fix. The `i64` return
    /// is genuine — `t.v` is a concrete field read.
    #[test]
    fn a043_renamed_fn_fix_pin() {
        let source = r#"
            struct T { v: i64; }
            pub fn my_memory() -> i64 { let t: T = T { v: 1 }; return t.v; }
        "#;
        assert!(
            !has_a043(source),
            "the renamed `pub fn my_memory` must not trip A043"
        );
        assert!(
            try_codegen(source).is_ok(),
            "the renamed `pub fn my_memory` must compile end-to-end"
        );
    }
}
