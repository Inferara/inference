//! Unit tests for TypeInfo methods
//!
//! These tests focus on TypeInfo internals without requiring integration context.
//! They complement the integration tests in tests/src/type_checker/ which test
//! end-to-end type checking with source code parsing.

use inference_ast::arena::AstArena;
use inference_ast::nodes::{
    Expr, ExprData, Ident, Location, SimpleTypeKind, TypeData, TypeNode,
};
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};
use rustc_hash::FxHashMap;

mod type_info_construction {
    use super::*;

    #[test]
    fn test_boolean_constructor() {
        let ti = TypeInfo::boolean();
        assert!(ti.is_bool());
        assert!(!ti.is_number());
        assert!(!ti.is_array());
        assert!(!ti.is_struct());
        assert!(!ti.is_generic());
    }

    #[test]
    fn test_string_constructor() {
        let ti = TypeInfo::string();
        assert!(matches!(ti.kind, TypeInfoKind::String));
        assert!(!ti.is_bool());
        assert!(!ti.is_number());
    }

    #[test]
    fn test_default_is_unit() {
        let ti = TypeInfo::default();
        assert!(matches!(ti.kind, TypeInfoKind::Unit));
        assert!(ti.type_params.is_empty());
    }

    #[test]
    fn test_clone() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec!["T".to_string()],
        };
        let cloned = ti.clone();
        assert_eq!(ti, cloned);
    }
}

mod type_info_predicates {
    use super::*;

    #[test]
    fn test_is_number_for_all_numeric_types() {
        let numeric_kinds = [
            NumberType::I8,
            NumberType::I16,
            NumberType::I32,
            NumberType::I64,
            NumberType::U8,
            NumberType::U16,
            NumberType::U32,
            NumberType::U64,
        ];

        for kind in numeric_kinds {
            let ti = TypeInfo {
                kind: TypeInfoKind::Number(kind),
                type_params: vec![],
            };
            assert!(ti.is_number(), "Expected {:?} to be a number", kind);
        }
    }

    #[test]
    fn test_is_array() {
        let element = TypeInfo::boolean();
        let array_type = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(element), 10),
            type_params: vec![],
        };
        assert!(array_type.is_array());
        assert!(!array_type.is_number());
    }

    #[test]
    fn test_is_struct() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string(), "Point".to_string()),
            type_params: vec![],
        };
        assert!(struct_type.is_struct());
        assert!(!struct_type.is_bool());
    }

    #[test]
    fn test_is_generic() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        assert!(generic.is_generic());
        assert!(!TypeInfo::boolean().is_generic());
    }

    #[test]
    fn test_non_numeric_types_are_not_numbers() {
        let non_numeric = vec![
            TypeInfo::boolean(),
            TypeInfo::string(),
            TypeInfo::default(),
            TypeInfo {
                kind: TypeInfoKind::Struct("Foo".to_string(), "Foo".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Enum("Color".to_string(), "Color".to_string()),
                type_params: vec![],
            },
        ];

        for ti in non_numeric {
            assert!(!ti.is_number(), "Expected {:?} to not be a number", ti.kind);
        }
    }
}

mod type_substitution {
    use super::*;

    #[test]
    fn test_substitute_generic_type() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = generic.substitute(&subs);
        assert!(result.is_bool());
    }

    #[test]
    fn test_substitute_unknown_generic_unchanged() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("U".to_string()),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = generic.substitute(&subs);
        assert!(result.is_generic());
        if let TypeInfoKind::Generic(name) = &result.kind {
            assert_eq!(name, "U");
        } else {
            panic!("Expected generic type");
        }
    }

    #[test]
    fn test_substitute_array_element() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Generic("T".to_string()),
                    type_params: vec![],
                }),
                10,
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert(
            "T".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
        );

        let result = array.substitute(&subs);
        if let TypeInfoKind::Array(elem, _) = &result.kind {
            assert!(elem.is_number());
        } else {
            panic!("Expected array type");
        }
    }

    #[test]
    fn test_substitute_array_preserves_length() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Generic("T".to_string()),
                    type_params: vec![],
                }),
                5,
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = array.substitute(&subs);
        if let TypeInfoKind::Array(elem, length) = &result.kind {
            assert!(elem.is_bool());
            assert_eq!(*length, 5);
        } else {
            panic!("Expected array type");
        }
    }

    #[test]
    fn test_substitute_primitive_unchanged() {
        let bool_type = TypeInfo::boolean();
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::string());

        let result = bool_type.substitute(&subs);
        assert!(result.is_bool());
    }

    #[test]
    fn test_substitute_empty_map() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        let subs = FxHashMap::default();

        let result = generic.substitute(&subs);
        assert!(result.is_generic());
    }

    #[test]
    fn test_substitute_nested_array() {
        let nested_array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Array(
                        Box::new(TypeInfo {
                            kind: TypeInfoKind::Generic("T".to_string()),
                            type_params: vec![],
                        }),
                        10,
                    ),
                    type_params: vec![],
                }),
                10,
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = nested_array.substitute(&subs);
        if let TypeInfoKind::Array(outer_elem, outer_len) = &result.kind {
            assert_eq!(*outer_len, 10);
            if let TypeInfoKind::Array(inner_elem, _) = &outer_elem.kind {
                assert!(inner_elem.is_bool());
            } else {
                panic!("Expected inner array");
            }
        } else {
            panic!("Expected outer array");
        }
    }
}

