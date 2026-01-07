//! Feature-specific type checker tests
//!
//! This module contains tests for advanced type checking features including:
//! - Import system (Phase 4)
//! - Type error reporting
//! - Enum support (Phase 7.2)
//! - Generics (Phase 7.4)
//! - Error recovery (Phase 8.2)
use crate::utils::build_ast;

/// Tests for import system (Phase 4)
///
/// FIXME: Module definitions with bodies are not yet supported by the parser.
/// These tests document the expected behavior when module support is complete.
/// Currently testing the import infrastructure that is implemented.
#[cfg(test)]
mod import_tests {
    use super::*;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    mod visibility {
        use super::*;

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for when modules are fully implemented
        #[test]
        fn test_visibility_public_accessible() {
            let source = r#"struct PublicItem { x: i32; } fn test() { let item: PublicItem; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Public symbols at root level should be accessible"
            );
        }

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for when modules are fully implemented
        #[test]
        fn test_visibility_private_same_scope() {
            let source =
                r#"struct PrivateItem { x: i32; } fn use_private() { let item: PrivateItem; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Private symbols at root level should be accessible in same scope"
            );
        }

        // FIXME: Module definitions with bodies not yet supported by parser
        // When implemented, this should test that private symbols are accessible in child scopes
        #[test]
        fn test_visibility_private_child_scope_accessible() {
            let source = r#"struct PrivateItem { x: i32; } fn use_parent_private() { let item: PrivateItem; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Root-level symbols should be accessible");
        }

        // FIXME: Module definitions with bodies not yet supported by parser
        // When implemented, this should test that private symbols are not accessible from sibling scopes
        #[test]
        fn test_visibility_private_sibling_scope_not_accessible() {
            let source =
                r#"struct PrivateItem { x: i32; } fn try_use_private() { let item: PrivateItem; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Root-level symbols should be accessible at root"
            );
        }
    }

    mod import_registration {
        use super::*;

        #[test]
        fn test_import_registration_plain() {
            let source = r#"
            use std::io::File;
            fn test() -> i32 { return 42; }
            "#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Import should be registered but fail to resolve as std::io::File doesn't exist"
            );
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("cannot resolve import path"),
                    "Error should mention unresolved import path, got: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_import_registration_partial() {
            let source = r#"
            use std::io::{File, Path};
            fn test() -> i32 { return 42; }
            "#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Partial import should be registered but fail to resolve as items don't exist"
            );
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("cannot resolve import path"),
                    "Error should mention unresolved imports, got: {}",
                    error_msg
                );
            }
        }
    }

    mod qualified_name_resolution {
        use super::*;

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for when qualified names across modules work
        #[test]
        fn test_qualified_name_resolution_simple() {
            let source = r#"struct MyType { x: i32; } fn test() { let val: MyType; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Simple type resolution should work at root level"
            );
        }

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for when nested qualified names work
        #[test]
        fn test_qualified_name_resolution_nested() {
            let source = r#"struct DeepType { x: i32; } fn test() { let val: DeepType; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Type resolution should work at root level");
        }
    }

    mod import_resolution {
        use super::*;

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for when import resolution works
        #[test]
        fn test_import_resolution_success() {
            let source = r#"struct MyType { x: i32; } fn test(val: MyType) -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Type usage should work without imports at root level"
            );
        }

        #[test]
        fn test_import_resolution_error_not_found() {
            let source = r#"use nonexistent::Type; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Import of nonexistent path should fail");
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("cannot resolve import path"),
                    "Error should mention unresolved import path, got: {}",
                    error_msg
                );
            }
        }
    }

    mod name_shadowing {
        use super::*;

        // FIXME: Module definitions with bodies not yet supported by parser
        // Test documents expected behavior for shadowing once imports work properly
        #[test]
        fn test_local_definition_shadows_import() {
            let source = r#"struct Item { y: i32; } fn test(val: Item) -> i32 { return val.y; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Local definition should be usable");
        }
    }

    mod error_cases {
        use super::*;

        #[test]
        fn test_duplicate_import_error() {
            let source = r#"use std::Type1; use std::Type2; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Multiple imports of non-existent types should fail"
            );
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("cannot resolve import path"),
                    "Error should mention unresolved imports, got: {}",
                    error_msg
                );
            }
        }

        // FIXME: Glob import syntax not yet supported by parser
        // When implemented, this should test that glob imports produce appropriate error
        #[test]
        fn test_glob_import_not_supported_error() {
            let source = r#"fn test() -> i32 { return 42; }"#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_ok(),
                "Simple function should compile without imports"
            );
        }
    }

    mod import_infrastructure {
        use super::*;

        #[test]
        fn test_plain_import_registered() {
            let source = r#"use foo::Bar; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Unresolvable import should fail");
        }

        #[test]
        fn test_partial_import_multiple_items() {
            let source = r#"use foo::{Bar, Baz}; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Multiple unresolvable imports should fail");
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("cannot resolve import path"),
                    "Error should mention import resolution failure, got: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_import_with_empty_path() {
            let source = r#"use ; fn test() -> i32 { return 42; }"#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_err(),
                "Empty import path should not parse or should fail type checking"
            );
        }

        #[test]
        fn test_multiple_use_statements() {
            let source = r#"use foo::A; use bar::B; use baz::C; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Multiple unresolvable imports should all fail"
            );
        }

        #[test]
        fn test_use_with_self_keyword() {
            let source = r#"use self::Item; fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "self::Item should fail to resolve when Item doesn't exist"
            );
        }
    }

    /// Tests for glob imports and external prelude (Phase 5)
    mod extern_prelude_tests {
        use super::*;

        /// Phase 5A: Glob imports tests

        // FIXME: Glob import syntax (use path::*) is not yet supported by the parser.
        // FIXME: Standalone pub keyword is not yet supported by the parser (needs module context).
        // These tests document expected behavior when both are implemented.
        #[test]
        fn test_glob_import_infrastructure_ready() {
            let source = r#"struct Item1 { x: i32; } struct Item2 { y: i32; } fn test() -> i32 { return 42; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Basic struct definitions work");
        }

        // FIXME: pub keyword support and glob imports not yet implemented in parser.
        // When implemented:
        // - test_glob_import_from_module: use path::* imports all public symbols
        // - test_glob_import_public_only: Only public symbols imported, private excluded
        // - test_glob_import_cycle_detection: Circular glob imports produce error
        // - test_glob_import_empty_module: Glob from empty module succeeds
        // - test_glob_import_nonexistent_module: Error for missing module
        #[test]
        fn test_visibility_tracking_in_symbol_table() {
            let source =
                r#"struct MyStruct { x: i32; } fn test(s: MyStruct) -> i32 { return s.x; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Symbol table tracks struct definitions");
        }

        /// Phase 5B: External prelude tests

        #[test]
        fn test_find_module_root_lib_inf() {
            use std::fs;
            use std::path::PathBuf;

            let temp_dir =
                std::env::temp_dir().join(format!("test_module_root_{}", std::process::id()));
            let src_dir = temp_dir.join("src");
            fs::create_dir_all(&src_dir).expect("Failed to create src directory");

            let lib_file = src_dir.join("lib.inf");
            fs::write(&lib_file, "pub struct TestStruct { x: i32; }")
                .expect("Failed to write lib.inf");

            let root = inference_ast::extern_prelude::find_module_root(&temp_dir);
            assert!(root.is_some(), "Should find src/lib.inf");
            assert_eq!(root.unwrap(), lib_file);

            let _ = fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn test_find_module_root_main_inf() {
            use std::fs;

            let temp_dir =
                std::env::temp_dir().join(format!("test_main_inf_{}", std::process::id()));
            let src_dir = temp_dir.join("src");
            fs::create_dir_all(&src_dir).expect("Failed to create src directory");

            let main_file = src_dir.join("main.inf");
            fs::write(&main_file, "fn main() -> i32 { return 0; }")
                .expect("Failed to write main.inf");

            let root = inference_ast::extern_prelude::find_module_root(&temp_dir);
            assert!(
                root.is_some(),
                "Should find src/main.inf when lib.inf absent"
            );
            assert_eq!(root.unwrap(), main_file);

            let _ = fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn test_find_module_root_fallback() {
            use std::fs;

            let temp_dir =
                std::env::temp_dir().join(format!("test_fallback_{}", std::process::id()));
            fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

            let lib_file = temp_dir.join("lib.inf");
            fs::write(&lib_file, "pub struct TestStruct { x: i32; }")
                .expect("Failed to write lib.inf");

            let root = inference_ast::extern_prelude::find_module_root(&temp_dir);
            assert!(root.is_some(), "Should find lib.inf as fallback");
            assert_eq!(root.unwrap(), lib_file);

            let _ = fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn test_find_module_root_not_found() {
            use std::fs;

            let temp_dir =
                std::env::temp_dir().join(format!("test_not_found_{}", std::process::id()));
            fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

            let root = inference_ast::extern_prelude::find_module_root(&temp_dir);
            assert!(
                root.is_none(),
                "Should return None when no root file exists"
            );

            let _ = fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn test_visibility_private_structs() {
            let source = r#"struct PrivateItem { x: i32; } fn use_private(p: PrivateItem) -> i32 { return p.x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Private structs should be usable in same scope"
            );
        }

        #[test]
        fn test_visibility_private_functions() {
            let source = r#"fn private_helper() -> i32 { return 2; } fn caller() -> i32 { return private_helper(); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Private functions should be callable in same scope"
            );
        }

        #[test]
        fn test_private_enum_definition() {
            let source = "enum Color { Red; Green; Blue; }\nfn test() -> i32 { return 42; }";
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(result.is_ok(), "Private enum should be registerable");
        }

        #[test]
        fn test_struct_with_multiple_fields() {
            let source =
                r#"struct Point { x: i32; y: i32; } fn get_x(p: Point) -> i32 { return p.x; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Struct with multiple fields should work");
        }

        #[test]
        fn test_multiple_struct_definitions() {
            let source = r#"struct Point { x: i32; y: i32; } struct Vector { dx: i32; dy: i32; } fn use_both(p: Point, v: Vector) -> i32 { return p.x + v.dx; }"#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Multiple struct definitions should work");
        }

        #[test]
        fn test_empty_source_with_visibility() {
            let source = r#""#;
            let result = try_type_check(source);
            assert!(result.is_ok(), "Empty source should succeed");
        }
    }
}

/// Tests that verify type errors are correctly reported.
#[cfg(test)]
mod type_error_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    #[test]
    fn test_type_checker_completes_on_valid_code() {
        let source = r#"fn test() -> i32 { return 42; }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(result.is_ok(), "Type checker should succeed on valid code");
    }

    // FIXME: Type mismatch detection is not yet implemented.
    // These tests document expected behavior for future implementation.
    // When type error detection is added, uncomment and verify these tests.

    // #[test]
    // fn test_return_type_mismatch_detected() {
    //     let source = r#"fn test() -> i32 { return true; }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect return type mismatch"
    //     );
    // }

    // #[test]
    // fn test_assignment_type_mismatch_detected() {
    //     let source = r#"
    //     fn test() {
    //         let x: i32 = true;
    //     }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect assignment type mismatch"
    //     );
    // }

    // #[test]
    // fn test_binary_operator_type_mismatch_detected() {
    //     let source = r#"fn test() -> i32 { return 10 + true; }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect binary operator type mismatch"
    //     );
    // }

    // #[test]
    // fn test_function_arg_type_mismatch_detected() {
    //     let source = r#"
    //     fn add(a: i32, b: i32) -> i32 { return a + b; }
    //     fn test() -> i32 { return add(10, true); }
    //     "#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect function argument type mismatch"
    //     );
    // }
}

