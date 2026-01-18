use crate::utils::{build_ast, parse_simple_type};
use inference_ast::nodes::{AstNode, Definition, Expression, SimpleTypeKind, Statement, Type};

/// Tests for `SimpleTypeKind::as_str()` - verifies canonical string representations.

#[test]
fn test_simple_type_kind_as_str_unit() {
    assert_eq!(SimpleTypeKind::Unit.as_str(), "unit");
}

#[test]
fn test_simple_type_kind_as_str_bool() {
    assert_eq!(SimpleTypeKind::Bool.as_str(), "bool");
}

#[test]
fn test_simple_type_kind_as_str_i8() {
    assert_eq!(SimpleTypeKind::I8.as_str(), "i8");
}

#[test]
fn test_simple_type_kind_as_str_i16() {
    assert_eq!(SimpleTypeKind::I16.as_str(), "i16");
}

#[test]
fn test_simple_type_kind_as_str_i32() {
    assert_eq!(SimpleTypeKind::I32.as_str(), "i32");
}

#[test]
fn test_simple_type_kind_as_str_i64() {
    assert_eq!(SimpleTypeKind::I64.as_str(), "i64");
}

#[test]
fn test_simple_type_kind_as_str_u8() {
    assert_eq!(SimpleTypeKind::U8.as_str(), "u8");
}

#[test]
fn test_simple_type_kind_as_str_u16() {
    assert_eq!(SimpleTypeKind::U16.as_str(), "u16");
}

#[test]
fn test_simple_type_kind_as_str_u32() {
    assert_eq!(SimpleTypeKind::U32.as_str(), "u32");
}

#[test]
fn test_simple_type_kind_as_str_u64() {
    assert_eq!(SimpleTypeKind::U64.as_str(), "u64");
}

/// Tests for `SimpleTypeKind` trait implementations.

#[test]
fn test_simple_type_kind_clone() {
    let original = SimpleTypeKind::I32;
    let cloned = original;
    assert_eq!(original, cloned);
}

#[test]
fn test_simple_type_kind_copy() {
    let original = SimpleTypeKind::Bool;
    let copied = original;
    assert_eq!(original, copied);
    let another = original;
    assert_eq!(another, copied);
}

#[test]
fn test_simple_type_kind_eq() {
    assert_eq!(SimpleTypeKind::I32, SimpleTypeKind::I32);
    assert_eq!(SimpleTypeKind::Bool, SimpleTypeKind::Bool);
    assert_eq!(SimpleTypeKind::Unit, SimpleTypeKind::Unit);
}

#[test]
fn test_simple_type_kind_ne() {
    assert_ne!(SimpleTypeKind::I32, SimpleTypeKind::I64);
    assert_ne!(SimpleTypeKind::U8, SimpleTypeKind::I8);
    assert_ne!(SimpleTypeKind::Bool, SimpleTypeKind::Unit);
}

#[test]
fn test_simple_type_kind_debug() {
    let debug_str = format!("{:?}", SimpleTypeKind::I32);
    assert!(debug_str.contains("I32"));
}

#[test]
fn test_simple_type_kind_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_value<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }

    let hash1 = hash_value(&SimpleTypeKind::I32);
    let hash2 = hash_value(&SimpleTypeKind::I32);
    assert_eq!(hash1, hash2, "Same values should produce same hash");

    let hash3 = hash_value(&SimpleTypeKind::I64);
    assert_ne!(
        hash1, hash3,
        "Different values should produce different hashes"
    );
}

/// Tests for parsing source code with primitive types into `Type::Simple` variants.

#[test]
fn test_parse_function_return_type_i32_is_simple() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    if let Type::Simple(simple_node) = returns {
        assert!(matches!(simple_node, SimpleTypeKind::I32));
        assert_eq!(simple_node.as_str(), "i32");
    } else {
        panic!(
            "Expected Type::Simple for i32 return type, got {:?}",
            returns
        );
    }
}