mod has_unresolved_params {
    use super::*;

    #[test]
    fn test_generic_has_unresolved() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        assert!(generic.has_unresolved_params());
    }

    #[test]
    fn test_primitive_no_unresolved() {
        assert!(!TypeInfo::boolean().has_unresolved_params());
        assert!(!TypeInfo::string().has_unresolved_params());
        assert!(!TypeInfo::default().has_unresolved_params());
    }

    #[test]
    fn test_numeric_no_unresolved() {
        let i32_type = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        };
        assert!(!i32_type.has_unresolved_params());
    }

    #[test]
    fn test_array_with_generic_element() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Generic("T".to_string()),
                    type_params: vec![],
                }),
                10,
            ),
            type_params: vec![],
        };
        assert!(array.has_unresolved_params());
    }

    #[test]
    fn test_array_with_concrete_element() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), 5),
            type_params: vec![],
        };
        assert!(!array.has_unresolved_params());
    }

    #[test]
    fn test_nested_array_with_generic() {
        let nested = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Array(
                        Box::new(TypeInfo {
                            kind: TypeInfoKind::Generic("T".to_string()),
                            type_params: vec![],
                        }),
                        10,
                    ),
                    type_params: vec![],
                }),
                10,
            ),
            type_params: vec![],
        };
        assert!(nested.has_unresolved_params());
    }

    #[test]
    fn test_struct_no_unresolved() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string(), "Point".to_string()),
            type_params: vec![],
        };
        assert!(!struct_type.has_unresolved_params());
    }

    #[test]
    fn test_enum_no_unresolved() {
        let enum_type = TypeInfo {
            kind: TypeInfoKind::Enum("Color".to_string(), "Color".to_string()),
            type_params: vec![],
        };
        assert!(!enum_type.has_unresolved_params());
    }
}

mod display {
    use super::*;

    #[test]
    fn test_display_unit() {
        let ti = TypeInfo::default();
        assert_eq!(ti.to_string(), "Unit");
    }

    #[test]
    fn test_display_bool() {
        let ti = TypeInfo::boolean();
        assert_eq!(ti.to_string(), "Bool");
    }

    #[test]
    fn test_display_string() {
        let ti = TypeInfo::string();
        assert_eq!(ti.to_string(), "String");
    }

