use std::fmt::{self, Display, Formatter};

use inference_ast::nodes::{Location, OperatorKind, UnaryOperatorKind};
use thiserror::Error;

use crate::type_info::TypeInfo;

/// Kind of symbol registration for registration error context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationKind {
    Type,
    Struct,
    Enum,
    Spec,
    Function,
    Method,
    Variable,
}

impl Display for RegistrationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RegistrationKind::Type => write!(f, "type"),
            RegistrationKind::Struct => write!(f, "struct"),
            RegistrationKind::Enum => write!(f, "enum"),
            RegistrationKind::Spec => write!(f, "spec"),
            RegistrationKind::Function => write!(f, "function"),
            RegistrationKind::Method => write!(f, "method"),
            RegistrationKind::Variable => write!(f, "variable"),
        }
    }
}

/// Context for type mismatch errors to provide better messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeMismatchContext {
    Assignment,
    Return,
    VariableDefinition,
    BinaryOperation(OperatorKind),
    Condition,
    FunctionArgument {
        function_name: String,
        arg_index: usize,
    },
    MethodArgument {
        type_name: String,
        method_name: String,
        arg_index: usize,
    },
    ArrayElement,
}

impl Display for TypeMismatchContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TypeMismatchContext::Assignment => write!(f, "in assignment"),
            TypeMismatchContext::Return => write!(f, "in return statement"),
            TypeMismatchContext::VariableDefinition => write!(f, "in variable definition"),
            TypeMismatchContext::BinaryOperation(op) => write!(f, "in binary operation `{op:?}`"),
            TypeMismatchContext::Condition => write!(f, "in condition"),
            TypeMismatchContext::FunctionArgument {
                function_name,
                arg_index,
            } => write!(
                f,
                "in argument {arg_index} of function `{function_name}`"
            ),
            TypeMismatchContext::MethodArgument {
                type_name,
                method_name,
                arg_index,
            } => write!(
                f,
                "in argument {arg_index} of method `{type_name}::{method_name}`"
            ),
            TypeMismatchContext::ArrayElement => write!(f, "in array element"),
        }
    }
}

/// Represents a type checking error with optional source location.
#[derive(Debug, Clone, Error)]
pub enum TypeCheckError {
    #[error("type mismatch {context}: expected `{expected}`, found `{found}`")]
    TypeMismatch {
        expected: TypeInfo,
        found: TypeInfo,
        context: TypeMismatchContext,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("unknown type `{name}`")]
    UnknownType {
        name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("use of undeclared variable `{name}`")]
    UnknownIdentifier {
        name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("call to undefined function `{name}`")]
    UndefinedFunction {
        name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("struct `{name}` is not defined")]
    UndefinedStruct {
        name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("field `{field_name}` not found on struct `{struct_name}`")]
    FieldNotFound {
        struct_name: String,
        field_name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("variant `{variant_name}` not found on enum `{enum_name}`")]
    VariantNotFound {
        enum_name: String,
        variant_name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("enum `{name}` is not defined")]
    UndefinedEnum {
        name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("type member access requires an enum type, found `{found}`")]
    ExpectedEnumType {
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("method `{method_name}` not found on type `{type_name}`")]
    MethodNotFound {
        type_name: String,
        method_name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("{kind} `{name}` expects {expected} arguments, but {found} provided")]
    ArgumentCountMismatch {
        kind: &'static str,
        name: String,
        expected: usize,
        found: usize,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("type parameter count mismatch for `{name}`: expected {expected}, found {found}")]
    TypeParameterCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error(
        "function `{function_name}` requires {expected} type parameters, but none were provided"
    )]
    MissingTypeParameters {
        function_name: String,
        expected: usize,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("{expected_kind} operator `{operator:?}` cannot be applied to {operand_desc}")]
    InvalidBinaryOperand {
        operator: OperatorKind,
        expected_kind: &'static str,
        operand_desc: &'static str,
        #[allow(dead_code)]
        found_types: (TypeInfo, TypeInfo),
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error(
        "unary operator `{operator:?}` can only be applied to {expected_type}, found `{found_type}`"
    )]
    InvalidUnaryOperand {
        operator: UnaryOperatorKind,
        expected_type: &'static str,
        found_type: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("cannot apply operator `{operator:?}` to operands of different types: `{left}` and `{right}`")]
    BinaryOperandTypeMismatch {
        operator: OperatorKind,
        left: TypeInfo,
        right: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("self reference not allowed in standalone function `{function_name}`")]
    SelfReferenceInFunction {
        function_name: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("self reference is only allowed in methods, not functions")]
    SelfReferenceOutsideMethod {
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("cannot resolve import path: {path}")]
    ImportResolutionFailed {
        path: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("circular glob import detected: {path}::*")]
    CircularImport {
        path: String,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("glob import path cannot be empty")]
    EmptyGlobImport {
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("error registering {kind} `{name}`{}", reason.as_ref().map_or(String::new(), |r| format!(": {}", r)))]
    RegistrationFailed {
        kind: RegistrationKind,
        name: String,
        reason: Option<String>,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("expected an array type, found `{found}`")]
    ExpectedArrayType {
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("member access requires a struct type, found `{found}`")]
    ExpectedStructType {
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("cannot call method on non-struct type `{found}`")]
    MethodCallOnNonStruct {
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("array index must be of number type, found `{found}`")]
    ArrayIndexNotNumeric {
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("array elements must be of the same type: expected `{expected}`, found `{found}`")]
    ArrayElementTypeMismatch {
        expected: TypeInfo,
        found: TypeInfo,
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("cannot infer type for uzumaki expression assigned to variable of unknown type")]
    CannotInferUzumakiType {
        #[allow(dead_code)]
        location: Option<Location>,
    },

    #[error("{0}")]
    General(String),
}