#[test]
fn test_parse_function_return_type_bool_is_simple() {
    let source = r#"fn is_valid() -> bool { return true; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    if let Type::Simple(simple_node) = returns {
        assert!(matches!(simple_node, SimpleTypeKind::Bool));
    } else {
        panic!(
            "Expected Type::Simple for bool return type, got {:?}",
            returns
        );
    }
}

#[test]
fn test_parse_function_return_type_i64_is_simple() {
    let source = r#"fn get_big() -> i64 { return 9223372036854775807; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    if let Type::Simple(simple_node) = returns {
        assert!(matches!(simple_node, SimpleTypeKind::I64));
    } else {
        panic!(
            "Expected Type::Simple for i64 return type, got {:?}",
            returns
        );
    }
}

#[test]
fn test_parse_function_argument_type_i32_is_simple() {
    let source = r#"fn process(x: i32) -> i32 { return x; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    assert_eq!(args.len(), 1);

    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        if let Type::Simple(simple_node) = &arg.ty {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!("Expected Type::Simple for argument type");
        }
    } else {
        panic!("Expected Argument type");
    }
}

#[test]
fn test_parse_variable_type_i32_is_simple() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());

    let var_defs = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));
    assert_eq!(var_defs.len(), 1);

    if let AstNode::Statement(Statement::VariableDefinition(var_def)) = &var_defs[0] {
        if let Type::Simple(simple_node) = &var_def.ty {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!(
                "Expected Type::Simple for variable type, got {:?}",
                var_def.ty
            );
        }
    }
}

#[test]
fn test_parse_variable_type_bool_is_simple() {
    let source = r#"fn test() { let flag: bool = true; }"#;
    let arena = build_ast(source.to_string());

    let var_defs = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));
    assert_eq!(var_defs.len(), 1);

    if let AstNode::Statement(Statement::VariableDefinition(var_def)) = &var_defs[0] {
        if let Type::Simple(simple_node) = &var_def.ty {
            assert!(matches!(simple_node, SimpleTypeKind::Bool));
        } else {
            panic!("Expected Type::Simple for variable type");
        }
    }
}

#[test]
fn test_parse_constant_type_i32_is_simple() {
    let source = r#"const MAX: i32 = 100;"#;
    let arena = build_ast(source.to_string());

    let const_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(const_defs.len(), 1);

    if let AstNode::Definition(Definition::Constant(const_def)) = &const_defs[0] {
        if let Type::Simple(simple_node) = &const_def.ty {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!(
                "Expected Type::Simple for constant type, got {:?}",
                const_def.ty
            );
        }
    }
}

#[test]
fn test_parse_constant_type_bool_is_simple() {
    let source = r#"const FLAG: bool = true;"#;
    let arena = build_ast(source.to_string());

    let const_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(const_defs.len(), 1);

    if let AstNode::Definition(Definition::Constant(const_def)) = &const_defs[0] {
        if let Type::Simple(simple_node) = &const_def.ty {
            assert!(matches!(simple_node, SimpleTypeKind::Bool));
        } else {
            panic!("Expected Type::Simple for constant type");
        }
    }
}

#[test]
fn test_parse_struct_field_type_i32_is_simple() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());

    let struct_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(struct_defs.len(), 1);

    if let AstNode::Definition(Definition::Struct(struct_def)) = &struct_defs[0] {
        assert_eq!(struct_def.fields.len(), 2);
        for field in &struct_def.fields {
            if let Type::Simple(simple_node) = &field.type_ {
                assert!(matches!(simple_node, SimpleTypeKind::I32));
            } else {
                panic!("Expected Type::Simple for struct field type");
            }
        }
    }
}