    #[test]
    fn test_display_i32() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        };
        assert_eq!(ti.to_string(), "i32");
    }

    #[test]
    fn test_display_all_numeric_types() {
        let cases = [
            (NumberType::I8, "i8"),
            (NumberType::I16, "i16"),
            (NumberType::I32, "i32"),
            (NumberType::I64, "i64"),
            (NumberType::U8, "u8"),
            (NumberType::U16, "u16"),
            (NumberType::U32, "u32"),
            (NumberType::U64, "u64"),
        ];

        for (kind, expected) in cases {
            let ti = TypeInfo {
                kind: TypeInfoKind::Number(kind),
                type_params: vec![],
            };
            assert_eq!(ti.to_string(), expected);
        }
    }

    #[test]
    fn test_display_array() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), 10),
            type_params: vec![],
        };
        assert_eq!(array.to_string(), "[Bool; 10]");
    }

    #[test]
    fn test_display_generic() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        assert_eq!(generic.to_string(), "T'");
    }

    #[test]
    fn test_display_struct() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string(), "Point".to_string()),
            type_params: vec![],
        };
        assert_eq!(struct_type.to_string(), "Point");
    }

    #[test]
    fn test_display_enum() {
        let enum_type = TypeInfo {
            kind: TypeInfoKind::Enum("Color".to_string(), "Color".to_string()),
            type_params: vec![],
        };
        assert_eq!(enum_type.to_string(), "Color");
    }

    #[test]
    fn test_display_with_type_params() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Struct("Vec".to_string(), "Vec".to_string()),
            type_params: vec!["T".to_string()],
        };
        assert_eq!(ti.to_string(), "Vec T'");
    }

    #[test]
    fn test_display_with_multiple_type_params() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Struct("Map".to_string(), "Map".to_string()),
            type_params: vec!["K".to_string(), "V".to_string()],
        };
        assert_eq!(ti.to_string(), "Map K' V'");
    }

    #[test]
    fn test_display_custom() {
        let custom = TypeInfo {
            kind: TypeInfoKind::Custom("MyType".to_string()),
            type_params: vec![],
        };
        assert_eq!(custom.to_string(), "MyType");
    }

    #[test]
    fn test_display_spec() {
        let spec = TypeInfo {
            kind: TypeInfoKind::Spec("Printable".to_string()),
            type_params: vec![],
        };
        assert_eq!(spec.to_string(), "Printable");
    }

    #[test]
    fn test_display_function() {
        let func = TypeInfo {
            kind: TypeInfoKind::Function("fn(i32) -> bool".to_string()),
            type_params: vec![],
        };
        assert_eq!(func.to_string(), "fn(i32) -> bool");
    }

    #[test]
    fn test_display_nested_array() {
        let nested = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), 5),
                    type_params: vec![],
                }),
                10,
            ),
            type_params: vec![],
        };
        assert_eq!(nested.to_string(), "[[Bool; 5]; 10]");
    }

    #[test]
    fn test_display_qualified_name() {
        let qualified_name = TypeInfo {
            kind: TypeInfoKind::QualifiedName("std::vec::Vec".to_string()),
            type_params: vec![],
        };
        assert_eq!(qualified_name.to_string(), "std::vec::Vec");
    }

    #[test]
    fn test_display_qualified() {
        let qualified = TypeInfo {
            kind: TypeInfoKind::Qualified("MyModule::MyType".to_string()),
            type_params: vec![],
        };
        assert_eq!(qualified.to_string(), "MyModule::MyType");
    }
}

mod type_info_kind {
    use super::*;

    #[test]
    fn test_kind_is_number() {
        let numeric_kind = TypeInfoKind::Number(NumberType::I32);
        assert!(numeric_kind.is_number());

        let bool_kind = TypeInfoKind::Bool;
        assert!(!bool_kind.is_number());
    }

    #[test]
    fn test_kind_equality() {
        let kind1 = TypeInfoKind::Number(NumberType::I32);
        let kind2 = TypeInfoKind::Number(NumberType::I32);
        let kind3 = TypeInfoKind::Number(NumberType::I64);

        assert_eq!(kind1, kind2);
        assert_ne!(kind1, kind3);
    }

    #[test]
    fn test_kind_hash() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let kind1 = TypeInfoKind::Bool;
        let kind2 = TypeInfoKind::Bool;

        let mut hasher1 = DefaultHasher::new();
        kind1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        kind2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    }
}

mod number_type_methods {
    use super::*;

    #[test]
    fn test_all_contains_all_variants() {
        assert_eq!(NumberType::ALL.len(), 8);
        assert!(NumberType::ALL.contains(&NumberType::I8));
        assert!(NumberType::ALL.contains(&NumberType::I16));
        assert!(NumberType::ALL.contains(&NumberType::I32));
        assert!(NumberType::ALL.contains(&NumberType::I64));
        assert!(NumberType::ALL.contains(&NumberType::U8));
        assert!(NumberType::ALL.contains(&NumberType::U16));
        assert!(NumberType::ALL.contains(&NumberType::U32));
        assert!(NumberType::ALL.contains(&NumberType::U64));
    }

    #[test]
    fn test_as_str_returns_correct_names() {
        assert_eq!(NumberType::I8.as_str(), "i8");
        assert_eq!(NumberType::I16.as_str(), "i16");
        assert_eq!(NumberType::I32.as_str(), "i32");
        assert_eq!(NumberType::I64.as_str(), "i64");
        assert_eq!(NumberType::U8.as_str(), "u8");
        assert_eq!(NumberType::U16.as_str(), "u16");
        assert_eq!(NumberType::U32.as_str(), "u32");
        assert_eq!(NumberType::U64.as_str(), "u64");
    }

    #[test]
    fn test_as_str_roundtrip_through_all() {
        for nt in NumberType::ALL {
            let s = nt.as_str();
            let parsed: NumberType = s.parse().expect("should parse back");
            assert_eq!(*nt, parsed);
        }
    }