impl TypeCheckError {
    /// Returns the source location associated with this error, if any.
    #[must_use]
    pub fn location(&self) -> Option<&Location> {
        match self {
            TypeCheckError::TypeMismatch { location, .. }
            | TypeCheckError::UnknownType { location, .. }
            | TypeCheckError::UnknownIdentifier { location, .. }
            | TypeCheckError::UndefinedFunction { location, .. }
            | TypeCheckError::UndefinedStruct { location, .. }
            | TypeCheckError::FieldNotFound { location, .. }
            | TypeCheckError::VariantNotFound { location, .. }
            | TypeCheckError::UndefinedEnum { location, .. }
            | TypeCheckError::ExpectedEnumType { location, .. }
            | TypeCheckError::MethodNotFound { location, .. }
            | TypeCheckError::ArgumentCountMismatch { location, .. }
            | TypeCheckError::TypeParameterCountMismatch { location, .. }
            | TypeCheckError::MissingTypeParameters { location, .. }
            | TypeCheckError::InvalidBinaryOperand { location, .. }
            | TypeCheckError::InvalidUnaryOperand { location, .. }
            | TypeCheckError::BinaryOperandTypeMismatch { location, .. }
            | TypeCheckError::SelfReferenceInFunction { location, .. }
            | TypeCheckError::SelfReferenceOutsideMethod { location }
            | TypeCheckError::ImportResolutionFailed { location, .. }
            | TypeCheckError::CircularImport { location, .. }
            | TypeCheckError::EmptyGlobImport { location }
            | TypeCheckError::RegistrationFailed { location, .. }
            | TypeCheckError::ExpectedArrayType { location, .. }
            | TypeCheckError::ExpectedStructType { location, .. }
            | TypeCheckError::MethodCallOnNonStruct { location, .. }
            | TypeCheckError::ArrayIndexNotNumeric { location, .. }
            | TypeCheckError::ArrayElementTypeMismatch { location, .. }
            | TypeCheckError::CannotInferUzumakiType { location } => location.as_ref(),
            TypeCheckError::General(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_info::{NumberTypeKindNumberType, TypeInfoKind};

    #[test]
    fn display_type_mismatch() {
        let err = TypeCheckError::TypeMismatch {
            expected: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            found: TypeInfo::default(),
            context: TypeMismatchContext::Assignment,
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "type mismatch in assignment: expected `Bool`, found `Unit`"
        );
    }

    #[test]
    fn display_unknown_type() {
        let err = TypeCheckError::UnknownType {
            name: "Foo".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "unknown type `Foo`");
    }

    #[test]
    fn display_field_not_found() {
        let err = TypeCheckError::FieldNotFound {
            struct_name: "Point".to_string(),
            field_name: "z".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "field `z` not found on struct `Point`");
    }

    #[test]
    fn display_registration_failed_without_reason() {
        let err = TypeCheckError::RegistrationFailed {
            kind: RegistrationKind::Type,
            name: "Foo".to_string(),
            reason: None,
            location: None,
        };
        assert_eq!(err.to_string(), "error registering type `Foo`");
    }

    #[test]
    fn display_registration_failed_with_reason() {
        let err = TypeCheckError::RegistrationFailed {
            kind: RegistrationKind::Method,
            name: "bar".to_string(),
            reason: Some("duplicate definition".to_string()),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "error registering method `bar`: duplicate definition"
        );
    }

    #[test]
    fn display_type_mismatch_context() {
        assert_eq!(TypeMismatchContext::Assignment.to_string(), "in assignment");
        assert_eq!(
            TypeMismatchContext::Return.to_string(),
            "in return statement"
        );
        assert_eq!(
            TypeMismatchContext::FunctionArgument {
                function_name: "foo".to_string(),
                arg_index: 0
            }
            .to_string(),
            "in argument 0 of function `foo`"
        );
    }

    #[test]
    fn display_registration_kind() {
        assert_eq!(RegistrationKind::Type.to_string(), "type");
        assert_eq!(RegistrationKind::Struct.to_string(), "struct");
        assert_eq!(RegistrationKind::Method.to_string(), "method");
    }

    #[test]
    fn error_location_accessor() {
        let loc = Location::default();
        let err = TypeCheckError::UnknownType {
            name: "Foo".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));

        let err_no_loc = TypeCheckError::General("test".to_string());
        assert!(err_no_loc.location().is_none());
    }

    #[test]
    fn display_unknown_identifier() {
        let err = TypeCheckError::UnknownIdentifier {
            name: "myVar".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "use of undeclared variable `myVar`");
    }

    #[test]
    fn display_undefined_function() {
        let err = TypeCheckError::UndefinedFunction {
            name: "myFunc".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "call to undefined function `myFunc`");
    }

    #[test]
    fn display_undefined_struct() {
        let err = TypeCheckError::UndefinedStruct {
            name: "MyStruct".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "struct `MyStruct` is not defined");
    }

    #[test]
    fn display_method_not_found() {
        let err = TypeCheckError::MethodNotFound {
            type_name: "Point".to_string(),
            method_name: "rotate".to_string(),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "method `rotate` not found on type `Point`"
        );
    }

    #[test]
    fn display_argument_count_mismatch() {
        let err = TypeCheckError::ArgumentCountMismatch {
            kind: "function",
            name: "add".to_string(),
            expected: 2,
            found: 3,
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "function `add` expects 2 arguments, but 3 provided"
        );
    }

    #[test]
    fn display_type_parameter_count_mismatch() {
        let err = TypeCheckError::TypeParameterCountMismatch {
            name: "Vec".to_string(),
            expected: 1,
            found: 2,
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "type parameter count mismatch for `Vec`: expected 1, found 2"
        );
    }

    #[test]
    fn display_missing_type_parameters() {
        let err = TypeCheckError::MissingTypeParameters {
            function_name: "generic_fn".to_string(),
            expected: 2,
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "function `generic_fn` requires 2 type parameters, but none were provided"
        );
    }

    #[test]
    fn display_invalid_binary_operand() {
        let err = TypeCheckError::InvalidBinaryOperand {
            operator: OperatorKind::Add,
            expected_kind: "numeric",
            operand_desc: "non-numeric types",
            found_types: (
                TypeInfo {
                    kind: TypeInfoKind::Bool,
                    type_params: vec![],
                },
                TypeInfo {
                    kind: TypeInfoKind::Bool,
                    type_params: vec![],
                },
            ),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "numeric operator `Add` cannot be applied to non-numeric types"
        );
    }

    #[test]
    fn display_invalid_unary_operand() {
        let err = TypeCheckError::InvalidUnaryOperand {
            operator: UnaryOperatorKind::Neg,
            expected_type: "numeric",
            found_type: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "unary operator `Neg` can only be applied to numeric, found `Bool`"
        );
    }

    #[test]
    fn display_binary_operand_type_mismatch() {
        let err = TypeCheckError::BinaryOperandTypeMismatch {
            operator: OperatorKind::Add,
            left: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
            right: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I64),
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "cannot apply operator `Add` to operands of different types: `i32` and `i64`"
        );
    }

    #[test]
    fn display_self_reference_in_function() {
        let err = TypeCheckError::SelfReferenceInFunction {
            function_name: "standalone_fn".to_string(),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "self reference not allowed in standalone function `standalone_fn`"
        );
    }

    #[test]
    fn display_self_reference_outside_method() {
        let err = TypeCheckError::SelfReferenceOutsideMethod { location: None };
        assert_eq!(
            err.to_string(),
            "self reference is only allowed in methods, not functions"
        );
    }

    #[test]
    fn display_import_resolution_failed() {
        let err = TypeCheckError::ImportResolutionFailed {
            path: "std::collections::HashMap".to_string(),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "cannot resolve import path: std::collections::HashMap"
        );
    }

    #[test]
    fn display_circular_import() {
        let err = TypeCheckError::CircularImport {
            path: "mod_a::mod_b".to_string(),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "circular glob import detected: mod_a::mod_b::*"
        );
    }

    #[test]
    fn display_empty_glob_import() {
        let err = TypeCheckError::EmptyGlobImport { location: None };
        assert_eq!(err.to_string(), "glob import path cannot be empty");
    }

    #[test]
    fn display_expected_array_type() {
        let err = TypeCheckError::ExpectedArrayType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(err.to_string(), "expected an array type, found `i32`");
    }

    #[test]
    fn display_expected_struct_type() {
        let err = TypeCheckError::ExpectedStructType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "member access requires a struct type, found `i32`"
        );
    }