#[test]
fn test_parse_struct_field_type_bool_is_simple() {
    let source = r#"struct Flags { a: bool; b: bool; }"#;
    let arena = build_ast(source.to_string());

    let struct_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(struct_defs.len(), 1);

    if let AstNode::Definition(Definition::Struct(struct_def)) = &struct_defs[0] {
        assert_eq!(struct_def.fields.len(), 2);
        for field in &struct_def.fields {
            if let Type::Simple(simple_node) = &field.type_ {
                assert!(matches!(simple_node, SimpleTypeKind::Bool));
            } else {
                panic!("Expected Type::Simple for struct field type");
            }
        }
    }
}

/// Tests for all primitive types being parsed correctly.

#[test]
#[allow(unused_variables)]
fn test_parse_all_signed_integer_types() {
    let source = r#"fn test(a: i8, b: i16, c: i32, d: i64) {}"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    assert_eq!(args.len(), 4);

    let expected_types = [
        SimpleTypeKind::I8,
        SimpleTypeKind::I16,
        SimpleTypeKind::I32,
        SimpleTypeKind::I64,
    ];

    for (i, (arg, expected)) in args.iter().zip(expected_types.iter()).enumerate() {
        if let inference_ast::nodes::ArgumentType::Argument(arg) = arg {
            if let Type::Simple(simple_node) = &arg.ty {
                assert!(matches!(simple_node, expected));
            } else {
                panic!("Expected Type::Simple for argument {}", i);
            }
        }
    }
}

#[test]
#[allow(unused_variables)]
fn test_parse_all_unsigned_integer_types() {
    let source = r#"fn test(a: u8, b: u16, c: u32, d: u64) {}"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    assert_eq!(args.len(), 4);

    let expected_types = [
        SimpleTypeKind::U8,
        SimpleTypeKind::U16,
        SimpleTypeKind::U32,
        SimpleTypeKind::U64,
    ];

    for (i, (arg, expected)) in args.iter().zip(expected_types.iter()).enumerate() {
        if let inference_ast::nodes::ArgumentType::Argument(arg) = arg {
            if let Type::Simple(simple_node) = &arg.ty {
                assert!(matches!(simple_node, expected));
            } else {
                panic!("Expected Type::Simple for argument {}", i);
            }
        }
    }
}

/// Tests for custom types (non-primitive) to ensure they are NOT Type::Simple.

#[test]
fn test_custom_type_is_not_simple() {
    let source = r#"struct Point { x: i32; }
fn test(p: Point) -> Point { return p; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        assert!(
            !matches!(&arg.ty, Type::Simple(_)),
            "Custom type Point should not be Type::Simple"
        );
        assert!(
            matches!(&arg.ty, Type::Custom(_)),
            "Custom type Point should be Type::Custom"
        );
    }

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    assert!(
        !matches!(returns, Type::Simple(_)),
        "Custom return type Point should not be Type::Simple"
    );
}

#[test]
fn test_array_type_is_not_simple() {
    let source = r#"fn test(arr: [i32; 10]) {}"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        assert!(
            !matches!(&arg.ty, Type::Simple(_)),
            "Array type should not be Type::Simple"
        );
        assert!(
            matches!(&arg.ty, Type::Array(_)),
            "Array type should be Type::Array"
        );
    }
}

#[test]
fn test_array_element_type_is_simple() {
    let source = r#"fn test(arr: [i32; 10]) {}"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0]
        && let Type::Array(arr_type) = &arg.ty
    {
        if let Type::Simple(simple_node) = &arr_type.element_type {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!("Array element type should be Type::Simple");
        }
    }
}

/// Tests for external function types with primitives.

#[test]
fn test_external_function_return_type_is_simple() {
    let source = r#"external fn get_value() -> i64;"#;
    let arena = build_ast(source.to_string());

    let ext_funcs = arena
        .filter_nodes(|node| matches!(node, AstNode::Definition(Definition::ExternalFunction(_))));
    assert_eq!(ext_funcs.len(), 1);

    if let AstNode::Definition(Definition::ExternalFunction(ext_func)) = &ext_funcs[0] {
        let returns = ext_func.returns.as_ref().expect("Should have return type");
        if let Type::Simple(simple_node) = returns {
            assert!(matches!(simple_node, SimpleTypeKind::I64));
        } else {
            panic!("External function return type should be Type::Simple");
        }
    }
}

