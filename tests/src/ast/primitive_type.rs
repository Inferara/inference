use crate::utils::{build_ast, find_function_by_name, parse_simple_type};
use inference_ast::ids::*;
use inference_ast::nodes::{ArgKind, Def, Expr, SimpleTypeKind, Stmt, TypeNode};

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

/// Tests for parsing source code with primitive types into `TypeNode::Simple` variants.

#[test]
fn test_parse_function_return_type_i32_is_simple() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "add").unwrap();
    if let Def::Function { returns, .. } = &arena[func_id].kind {
        let ret_ty = returns.expect("Should have return type");
        if let TypeNode::Simple(kind) = &arena[ret_ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I32));
            assert_eq!(kind.as_str(), "i32");
        } else {
            panic!("Expected TypeNode::Simple for i32 return type, got {:?}", arena[ret_ty].kind);
        }
    }
}

#[test]
fn test_parse_function_return_type_bool_is_simple() {
    let source = r#"fn is_valid() -> bool { return true; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "is_valid").unwrap();
    if let Def::Function { returns, .. } = &arena[func_id].kind {
        let ret_ty = returns.expect("Should have return type");
        if let TypeNode::Simple(kind) = &arena[ret_ty].kind {
            assert!(matches!(kind, SimpleTypeKind::Bool));
        } else {
            panic!("Expected TypeNode::Simple for bool return type");
        }
    }
}

#[test]
fn test_parse_function_return_type_i64_is_simple() {
    let source = r#"fn get_big() -> i64 { return 9223372036854775807; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "get_big").unwrap();
    if let Def::Function { returns, .. } = &arena[func_id].kind {
        let ret_ty = returns.expect("Should have return type");
        if let TypeNode::Simple(kind) = &arena[ret_ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I64));
        } else {
            panic!("Expected TypeNode::Simple for i64 return type");
        }
    }
}

#[test]
fn test_parse_function_argument_type_i32_is_simple() {
    let source = r#"fn process(x: i32) -> i32 { return x; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "process").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        assert_eq!(args.len(), 1);
        if let ArgKind::Named { ty, .. } = &args[0].kind {
            if let TypeNode::Simple(kind) = &arena[*ty].kind {
                assert!(matches!(kind, SimpleTypeKind::I32));
            } else {
                panic!("Expected TypeNode::Simple for argument type");
            }
        }
    }
}

#[test]
fn test_parse_variable_type_i32_is_simple() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        if let Stmt::VarDef { ty, .. } = &arena[block.stmts[0]].kind {
            if let TypeNode::Simple(kind) = &arena[*ty].kind {
                assert!(matches!(kind, SimpleTypeKind::I32));
            } else {
                panic!("Expected TypeNode::Simple for variable type, got {:?}", arena[*ty].kind);
            }
        }
    }
}

#[test]
fn test_parse_variable_type_bool_is_simple() {
    let source = r#"fn test() { let flag: bool = true; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        if let Stmt::VarDef { ty, .. } = &arena[block.stmts[0]].kind {
            if let TypeNode::Simple(kind) = &arena[*ty].kind {
                assert!(matches!(kind, SimpleTypeKind::Bool));
            } else {
                panic!("Expected TypeNode::Simple for variable type");
            }
        }
    }
}

#[test]
fn test_parse_constant_type_i32_is_simple() {
    let source = r#"const MAX: i32 = 100;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Constant { ty, .. } = &arena[def_id].kind {
        if let TypeNode::Simple(kind) = &arena[*ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I32));
        } else {
            panic!("Expected TypeNode::Simple for constant type");
        }
    }
}

#[test]
fn test_parse_constant_type_bool_is_simple() {
    let source = r#"const FLAG: bool = true;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Constant { ty, .. } = &arena[def_id].kind {
        if let TypeNode::Simple(kind) = &arena[*ty].kind {
            assert!(matches!(kind, SimpleTypeKind::Bool));
        } else {
            panic!("Expected TypeNode::Simple for constant type");
        }
    }
}

