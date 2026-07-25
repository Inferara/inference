//! Negative codegen tests.
//!
//! Each test verifies that an input which passes parsing, type-checking, and analysis
//! is correctly rejected (or panics) during WebAssembly code generation, and that the
//! resulting error message contains the expected diagnostic substring.

mod unimplemented_operators {
    use crate::utils::try_codegen;

    #[test]
    fn power_operator_literals() {
        let result = try_codegen("pub fn test() -> i32 { return 2 ** 3; }");
        assert!(
            result.is_err(),
            "power operator on literals should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Power operator"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn power_operator_variables() {
        let result = try_codegen("pub fn test(a: i32, b: i32) -> i32 { return a ** b; }");
        assert!(
            result.is_err(),
            "power operator on variables should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Power operator"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn power_operator_i64() {
        let result = try_codegen("pub fn test(a: i64, b: i64) -> i64 { return a ** b; }");
        assert!(result.is_err(), "power operator on i64 should fail codegen");
        let err = result.unwrap_err();
        assert!(
            err.contains("Power operator"),
            "unexpected error message: {err}"
        );
    }
}

mod uninitialized_variables {
    use crate::utils::build_ast;
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_analyze(source: &str) -> Result<(), Vec<AnalysisDiagnostic>> {
        let arena = build_ast(source.to_string());
        let ctx = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for uninitialized variable tests")
            .typed_context();
        match inference_analysis::analyze(&ctx) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.errors().to_vec()),
        }
    }

    #[test]
    fn uninitialized_i32() {
        let errors = try_analyze("pub fn test() { let x: i32; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized i32 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_i64() {
        let errors = try_analyze("pub fn test() { let x: i64; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized i64 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_u32() {
        let errors = try_analyze("pub fn test() { let x: u32; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized u32 should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_bool() {
        let errors = try_analyze("pub fn test() { let x: bool; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized bool should fail analysis: {errors:?}"
        );
    }

    #[test]
    fn uninitialized_struct() {
        let errors = try_analyze("struct P { x: i32; }\npub fn test() { let p: P; }").unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. })),
            "uninitialized struct should fail analysis: {errors:?}"
        );
    }
}

mod ignored_arguments {
    use crate::utils::try_codegen;