/// Tests for type definitions with primitive types.

#[test]
fn test_type_alias_to_primitive_is_simple() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());

    let type_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Type(_))));
    assert_eq!(type_defs.len(), 1);

    if let AstNode::Definition(Definition::Type(type_def)) = &type_defs[0] {
        if let Type::Simple(simple_node) = &type_def.ty {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!("Type alias should point to Type::Simple");
        }
    }
}

/// Tests for function type parameters with primitive types.

#[test]
fn test_function_type_with_primitive_return() {
    let source = r#"fn apply(f: fn() -> i32) -> i32 { return f(); }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        if let Type::Function(fn_type) = &arg.ty {
            let returns = fn_type.returns.as_ref().expect("Should have return type");
            if let Type::Simple(simple_node) = returns {
                assert!(matches!(simple_node, SimpleTypeKind::I32));
            } else {
                panic!(
                    "Function type return should be Type::Simple, got {:?}",
                    returns
                );
            }
        } else {
            panic!("Expected function type for first argument");
        }
    }
}

/// Tests for ignore arguments with primitive types.

#[test]
fn test_ignore_argument_type_is_simple() {
    let source = r#"fn test(_: i32) -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());

    let ignore_args = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::ArgumentType(inference_ast::nodes::ArgumentType::IgnoreArgument(_))
        )
    });
    assert_eq!(ignore_args.len(), 1);

    if let AstNode::ArgumentType(inference_ast::nodes::ArgumentType::IgnoreArgument(ignore_arg)) =
        &ignore_args[0]
    {
        if let Type::Simple(simple_node) = &ignore_arg.ty {
            assert!(matches!(simple_node, SimpleTypeKind::I32));
        } else {
            panic!("Ignore argument type should be Type::Simple");
        }
    }
}

/// Tests for mixed primitive and non-primitive types in same context.

#[test]
fn test_mixed_simple_and_custom_types_in_struct() {
    let source = r#"struct Mixed { x: i32; name: String; flag: bool; }"#;
    let arena = build_ast(source.to_string());

    let struct_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));

    if let AstNode::Definition(Definition::Struct(struct_def)) = &struct_defs[0] {
        assert_eq!(struct_def.fields.len(), 3);

        if let Type::Simple(simple) = &struct_def.fields[0].type_ {
            assert!(matches!(simple, SimpleTypeKind::I32));
        } else {
            panic!("First field should be simple");
        }

        assert!(
            matches!(&struct_def.fields[1].type_, Type::Custom(_)),
            "Second field should be custom type"
        );

        if let Type::Simple(simple) = &struct_def.fields[2].type_ {
            assert!(matches!(simple, SimpleTypeKind::Bool));
        } else {
            panic!("Third field should be simple");
        }
    }
}

#[test]
fn test_mixed_simple_and_custom_types_in_function_args() {
    let source = r#"fn process(count: i32, name: String, active: bool) {}"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();

    let args = functions[0]
        .arguments
        .as_ref()
        .expect("Should have arguments");
    assert_eq!(args.len(), 3);

    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        assert!(matches!(&arg.ty, Type::Simple(_)));
    }
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[1] {
        assert!(matches!(&arg.ty, Type::Custom(_)));
    }
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[2] {
        assert!(matches!(&arg.ty, Type::Simple(_)));
    }
}

/// Tests for Type enum id() and location() methods with Simple variant.

#[test]
fn test_type_simple_id_method() {
    let source = r#"fn test() -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    let id = returns.id();
    assert!(id > 0, "Type::Simple should return valid id");
}

#[test]
fn test_type_simple_method() {
    let source = r#"fn test() -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();

    let returns = functions[0]
        .returns
        .as_ref()
        .expect("Should have return type");
    matches!(returns, Type::Simple(SimpleTypeKind::I32));
}
