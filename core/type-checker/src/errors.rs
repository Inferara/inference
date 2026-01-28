//! Type Checking Error Types
//!
//! This module defines the error types produced by the type checker, providing
//! detailed context and location information for all type checking failures.
//!
//! ## Error Design
//!
//! All type checking errors:
//! - Include precise source location (line and column)
//! - Provide contextual information about the error
//! - Use descriptive error messages via `thiserror`
//! - Are collected and reported together (error recovery)
//!
//! ## Error Categories
//!
//! The error types are organized into logical categories:
//!
//! **Type Errors**:
//! - [`TypeCheckError::TypeMismatch`] - Type doesn't match expected type
//! - [`TypeCheckError::UnknownType`] - Reference to undefined type
//! - [`TypeCheckError::ExpectedArrayType`] - Expected array, found other type
//! - [`TypeCheckError::ExpectedStructType`] - Expected struct, found other type
//! - [`TypeCheckError::ExpectedEnumType`] - Expected enum, found other type
//!
//! **Symbol Resolution Errors**:
//! - [`TypeCheckError::UnknownIdentifier`] - Undeclared variable
//! - [`TypeCheckError::UndefinedFunction`] - Call to undefined function
//! - [`TypeCheckError::UndefinedStruct`] - Reference to undefined struct
//! - [`TypeCheckError::UndefinedEnum`] - Reference to undefined enum
//!
//! **Visibility Errors**:
//! - [`TypeCheckError::PrivateAccessViolation`] - Access to private symbol
//!
//! **Operator Errors**:
//! - [`TypeCheckError::InvalidBinaryOperand`] - Invalid types for binary operator
//! - [`TypeCheckError::InvalidUnaryOperand`] - Invalid type for unary operator
//! - [`TypeCheckError::BinaryOperandTypeMismatch`] - Operand types don't match
//!
//! **Function and Method Errors**:
//! - [`TypeCheckError::ArgumentCountMismatch`] - Wrong number of arguments
//! - [`TypeCheckError::MethodNotFound`] - Undefined method on type
//! - [`TypeCheckError::MethodCallOnNonStruct`] - Method call on primitive type
//!
//! **Other Errors**:
//! - [`TypeCheckError::FieldNotFound`] - Undefined struct field
//! - [`TypeCheckError::VariantNotFound`] - Undefined enum variant
//! - [`TypeCheckError::ArrayIndexNotNumeric`] - Non-numeric array index
//! - And more...
//!
//! ## Error Recovery
//!
//! The type checker implements error recovery to collect multiple errors:
//!
//! ```ignore
//! fn example() -> i32 {
//!     let x: bool = 42;        // Error 1: type mismatch
//!     let y = undefined_var;   // Error 2: undeclared variable
//!     return "string";         // Error 3: wrong return type
//! }
//! // All three errors reported together
//! ```
//!
//! ## Usage Example
//!
//! ```ignore
//! use inference_type_checker::TypeCheckerBuilder;
//!
//! match TypeCheckerBuilder::build_typed_context(arena) {
//!     Ok(completed) => {
//!         // Type checking succeeded
//!     }
//!     Err(e) => {
//!         // Error contains all collected errors
//!         eprintln!("Type checking failed:");
//!         for error_msg in e.to_string().split("; ") {
//!             eprintln!("  - {}", error_msg);
//!         }
//!     }
//! }
//! ```

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
        arg_name: String,
        arg_index: usize,
    },
    MethodArgument {
        type_name: String,
        method_name: String,
        arg_name: String,
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
                arg_name,
                arg_index,
            } => write!(
                f,
                "in argument {arg_index} `{arg_name}` of function `{function_name}`"
            ),
            TypeMismatchContext::MethodArgument {
                type_name,
                method_name,
                arg_name,
                arg_index,
            } => write!(
                f,
                "in argument {arg_index} `{arg_name}` of method `{type_name}::{method_name}`"
            ),
            TypeMismatchContext::ArrayElement => write!(f, "in array element"),
        }
    }
}