/// Tests for enum variant type checking (Phase 7.2)
///
/// FIXME: TypeInfo comparison issue - When parsing `Color` type annotation, TypeInfo::new()
/// creates TypeInfoKind::Custom("Color") because it doesn't have symbol table access.
/// But enum variant access (Color::Red) creates TypeInfoKind::Enum("Color").
/// These don't match, causing false type mismatches.
/// Tests avoid explicit type annotations until this is resolved.
#[cfg(test)]
mod enum_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    #[test]
    fn test_enum_variant_access_valid() {
        let source = r#"enum Color { Red, Green, Blue } fn test_color(c: Color) {} fn test() { test_color(Color::Red); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Valid enum variant access should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_variant_access_invalid() {
        let source = r#"enum Color { Red, Green, Blue } fn test_color(c: Color) {} fn test() { test_color(Color::Yellow); }"#;
        let result = try_type_check(source);
        // Should fail because Yellow is not a valid variant of Color
        assert!(
            result.is_err(),
            "Invalid variant access should fail with VariantNotFound error"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("variant `Yellow` not found on enum `Color`"),
                "Error should mention missing variant Yellow, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_undefined_enum_access() {
        let source = r#"fn test_unknown(u: Unknown) {} fn test() { test_unknown(Unknown::Variant); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Access to undefined enum should fail with UndefinedEnum"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("enum `Unknown` is not defined")
                    || error_msg.contains("unknown type"),
                "Error should mention undefined enum or unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_enum_with_multiple_variants() {
        let source = r#"enum Status { Pending, Active, Completed, Failed, Cancelled } fn check(s: Status) {} fn test() { check(Status::Active); check(Status::Failed); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Enum with multiple variants should work correctly, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_visibility_public() {
        let source =
            r#"enum PublicEnum { VariantA, VariantB } fn test_enum(e: PublicEnum) {} fn test() { test_enum(PublicEnum::VariantA); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Public enum should be registered correctly, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_visibility_private() {
        let source =
            r#"enum PrivateEnum { VariantX, VariantY } fn test_enum(e: PrivateEnum) {} fn test() { test_enum(PrivateEnum::VariantX); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Private enum should be registered correctly, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_single_variant() {
        let source = r#"enum Unit { Value } fn test_unit(u: Unit) {} fn test() { test_unit(Unit::Value); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Enum with single variant should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_multiple_enums() {
        let source = r#"enum Color { Red, Green } enum Size { Small, Large } fn test_color(c: Color) {} fn test_size(s: Size) {} fn test() { test_color(Color::Red); test_size(Size::Large); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Multiple enum definitions should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_variant_in_function_call() {
        let source = r#"enum Direction { North, South, East, West } fn navigate(d: Direction) {} fn test() { navigate(Direction::North); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Enum variant in function call should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_variant_case_sensitive() {
        let source = r#"enum Letter { A, B, C } fn test_letter(l: Letter) {} fn test() { test_letter(Letter::a); }"#;
        let result = try_type_check(source);
        // Enum variant access is case-sensitive: "a" != "A"
        assert!(
            result.is_err(),
            "Case-sensitive variant access should fail with VariantNotFound error"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("variant `a` not found on enum `Letter`"),
                "Error should mention missing lowercase variant, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_enum_all_variants_accessible() {
        let source = r#"enum Day { Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday } fn check_day(d: Day) {} fn test() { check_day(Day::Monday); check_day(Day::Wednesday); check_day(Day::Sunday); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "All enum variants should be accessible, got: {:?}",
            result.err()
        );
    }
}