    #[test]
    fn test_from_str_lowercase() {
        assert_eq!("i8".parse::<NumberType>(), Ok(NumberType::I8));
        assert_eq!("i16".parse::<NumberType>(), Ok(NumberType::I16));
        assert_eq!("i32".parse::<NumberType>(), Ok(NumberType::I32));
        assert_eq!("i64".parse::<NumberType>(), Ok(NumberType::I64));
        assert_eq!("u8".parse::<NumberType>(), Ok(NumberType::U8));
        assert_eq!("u16".parse::<NumberType>(), Ok(NumberType::U16));
        assert_eq!("u32".parse::<NumberType>(), Ok(NumberType::U32));
        assert_eq!("u64".parse::<NumberType>(), Ok(NumberType::U64));
    }

    #[test]
    fn test_from_str_case_sensitive() {
        assert!("I8".parse::<NumberType>().is_err());
        assert!("I16".parse::<NumberType>().is_err());
        assert!("I32".parse::<NumberType>().is_err());
        assert!("I64".parse::<NumberType>().is_err());
        assert!("U8".parse::<NumberType>().is_err());
        assert!("U16".parse::<NumberType>().is_err());
        assert!("U32".parse::<NumberType>().is_err());
        assert!("U64".parse::<NumberType>().is_err());
    }

    #[test]
    fn test_from_str_invalid() {
        assert!("invalid".parse::<NumberType>().is_err());
        assert!("f32".parse::<NumberType>().is_err());
        assert!("i128".parse::<NumberType>().is_err());
        assert!("".parse::<NumberType>().is_err());
    }
}

mod type_info_kind_builtin_methods {
    use super::*;

    #[test]
    fn test_non_numeric_builtins_contains_all() {
        assert_eq!(TypeInfoKind::NON_NUMERIC_BUILTINS.len(), 4);

        let names: Vec<&str> = TypeInfoKind::NON_NUMERIC_BUILTINS
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(names.contains(&"unit"));
        assert!(names.contains(&"bool"));
        assert!(names.contains(&"string"));
        assert!(names.contains(&"String"));
    }

    #[test]
    fn test_as_builtin_str_for_unit() {
        assert_eq!(TypeInfoKind::Unit.as_builtin_str(), Some("unit"));
    }

    #[test]
    fn test_as_builtin_str_for_bool() {
        assert_eq!(TypeInfoKind::Bool.as_builtin_str(), Some("bool"));
    }

    #[test]
    fn test_as_builtin_str_for_string() {
        assert_eq!(TypeInfoKind::String.as_builtin_str(), Some("string"));
    }

    #[test]
    fn test_as_builtin_str_for_numbers() {
        for nt in NumberType::ALL {
            let kind = TypeInfoKind::Number(*nt);
            assert_eq!(kind.as_builtin_str(), Some(nt.as_str()));
        }
    }

    #[test]
    fn test_as_builtin_str_returns_none_for_non_builtins() {
        assert_eq!(
            TypeInfoKind::Custom("Foo".to_string()).as_builtin_str(),
            None
        );
        assert_eq!(
            TypeInfoKind::Struct("Bar".to_string(), "Bar".to_string()).as_builtin_str(),
            None
        );
        assert_eq!(
            TypeInfoKind::Array(Box::new(TypeInfo::boolean()), 10).as_builtin_str(),
            None
        );
        assert_eq!(
            TypeInfoKind::Generic("T".to_string()).as_builtin_str(),
            None
        );
    }

    #[test]
    fn test_from_builtin_str_numbers() {
        for nt in NumberType::ALL {
            let result = TypeInfoKind::from_builtin_str(nt.as_str());
            assert_eq!(result, Some(TypeInfoKind::Number(*nt)));
        }
    }

    #[test]
    fn test_from_builtin_str_non_numeric() {
        assert_eq!(
            TypeInfoKind::from_builtin_str("unit"),
            Some(TypeInfoKind::Unit)
        );
        assert_eq!(
            TypeInfoKind::from_builtin_str("bool"),
            Some(TypeInfoKind::Bool)
        );
        assert_eq!(
            TypeInfoKind::from_builtin_str("string"),
            Some(TypeInfoKind::String)
        );
    }