/// Context for visibility violation errors to provide specific error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityContext {
    Function {
        name: String,
    },
    Struct {
        name: String,
    },
    Enum {
        name: String,
    },
    Field {
        struct_name: String,
        field_name: String,
    },
    Method {
        type_name: String,
        method_name: String,
    },
    Import {
        path: String,
    },
}

impl Display for VisibilityContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            VisibilityContext::Function { name } => write!(f, "function `{name}`"),
            VisibilityContext::Struct { name } => write!(f, "struct `{name}`"),
            VisibilityContext::Enum { name } => write!(f, "enum `{name}`"),
            VisibilityContext::Field {
                struct_name,
                field_name,
            } => write!(f, "field `{field_name}` of struct `{struct_name}`"),
            VisibilityContext::Method {
                type_name,
                method_name,
            } => write!(f, "method `{method_name}` on type `{type_name}`"),
            VisibilityContext::Import { path } => write!(f, "item `{path}`"),
        }
    }
}

/// Represents a type checking error with source location.
/// All type errors are tied to AST nodes and must have a location.
#[derive(Debug, Clone, Error)]
pub enum TypeCheckError {
    #[error("{location}: type mismatch {context}: expected `{expected}`, found `{found}`")]
    TypeMismatch {
        expected: TypeInfo,
        found: TypeInfo,
        context: TypeMismatchContext,
        location: Location,
    },

    #[error("{location}: unknown type `{name}`")]
    UnknownType { name: String, location: Location },

    #[error("{location}: use of undeclared variable `{name}`")]
    UnknownIdentifier { name: String, location: Location },

    #[error("{location}: call to undefined function `{name}`")]
    UndefinedFunction { name: String, location: Location },

    #[error("{location}: struct `{name}` is not defined")]
    UndefinedStruct { name: String, location: Location },

    #[error("{location}: field `{field_name}` not found on struct `{struct_name}`")]
    FieldNotFound {
        struct_name: String,
        field_name: String,
        location: Location,
    },

    #[error("{location}: variant `{variant_name}` not found on enum `{enum_name}`")]
    VariantNotFound {
        enum_name: String,
        variant_name: String,
        location: Location,
    },

    #[error("{location}: enum `{name}` is not defined")]
    UndefinedEnum { name: String, location: Location },

    #[error("{location}: type member access requires an enum type, found `{found}`")]
    ExpectedEnumType { found: TypeInfo, location: Location },

    #[error("{location}: method `{method_name}` not found on type `{type_name}`")]
    MethodNotFound {
        type_name: String,
        method_name: String,
        location: Location,
    },

    #[error("{location}: {kind} `{name}` expects {expected} arguments, but {found} provided")]
    ArgumentCountMismatch {
        kind: &'static str,
        name: String,
        expected: usize,
        found: usize,
        location: Location,
    },