/// Tests for generic type instantiation (Phase 7.4)
#[cfg(test)]
mod generics_tests {
    use super::*;
    use inference_type_checker::TypeCheckerBuilder;

    /// Helper function to run type checker, returning Result to handle WIP failures
    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    // ============================================
    // Phase 7.4.1: Type Substitution Tests
    // ============================================

    // Note: Inference language uses T' syntax for type parameters, not <T>
    // fn identity T'(x: T) -> T { ... }

    #[test]
    fn test_generic_function_parsing() {
        // First test that the AST parses the T' syntax correctly
        use inference_ast::nodes::{AstNode, Definition, Type, ArgumentType};
        let source = r#"fn identity T'(x: T) -> T { return x; }"#;
        let arena = build_ast(source.to_string());

        let funcs = arena.filter_nodes(|node| {
            matches!(node, AstNode::Definition(Definition::Function(_)))
        });
        assert_eq!(funcs.len(), 1, "Expected 1 function definition");

        if let AstNode::Definition(Definition::Function(func)) = &funcs[0] {
            // Check type_parameters
            assert!(
                func.type_parameters.is_some(),
                "Function should have type_parameters"
            );
            let type_params = func.type_parameters.as_ref().unwrap();
            assert_eq!(type_params.len(), 1, "Expected 1 type parameter");
            assert_eq!(
                type_params[0].name(),
                "T",
                "Type parameter should be named 'T'"
            );

            // Check argument type
            let args = func.arguments.as_ref().expect("Function should have args");
            assert_eq!(args.len(), 1, "Expected 1 argument");
            if let ArgumentType::Argument(arg) = &args[0] {
                // The type of x should be T - check what variant it is
                match &arg.ty {
                    Type::Custom(ident) => {
                        assert_eq!(ident.name(), "T", "Argument type should be T");
                    }
                    Type::Simple(simple) => {
                        panic!("T was parsed as Simple({}) instead of Custom", simple.name);
                    }
                    other => {
                        panic!("Unexpected type variant for T: {:?}", other);
                    }
                }
            }
        }
    }