    #[test]
    fn ignored_argument_single() {
        let result = try_codegen("pub fn test(_: i32) -> i32 { return 1; }");
        assert!(result.is_err(), "ignored argument should fail codegen");
        let err = result.unwrap_err();
        assert!(
            err.contains("Ignore arguments"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn ignored_argument_multiple() {
        let result = try_codegen("pub fn test(_: i32, _: i32) -> i32 { return 1; }");
        assert!(
            result.is_err(),
            "multiple ignored arguments should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Ignore arguments"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn ignored_argument_mixed() {
        let result = try_codegen("pub fn test(a: i32, _: i32) -> i32 { return a; }");
        assert!(
            result.is_err(),
            "mixed ignored argument should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Ignore arguments"),
            "unexpected error message: {err}"
        );
    }
}

mod unit_void_return {
    use crate::utils::try_codegen;

    #[test]
    fn explicit_void_return() {
        let result = try_codegen("pub fn test() { return; }");
        assert!(result.is_err(), "explicit void return should fail codegen");
        let err = result.unwrap_err();
        assert!(
            err.contains("not yet implemented"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn void_function_with_return() {
        let result =
            try_codegen("fn helper() { return; }\npub fn test() -> i32 { helper(); return 1; }");
        assert!(
            result.is_err(),
            "void function with return should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("not yet implemented"),
            "unexpected error message: {err}"
        );
    }
}

mod unsupported_compound_types {
    use crate::utils::try_codegen;

    #[test]
    fn array_of_arrays_succeeds() {
        let result = try_codegen(
            "pub fn test() -> i32 { let a: [[i32; 2]; 2] = [[1,2],[3,4]]; return a[0][0]; }",
        );
        assert!(
            result.is_ok(),
            "multi-dimensional array literal init should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn array_of_structs_succeeds() {
        let result = try_codegen(
            "struct P { x: i32; }\npub fn test() -> i32 { let a: [P; 2] = [P{x:1}, P{x:2}]; return 1; }",
        );
        assert!(
            result.is_ok(),
            "array of structs should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn struct_with_array_field_succeeds() {
        let result = try_codegen(
            "struct S { arr: [i32; 2]; }\npub fn test() -> i32 { let s: S = S { arr: [1, 2] }; return 1; }",
        );
        assert!(
            result.is_ok(),
            "struct with array field should succeed codegen: {:?}",
            result.err()
        );
    }

    #[test]
    fn nested_array_of_structs_succeeds() {
        let result = try_codegen(
            "struct P { x: i32; y: i32; }\npub fn test() -> i32 { let g: [[P; 2]; 2] = [[P{x:1,y:2}, P{x:3,y:4}], [P{x:5,y:6}, P{x:7,y:8}]]; return g[1][0].x; }",
        );
        assert!(
            result.is_ok(),
            "nested array-of-structs literal init should succeed codegen: {:?}",
            result.err()
        );
    }
}

mod type_def_statement {
    use crate::utils::try_codegen;

    #[test]
    fn type_def_in_function_body() {
        let result = try_codegen("pub fn test() -> i32 { type T = i32; return 1; }");
        assert!(result.is_err(), "type def in function body should fail codegen");
        let err = result.unwrap_err();
        assert!(
            err.contains("not yet implemented"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn multiple_type_defs_in_function() {
        let result =
            try_codegen("pub fn test() -> i32 { type A = i32; type B = i64; return 1; }");
        assert!(
            result.is_err(),
            "multiple type defs in function should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("not yet implemented"),
            "unexpected error message: {err}"
        );
    }
}

mod uzumaki_compound_types {
    use crate::utils::{try_codegen, try_codegen_no_analysis, wasm_codegen_no_analysis};

    #[test]
    fn uzumaki_struct_in_forall() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let wasm_bytes = wasm_codegen_no_analysis(
            "struct P { x: i32; }\npub fn test() { forall { let p: P = @; } }",
        );
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn uzumaki_struct_return() {
        let result =
            try_codegen_no_analysis("struct P { x: i32; }\npub fn test() -> P { return @; }");
        assert!(
            result.is_err(),
            "uzumaki as sret return should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("sret return lowering failed"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn uzumaki_array_return() {
        let result = try_codegen_no_analysis("pub fn test() -> [i32; 3] { return @; }");
        assert!(
            result.is_err(),
            "uzumaki as array sret return should fail codegen"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("sret return lowering failed"),
            "unexpected error message: {err}"
        );
    }
}

mod compound_reassignment {
    use crate::utils::try_codegen;

    #[test]
    fn array_literal_reassignment() {
        let result = try_codegen(
            "pub fn test() -> i32 { let mut a: [i32; 2] = [1, 2]; a = [3, 4]; return a[0]; }",
        );
        assert!(
            result.is_ok(),
            "array literal reassignment should succeed codegen"
        );
    }
}

mod extern_function_call {
    use crate::utils::build_ast;

    #[test]
    fn extern_function_call_rejected_before_codegen() {
        let source = "external fn print(val: i32) -> ();\npub fn main() { print(42); }";
        let arena = build_ast(source.to_string());
        let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should pass for extern function call")
            .typed_context();
        let analysis_result = inference_analysis::analyze(&typed_context);
        assert!(
            analysis_result.is_err(),
            "call to extern function should be rejected by analysis"
        );
        let err = analysis_result.unwrap_err().to_string();
        assert!(
            err.contains("external function") && err.contains("print"),
            "expected analysis error about external function call, got: {err}"
        );
    }
}

mod duplicate_local_name {
    use crate::utils::try_codegen_no_analysis;

    /// The issue repro — two sequential sibling `if`s each declaring `x` — is
    /// rejected by analysis rule A041, so it only reaches codegen on the
    /// no-analysis path. There, `pre_scan_locals`' flat `locals_map` still
    /// catches the duplicate as a defense-in-depth backstop. This pins that the
    /// backstop survives and that A041 and codegen agree on this shape.
    #[test]
    fn duplicate_local_backstop_assert_still_fires_without_analysis() {
        let result = try_codegen_no_analysis(
            r#"pub fn f(c: bool) -> i32 { if c { let x: i32 = 1; return x; } if !c { let x: i32 = 2; return x; } let z: i32 = 0; return z; }"#,
        );
        assert!(
            result.is_err(),
            "duplicate function-local name should panic in codegen without analysis"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("collides with an existing entry in locals_map"),
            "unexpected error message: {err}"
        );
    }
}