#[test]
fn test_parse_struct_field_type_i32_is_simple() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { fields, .. } = &arena[def_id].kind {
        assert_eq!(fields.len(), 2);
        for field in fields {
            if let TypeNode::Simple(kind) = &arena[field.ty].kind {
                assert!(matches!(kind, SimpleTypeKind::I32));
            } else {
                panic!("Expected TypeNode::Simple for struct field type");
            }
        }
    }
}

#[test]
fn test_parse_struct_field_type_bool_is_simple() {
    let source = r#"struct Flags { a: bool; b: bool; }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { fields, .. } = &arena[def_id].kind {
        assert_eq!(fields.len(), 2);
        for field in fields {
            if let TypeNode::Simple(kind) = &arena[field.ty].kind {
                assert!(matches!(kind, SimpleTypeKind::Bool));
            } else {
                panic!("Expected TypeNode::Simple for struct field type");
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

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        assert_eq!(args.len(), 4);

        let expected_types = [
            SimpleTypeKind::I8,
            SimpleTypeKind::I16,
            SimpleTypeKind::I32,
            SimpleTypeKind::I64,
        ];

        for (i, (arg, expected)) in args.iter().zip(expected_types.iter()).enumerate() {
            if let ArgKind::Named { ty, .. } = &arg.kind {
                if let TypeNode::Simple(kind) = &arena[*ty].kind {
                    assert_eq!(kind, expected, "Argument {i} type mismatch");
                } else {
                    panic!("Expected TypeNode::Simple for argument {i}");
                }
            }
        }
    }
}

#[test]
#[allow(unused_variables)]
fn test_parse_all_unsigned_integer_types() {
    let source = r#"fn test(a: u8, b: u16, c: u32, d: u64) {}"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        assert_eq!(args.len(), 4);

        let expected_types = [
            SimpleTypeKind::U8,
            SimpleTypeKind::U16,
            SimpleTypeKind::U32,
            SimpleTypeKind::U64,
        ];

        for (i, (arg, expected)) in args.iter().zip(expected_types.iter()).enumerate() {
            if let ArgKind::Named { ty, .. } = &arg.kind {
                if let TypeNode::Simple(kind) = &arena[*ty].kind {
                    assert_eq!(kind, expected, "Argument {i} type mismatch");
                } else {
                    panic!("Expected TypeNode::Simple for argument {i}");
                }
            }
        }
    }
}

/// Tests for custom types (non-primitive) to ensure they are NOT TypeNode::Simple.

#[test]
fn test_custom_type_is_not_simple() {
    let source = r#"struct Point { x: i32; }
fn test(p: Point) -> Point { return p; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, returns, .. } = &arena[func_id].kind {
        if let ArgKind::Named { ty, .. } = &args[0].kind {
            assert!(
                !matches!(&arena[*ty].kind, TypeNode::Simple(_)),
                "Custom type Point should not be TypeNode::Simple"
            );
            assert!(
                matches!(&arena[*ty].kind, TypeNode::Custom(_)),
                "Custom type Point should be TypeNode::Custom"
            );
        }

        let ret_ty = returns.expect("Should have return type");
        assert!(
            !matches!(&arena[ret_ty].kind, TypeNode::Simple(_)),
            "Custom return type Point should not be TypeNode::Simple"
        );
    }
}

#[test]
fn test_array_type_is_not_simple() {
    let source = r#"fn test(arr: [i32; 10]) {}"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        if let ArgKind::Named { ty, .. } = &args[0].kind {
            assert!(
                !matches!(&arena[*ty].kind, TypeNode::Simple(_)),
                "Array type should not be TypeNode::Simple"
            );
            assert!(
                matches!(&arena[*ty].kind, TypeNode::Array { .. }),
                "Array type should be TypeNode::Array"
            );
        }
    }
}

