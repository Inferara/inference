use inference_ast::type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind};

#[test]
fn test_type_info_unit() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Unit,
        type_params: vec![],
    };
    assert_eq!(ty.kind, TypeInfoKind::Unit);
    assert_eq!(format!("{ty}"), "Unit");
}

#[test]
fn test_type_info_bool() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Bool,
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "Bool");
}

#[test]
fn test_type_info_string() {
    let ty = TypeInfo {
        kind: TypeInfoKind::String,
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "String");
}

#[test]
fn test_type_info_i8() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I8),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "i8");
}

#[test]
fn test_type_info_i16() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I16),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "i16");
}

#[test]
fn test_type_info_i32() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "i32");
}

#[test]
fn test_type_info_i64() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I64),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "i64");
}

#[test]
fn test_type_info_u8() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::U8),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "u8");
}

#[test]
fn test_type_info_u16() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::U16),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "u16");
}

#[test]
fn test_type_info_u32() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::U32),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "u32");
}

#[test]
fn test_type_info_u64() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::U64),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "u64");
}

#[test]
fn test_type_info_custom() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Custom("MyType".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "MyType");
}

#[test]
fn test_type_info_array_no_length() {
    let elem_ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    let ty = TypeInfo {
        kind: TypeInfoKind::Array(Box::new(elem_ty), None),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "[i32]");
}

#[test]
fn test_type_info_array_with_length() {
    let elem_ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    let ty = TypeInfo {
        kind: TypeInfoKind::Array(Box::new(elem_ty), Some(10)),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "[i32; 10]");
}

#[test]
fn test_type_info_generic() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Generic("T".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "<T>");
}

#[test]
fn test_type_info_qualified_name() {
    let ty = TypeInfo {
        kind: TypeInfoKind::QualifiedName("std::vec::Vec".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "std::vec::Vec");
}

#[test]
fn test_type_info_qualified() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Qualified("MyModule::MyType".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "MyModule::MyType");
}

#[test]
fn test_type_info_function() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Function("my_function".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "my_function");
}

#[test]
fn test_type_info_struct() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Struct("Point".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "Point");
}

#[test]
fn test_type_info_enum() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Enum("Color".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "Color");
}

#[test]
fn test_type_info_spec() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Spec("MySpec".to_string()),
        type_params: vec![],
    };
    assert_eq!(format!("{ty}"), "MySpec");
}

#[test]
fn test_type_info_with_type_params() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Custom("Vec".to_string()),
        type_params: vec!["T".to_string(), "U".to_string()],
    };
    assert_eq!(format!("{ty}"), "Vec<T, U>");
}

#[test]
fn test_type_info_default() {
    let ty = TypeInfo::default();
    assert_eq!(ty.kind, TypeInfoKind::Unit);
    assert!(ty.type_params.is_empty());
}

#[test]
fn test_type_info_is_number() {
    let num_ty = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
    assert!(num_ty.is_number());

    let str_ty = TypeInfoKind::String;
    assert!(!str_ty.is_number());

    let custom_ty = TypeInfoKind::Custom("MyType".to_string());
    assert!(!custom_ty.is_number());
}

#[test]
fn test_type_info_clone() {
    let ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec!["T".to_string()],
    };
    let cloned = ty.clone();
    assert_eq!(ty, cloned);
}

#[test]
fn test_type_info_kind_eq() {
    let ty1 = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
    let ty2 = TypeInfoKind::Number(NumberTypeKindNumberType::I32);
    assert_eq!(ty1, ty2);

    let ty3 = TypeInfoKind::Number(NumberTypeKindNumberType::U32);
    assert_ne!(ty1, ty3);
}

#[test]
fn test_nested_array_types() {
    let inner_elem = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    let middle_array = TypeInfo {
        kind: TypeInfoKind::Array(Box::new(inner_elem), Some(5)),
        type_params: vec![],
    };
    let outer_array = TypeInfo {
        kind: TypeInfoKind::Array(Box::new(middle_array), Some(10)),
        type_params: vec![],
    };
    assert_eq!(format!("{outer_array}"), "[[i32; 5]; 10]");
}

#[test]
fn test_type_info_is_array() {
    let array_ty = TypeInfo {
        kind: TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            }),
            None,
        ),
        type_params: vec![],
    };
    assert!(array_ty.is_array());

    let non_array_ty = TypeInfo {
        kind: TypeInfoKind::Bool,
        type_params: vec![],
    };
    assert!(!non_array_ty.is_array());
}

#[test]
fn test_type_info_is_bool() {
    let bool_ty = TypeInfo {
        kind: TypeInfoKind::Bool,
        type_params: vec![],
    };
    assert!(bool_ty.is_bool());

    let non_bool_ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    assert!(!non_bool_ty.is_bool());
}

#[test]
fn test_type_info_is_struct() {
    let struct_ty = TypeInfo {
        kind: TypeInfoKind::Struct("MyStruct".to_string()),
        type_params: vec![],
    };
    assert!(struct_ty.is_struct());

    let non_struct_ty = TypeInfo {
        kind: TypeInfoKind::Enum("MyEnum".to_string()),
        type_params: vec![],
    };
    assert!(!non_struct_ty.is_struct());
}

#[test]
fn test_type_info_is_number_method() {
    let num_ty = TypeInfo {
        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
        type_params: vec![],
    };
    assert!(num_ty.is_number());

    let non_num_ty = TypeInfo {
        kind: TypeInfoKind::String,
        type_params: vec![],
    };
    assert!(!non_num_ty.is_number());
}