    #[test]
    fn test_from_builtin_str_case_sensitive() {
        assert_eq!(TypeInfoKind::from_builtin_str("BOOL"), None);
        assert_eq!(TypeInfoKind::from_builtin_str("Bool"), None);
        assert_eq!(TypeInfoKind::from_builtin_str("STRING"), None);
        assert_eq!(TypeInfoKind::from_builtin_str("Unit"), None);
        assert_eq!(TypeInfoKind::from_builtin_str("I32"), None);
    }

    #[test]
    fn test_from_builtin_str_string_capital_s() {
        assert_eq!(
            TypeInfoKind::from_builtin_str("String"),
            Some(TypeInfoKind::String)
        );
    }

    #[test]
    fn test_from_builtin_str_invalid() {
        assert_eq!(TypeInfoKind::from_builtin_str("invalid"), None);
        assert_eq!(TypeInfoKind::from_builtin_str("CustomType"), None);
        assert_eq!(TypeInfoKind::from_builtin_str(""), None);
    }

    #[test]
    fn test_as_builtin_str_roundtrip() {
        let builtins = [
            TypeInfoKind::Unit,
            TypeInfoKind::Bool,
            TypeInfoKind::String,
            TypeInfoKind::Number(NumberType::I32),
            TypeInfoKind::Number(NumberType::U64),
        ];

        for kind in builtins {
            let name = kind.as_builtin_str().expect("should have builtin name");
            let parsed = TypeInfoKind::from_builtin_str(name).expect("should parse back");
            assert_eq!(kind, parsed);
        }
    }
}

mod type_info_from_ast {
    use super::*;

    fn dummy_location() -> Location {
        Location::new(0, 0, 0, 0, 0, 0)
    }

    fn alloc_ident(arena: &mut AstArena, name: &str) -> inference_ast::ids::IdentId {
        arena.idents.alloc(Ident {
            location: dummy_location(),
            name: name.to_string(),
        })
    }

