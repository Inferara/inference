//! Unit tests for TypeInfo methods
//!
//! These tests focus on TypeInfo internals without requiring integration context.
//! They complement the integration tests in tests/src/type_checker/ which test
//! end-to-end type checking with source code parsing.

use inference_type_checker::type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind};
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
}

mod type_info_predicates {
    use super::*;

    #[test]
    fn test_is_number_for_all_numeric_types() {
        let numeric_kinds = [
            NumberTypeKindNumberType::I8,
            NumberTypeKindNumberType::I16,
            NumberTypeKindNumberType::I32,
            NumberTypeKindNumberType::I64,
            NumberTypeKindNumberType::U8,
            NumberTypeKindNumberType::U16,
            NumberTypeKindNumberType::U32,
            NumberTypeKindNumberType::U64,
        ];

        for kind in numeric_kinds {
            let ti = TypeInfo {
                kind: TypeInfoKind::Number(kind.clone()),
                type_params: vec![],
            };
            assert!(ti.is_number(), "Expected {:?} to be a number", kind);
        }
    }

    #[test]
    fn test_is_array() {
        let element = TypeInfo::boolean();
        let array_type = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(element), Some(10)),
            type_params: vec![],
        };
        assert!(array_type.is_array());
        assert!(!array_type.is_number());
    }

    #[test]
    fn test_is_array_without_length() {
        let element = TypeInfo::boolean();
        let array_type = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(element), None),
            type_params: vec![],
        };
        assert!(array_type.is_array());
    }

    #[test]
    fn test_is_struct() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string()),
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
                kind: TypeInfoKind::Struct("Foo".to_string()),
                type_params: vec![],
            },
            TypeInfo {
                kind: TypeInfoKind::Enum("Color".to_string()),
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
                None,
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert(
            "T".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
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
                Some(5),
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = array.substitute(&subs);
        if let TypeInfoKind::Array(elem, length) = &result.kind {
            assert!(elem.is_bool());
            assert_eq!(*length, Some(5));
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
                        None,
                    ),
                    type_params: vec![],
                }),
                Some(10),
            ),
            type_params: vec![],
        };
        let mut subs = FxHashMap::default();
        subs.insert("T".to_string(), TypeInfo::boolean());

        let result = nested_array.substitute(&subs);
        if let TypeInfoKind::Array(outer_elem, outer_len) = &result.kind {
            assert_eq!(*outer_len, Some(10));
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
            kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
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
                None,
            ),
            type_params: vec![],
        };
        assert!(array.has_unresolved_params());
    }

    #[test]
    fn test_array_with_concrete_element() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), Some(5)),
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
                        None,
                    ),
                    type_params: vec![],
                }),
                None,
            ),
            type_params: vec![],
        };
        assert!(nested.has_unresolved_params());
    }

    #[test]
    fn test_struct_no_unresolved() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string()),
            type_params: vec![],
        };
        assert!(!struct_type.has_unresolved_params());
    }

    #[test]
    fn test_enum_no_unresolved() {
        let enum_type = TypeInfo {
            kind: TypeInfoKind::Enum("Color".to_string()),
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
            kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
            type_params: vec![],
        };
        assert_eq!(ti.to_string(), "i32");
    }

    #[test]
    fn test_display_all_numeric_types() {
        let cases = [
            (NumberTypeKindNumberType::I8, "i8"),
            (NumberTypeKindNumberType::I16, "i16"),
            (NumberTypeKindNumberType::I32, "i32"),
            (NumberTypeKindNumberType::I64, "i64"),
            (NumberTypeKindNumberType::U8, "u8"),
            (NumberTypeKindNumberType::U16, "u16"),
            (NumberTypeKindNumberType::U32, "u32"),
            (NumberTypeKindNumberType::U64, "u64"),
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
    fn test_display_array_with_length() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), Some(10)),
            type_params: vec![],
        };
        assert_eq!(array.to_string(), "[Bool; 10]");
    }

    #[test]
    fn test_display_array_without_length() {
        let array = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), None),
            type_params: vec![],
        };
        assert_eq!(array.to_string(), "[Bool]");
    }

    #[test]
    fn test_display_generic() {
        let generic = TypeInfo {
            kind: TypeInfoKind::Generic("T".to_string()),
            type_params: vec![],
        };
        assert_eq!(generic.to_string(), "<T>");
    }

    #[test]
    fn test_display_struct() {
        let struct_type = TypeInfo {
            kind: TypeInfoKind::Struct("Point".to_string()),
            type_params: vec![],
        };
        assert_eq!(struct_type.to_string(), "Point");
    }

    #[test]
    fn test_display_enum() {
        let enum_type = TypeInfo {
            kind: TypeInfoKind::Enum("Color".to_string()),
            type_params: vec![],
        };
        assert_eq!(enum_type.to_string(), "Color");
    }

    #[test]
    fn test_display_with_type_params() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Struct("Vec".to_string()),
            type_params: vec!["T".to_string()],
        };
        assert_eq!(ti.to_string(), "Vec<T>");
    }

    #[test]
    fn test_display_with_multiple_type_params() {
        let ti = TypeInfo {
            kind: TypeInfoKind::Struct("Map".to_string()),
            type_params: vec!["K".to_string(), "V".to_string()],
        };
        assert_eq!(ti.to_string(), "Map<K, V>");
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
                    kind: TypeInfoKind::Array(Box::new(TypeInfo::boolean()), Some(5)),
                    type_params: vec![],
                }),
                Some(10),
            ),
            type_params: vec![],
        };
        assert_eq!(nested.to_string(), "[[Bool; 5]; 10]");
    }
}

mod type_info_kind {
    use super::*;

    #[test]
    fn test_kind_is_number() {
        let numeric_kind = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
        assert!(numeric_kind.is_number());

        let bool_kind = TypeInfoKind::Bool;
        assert!(!bool_kind.is_number());
    }

    #[test]
    fn test_kind_equality() {
        let kind1 = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
        let kind2 = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
        let kind3 = TypeInfoKind::Number(NumberTypeKindNumberType::I64);

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