    #[error(
        "{location}: type parameter count mismatch for `{name}`: expected {expected}, found {found}"
    )]
    TypeParameterCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        location: Location,
    },

    #[error(
        "{location}: function `{function_name}` requires {expected} type parameters, but none were provided"
    )]
    MissingTypeParameters {
        function_name: String,
        expected: usize,
        location: Location,
    },

    #[error(
        "{location}: {expected_kind} operator `{operator:?}` cannot be applied to {operand_desc}"
    )]
    InvalidBinaryOperand {
        operator: OperatorKind,
        expected_kind: &'static str,
        operand_desc: &'static str,
        found_types: (TypeInfo, TypeInfo),
        location: Location,
    },

    #[error(
        "{location}: unary operator `{operator:?}` can only be applied to {expected_type}, found `{found_type}`"
    )]
    InvalidUnaryOperand {
        operator: UnaryOperatorKind,
        expected_type: &'static str,
        found_type: TypeInfo,
        location: Location,
    },

    #[error(
        "{location}: cannot apply operator `{operator:?}` to operands of different types: `{left}` and `{right}`"
    )]
    BinaryOperandTypeMismatch {
        operator: OperatorKind,
        left: TypeInfo,
        right: TypeInfo,
        location: Location,
    },

    #[error("{location}: self reference not allowed in standalone function `{function_name}`")]
    SelfReferenceInFunction {
        function_name: String,
        location: Location,
    },

    #[error("{location}: self reference is only allowed in methods, not functions")]
    SelfReferenceOutsideMethod { location: Location },

    #[error("{location}: cannot resolve import path: {path}")]
    ImportResolutionFailed { path: String, location: Location },

    #[error("{location}: circular glob import detected: {path}::*")]
    CircularImport { path: String, location: Location },

    #[error("{location}: glob import path cannot be empty")]
    EmptyGlobImport { location: Location },

    #[error("{location}: error registering {kind} `{name}`{}", reason.as_ref().map_or(String::new(), |r| format!(": {}", r)))]
    RegistrationFailed {
        kind: RegistrationKind,
        name: String,
        reason: Option<String>,
        location: Location,
    },

    #[error("{location}: expected an array type, found `{found}`")]
    ExpectedArrayType { found: TypeInfo, location: Location },

    #[error("{location}: member access requires a struct type, found `{found}`")]
    ExpectedStructType { found: TypeInfo, location: Location },

    #[error("{location}: cannot call method on non-struct type `{found}`")]
    MethodCallOnNonStruct { found: TypeInfo, location: Location },

    #[error("{location}: array index must be of number type, found `{found}`")]
    ArrayIndexNotNumeric { found: TypeInfo, location: Location },

    #[error(
        "{location}: array elements must be of the same type: expected `{expected}`, found `{found}`"
    )]
    ArrayElementTypeMismatch {
        expected: TypeInfo,
        found: TypeInfo,
        location: Location,
    },

    #[error(
        "{location}: cannot infer type for uzumaki expression assigned to variable of unknown type"
    )]
    CannotInferUzumakiType { location: Location },

    #[error(
        "{location}: cannot infer type parameter `{param_name}` for `{function_name}` - consider adding explicit type arguments"
    )]
    CannotInferTypeParameter {
        function_name: String,
        param_name: String,
        location: Location,
    },

    #[error(
        "{location}: conflicting types for type parameter `{param_name}`: inferred `{first}` and `{second}`"
    )]
    ConflictingTypeInference {
        param_name: String,
        first: TypeInfo,
        second: TypeInfo,
        location: Location,
    },

    #[error("{location}: cannot access private {context}")]
    PrivateAccessViolation {
        context: VisibilityContext,
        location: Location,
    },

    /// Instance method called as associated function.
    ///
    /// This occurs when `Type::method()` syntax is used for a method that requires `self`.
    /// Use `instance.method()` instead.
    #[error("{location}: instance method `{type_name}::{method_name}` requires a receiver, use `instance.{method_name}()` instead")]
    InstanceMethodCalledAsAssociated {
        type_name: String,
        method_name: String,
        location: Location,
    },

    /// Associated function called as instance method.
    ///
    /// This occurs when `instance.function()` syntax is used for an associated function
    /// that doesn't take `self`. Use `Type::function()` instead.
    #[error("{location}: associated function `{type_name}::{method_name}` cannot be called on an instance, use `{type_name}::{method_name}()` instead")]
    AssociatedFunctionCalledAsMethod {
        type_name: String,
        method_name: String,
        location: Location,
    },

    /// Double unary operator is prohibited.
    ///
    /// This occurs when the same unary operator is applied twice consecutively,
    /// such as `--x` or `~~x`. These patterns are prohibited as they are likely mistakes.
    #[error("{location}: double unary operator `{operator:?}{operator:?}` is prohibited")]
    DoubleUnaryOperator {
        operator: UnaryOperatorKind,
        location: Location,
    },
}