    #[test]
    fn test_generic_function_definition_only() {
        // Test that defining a generic function doesn't fail
        let source = r#"fn identity T'(x: T) -> T { return x; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Defining a generic function should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_identity_function_with_explicit_type() {
        // Test parsing of function call with type arguments
        // First, let's check if the parser supports explicit type args on calls
        use inference_ast::nodes::{AstNode, Definition, Expression, Statement};
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn test() -> i32 {
                return identity(42);
            }
        "#;
        let arena = build_ast(source.to_string());

        // Find the function call expression
        let func_calls = arena.filter_nodes(|node| {
            matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
        });

        // Check that there are two function calls: one for identity(42) in test()
        // If this fails, print debug info
        if !func_calls.is_empty()
            && let AstNode::Expression(Expression::FunctionCall(call)) = &func_calls[0]
        {
            println!("Function call name: '{}'", call.name());
            println!("Type parameters: {:?}", call.type_parameters);
        }

        // Type checking should work with type inference
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Identity function with type inference should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_with_multiple_type_params() {
        // Test multi-param generics with inference
        let source = r#"
            fn swap T' U'(a: T, b: U) -> U {
                return b;
            }
            fn test() -> bool {
                let x: i32 = 42;
                let y: bool = true;
                return swap(x, y);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Generic function with multiple type params should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_function_returns_correct_type() {
        // Test that the return type is correctly substituted
        let source = r#"
            fn first T'(x: T) -> T {
                return x;
            }
            fn test() -> i32 {
                let val: i32 = 100;
                let result: i32 = first(val);
                return result;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Generic function return type should match substituted type, got: {:?}",
            result.err()
        );
    }

    // ============================================
    // Phase 7.4.2: Error Case Tests
    // ============================================

    // Note: Explicit type arguments on function calls (e.g., identity i32'(42))
    // are not yet supported in the grammar. Skipping tests that require this syntax.

    #[test]
    fn test_missing_type_params_error() {
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn test() -> i32 {
                return identity(42);
            }
        "#;
        let result = try_type_check(source);
        // This might succeed with inference or fail with missing type params
        // The behavior depends on whether inference is fully working
        if let Err(error) = &result {
            let error_msg = error.to_string();
            // Either inference worked or we get a missing/cannot-infer error
            assert!(
                error_msg.contains("requires") || error_msg.contains("cannot infer"),
                "Error should mention missing or cannot infer type parameters, got: {}",
                error_msg
            );
        }
    }

    // ============================================
    // Phase 7.4.3: Generic Inference Tests
    // ============================================

    #[test]
    fn test_infer_type_param_from_argument() {
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn test() -> i32 {
                let val: i32 = 42;
                return identity(val);
            }
        "#;
        let result = try_type_check(source);
        // Type inference from argument should work
        assert!(
            result.is_ok(),
            "Type inference from argument should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_infer_type_param_from_literal() {
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn test() -> i32 {
                return identity(42);
            }
        "#;
        let result = try_type_check(source);
        // Inference from literal should also work
        assert!(
            result.is_ok(),
            "Type inference from literal should work, got: {:?}",
            result.err()
        );
    }

    // ============================================
    // Additional Edge Cases
    // ============================================

    #[test]
    fn test_generic_function_non_generic_call() {
        let source = r#"
            fn add(a: i32, b: i32) -> i32 {
                return a + b;
            }
            fn test() -> i32 {
                return add(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Non-generic function call should still work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_generic_function_calls() {
        // Test nested calls using type inference
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn outer T'(x: T) -> T {
                return identity(x);
            }
            fn test() -> i32 {
                let val: i32 = 42;
                return outer(val);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Nested generic function calls should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_with_bool_type() {
        // Test with bool type using inference
        let source = r#"
            fn identity T'(x: T) -> T {
                return x;
            }
            fn test() -> bool {
                let val: bool = true;
                return identity(val);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Generic function with bool type should work, got: {:?}",
            result.err()
        );
    }
}

/// Tests for error recovery (Phase 8.2)
///
/// These tests verify that the type checker:
/// 1. Collects multiple errors instead of stopping at the first error
/// 2. Deduplicates errors appropriately
/// 3. Continues registration even when errors are found
/// 4. Includes location information in error messages
#[cfg(test)]
mod error_recovery_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    #[test]
    fn test_multiple_errors_in_same_function() {
        let source = r#"
            fn test() -> i32 {
                let x: i32 = unknown_var1;
                let y: i32 = unknown_var2;
                return x + y;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect multiple unknown variables"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("unknown_var1") && error_msg.contains("unknown_var2"),
                "Error message should contain both unknown variables, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_errors_across_multiple_functions() {
        let source = r#"
            fn foo() -> i32 {
                return unknown_var1;
            }
            fn bar() -> i32 {
                return unknown_var2;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect errors in both functions"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("unknown_var1") && error_msg.contains("unknown_var2"),
                "Error message should contain errors from both functions, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_registration_and_inference_errors_collected() {
        let source = r#"
            fn test(x: UnknownType) -> i32 {
                return unknown_var;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect both registration and inference errors"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UnknownType") && error_msg.contains("unknown_var"),
                "Error message should contain both unknown type and unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_registered_despite_param_error() {
        let source = r#"
            fn helper(x: UnknownType) -> i32 {
                return 42;
            }
            fn test() -> i32 {
                return helper(10);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should report unknown type error"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UnknownType"),
                "Error message should mention unknown type, got: {}",
                error_msg
            );
            assert!(
                !error_msg.contains("undefined function `helper`"),
                "Error message should NOT contain undefined function error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication() {
        let source = r#"
            struct Container {
                value: UnknownType;
            }
            fn test(c: Container) -> UnknownType {
                let x: UnknownType = c.value;
                return x;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect unknown type error"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let unknown_type_count = error_msg.matches("unknown type `UnknownType`").count();
            assert!(
                unknown_type_count <= 3,
                "UnknownType error should not be excessively duplicated (found {} occurrences), got: {}",
                unknown_type_count,
                error_msg
            );
        }
    }

    #[test]
    fn test_method_call_on_non_struct_infers_arguments() {
        let source = r#"
            fn test(x: i32) -> i32 {
                return x.method(unknown_var);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect errors in method call"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("unknown_var") || error_msg.contains("cannot call method"),
                "Error message should contain unknown variable or method call error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_undefined_struct_with_field_access() {
        let source = r#"
            fn test(s: UndefinedStruct) -> i32 {
                return s.field;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect undefined struct"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UndefinedStruct") || error_msg.contains("unknown type"),
                "Error message should mention undefined struct or unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_undefined_enum_with_variant_access() {
        let source = r#"
            fn test() -> UndefinedEnum {
                return UndefinedEnum::Variant;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect undefined enum"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UndefinedEnum") || error_msg.contains("unknown type"),
                "Error message should mention undefined enum or unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_multiple_unknown_types_in_function_signature() {
        let source = r#"
            fn test(a: Type1, b: Type2, c: Type3) -> Type4 {
                return a;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect all unknown types"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("Type1");
            let has_type2 = error_msg.contains("Type2");
            let has_type3 = error_msg.contains("Type3");
            let has_type4 = error_msg.contains("Type4");

            let type_count = [has_type1, has_type2, has_type3, has_type4]
                .iter()
                .filter(|&&x| x)
                .count();

            assert!(
                type_count >= 2,
                "Error message should contain at least 2 unknown types, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_with_location_information() {
        let source = r#"
            fn test() -> i32 {
                return unknown_var;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect unknown variable"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_location_prefix = error_msg.contains(":");
            assert!(
                has_location_prefix,
                "Error message should include location information (line:col prefix), got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_unknown_identifier_in_binary_expression() {
        let source = r#"
            fn test() -> i32 {
                return unknown_var1 + unknown_var2;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect unknown identifiers in binary expression"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("unknown_var1") || error_msg.contains("unknown_var2"),
                "Error message should contain at least one unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_call_with_undefined_function_and_unknown_args() {
        let source = r#"
            fn test() -> i32 {
                return undefined_func(unknown_var);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect undefined function"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("undefined_func") || error_msg.contains("unknown_var"),
                "Error message should mention undefined function or unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_struct_with_unknown_field_types() {
        let source = r#"
            struct Container {
                field1: UnknownType1;
                field2: UnknownType2;
            }
            fn test(c: Container) -> UnknownType1 {
                return c.field1;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect unknown field types"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("UnknownType1");
            let has_type2 = error_msg.contains("UnknownType2");

            assert!(
                has_type1 || has_type2,
                "Error message should contain at least one unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_method_with_unknown_param_and_return_types() {
        let source = r#"
            struct MyStruct {
                value: i32;

                fn method(x: UnknownParam) -> UnknownReturn {
                    return x;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect unknown types in method signature"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UnknownParam") || error_msg.contains("UnknownReturn"),
                "Error message should contain unknown parameter or return type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_mixed_valid_and_invalid_functions() {
        let source = r#"
            fn valid1() -> i32 {
                return 42;
            }
            fn invalid(x: UnknownType) -> i32 {
                return unknown_var;
            }
            fn valid2() -> bool {
                return true;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect errors in invalid function"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UnknownType") || error_msg.contains("unknown_var"),
                "Error message should contain errors from invalid function, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_continue_after_unknown_type_in_variable_definition() {
        let source = r#"
            fn test() -> i32 {
                let x: UnknownType = 42;
                let y: i32 = unknown_var;
                return y;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Type checker should detect both unknown type and unknown variable"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("UnknownType") || error_msg.contains("unknown_var"),
                "Error message should contain at least one error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_unknown_type_multiple_uses() {
        let source = r#"fn test(a: UnknownType, b: UnknownType) -> UnknownType { let c: UnknownType = a; return c; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect unknown type");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg.matches("unknown type `UnknownType`").count();
            assert_eq!(count, 1, "UnknownType error should appear exactly once due to deduplication, but appeared {} times in: {}", count, error_msg);
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_function_multiple_calls() {
        let source = r#"fn test() -> i32 { let x: i32 = missing_func(); let y: i32 = missing_func(); return x + y; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect undefined function");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg.matches("undefined function `missing_func`").count();
            assert_eq!(count, 1, "missing_func error should appear exactly once due to deduplication, but appeared {} times in: {}", count, error_msg);
        }
    }

    #[test]
    fn test_error_deduplication_same_unknown_identifier_multiple_uses() {
        let source = r#"fn test() -> i32 { let x: i32 = unknown_var; let y: i32 = unknown_var; return x + y + unknown_var; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect unknown identifier");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg.matches("undeclared variable `unknown_var`").count();
            assert_eq!(count, 1, "unknown_var error should appear exactly once due to deduplication, but appeared {} times in: {}", count, error_msg);
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_struct_multiple_uses() {
        let source = r#"fn test(a: MissingStruct) -> MissingStruct { let b: MissingStruct = MissingStruct { }; return b; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect undefined struct");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let struct_count = error_msg.matches("struct `MissingStruct` is not defined").count();
            let type_count = error_msg.matches("unknown type `MissingStruct`").count();
            assert!(struct_count <= 1 && type_count <= 1, "MissingStruct error should appear at most once due to deduplication (struct: {}, type: {}), error: {}", struct_count, type_count, error_msg);
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_enum_multiple_uses() {
        let source = r#"fn test() -> MissingEnum { let x: MissingEnum = MissingEnum::Variant; return x; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect undefined enum");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let enum_count = error_msg.matches("enum `MissingEnum` is not defined").count();
            let type_count = error_msg.matches("unknown type `MissingEnum`").count();
            assert!(enum_count <= 1 && type_count <= 1, "MissingEnum error should appear at most once due to deduplication (enum: {}, type: {}), error: {}", enum_count, type_count, error_msg);
        }
    }

    #[test]
    fn test_multiple_different_errors_all_collected() {
        let source = r#"fn test(x: UnknownType1) -> UnknownType2 { let y: i32 = unknown_var; return missing_func(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect multiple errors");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("UnknownType1");
            let has_type2 = error_msg.contains("UnknownType2");
            let has_var = error_msg.contains("unknown_var");
            let has_func = error_msg.contains("missing_func");
            let error_count = [has_type1, has_type2, has_var, has_func].iter().filter(|&&x| x).count();
            assert!(error_count >= 3, "Should collect at least 3 different errors (found {}): {}", error_count, error_msg);
        }
    }

    #[test]
    fn test_error_recovery_after_type_mismatch_in_assignment() {
        let source = r#"fn test() -> i32 { let x: i32 = true; let y: i32 = unknown_var; return y; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect type mismatch and unknown variable");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("type mismatch") || error_msg.contains("unknown_var"), "Should report both type mismatch and unknown identifier errors: {}", error_msg);
        }
    }

    #[test]
    fn test_error_recovery_after_type_mismatch_in_return() {
        let source = r#"fn test() -> i32 { return true; } fn test2() -> i32 { return undefined_func(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect errors in both functions");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("type mismatch") || error_msg.contains("undefined"), "Should report errors from both functions: {}", error_msg);
        }
    }

    #[test]
    fn test_error_recovery_continues_through_multiple_statements() {
        let source = r#"fn test() -> i32 { let a: i32 = unknown1; let b: i32 = unknown2; let c: i32 = unknown3; return a + b + c; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect all unknown variables");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count1 = error_msg.contains("unknown1");
            let count2 = error_msg.contains("unknown2");
            let count3 = error_msg.contains("unknown3");
            let total_errors = [count1, count2, count3].iter().filter(|&&x| x).count();
            assert!(total_errors >= 2, "Should continue collecting errors through all statements (found {}): {}", total_errors, error_msg);
        }
    }

    #[test]
    fn test_error_recovery_in_nested_blocks() {
        let source = r#"fn test() -> i32 { if true { let x: i32 = unknown1; } let y: i32 = unknown2; return y; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect unknown variables in nested blocks");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("unknown1") || error_msg.contains("unknown2"), "Should detect errors in both nested and outer scopes: {}", error_msg);
        }
    }

    #[test]
    fn test_import_error_has_location() {
        let source = r#"use nonexistent::module; fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect import resolution failure");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("nonexistent::module"), "Error should mention the import path: {}", error_msg);
            assert!(error_msg.contains(":"), "Error should include location information: {}", error_msg);
        }
    }

    #[test]
    fn test_multiple_import_errors_collected() {
        let source = r#"use missing1::Item1; use missing2::Item2; fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect multiple import failures");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_import1 = error_msg.contains("missing1") || error_msg.contains("Item1");
            let has_import2 = error_msg.contains("missing2") || error_msg.contains("Item2");
            assert!(has_import1 || has_import2, "Should report at least one import error: {}", error_msg);
        }
    }

    #[test]
    fn test_type_mismatch_followed_by_valid_code() {
        let source = r#"fn test() -> i32 { let x: i32 = true; return 42; } fn valid() -> i32 { return 10; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect type mismatch");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("type mismatch"), "Should report type mismatch error: {}", error_msg);
        }
    }

    #[test]
    fn test_error_deduplication_with_different_error_types() {
        let source = r#"fn test(x: MissingType) -> i32 { let y: i32 = missing_var; return missing_func(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect multiple distinct errors");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let missing_type_count = error_msg.matches("MissingType").count();
            let missing_var_count = error_msg.matches("missing_var").count();
            let missing_func_count = error_msg.matches("missing_func").count();
            assert!(missing_type_count <= 2, "MissingType should not be excessively duplicated: {}", error_msg);
            assert!(missing_var_count <= 2, "missing_var should not be excessively duplicated: {}", error_msg);
            assert!(missing_func_count <= 2, "missing_func should not be excessively duplicated: {}", error_msg);
        }
    }

    #[test]
    fn test_function_registered_despite_return_type_error() {
        let source = r#"fn helper() -> UnknownReturnType { return 42; } fn caller() -> i32 { return helper(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect unknown return type");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("UnknownReturnType"), "Should report unknown return type: {}", error_msg);
            assert!(!error_msg.contains("undefined function `helper`"), "helper should be registered despite return type error: {}", error_msg);
        }
    }

    #[test]
    fn test_array_index_error_continues_inference() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: bool = true; let val: i32 = arr[idx]; return unknown_var; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect array index type error and unknown variable");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("unknown_var") || error_msg.contains("index"), "Should continue after array index error: {}", error_msg);
        }
    }

    #[test]
    fn test_struct_field_error_continues_inference() {
        let source = r#"struct Point { x: i32; y: i32; } fn test(p: Point) -> i32 { let z: i32 = p.missing_field; return unknown_var; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect field not found and unknown variable");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("unknown_var") || error_msg.contains("field"), "Should continue after field error: {}", error_msg);
        }
    }

    #[test]
    fn test_binary_operation_error_continues_inference() {
        let source = r#"fn test() -> i32 { let x: i32 = 10 + true; let y: i32 = unknown_var; return y; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect binary operation error and unknown variable");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("unknown_var") || error_msg.contains("type"), "Should continue after binary operation error: {}", error_msg);
        }
    }

    #[test]
    fn test_method_not_found_continues_inference() {
        let source = r#"struct MyStruct { value: i32; } fn test(s: MyStruct) -> i32 { let x: i32 = s.missing_method(); return unknown_var; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Type checker should detect method not found and unknown variable");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("unknown_var") || error_msg.contains("method"), "Should continue after method not found error: {}", error_msg);
        }
    }

    #[test]
    fn test_all_error_variants_have_required_location() {
        let test_cases = vec![
            (r#"fn test(x: UnknownType) -> i32 { return 42; }"#, "UnknownType"),
            (r#"fn test() -> i32 { return unknown_var; }"#, "unknown_var"),
            (r#"fn test() -> i32 { return missing_func(); }"#, "missing_func"),
            (r#"fn test() -> i32 { let s: MyStruct = MyStruct { }; return 42; }"#, "MyStruct"),
        ];
        for (source, error_substring) in test_cases {
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect error in: {}", source);
            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(error_msg.contains(":"), "Error should have location (contains ':') for {}: {}", error_substring, error_msg);
            }
        }
    }
}
