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
    Assert,
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
            TypeMismatchContext::Assert => write!(f, "in assert statement"),
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
    Constant {
        name: String,
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
            VisibilityContext::Constant { name } => write!(f, "constant `{name}`"),
        }
    }
}

/// Categorizes errors that participate in `(DedupKind, name)` deduplication.
///
/// The set is empirical: only variants that the registration and inference
/// passes have actually been observed to emit for the same symbol twice are
/// listed here. Other `TypeCheckError` variants are always recorded as-is.
/// When adding a new diagnostic that the walker can hit from multiple
/// passes, extend both this enum and `TypeCheckError::dedup_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DedupKind {
    UnknownType,
    UndefinedFunction,
    UnknownIdentifier,
    UndefinedStruct,
    UndefinedEnum,
    SpecFunctionShadowsTopLevel,
    ImportedItemNotFound,
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

    /// A `::`-qualified path whose prefix names a known namespace (a file or an
    /// imported namespace) but whose final segment does not name a value in it —
    /// either nothing of that name exists, or it names a non-value item such as a
    /// function. Replaces the misleading "enum `lib` is not defined" that the
    /// enum-variant fallback would otherwise produce for a namespace path.
    #[error("{location}: cannot resolve `{path}`{}", names.as_ref().map_or(String::new(), |n| format!(": `{path}` names {n}, not a value")))]
    QualifiedPathNotAValue {
        path: String,
        /// What the final segment names instead of a value (`a function`), or
        /// `None` when nothing of that name exists in the namespace.
        names: Option<String>,
        location: Location,
    },

    /// A namespace-qualified call whose target function exists, but the path is
    /// blocked because an intermediate file imported the next namespace with a
    /// plain `use` rather than `pub use`. The function is reachable in principle
    /// — only the missing re-export hides it — so the fix is to add `pub use`,
    /// not to correct the path. Distinct from [`Self::UndefinedFunction`], which
    /// fires when no such function exists.
    #[error(
        "{location}: call to `{path}` is blocked: an intermediate file imports the next namespace with a plain `use`; change it to `pub use` to re-export the path"
    )]
    QualifiedPathNotReexported { path: String, location: Location },

    /// A path-form `use a::b;` was written without a project context (the
    /// string-parse and REPL paths have only the entry file), so the named file
    /// cannot exist. Distinct from a typo: the fix is to build the project, not
    /// to correct the path.
    #[error(
        "{location}: file imports require a project context: `use {path};` names a source file, which is only resolvable when building a project (not a single string-parsed file)"
    )]
    FileImportWithoutProjectContext { path: String, location: Location },

    /// A qualified call `ns::fn()` whose head `ns` names a file in the project but
    /// was never bound by a `use` in the calling file. The head is a namespace, not
    /// a type, so a "method not found on type `ns`" diagnostic would point at the
    /// wrong fix; the fix is to import the namespace.
    #[error(
        "{location}: namespace `{namespace}` is not imported; add `use {namespace};` to call `{namespace}::{function}`"
    )]
    UnimportedNamespaceCall {
        namespace: String,
        function: String,
        location: Location,
    },

    /// An absolute `dir::file::item` reference whose namespace prefix names a real
    /// project file the accessing file never imported. The full namespace path is
    /// known here, so the fix is exact (`use lib::geom;`). A file may only reach
    /// another file's surface through a `use`; an absolute path — whether a call,
    /// a type, or a const value — is not an exception to that, which is why this
    /// is reported rather than silently resolved.
    #[error(
        "{location}: namespace `{namespace}` is not imported; add `use {namespace};` to reach `{namespace}::{item}`"
    )]
    UnimportedAbsoluteNamespacePath {
        namespace: String,
        item: String,
        location: Location,
    },

    /// A `::`-qualified path whose namespace portion is not an imported namespace,
    /// and whose target file is not in the compilation closure (no `mod_scopes`
    /// key covers the namespace portion). Unlike
    /// [`Self::UnimportedAbsoluteNamespacePath`], the namespace cannot be *proven*
    /// to exist here, so the suggestion is hedged: if `{namespace}` does name a
    /// source file, importing it is the fix; if it is a typo, the path is simply
    /// wrong. The hedged wording keeps the confident variant's "the namespace
    /// provably exists; this exact `use` resolves it" contract intact.
    #[error(
        "{location}: could not resolve `{path}`: `{namespace}` is not an imported namespace. if `{namespace}` names a source file, import it with `use {namespace};`"
    )]
    UnresolvedNamespacePath {
        path: String,
        namespace: String,
        location: Location,
    },

    /// An item import `use a::b::{x};` named an item `x` that does not exist in
    /// file `a::b`.
    #[error("{location}: item `{item}` not found in file `{file}`")]
    ImportedItemNotFound {
        item: String,
        file: String,
        location: Location,
    },

    /// An item import named an item that exists in the target file but is not
    /// `pub`, so it cannot cross the file boundary. Carries the definition site so
    /// the fix (adding `pub`) can be pointed at directly.
    #[error(
        "{location}: item `{item}` in file `{file}` is private\nnote: `{item}` is defined at {definition_location} in file `{file}`; add `pub` to export it"
    )]
    ImportedItemPrivate {
        item: String,
        file: String,
        location: Location,
        definition_location: Location,
    },

    /// An imported name collides with a name already bound in the importing
    /// file — either a local definition or an earlier import.
    #[error("{location}: imported name `{name}` collides with {with} of the same name")]
    ImportNameCollision {
        name: String,
        with: String,
        location: Location,
    },

    /// `use a::b::{};` — a braced item import with no items. The braces say
    /// "import items" but none are listed, so the directive does nothing.
    #[error(
        "{location}: empty import list in `use {path}::{{}};` — import the file (`use {path};`) or list the items to import (`use {path}::{{x, y}};`)"
    )]
    EmptyImportList { path: String, location: Location },

    /// A cycle among definition *values* — consts whose initializers reference
    /// each other, or mutually recursive type aliases — across one or more files.
    /// `cycle` names the members in order (e.g. `A -> B -> A`). File-to-file
    /// import cycles are allowed and never reach here; only value cycles, which
    /// have no computable evaluation order, are rejected.
    #[error("{location}: circular definition detected: {cycle}")]
    CircularDefinition { cycle: String, location: Location },

    #[error("{location}: error registering {kind} `{name}`{}", reason.as_ref().map_or(String::new(), |r| format!(": {}", r)))]
    RegistrationFailed {
        kind: RegistrationKind,
        name: String,
        reason: Option<String>,
        location: Location,
    },

    /// A function defined inside a `spec` block shadows a top-level function
    /// of the same name. Shadowing across the spec/top-level boundary is
    /// rejected because codegen prefers the spec-mangled lookup but the
    /// type-checker types call sites against the closest binding, so the two
    /// layers would silently disagree on which callee is invoked. Rename one
    /// side to disambiguate.
    #[error(
        "{location}: function `{function_name}` inside spec `{spec_name}` shadows a top-level function of the same name; rename one to disambiguate"
    )]
    SpecFunctionShadowsTopLevel {
        spec_name: String,
        function_name: String,
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

    /// Access to a private item from outside its defining file.
    ///
    /// Carries both the use site (`location`) and the definition site
    /// (`definition_location` in file `definition_file`) so the diagnostic points
    /// the user at where to add `pub`. `definition_file` is the `::`-joined module
    /// path of the defining file, empty for the entry file.
    #[error(
        "{location}: cannot access private {context}\nnote: {context} is defined at {definition_location}{}; add `pub` to export it",
        if definition_file.is_empty() { String::new() } else { format!(" in file `{definition_file}`") }
    )]
    PrivateAccessViolation {
        context: VisibilityContext,
        location: Location,
        definition_location: Location,
        definition_file: String,
    },

    /// A `spec`-inner function reached through a qualified path (`Check::verify`,
    /// `lib::Check::verify`). `spec` blocks are proof-only: their functions exist
    /// for verification and are never assigned an executable index, so a qualified
    /// call to one would type-check and then have no callee to lower. A spec
    /// function is reached only by its bare name from within the same spec; this
    /// rejects every qualified form so the proof-only boundary is explicit. The
    /// message names the bare function so the fix (drop the qualifier, call from
    /// within the spec) is concrete.
    #[error(
        "{location}: cannot call spec function `{path}` through a qualified path; spec functions are proof-only and are reached only by their bare name `{function_name}` within the spec"
    )]
    SpecFunctionNotCallable {
        path: String,
        function_name: String,
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

    /// Assignment to an immutable variable.
    ///
    /// Variables declared with `let` (without `mut`) cannot be reassigned.
    /// Use `let mut` to declare a mutable binding.
    #[error("{location}: cannot assign to immutable variable `{name}`")]
    AssignToImmutable { name: String, location: Location },

    /// Variable shadows a binding from an outer scope.
    ///
    /// Shadowing is prohibited to prevent ambiguity about which binding is
    /// referenced. Rename the inner variable to avoid confusion.
    #[error("{location}: variable `{name}` shadows a binding from an outer scope")]
    VariableShadowed { name: String, location: Location },

    // LiteralOutOfRange, ArrayLiteralAsArgument, StructLiteralAsArgument,
    // CompoundLiteralInUnsupportedPosition, ArrayUzumakiAsArgument,
    // CompoundReturnCallInExpressionPosition, ArrayIndex64Bit:
    // Migrated to analysis rules A012-A019, A022.

    /// Array size is invalid: zero, negative, or exceeds `u32::MAX`.
    ///
    /// Array sizes must be positive integer literals that fit in 32 bits.
    #[error(
        "{location}: invalid array size `{size}`; must be a positive integer that fits in 32 bits"
    )]
    InvalidArraySize { size: String, location: Location },

    /// A required field is missing from a struct literal.
    #[error("{location}: missing field `{field_name}` in struct literal `{struct_name}`")]
    MissingStructField {
        struct_name: String,
        field_name: String,
        location: Location,
    },

    /// An unknown field is provided in a struct literal.
    #[error("{location}: unknown field `{field_name}` in struct literal `{struct_name}`")]
    UnknownStructField {
        struct_name: String,
        field_name: String,
        location: Location,
    },

    /// A field is provided more than once in a struct literal.
    #[error("{location}: duplicate field `{field_name}` in struct literal `{struct_name}`")]
    DuplicateStructField {
        struct_name: String,
        field_name: String,
        location: Location,
    },

    // MethodCallChainOnCompoundReturn, CompoundReturnCallInAssignment:
    // Migrated to analysis rules A017-A018.

    /// Duplicate field name in a struct definition.
    ///
    /// Each field in a struct definition must have a unique name.
    #[error("{location}: duplicate field `{field_name}` in struct definition `{struct_name}`")]
    DuplicateStructFieldDefinition {
        struct_name: String,
        field_name: String,
        location: Location,
    },

    /// Recursive struct definition where a field's type references the struct itself.
    ///
    /// Struct types must have a finite size, so a struct cannot contain itself
    /// (directly or transitively) as a field.
    #[error(
        "{location}: recursive struct definition: field `{field_name}` of struct `{struct_name}` has type `{field_type}` which creates a cycle"
    )]
    RecursiveStructDefinition {
        struct_name: String,
        field_name: String,
        field_type: String,
        location: Location,
    },

    /// Assignment target is not a valid lvalue.
    ///
    /// Only identifiers, array index accesses, and struct member accesses can
    /// appear on the left side of an assignment.
    #[error("{location}: invalid assignment target; expected a variable, array element, or struct field")]
    InvalidAssignmentTarget { location: Location },

    /// Array literal size does not match the declared array type size.
    #[error(
        "{location}: array literal has {actual} elements but the declared type expects {expected}"
    )]
    ArrayLiteralSizeMismatch {
        expected: u32,
        actual: usize,
        location: Location,
    },

    /// Division or modulo by literal zero.
    #[error("{location}: division by zero")]
    DivisionByZero { location: Location },

    /// Duplicate variant name in an enum definition.
    ///
    /// Each variant in an enum definition must have a unique name.
    #[error("{location}: duplicate variant `{variant_name}` in enum definition `{enum_name}`")]
    DuplicateEnumVariant {
        enum_name: String,
        variant_name: String,
        location: Location,
    },

    /// An `external fn` is named by more than one `use … from <module>` clause,
    /// each referring to a different module.
    ///
    /// Extern provenance must be unambiguous: the linker needs exactly one
    /// source module per extern. List the offending modules and rename or
    /// remove the conflicting `use` clauses to disambiguate.
    #[error(
        "{location}: external function `{name}` is bound to multiple modules ({modules}); each extern must come from exactly one module"
    )]
    AmbiguousExternModule {
        name: String,
        modules: String,
        location: Location,
    },

    /// A `use { name } from <module>` clause names an import that has no
    /// matching `external fn` declaration.
    ///
    /// A `from` import binds an extern to its source module; without a
    /// corresponding `external fn name(...)` declaration there is nothing to
    /// bind, so the import is dangling. Declare the extern or drop the import.
    #[error(
        "{location}: `use` imports `{name}` from module `{module}`, but no `external fn {name}` is declared"
    )]
    ExternImportNotDeclared {
        name: String,
        module: String,
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
            | TypeCheckError::QualifiedPathNotAValue { location, .. }
            | TypeCheckError::QualifiedPathNotReexported { location, .. }
            | TypeCheckError::FileImportWithoutProjectContext { location, .. }
            | TypeCheckError::UnimportedNamespaceCall { location, .. }
            | TypeCheckError::UnimportedAbsoluteNamespacePath { location, .. }
            | TypeCheckError::UnresolvedNamespacePath { location, .. }
            | TypeCheckError::ImportedItemNotFound { location, .. }
            | TypeCheckError::ImportedItemPrivate { location, .. }
            | TypeCheckError::ImportNameCollision { location, .. }
            | TypeCheckError::EmptyImportList { location, .. }
            | TypeCheckError::CircularDefinition { location, .. }
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
            | TypeCheckError::SpecFunctionNotCallable { location, .. }
            | TypeCheckError::InstanceMethodCalledAsAssociated { location, .. }
            | TypeCheckError::AssociatedFunctionCalledAsMethod { location, .. }
            | TypeCheckError::AssignToImmutable { location, .. }
            | TypeCheckError::VariableShadowed { location, .. }
            | TypeCheckError::InvalidArraySize { location, .. }
            | TypeCheckError::MissingStructField { location, .. }
            | TypeCheckError::UnknownStructField { location, .. }
            | TypeCheckError::DuplicateStructField { location, .. }
            | TypeCheckError::DuplicateStructFieldDefinition { location, .. }
            | TypeCheckError::RecursiveStructDefinition { location, .. }
            | TypeCheckError::InvalidAssignmentTarget { location, .. }
            | TypeCheckError::ArrayLiteralSizeMismatch { location, .. }
            | TypeCheckError::DivisionByZero { location, .. }
            | TypeCheckError::DuplicateEnumVariant { location, .. }
            | TypeCheckError::AmbiguousExternModule { location, .. }
            | TypeCheckError::ExternImportNotDeclared { location, .. }
            | TypeCheckError::SpecFunctionShadowsTopLevel { location, .. } => location,
        }
    }

    /// Returns the deduplication key for variants that participate in
    /// name-based deduplication, or `None` for variants that are always
    /// reported as-is. The returned tuple of `(DedupKind, String)` is used
    /// as a `FxHashSet` key inside the type checker so the same diagnostic
    /// for the same symbol is recorded only once even when both registration
    /// and inference visit it.
    pub(crate) fn dedup_key(&self) -> Option<(DedupKind, String)> {
        match self {
            TypeCheckError::UnknownType { name, .. } => {
                Some((DedupKind::UnknownType, name.clone()))
            }
            TypeCheckError::UndefinedFunction { name, .. } => {
                Some((DedupKind::UndefinedFunction, name.clone()))
            }
            TypeCheckError::UnknownIdentifier { name, .. } => {
                Some((DedupKind::UnknownIdentifier, name.clone()))
            }
            TypeCheckError::UndefinedStruct { name, .. } => {
                Some((DedupKind::UndefinedStruct, name.clone()))
            }
            TypeCheckError::UndefinedEnum { name, .. } => {
                Some((DedupKind::UndefinedEnum, name.clone()))
            }
            TypeCheckError::SpecFunctionShadowsTopLevel {
                spec_name,
                function_name,
                ..
            } => Some((
                DedupKind::SpecFunctionShadowsTopLevel,
                format!("{spec_name}:{function_name}"),
            )),
            // A cyclic or transitive unresolvable item re-export is reported from
            // every failing import site that names the same target file, producing
            // identical "item X not found in file Y" text. Dedup by `(item, file)`
            // — deliberately excluding the location — so each unresolved item is
            // surfaced once regardless of how many import sites hit it.
            TypeCheckError::ImportedItemNotFound { item, file, .. } => Some((
                DedupKind::ImportedItemNotFound,
                format!("{item}@{file}"),
            )),
            _ => None,
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
        assert_eq!(TypeMismatchContext::Condition.to_string(), "in condition");
        assert_eq!(
            TypeMismatchContext::Assert.to_string(),
            "in assert statement"
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
    fn display_circular_definition() {
        let err = TypeCheckError::CircularDefinition {
            cycle: "A -> B -> A".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: circular definition detected: A -> B -> A"
        );
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
            definition_location: test_location(),
            definition_file: "lib::arith".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private function `helper`\n\
             note: function `helper` is defined at 1:5 in file `lib::arith`; add `pub` to export it"
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
            definition_location: test_location(),
            definition_file: "lib::geo".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private field `x` of struct `Point`\n\
             note: field `x` of struct `Point` is defined at 1:5 in file `lib::geo`; add `pub` to export it"
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
            definition_location: test_location(),
            definition_file: "lib::counter".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private method `reset` on type `Counter`\n\
             note: method `reset` on type `Counter` is defined at 1:5 in file `lib::counter`; add `pub` to export it"
        );
    }

    #[test]
    fn display_private_access_violation_entry_file_omits_file_note() {
        let err = TypeCheckError::PrivateAccessViolation {
            context: VisibilityContext::Function {
                name: "helper".to_string(),
            },
            location: test_location(),
            definition_location: test_location(),
            definition_file: String::new(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot access private function `helper`\n\
             note: function `helper` is defined at 1:5; add `pub` to export it"
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
    fn display_assign_to_immutable() {
        let err = TypeCheckError::AssignToImmutable {
            name: "x".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: cannot assign to immutable variable `x`"
        );
    }

    // Tests for migrated error variants (A012-A019, A022) removed.
    // See analysis rules tests for coverage.

    #[test]
    fn display_invalid_array_size_overflow() {
        let err = TypeCheckError::InvalidArraySize {
            size: "999999999999999999".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: invalid array size `999999999999999999`; must be a positive integer that fits in 32 bits"
        );
    }

    #[test]
    fn display_invalid_array_size_zero() {
        let err = TypeCheckError::InvalidArraySize {
            size: "0".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: invalid array size `0`; must be a positive integer that fits in 32 bits"
        );
    }

    #[test]
    fn display_variable_shadowed() {
        let err = TypeCheckError::VariableShadowed {
            name: "x".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: variable `x` shadows a binding from an outer scope"
        );
    }

    #[test]
    fn display_missing_struct_field() {
        let err = TypeCheckError::MissingStructField {
            struct_name: "Point".to_string(),
            field_name: "y".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: missing field `y` in struct literal `Point`"
        );
    }

    #[test]
    fn display_unknown_struct_field() {
        let err = TypeCheckError::UnknownStructField {
            struct_name: "Point".to_string(),
            field_name: "z".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: unknown field `z` in struct literal `Point`"
        );
    }

    #[test]
    fn display_duplicate_struct_field() {
        let err = TypeCheckError::DuplicateStructField {
            struct_name: "Point".to_string(),
            field_name: "x".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: duplicate field `x` in struct literal `Point`"
        );
    }

    #[test]
    fn display_duplicate_struct_field_definition() {
        let err = TypeCheckError::DuplicateStructFieldDefinition {
            struct_name: "Point".to_string(),
            field_name: "x".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: duplicate field `x` in struct definition `Point`"
        );
    }

    #[test]
    fn display_recursive_struct_definition() {
        let err = TypeCheckError::RecursiveStructDefinition {
            struct_name: "Node".to_string(),
            field_name: "next".to_string(),
            field_type: "Node".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: recursive struct definition: field `next` of struct `Node` has type `Node` which creates a cycle"
        );
    }

    #[test]
    fn display_invalid_assignment_target() {
        let err = TypeCheckError::InvalidAssignmentTarget {
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: invalid assignment target; expected a variable, array element, or struct field"
        );
    }

    #[test]
    fn display_array_literal_size_mismatch() {
        let err = TypeCheckError::ArrayLiteralSizeMismatch {
            expected: 3,
            actual: 5,
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: array literal has 5 elements but the declared type expects 3"
        );
    }

    #[test]
    fn display_division_by_zero() {
        let err = TypeCheckError::DivisionByZero {
            location: test_location(),
        };
        assert_eq!(err.to_string(), "1:5: division by zero");
    }

    #[test]
    fn display_duplicate_enum_variant() {
        let err = TypeCheckError::DuplicateEnumVariant {
            enum_name: "Color".to_string(),
            variant_name: "Red".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: duplicate variant `Red` in enum definition `Color`"
        );
    }

    #[test]
    fn display_ambiguous_extern_module() {
        let err = TypeCheckError::AmbiguousExternModule {
            name: "sort".to_string(),
            modules: "`sorting`, `collections`".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: external function `sort` is bound to multiple modules (`sorting`, `collections`); each extern must come from exactly one module"
        );
    }

    #[test]
    fn display_extern_import_not_declared() {
        let err = TypeCheckError::ExternImportNotDeclared {
            name: "hash".to_string(),
            module: "crypto".to_string(),
            location: test_location(),
        };
        assert_eq!(
            err.to_string(),
            "1:5: `use` imports `hash` from module `crypto`, but no `external fn hash` is declared"
        );
    }

    // Tests for CompoundReturnCallInAssignment and MethodCallChainOnCompoundReturn
    // migrated to analysis rules A017 and A018.
}