    #[test]
    fn display_method_call_on_non_struct() {
        let err = TypeCheckError::MethodCallOnNonStruct {
            found: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "cannot call method on non-struct type `Bool`"
        );
    }

    #[test]
    fn display_array_index_not_numeric() {
        let err = TypeCheckError::ArrayIndexNotNumeric {
            found: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "array index must be of number type, found `Bool`"
        );
    }

    #[test]
    fn display_array_element_type_mismatch() {
        let err = TypeCheckError::ArrayElementTypeMismatch {
            expected: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I64),
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "array elements must be of the same type: expected `i32`, found `i64`"
        );
    }

    #[test]
    fn display_cannot_infer_uzumaki_type() {
        let err = TypeCheckError::CannotInferUzumakiType { location: None };
        assert_eq!(
            err.to_string(),
            "cannot infer type for uzumaki expression assigned to variable of unknown type"
        );
    }

    #[test]
    fn display_general() {
        let err = TypeCheckError::General("custom error message".to_string());
        assert_eq!(err.to_string(), "custom error message");
    }

    #[test]
    fn location_unknown_identifier() {
        let loc = Location::default();
        let err = TypeCheckError::UnknownIdentifier {
            name: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));

        let err_no_loc = TypeCheckError::UnknownIdentifier {
            name: "test".to_string(),
            location: None,
        };
        assert!(err_no_loc.location().is_none());
    }

    #[test]
    fn location_undefined_function() {
        let loc = Location::default();
        let err = TypeCheckError::UndefinedFunction {
            name: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_undefined_struct() {
        let loc = Location::default();
        let err = TypeCheckError::UndefinedStruct {
            name: "Test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_method_not_found() {
        let loc = Location::default();
        let err = TypeCheckError::MethodNotFound {
            type_name: "Point".to_string(),
            method_name: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_argument_count_mismatch() {
        let loc = Location::default();
        let err = TypeCheckError::ArgumentCountMismatch {
            kind: "function",
            name: "test".to_string(),
            expected: 1,
            found: 2,
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_type_parameter_count_mismatch() {
        let loc = Location::default();
        let err = TypeCheckError::TypeParameterCountMismatch {
            name: "Test".to_string(),
            expected: 1,
            found: 2,
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_missing_type_parameters() {
        let loc = Location::default();
        let err = TypeCheckError::MissingTypeParameters {
            function_name: "test".to_string(),
            expected: 2,
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_invalid_binary_operand() {
        let loc = Location::default();
        let err = TypeCheckError::InvalidBinaryOperand {
            operator: OperatorKind::Add,
            expected_kind: "numeric",
            operand_desc: "test",
            found_types: (TypeInfo::default(), TypeInfo::default()),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_invalid_unary_operand() {
        let loc = Location::default();
        let err = TypeCheckError::InvalidUnaryOperand {
            operator: UnaryOperatorKind::Neg,
            expected_type: "numeric",
            found_type: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_binary_operand_type_mismatch() {
        let loc = Location::default();
        let err = TypeCheckError::BinaryOperandTypeMismatch {
            operator: OperatorKind::Add,
            left: TypeInfo::default(),
            right: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_self_reference_in_function() {
        let loc = Location::default();
        let err = TypeCheckError::SelfReferenceInFunction {
            function_name: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_self_reference_outside_method() {
        let loc = Location::default();
        let err = TypeCheckError::SelfReferenceOutsideMethod {
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_import_resolution_failed() {
        let loc = Location::default();
        let err = TypeCheckError::ImportResolutionFailed {
            path: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_circular_import() {
        let loc = Location::default();
        let err = TypeCheckError::CircularImport {
            path: "test".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_empty_glob_import() {
        let loc = Location::default();
        let err = TypeCheckError::EmptyGlobImport {
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_expected_array_type() {
        let loc = Location::default();
        let err = TypeCheckError::ExpectedArrayType {
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_expected_struct_type() {
        let loc = Location::default();
        let err = TypeCheckError::ExpectedStructType {
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_method_call_on_non_struct() {
        let loc = Location::default();
        let err = TypeCheckError::MethodCallOnNonStruct {
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_array_index_not_numeric() {
        let loc = Location::default();
        let err = TypeCheckError::ArrayIndexNotNumeric {
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_array_element_type_mismatch() {
        let loc = Location::default();
        let err = TypeCheckError::ArrayElementTypeMismatch {
            expected: TypeInfo::default(),
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_cannot_infer_uzumaki_type() {
        let loc = Location::default();
        let err = TypeCheckError::CannotInferUzumakiType {
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));
    }

    #[test]
    fn location_general() {
        let err = TypeCheckError::General("test".to_string());
        assert!(err.location().is_none());
    }

    #[test]
    fn display_variant_not_found() {
        let err = TypeCheckError::VariantNotFound {
            enum_name: "Color".to_string(),
            variant_name: "Yellow".to_string(),
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "variant `Yellow` not found on enum `Color`"
        );
    }

    #[test]
    fn display_undefined_enum() {
        let err = TypeCheckError::UndefinedEnum {
            name: "UnknownEnum".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "enum `UnknownEnum` is not defined");
    }

    #[test]
    fn display_expected_enum_type() {
        let err = TypeCheckError::ExpectedEnumType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
            location: None,
        };
        assert_eq!(
            err.to_string(),
            "type member access requires an enum type, found `i32`"
        );
    }

    #[test]
    fn location_variant_not_found() {
        let loc = Location::default();
        let err = TypeCheckError::VariantNotFound {
            enum_name: "Color".to_string(),
            variant_name: "Yellow".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));

        let err_no_loc = TypeCheckError::VariantNotFound {
            enum_name: "Color".to_string(),
            variant_name: "Yellow".to_string(),
            location: None,
        };
        assert!(err_no_loc.location().is_none());
    }

    #[test]
    fn location_undefined_enum() {
        let loc = Location::default();
        let err = TypeCheckError::UndefinedEnum {
            name: "UnknownEnum".to_string(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));

        let err_no_loc = TypeCheckError::UndefinedEnum {
            name: "UnknownEnum".to_string(),
            location: None,
        };
        assert!(err_no_loc.location().is_none());
    }

    #[test]
    fn location_expected_enum_type() {
        let loc = Location::default();
        let err = TypeCheckError::ExpectedEnumType {
            found: TypeInfo::default(),
            location: Some(loc.clone()),
        };
        assert_eq!(err.location(), Some(&loc));

        let err_no_loc = TypeCheckError::ExpectedEnumType {
            found: TypeInfo::default(),
            location: None,
        };
        assert!(err_no_loc.location().is_none());
    }
}