#[test]
fn test_array_element_type_is_simple() {
    let source = r#"fn test(arr: [i32; 10]) {}"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        if let ArgKind::Named { ty, .. } = &args[0].kind {
            if let TypeNode::Array { element, .. } = &arena[*ty].kind {
                if let TypeNode::Simple(kind) = &arena[*element].kind {
                    assert!(matches!(kind, SimpleTypeKind::I32));
                } else {
                    panic!("Array element type should be TypeNode::Simple");
                }
            }
        }
    }
}

/// Tests for external function types with primitives.

#[test]
fn test_external_function_return_type_is_simple() {
    let source = r#"external fn get_value() -> i64;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::ExternFunction { returns, .. } = &arena[def_id].kind {
        let ret_ty = returns.expect("Should have return type");
        if let TypeNode::Simple(kind) = &arena[ret_ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I64));
        } else {
            panic!("External function return type should be TypeNode::Simple");
        }
    }
}

/// Tests for type definitions with primitive types.

#[test]
fn test_type_alias_to_primitive_is_simple() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::TypeAlias { ty, .. } = &arena[def_id].kind {
        if let TypeNode::Simple(kind) = &arena[*ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I32));
        } else {
            panic!("Type alias should point to TypeNode::Simple");
        }
    }
}

/// Tests for function type parameters with primitive types.

#[test]
fn test_function_type_with_primitive_return() {
    let source = r#"fn apply(f: fn() -> i32) -> i32 { return f(); }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "apply").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        if let ArgKind::Named { ty, .. } = &args[0].kind {
            if let TypeNode::Function { ret, .. } = &arena[*ty].kind {
                let ret_ty = ret.expect("Should have return type");
                if let TypeNode::Simple(kind) = &arena[ret_ty].kind {
                    assert!(matches!(kind, SimpleTypeKind::I32));
                } else {
                    panic!("Function type return should be TypeNode::Simple, got {:?}", arena[ret_ty].kind);
                }
            } else {
                panic!("Expected function type for first argument");
            }
        }
    }
}

/// Tests for ignore arguments with primitive types.

#[test]
fn test_ignore_argument_type_is_simple() {
    let source = r#"fn test(_: i32) -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        assert_eq!(args.len(), 1);
        if let ArgKind::Ignored { ty } = &args[0].kind {
            if let TypeNode::Simple(kind) = &arena[*ty].kind {
                assert!(matches!(kind, SimpleTypeKind::I32));
            } else {
                panic!("Ignore argument type should be TypeNode::Simple");
            }
        } else {
            panic!("Expected Ignored argument kind");
        }
    }
}

/// Tests for mixed primitive and non-primitive types in same context.

#[test]
fn test_mixed_simple_and_custom_types_in_struct() {
    let source = r#"struct Mixed { x: i32; name: String; flag: bool; }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { fields, .. } = &arena[def_id].kind {
        assert_eq!(fields.len(), 3);

        if let TypeNode::Simple(kind) = &arena[fields[0].ty].kind {
            assert!(matches!(kind, SimpleTypeKind::I32));
        } else {
            panic!("First field should be simple");
        }

        assert!(
            matches!(&arena[fields[1].ty].kind, TypeNode::Custom(_)),
            "Second field should be custom type"
        );

        if let TypeNode::Simple(kind) = &arena[fields[2].ty].kind {
            assert!(matches!(kind, SimpleTypeKind::Bool));
        } else {
            panic!("Third field should be simple");
        }
    }
}

#[test]
fn test_mixed_simple_and_custom_types_in_function_args() {
    let source = r#"fn process(count: i32, name: String, active: bool) {}"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "process").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        assert_eq!(args.len(), 3);

        if let ArgKind::Named { ty, .. } = &args[0].kind {
            assert!(matches!(&arena[*ty].kind, TypeNode::Simple(_)));
        }
        if let ArgKind::Named { ty, .. } = &args[1].kind {
            assert!(matches!(&arena[*ty].kind, TypeNode::Custom(_)));
        }
        if let ArgKind::Named { ty, .. } = &args[2].kind {
            assert!(matches!(&arena[*ty].kind, TypeNode::Simple(_)));
        }
    }
}