    fn alloc_simple_type(arena: &mut AstArena, name: &str) -> inference_ast::ids::TypeId {
        let kind = match name.to_lowercase().as_str() {
            "unit" => SimpleTypeKind::Unit,
            "bool" => SimpleTypeKind::Bool,
            "i8" => SimpleTypeKind::I8,
            "i16" => SimpleTypeKind::I16,
            "i32" => SimpleTypeKind::I32,
            "i64" => SimpleTypeKind::I64,
            "u8" => SimpleTypeKind::U8,
            "u16" => SimpleTypeKind::U16,
            "u32" => SimpleTypeKind::U32,
            "u64" => SimpleTypeKind::U64,
            _ => panic!("Unknown simple type kind: {}", name),
        };
        arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Simple(kind),
        })
    }

    fn alloc_number_literal_expr(
        arena: &mut AstArena,
        value: &str,
    ) -> inference_ast::ids::ExprId {
        arena.exprs.alloc(ExprData {
            location: dummy_location(),
            kind: Expr::NumberLiteral {
                value: value.to_string(),
            },
        })
    }

    #[test]
    fn test_new_from_simple_builtin_i32() {
        let mut arena = AstArena::default();
        let ty_id = alloc_simple_type(&mut arena, "i32");
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Number(NumberType::I32));
        assert!(ti.type_params.is_empty());
    }

    #[test]
    fn test_new_from_simple_builtin_bool() {
        let mut arena = AstArena::default();
        let ty_id = alloc_simple_type(&mut arena, "bool");
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Bool);
    }

    #[test]
    fn test_new_from_string_custom_type() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "string");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::String);
    }

    #[test]
    fn test_new_from_simple_builtin_unit() {
        let mut arena = AstArena::default();
        let ty_id = alloc_simple_type(&mut arena, "unit");
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Unit);
    }

    #[test]
    fn test_new_from_simple_all_numeric_types() {
        let cases = [
            ("i8", NumberType::I8),
            ("i16", NumberType::I16),
            ("i32", NumberType::I32),
            ("i64", NumberType::I64),
            ("u8", NumberType::U8),
            ("u16", NumberType::U16),
            ("u32", NumberType::U32),
            ("u64", NumberType::U64),
        ];

        for (name, expected) in cases {
            let mut arena = AstArena::default();
            let ty_id = alloc_simple_type(&mut arena, name);
            let ti = TypeInfo::from_type_id(&arena, ty_id);
            assert_eq!(ti.kind, TypeInfoKind::Number(expected), "Failed for {name}");
        }
    }

    #[test]
    fn test_new_from_custom_type() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "MyCustomType");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Custom("MyCustomType".to_string()));
    }

    #[test]
    fn test_new_from_generic_type() {
        let mut arena = AstArena::default();
        let base_id = alloc_ident(&mut arena, "Container");
        let param_t = alloc_ident(&mut arena, "T");
        let param_u = alloc_ident(&mut arena, "U");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Generic {
                base: base_id,
                params: vec![param_t, param_u],
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Generic("Container".to_string()));
        assert_eq!(ti.type_params, vec!["T".to_string(), "U".to_string()]);
    }

    #[test]
    fn test_new_from_qualified_name() {
        let mut arena = AstArena::default();
        let qualifier_id = alloc_ident(&mut arena, "std");
        let name_id = alloc_ident(&mut arena, "Vec");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::QualifiedName {
                qualifier: qualifier_id,
                name: name_id,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::QualifiedName("std::Vec".to_string()));
    }

    #[test]
    fn test_new_from_qualified() {
        let mut arena = AstArena::default();
        let alias_id = alloc_ident(&mut arena, "Module");
        let name_id = alloc_ident(&mut arena, "Type");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Qualified {
                qualifier: vec![alias_id],
                name: name_id,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Qualified("Module::Type".to_string()));
    }

    #[test]
    fn test_new_from_multi_segment_qualified() {
        let mut arena = AstArena::default();
        let lib_id = alloc_ident(&mut arena, "lib");
        let geom_id = alloc_ident(&mut arena, "geom");
        let name_id = alloc_ident(&mut arena, "Point");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Qualified {
                qualifier: vec![lib_id, geom_id],
                name: name_id,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(
            ti.kind,
            TypeInfoKind::Qualified("lib::geom::Point".to_string())
        );
    }

    #[test]
    fn test_new_from_array_type() {
        let mut arena = AstArena::default();
        let elem_ty = alloc_simple_type(&mut arena, "i32");
        let size_expr = alloc_number_literal_expr(&mut arena, "10");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Array {
                element: elem_ty,
                size: size_expr,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);

        if let TypeInfoKind::Array(elem, size) = &ti.kind {
            assert_eq!(elem.kind, TypeInfoKind::Number(NumberType::I32));
            assert_eq!(*size, 10);
        } else {
            panic!("Expected array type");
        }
    }

    #[test]
    fn test_new_from_nested_array_type() {
        let mut arena = AstArena::default();
        let inner_elem_ty = alloc_simple_type(&mut arena, "bool");
        let inner_size = alloc_number_literal_expr(&mut arena, "5");
        let inner_array_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Array {
                element: inner_elem_ty,
                size: inner_size,
            },
        });
        let outer_size = alloc_number_literal_expr(&mut arena, "3");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Array {
                element: inner_array_ty,
                size: outer_size,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);

        if let TypeInfoKind::Array(outer_elem, outer_size) = &ti.kind {
            assert_eq!(*outer_size, 3);
            if let TypeInfoKind::Array(inner_elem, inner_size) = &outer_elem.kind {
                assert_eq!(*inner_size, 5);
                assert_eq!(inner_elem.kind, TypeInfoKind::Bool);
            } else {
                panic!("Expected inner array type");
            }
        } else {
            panic!("Expected outer array type");
        }
    }

    #[test]
    fn test_new_from_function_type_no_params_no_return() {
        let mut arena = AstArena::default();
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Function {
                params: vec![],
                ret: None,
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);

        if let TypeInfoKind::Function(sig) = &ti.kind {
            assert!(sig.contains("Function<0"));
            assert!(sig.contains("Unit"));
        } else {
            panic!("Expected function type");
        }
    }

    #[test]
    fn test_new_from_function_type_with_params() {
        let mut arena = AstArena::default();
        let param_i32 = alloc_simple_type(&mut arena, "i32");
        let param_bool = alloc_simple_type(&mut arena, "bool");
        let ret_ident = alloc_ident(&mut arena, "string");
        let ret_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ret_ident),
        });
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Function {
                params: vec![param_i32, param_bool],
                ret: Some(ret_ty),
            },
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);

        if let TypeInfoKind::Function(sig) = &ti.kind {
            assert!(sig.contains("Function<2"));
            assert!(sig.contains("String"));
        } else {
            panic!("Expected function type");
        }
    }

    #[test]
    fn test_new_from_custom_identifier() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "Point");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let ti = TypeInfo::from_type_id(&arena, ty_id);
        assert_eq!(ti.kind, TypeInfoKind::Custom("Point".to_string()));
    }
}

mod is_signed_methods {
    use super::*;

