# Type Checker Error Reference

Complete catalog of type checking errors with examples and solutions.

## Error Overview

The type checker produces 33 distinct error variants, each with specific context and location
information. All errors implement the `Error` trait and provide detailed messages.

All errors include a precise source location in the form `line:column:` at the start of their
message. The `TypeCheckError::location()` method returns the associated `Location` directly.

## Error Categories

1. [Type Mismatch Errors](#type-mismatch-errors)
2. [Symbol Resolution Errors](#symbol-resolution-errors)
3. [Visibility Errors](#visibility-errors)
4. [Function and Method Errors](#function-and-method-errors)
5. [Operator Errors](#operator-errors)
6. [Import Errors](#import-errors)
7. [Registration Errors](#registration-errors)
8. [Structural Errors](#structural-errors)
9. [Generic Type Errors](#generic-type-errors)
10. [Non-Deterministic Errors](#non-deterministic-errors)

---

## Type Mismatch Errors

### `TypeMismatch`

Type of an expression does not match the expected type.

**`TypeMismatchContext` variants**:

```rust
pub enum TypeMismatchContext {
    Assignment,
    Return,
    VariableDefinition,
    BinaryOperation(OperatorKind),
    Condition,
    FunctionArgument { function_name, arg_name, arg_index },
    MethodArgument { type_name, method_name, arg_name, arg_index },
    ArrayElement,
}
```

**Examples**:

```rust
// Return statement mismatch
fn test() -> i32 {
    return true;  // Error: type mismatch in return statement: expected `i32`, found `Bool`
}

// Variable definition mismatch
fn test() {
    let x: i32 = true;  // Error: type mismatch in variable definition: expected `i32`, found `Bool`
}

// Function argument mismatch
fn greet(name: string) -> string { return name; }

fn test() {
    greet(42);  // Error: type mismatch in argument 0 `name` of function `greet`: expected `string`, found `i32`
}
```

**Solution**: Ensure the expression evaluates to the expected type.

### `ArrayElementTypeMismatch`

Array elements must all share the same type. Emitted when a later element differs from the
first element's type.

**Example**:

```rust
fn test() {
    let arr: [i32; 3] = [1, 2, true];
    // Error: array elements must be of the same type: expected `i32`, found `Bool`
}
```

---

## Symbol Resolution Errors

### `UnknownType`

Referenced type name is not defined in scope.

**Example**:

```rust
fn test(x: UndefinedType) -> i32 {  // Error: unknown type `UndefinedType`
    return 42;
}
```

**Solution**: Define the type before using it, or check for typos in the type name.

### `UnknownIdentifier`

Variable or identifier is used before declaration.

**Example**:

```rust
fn test() {
    let y = x + 10;  // Error: use of undeclared variable `x`
}
```

**Solution**: Declare the variable before use, or check for typos.

### `UndefinedFunction`

Function is called but not defined.

**Example**:

```rust
fn test() {
    let result = unknown_function(42);  // Error: call to undefined function `unknown_function`
}
```

**Solution**: Define the function, import it, or check for typos.

### `UndefinedStruct`

Struct literal or field access references a struct that is not defined.

**Example**:

```rust
fn test() {
    let p = MissingStruct { x: 1 };  // Error: struct `MissingStruct` is not defined
}
```

### `UndefinedEnum`

Enum variant access references an enum that is not defined.

**Example**:

```rust
fn test() {
    let c = UnknownEnum::Variant;  // Error: enum `UnknownEnum` is not defined
}
```

### `MethodNotFound`

Method is called on a type but the method is not defined in any `impl` block for that type.

**Example**:

```rust
struct Point { x: i32, y: i32 }

fn test() {
    let p = Point { x: 10, y: 20 };
    let result = p.distance();  // Error: method `distance` not found on type `Point`
}
```

**Solution**: Define the method in an `impl` block for the type, or check for typos.

### `FieldNotFound`

Struct field access names a field that does not exist on the struct.

**Example**:

```rust
struct Point { x: i32, y: i32 }

fn test() {
    let p = Point { x: 10, y: 20 };
    let z = p.z;  // Error: field `z` not found on struct `Point`
}
```

**Solution**: Use an existing field name or add the field to the struct definition.

### `VariantNotFound`

Enum variant access names a variant that is not defined on the enum.

**Example**:

```rust
enum Color { Red, Green, Blue }

fn test() {
    let c = Color::Yellow;  // Error: variant `Yellow` not found on enum `Color`
}
```

**Solution**: Use an existing variant or add it to the enum definition.

---

## Visibility Errors

### `PrivateAccessViolation`

Attempting to access a private symbol from outside its defining scope.

**`VisibilityContext` variants**:

```rust
pub enum VisibilityContext {
    Function { name },
    Struct { name },
    Enum { name },
    Field { struct_name, field_name },
    Method { type_name, method_name },
    Import { path },
}
```

**Examples**:

```rust
// Private field access
struct Point {
    x: i32,  // Private by default
    y: i32,
}

fn test() {
    let p = Point { x: 10, y: 20 };
    let x = p.x;  // Error: cannot access private field `x` of struct `Point`
}

// Private method
struct Counter { value: i32 }

impl Counter {
    fn internal_reset(&self) {}  // Private method
}

fn test() {
    let c = Counter { value: 0 };
    c.internal_reset();  // Error: cannot access private method `internal_reset` on type `Counter`
}
```

**Solution**: Make the symbol public with `pub` or access it only from within its defining scope.

---

## Function and Method Errors

### `ArgumentCountMismatch`

Function or method called with the wrong number of arguments.

**Example**:

```rust
fn add(a: i32, b: i32) -> i32 { return a + b; }

fn test() {
    let result = add(42);  // Error: function `add` expects 2 arguments, but 1 provided
}
```

**Solution**: Provide the correct number of arguments.

### `SelfReferenceInFunction`

`self` appears inside a free function (not inside an `impl` method).

**Example**:

```rust
fn test() {
    let x = self.value;  // Error: self reference not allowed in standalone function `test`
}
```

**Solution**: `self` is only valid inside method definitions.

### `SelfReferenceOutsideMethod`

`self` is used in a position that is not inside any method or `impl` block.

**Example**:

```rust
fn test() {
    return self;  // Error: self reference is only allowed in methods, not functions
}
```

### `InstanceMethodCalledAsAssociated`

An instance method (one that takes `self`) was called using `Type::method()` syntax instead of
`instance.method()`.

**Example**:

```rust
struct Point { x: i32 }

impl Point {
    fn mirror(&self) -> Point { return Point { x: -self.x }; }
}

fn test() {
    Point::mirror();
    // Error: instance method `Point::mirror` requires a receiver, use `instance.mirror()` instead
}
```

### `AssociatedFunctionCalledAsMethod`

An associated function (one that does not take `self`) was called on an instance using
`instance.function()` syntax instead of `Type::function()`.

**Example**:

```rust
struct Counter { value: i32 }

impl Counter {
    fn new() -> Counter { return Counter { value: 0 }; }
}

fn test() {
    let c = Counter { value: 0 };
    c.new();
    // Error: associated function `Counter::new` cannot be called on an instance,
    //        use `Counter::new()` instead
}
```

### `MethodCallOnNonStruct`

A method call is made on a non-struct (primitive) type.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    x.some_method();  // Error: cannot call method on non-struct type `i32`
}
```

---

## Operator Errors

### `InvalidBinaryOperand`

A binary operator is applied to operands whose types are incompatible with that operator
(e.g., arithmetic on booleans).

**Example**:

```rust
fn test() {
    let x: bool = true;
    let y: bool = false;
    let result = x + y;  // Error: numeric operator `Add` cannot be applied to non-numeric types
}
```

**Operator requirements**:

| Operators | Required operand types |
|-----------|----------------------|
| `+`, `-`, `*`, `/`, `%`, `**` | Numeric (same type) |
| `<`, `<=`, `>`, `>=` | Numeric (same type) |
| `==`, `!=` | Any (same type) |
| `&&`, `\|\|` | `bool` |
| `&`, `\|`, `^`, `<<`, `>>` | Integer (same type) |

### `InvalidUnaryOperand`

A unary operator is applied to an incompatible type.

**Examples**:

```rust
fn test() {
    let x: u32 = 10;
    let neg = -x;  // Error: unary operator `Neg` can only be applied to signed integers, found `u32`

    let y: i32 = 42;
    let not = !y;  // Error: unary operator `Not` can only be applied to booleans, found `i32`
}
```

**Unary operator requirements**:

| Operator | Source syntax | Required type | Result type |
|----------|--------------|---------------|-------------|
| `Not` | `!x` | `bool` | `bool` |
| `Neg` | `-x` | Signed integer (i8/i16/i32/i64) | Same as operand |
| `BitNot` | `~x` | Integer (signed or unsigned) | Same as operand |

### `BinaryOperandTypeMismatch`

A binary operator is applied to two operands of different types.

**Example**:

```rust
fn test() {
    let x: i32 = 10;
    let y: i64 = 20;
    let z = x + y;
    // Error: cannot apply operator `Add` to operands of different types: `i32` and `i64`
}
```

**Solution**: Ensure both operands have the same type. Inference does not perform implicit widening.

---

## Import Errors

### `ImportResolutionFailed`

Import path does not resolve to a valid module or symbol.

**Example**:

```rust
use std::nonexistent::Module;  // Error: cannot resolve import path: std::nonexistent::Module
```

### `CircularImport`

A glob import creates a circular dependency.

**Example**:

```rust
use mod_a::*;  // mod_a itself imports from the current module → Error: circular glob import detected
```

### `EmptyGlobImport`

A glob import (`use path::*`) has an empty path segment.

---

## Registration Errors

### `RegistrationFailed`

Failed to register a symbol (type, struct, enum, function, method, or variable) in the symbol
table. The most common cause is a duplicate definition.

**`RegistrationKind` variants**:

```rust
pub enum RegistrationKind {
    Type,
    Struct,
    Enum,
    Spec,
    Function,
    Method,
    Variable,
}
```

**Example**:

```rust
fn test() {}
fn test() {}  // Error: error registering function `test`
```

**Solution**: Ensure symbol names are unique within their scope.

---

## Structural Errors

### `ExpectedArrayType`

An array indexing expression (`arr[i]`) is applied to a value that is not an array.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    let y = x[0];  // Error: expected an array type, found `i32`
}
```

### `ExpectedStructType`

A member access expression (`value.field`) is applied to a non-struct type.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    let y = x.value;  // Error: member access requires a struct type, found `i32`
}
```

### `ExpectedEnumType`

A type member access expression (`Type::Variant`) is applied to a non-enum type.

**Example**:

```rust
struct Point {}

fn test() {
    let x = Point::SomeVariant;
    // Error: type member access requires an enum type, found `Point`
}
```

### `ArrayIndexNotNumeric`

The index expression in an array access is not a numeric type.

**Example**:

```rust
fn test() {
    let arr: [i32; 5] = [1, 2, 3, 4, 5];
    let x = arr[true];  // Error: array index must be of number type, found `Bool`
}
```

---

## Generic Type Errors

### `TypeParameterCountMismatch`

A generic function call provides a different number of explicit type arguments than the
function's type parameter list.

**Example**:

```rust
fn identity<T>(x: T) -> T { return x; }

fn test() {
    identity::<i32, bool>(42);
    // Error: type parameter count mismatch for `identity`: expected 1, found 2
}
```

### `MissingTypeParameters`

A generic function requires type parameters but none were provided and they could not be
inferred.

**Example**:

```rust
fn make<T>() -> T { /* ... */ }

fn test() {
    let x = make();
    // Error: function `make` requires 1 type parameters, but none were provided
}
```

### `CannotInferTypeParameter`

A type parameter could not be inferred from the arguments at the call site.

**Example**:

```rust
fn example<T>(flag: bool) -> i32 { return 0; }

fn test() {
    example(true);
    // Error: cannot infer type parameter `T` for `example` - consider adding explicit type arguments
}
```

### `ConflictingTypeInference`

The same type parameter was inferred as two different concrete types from different arguments.

**Example**:

```rust
fn pair<T>(a: T, b: T) -> bool { return true; }

fn test() {
    pair(42, true);
    // Error: conflicting types for type parameter `T`: inferred `i32` and `Bool`
}
```

---

## Non-Deterministic Errors

### `CannotInferUzumakiType`

A `@` (uzumaki) expression was used in a context where the target variable has no known type.
The type checker cannot determine what type the uzumaki expression should produce.

**Example**:

```rust
forall {
    let x = @;  // Error: cannot infer type for uzumaki expression assigned to variable of unknown type
}
```

**Solution**: Add an explicit type annotation to the variable:

```rust
forall {
    let x: i32 = @;  // OK: uzumaki produces an i32
}
```

---

## Error Recovery

The type checker implements error recovery to collect multiple errors before failing. All five
compilation phases run to completion even when earlier phases produce errors. Within the
inference phase, errors from one function do not prevent checking subsequent functions.

**Example**:

```rust
fn test() -> i32 {
    let x: bool = 42;        // Error 1: type mismatch in variable definition
    let y = undefined_var;   // Error 2: use of undeclared variable
    return "string";         // Error 3: type mismatch in return statement
}
// All three errors are reported together
```

## Error Deduplication

Errors are deduplicated by a key derived from the error kind and location. The same error
(same variant, same source position) will not be reported more than once even if the checker
encounters the same expression multiple times during analysis.

## Location Information

All errors include a `Location` value accessible via `TypeCheckError::location()`. The struct
has flat fields (not nested `Position` structs):

```rust
pub struct Location {
    pub offset_start: u32,
    pub offset_end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}
```

Error messages format the location as `start_line:start_column:` at the beginning:

```
1:5: type mismatch in return statement: expected `i32`, found `Bool`
```

## Related Documentation

- [API Guide](./api-guide.md) - How to handle errors in code
- [Architecture](./architecture.md) - Error recovery implementation details
- [Type System](./type-system.md) - Type checking rules