impl TypeCheckError {
    /// Returns the source location associated with this error.
    #[must_use]
    pub fn location(&self) -> &Location {
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
            | TypeCheckError::CannotInferUzumakiType { location }
            | TypeCheckError::CannotInferTypeParameter { location, .. }
            | TypeCheckError::ConflictingTypeInference { location, .. }
            | TypeCheckError::PrivateAccessViolation { location, .. }
            | TypeCheckError::InstanceMethodCalledAsAssociated { location, .. }
            | TypeCheckError::AssociatedFunctionCalledAsMethod { location, .. }
            | TypeCheckError::DoubleUnaryOperator { location, .. } => location,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_info::{NumberType, TypeInfoKind};

    fn test_location() -> Location {
        Location {
            offset_start: 4,
            offset_end: 9,
            start_line: 1,
            start_column: 5,
            end_line: 1,
            end_column: 10,
        }
    }

    #[test]
    fn display_type_mismatch() {
        let err = TypeCheckError::TypeMismatch {
            expected: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            found: TypeInfo::default(),
            context: TypeMismatchContext::Assignment,
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: type mismatch in assignment: expected `Bool`, found `Unit`"
        );
    }

    #[test]
    fn display_unknown_type() {
        let err = TypeCheckError::UnknownType {
            name: "Foo".to_string(),
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: unknown type `Foo`");
    }

    #[test]
    fn display_field_not_found() {
        let err = TypeCheckError::FieldNotFound {
            struct_name: "Point".to_string(),
            field_name: "z".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: field `z` not found on struct `Point`"
        );
    }

    #[test]
    fn display_registration_failed_without_reason() {
        let err = TypeCheckError::RegistrationFailed {
            kind: RegistrationKind::Type,
            name: "Foo".to_string(),
            reason: None,
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: error registering type `Foo`");
    }

    #[test]
    fn display_registration_failed_with_reason() {
        let err = TypeCheckError::RegistrationFailed {
            kind: RegistrationKind::Method,
            name: "bar".to_string(),
            reason: Some("duplicate definition".to_string()),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: error registering method `bar`: duplicate definition"
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
                arg_name: "x".to_string(),
                arg_index: 0
            }
            .to_string(),
            "in argument 0 `x` of function `foo`"
        );
        assert_eq!(
            TypeMismatchContext::MethodArgument {
                type_name: "Point".to_string(),
                method_name: "move_by".to_string(),
                arg_name: "dx".to_string(),
                arg_index: 0
            }
            .to_string(),
            "in argument 0 `dx` of method `Point::move_by`"
        );
    }

    #[test]
    fn display_registration_kind() {
        assert_eq!(RegistrationKind::Type.to_string(), "type");
        assert_eq!(RegistrationKind::Struct.to_string(), "struct");
        assert_eq!(RegistrationKind::Enum.to_string(), "enum");
        assert_eq!(RegistrationKind::Spec.to_string(), "spec");
        assert_eq!(RegistrationKind::Function.to_string(), "function");
        assert_eq!(RegistrationKind::Method.to_string(), "method");
        assert_eq!(RegistrationKind::Variable.to_string(), "variable");
    }

    #[test]
    fn error_location_accessor() {
        let loc = test_location();
        let err = TypeCheckError::UnknownType {
            name: "Foo".to_string(),
            location: loc,
        };
        assert_eq!(err.location(), &loc);
    }

    #[test]
    fn display_unknown_identifier() {
        let err = TypeCheckError::UnknownIdentifier {
            name: "myVar".to_string(),
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: use of undeclared variable `myVar`");
    }

    #[test]
    fn display_undefined_function() {
        let err = TypeCheckError::UndefinedFunction {
            name: "myFunc".to_string(),
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: call to undefined function `myFunc`");
    }

    #[test]
    fn display_undefined_struct() {
        let err = TypeCheckError::UndefinedStruct {
            name: "MyStruct".to_string(),
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: struct `MyStruct` is not defined");
    }

    #[test]
    fn display_method_not_found() {
        let err = TypeCheckError::MethodNotFound {
            type_name: "Point".to_string(),
            method_name: "rotate".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: method `rotate` not found on type `Point`"
        );
    }

    #[test]
    fn display_argument_count_mismatch() {
        let err = TypeCheckError::ArgumentCountMismatch {
            kind: "function",
            name: "add".to_string(),
            expected: 2,
            found: 3,
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: function `add` expects 2 arguments, but 3 provided"
        );
    }

    #[test]
    fn display_type_parameter_count_mismatch() {
        let err = TypeCheckError::TypeParameterCountMismatch {
            name: "Vec".to_string(),
            expected: 1,
            found: 2,
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: type parameter count mismatch for `Vec`: expected 1, found 2"
        );
    }

    #[test]
    fn display_missing_type_parameters() {
        let err = TypeCheckError::MissingTypeParameters {
            function_name: "generic_fn".to_string(),
            expected: 2,
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: function `generic_fn` requires 2 type parameters, but none were provided"
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
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: numeric operator `Add` cannot be applied to non-numeric types"
        );
    }

    #[test]
    fn display_invalid_unary_operand() {
        let err = TypeCheckError::InvalidUnaryOperand {
            operator: UnaryOperatorKind::Not,
            expected_type: "booleans",
            found_type: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: unary operator `Not` can only be applied to booleans, found `Bool`"
        );
    }

    #[test]
    fn display_binary_operand_type_mismatch() {
        let err = TypeCheckError::BinaryOperandTypeMismatch {
            operator: OperatorKind::Add,
            left: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            right: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot apply operator `Add` to operands of different types: `i32` and `i64`"
        );
    }

    #[test]
    fn display_self_reference_in_function() {
        let err = TypeCheckError::SelfReferenceInFunction {
            function_name: "standalone_fn".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: self reference not allowed in standalone function `standalone_fn`"
        );
    }

    #[test]
    fn display_self_reference_outside_method() {
        let err = TypeCheckError::SelfReferenceOutsideMethod {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: self reference is only allowed in methods, not functions"
        );
    }

    #[test]
    fn display_import_resolution_failed() {
        let err = TypeCheckError::ImportResolutionFailed {
            path: "std::collections::HashMap".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot resolve import path: std::collections::HashMap"
        );
    }

    #[test]
    fn display_circular_import() {
        let err = TypeCheckError::CircularImport {
            path: "mod_a::mod_b".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: circular glob import detected: mod_a::mod_b::*"
        );
    }

    #[test]
    fn display_empty_glob_import() {
        let err = TypeCheckError::EmptyGlobImport {
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: glob import path cannot be empty");
    }

    #[test]
    fn display_expected_array_type() {
        let err = TypeCheckError::ExpectedArrayType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: expected an array type, found `i32`");
    }

    #[test]
    fn display_expected_struct_type() {
        let err = TypeCheckError::ExpectedStructType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: member access requires a struct type, found `i32`"
        );
    }

    #[test]
    fn display_method_call_on_non_struct() {
        let err = TypeCheckError::MethodCallOnNonStruct {
            found: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot call method on non-struct type `Bool`"
        );
    }

    #[test]
    fn display_array_index_not_numeric() {
        let err = TypeCheckError::ArrayIndexNotNumeric {
            found: TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: array index must be of number type, found `Bool`"
        );
    }

    #[test]
    fn display_array_element_type_mismatch() {
        let err = TypeCheckError::ArrayElementTypeMismatch {
            expected: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: array elements must be of the same type: expected `i32`, found `i64`"
        );
    }

    #[test]
    fn display_cannot_infer_uzumaki_type() {
        let err = TypeCheckError::CannotInferUzumakiType {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot infer type for uzumaki expression assigned to variable of unknown type"
        );
    }

    #[test]
    fn display_variant_not_found() {
        let err = TypeCheckError::VariantNotFound {
            enum_name: "Color".to_string(),
            variant_name: "Yellow".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: variant `Yellow` not found on enum `Color`"
        );
    }

    #[test]
    fn display_undefined_enum() {
        let err = TypeCheckError::UndefinedEnum {
            name: "UnknownEnum".to_string(),
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: enum `UnknownEnum` is not defined");
    }

    #[test]
    fn display_expected_enum_type() {
        let err = TypeCheckError::ExpectedEnumType {
            found: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: type member access requires an enum type, found `i32`"
        );
    }

    #[test]
    fn display_visibility_context_function() {
        let ctx = VisibilityContext::Function {
            name: "helper".to_string(),
        };
        assert_eq!(ctx.to_string(), "function `helper`");
    }

    #[test]
    fn display_visibility_context_struct() {
        let ctx = VisibilityContext::Struct {
            name: "Data".to_string(),
        };
        assert_eq!(ctx.to_string(), "struct `Data`");
    }

    #[test]
    fn display_visibility_context_enum() {
        let ctx = VisibilityContext::Enum {
            name: "Color".to_string(),
        };
        assert_eq!(ctx.to_string(), "enum `Color`");
    }

    #[test]
    fn display_visibility_context_field() {
        let ctx = VisibilityContext::Field {
            struct_name: "Point".to_string(),
            field_name: "x".to_string(),
        };
        assert_eq!(ctx.to_string(), "field `x` of struct `Point`");
    }

    #[test]
    fn display_visibility_context_method() {
        let ctx = VisibilityContext::Method {
            type_name: "Counter".to_string(),
            method_name: "increment".to_string(),
        };
        assert_eq!(ctx.to_string(), "method `increment` on type `Counter`");
    }

    #[test]
    fn display_visibility_context_import() {
        let ctx = VisibilityContext::Import {
            path: "inner::private_fn".to_string(),
        };
        assert_eq!(ctx.to_string(), "item `inner::private_fn`");
    }

    #[test]
    fn display_private_access_violation_function() {
        let err = TypeCheckError::PrivateAccessViolation {
            context: VisibilityContext::Function {
                name: "helper".to_string(),
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private function `helper`"
        );
    }

    #[test]
    fn display_private_access_violation_field() {
        let err = TypeCheckError::PrivateAccessViolation {
            context: VisibilityContext::Field {
                struct_name: "Point".to_string(),
                field_name: "x".to_string(),
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private field `x` of struct `Point`"
        );
    }

    #[test]
    fn display_private_access_violation_method() {
        let err = TypeCheckError::PrivateAccessViolation {
            context: VisibilityContext::Method {
                type_name: "Counter".to_string(),
                method_name: "reset".to_string(),
            },
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private method `reset` on type `Counter`"
        );
    }

    #[test]
    fn display_instance_method_called_as_associated() {
        let err = TypeCheckError::InstanceMethodCalledAsAssociated {
            type_name: "Point".to_string(),
            method_name: "distance".to_string(),
            location: test_location(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Point"));
        assert!(msg.contains("distance"));
        assert!(msg.contains("requires a receiver"));
    }

    #[test]
    fn display_associated_function_called_as_method() {
        let err = TypeCheckError::AssociatedFunctionCalledAsMethod {
            type_name: "Point".to_string(),
            method_name: "new".to_string(),
            location: test_location(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Point"));
        assert!(msg.contains("new"));
        assert!(msg.contains("cannot be called on an instance"));
    }

    #[test]
    fn display_double_unary_operator() {
        let err = TypeCheckError::DoubleUnaryOperator {
            operator: UnaryOperatorKind::Neg,
            location: test_location(),
        };
        let msg = err.to_string();
        assert!(msg.contains("double unary operator"));
        assert!(msg.contains("Neg"));
        assert!(msg.contains("prohibited"));
    }

    #[test]
    fn display_double_unary_operator_bitnot() {
        let err = TypeCheckError::DoubleUnaryOperator {
            operator: UnaryOperatorKind::BitNot,
            location: test_location(),
        };
        let msg = err.to_string();
        assert!(msg.contains("double unary operator"));
        assert!(msg.contains("BitNot"));
        assert!(msg.contains("prohibited"));
    }
}