    #[test]
    fn test_number_type_is_signed_signed_types() {
        assert!(NumberType::I8.is_signed(), "i8 should be signed");
        assert!(NumberType::I16.is_signed(), "i16 should be signed");
        assert!(NumberType::I32.is_signed(), "i32 should be signed");
        assert!(NumberType::I64.is_signed(), "i64 should be signed");
    }

    #[test]
    fn test_number_type_is_signed_unsigned_types() {
        assert!(!NumberType::U8.is_signed(), "u8 should not be signed");
        assert!(!NumberType::U16.is_signed(), "u16 should not be signed");
        assert!(!NumberType::U32.is_signed(), "u32 should not be signed");
        assert!(!NumberType::U64.is_signed(), "u64 should not be signed");
    }

    #[test]
    fn test_number_type_is_signed_all_variants() {
        let signed_types = [
            NumberType::I8,
            NumberType::I16,
            NumberType::I32,
            NumberType::I64,
        ];
        let unsigned_types = [
            NumberType::U8,
            NumberType::U16,
            NumberType::U32,
            NumberType::U64,
        ];

        for nt in signed_types {
            assert!(nt.is_signed(), "{:?} should be signed", nt);
        }

        for nt in unsigned_types {
            assert!(!nt.is_signed(), "{:?} should not be signed", nt);
        }
    }

    #[test]
    fn test_type_info_is_signed_integer_signed_types() {
        let signed_types = [
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I8),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I16),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            },
        ];

        for ti in signed_types {
            assert!(
                ti.is_signed_integer(),
                "{:?} should be a signed integer",
                ti.kind
            );
        }
    }

    #[test]
    fn test_type_info_is_signed_integer_unsigned_types() {
        let unsigned_types = [
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U8),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U16),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U32),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U64),
                type_params: vec![],
            },
        ];

        for ti in unsigned_types {
            assert!(
                !ti.is_signed_integer(),
                "{:?} should not be a signed integer",
                ti.kind
            );
        }
    }

    #[test]
    fn test_type_info_is_signed_integer_non_numeric_types() {
        let non_numeric = [
            TypeInfo::boolean(),
            TypeInfo::string(),
            TypeInfo::default(),
            TypeInfo {
                kind: TypeInfoKind::Struct("Point".to_string(), "Point".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Enum("Color".to_string(), "Color".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Generic("T".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Custom("MyType".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), 10),
                type_params: vec![],
            },
        ];

        for ti in non_numeric {
            assert!(
                !ti.is_signed_integer(),
                "{:?} should not be a signed integer",
                ti.kind
            );
        }
    }

    #[test]
    fn test_type_info_is_signed_integer_with_type_params() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec!["T".to_string()],
        };
        assert!(
            ti.is_signed_integer(),
            "i32 with type params should still be a signed integer"
        );
    }
}

mod type_info_with_type_params {
    use super::*;

    fn dummy_location() -> Location {
        Location::new(0, 0, 0, 0, 0, 0)
    }

    fn alloc_ident(arena: &mut AstArena, name: &str) -> inference_ast::ids::IdentId {
        arena.idents.alloc(Ident {
            location: dummy_location(),
            name: name.to_string(),
        })
    }

    fn alloc_simple_type(arena: &mut AstArena, name: &str) -> inference_ast::ids::TypeId {
        let kind = match name.to_lowercase().as_str() {
            "unit" => SimpleTypeKind::Unit,
            "bool" => SimpleTypeKind::Bool,
            "i8" => SimpleTypeKind::I8,
            "i16" => SimpleTypeKind::I16,
            "i32" => SimpleTypeKind::I32,
            "i64" => SimpleTypeKind::I64,
            "u8" => SimpleTypeKind::U8,
            "u16" => SimpleTypeKind::U16,
            "u32" => SimpleTypeKind::U32,
            "u64" => SimpleTypeKind::U64,
            _ => panic!("Unknown simple type kind: {}", name),
        };
        arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Simple(kind),
        })
    }

    fn alloc_number_literal_expr(
        arena: &mut AstArena,
        value: &str,
    ) -> inference_ast::ids::ExprId {
        arena.exprs.alloc(ExprData {
            location: dummy_location(),
            kind: Expr::NumberLiteral {
                value: value.to_string(),
            },
        })
    }

    #[test]
    fn test_custom_type_becomes_generic_when_in_type_params_list() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "T");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let type_params = vec!["T".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Generic("T".to_string()));
    }

    #[test]
    fn test_custom_type_stays_custom_when_not_in_type_params_list() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "T");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let type_params = vec!["U".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Custom("T".to_string()));
    }

    #[test]
    fn test_custom_type_becomes_generic_when_in_params() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "T");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let type_params = vec!["T".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Generic("T".to_string()));
    }

    #[test]
    fn test_custom_type_stays_custom_when_not_in_params() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "MyStruct");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let type_params = vec!["T".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Custom("MyStruct".to_string()));
    }

    #[test]
    fn test_array_element_becomes_generic() {
        let mut arena = AstArena::default();
        let elem_ident = alloc_ident(&mut arena, "T");
        let elem_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(elem_ident),
        });
        let size_expr = alloc_number_literal_expr(&mut arena, "5");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Array {
                element: elem_ty,
                size: size_expr,
            },
        });
        let type_params = vec!["T".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        if let TypeInfoKind::Array(elem, size) = &ti.kind {
            assert_eq!(elem.kind, TypeInfoKind::Generic("T".to_string()));
            assert_eq!(*size, 5);
        } else {
            panic!("Expected array type");
        }
    }

    #[test]
    fn test_function_params_become_generic() {
        let mut arena = AstArena::default();
        let param_ident = alloc_ident(&mut arena, "T");
        let param_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(param_ident),
        });
        let ret_ident = alloc_ident(&mut arena, "U");
        let ret_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ret_ident),
        });
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Function {
                params: vec![param_ty],
                ret: Some(ret_ty),
            },
        });
        let type_params = vec!["T".to_string(), "U".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert!(matches!(ti.kind, TypeInfoKind::Function(_)));
    }

    #[test]
    fn test_multiple_type_params_all_resolved() {
        let mut arena = AstArena::default();
        let elem_ident = alloc_ident(&mut arena, "K");
        let elem_ty = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(elem_ident),
        });
        let size_expr = alloc_number_literal_expr(&mut arena, "10");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Array {
                element: elem_ty,
                size: size_expr,
            },
        });
        let type_params = vec!["K".to_string(), "V".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        if let TypeInfoKind::Array(elem, _) = &ti.kind {
            assert_eq!(elem.kind, TypeInfoKind::Generic("K".to_string()));
        } else {
            panic!("Expected array type");
        }
    }

    #[test]
    fn test_empty_type_params_no_generics() {
        let mut arena = AstArena::default();
        let ident_id = alloc_ident(&mut arena, "T");
        let ty_id = arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Custom(ident_id),
        });
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &[]);

        assert_eq!(ti.kind, TypeInfoKind::Custom("T".to_string()));
    }

    #[test]
    fn test_simple_type_cannot_be_shadowed_by_type_param() {
        let mut arena = AstArena::default();
        let ty_id = alloc_simple_type(&mut arena, "i32");
        let type_params = vec!["i32".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Number(NumberType::I32));
    }

    #[test]
    fn test_builtin_without_matching_type_param_stays_builtin() {
        let mut arena = AstArena::default();
        let ty_id = alloc_simple_type(&mut arena, "i32");
        let type_params = vec!["T".to_string()];
        let ti = TypeInfo::from_type_id_with_type_params(&arena, ty_id, &type_params);

        assert_eq!(ti.kind, TypeInfoKind::Number(NumberType::I32));
    }
}

mod type_equality {
    use super::*;

    #[test]
    fn identical_types_are_equal() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        };
        assert_eq!(ti, ti);
    }

    #[test]
    fn different_number_types_not_equal() {
        let i32_type = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        };
        let i64_type = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I64),
            type_params: vec![],
        };
        assert_ne!(i32_type, i64_type);
    }

    #[test]
    fn arrays_same_size_are_equal() {
        let arr_a = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                5,
            ),
            type_params: vec![],
        };
        let arr_b = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                5,
            ),
            type_params: vec![],
        };
        assert_eq!(arr_a, arr_b);
    }

    #[test]
    fn arrays_different_size_not_equal() {
        let arr_3 = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                3,
            ),
            type_params: vec![],
        };
        let arr_5 = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                5,
            ),
            type_params: vec![],
        };
        assert_ne!(arr_3, arr_5);
    }

    #[test]
    fn arrays_different_element_type_not_equal() {
        let arr_i32 = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                3,
            ),
            type_params: vec![],
        };
        let arr_i64 = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I64),
                    type_params: vec![],
                }),
                3,
            ),
            type_params: vec![],
        };
        assert_ne!(arr_i32, arr_i64);
    }

    #[test]
    fn bool_not_equal_to_i32() {
        let bool_type = TypeInfo {
            kind: TypeInfoKind::Bool,
            type_params: vec![],
        };
        let i32_type = TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        };
        assert_ne!(bool_type, i32_type);
    }
}
