# Type Checker Error Reference

Complete catalog of type checking errors with examples and solutions.

## Error Overview

The type checker produces 29 distinct error variants, each with specific context and location information. All errors implement the `Error` trait and provide detailed messages.

## Error Categories

1. [Type Mismatch Errors](#type-mismatch-errors)
2. [Symbol Resolution Errors](#symbol-resolution-errors)
3. [Visibility Errors](#visibility-errors)
4. [Function and Method Errors](#function-and-method-errors)
5. [Operator Errors](#operator-errors)
6. [Import Errors](#import-errors)
7. [Registration Errors](#registration-errors)
8. [Structural Errors](#structural-errors)

## Type Mismatch Errors

### TypeMismatch

**Description**: Type of an expression doesn't match the expected type.

**Context Variants**:
- `Assignment`
- `Return`
- `VariableDefinition`
- `BinaryOperation(operator)`
- `Condition`
- `FunctionArgument { function_name, arg_name, arg_index }`
- `MethodArgument { type_name, method_name, arg_name, arg_index }`
- `ArrayElement`

**Examples**:

```rust
// Return statement mismatch
fn test() -> i32 {
    return true;  // Error: type mismatch in return: expected `i32`, found `bool`
}

// Variable definition mismatch
fn test() {
    let x: i32 = "hello";  // Error: type mismatch in variable definition: expected `i32`, found `string`
}

// Assignment mismatch
fn test() {
    let x: bool = false;
    x = 42;  // Error: type mismatch in assignment: expected `bool`, found `i32`
}

// Binary operation mismatch
fn test() {
    let result = 10 + true;  // Error: type mismatch in binary operation `Add`: expected numeric type
}

// Function argument mismatch
fn greet(name: string) -> string {
    return name;
}

fn test() {
    greet(42);  // Error: type mismatch in argument 0 `name` of function `greet`: expected `string`, found `i32`
}

// Array element mismatch
fn test() {
    let arr: [i32; 3] = [1, 2, true];  // Error: type mismatch in array element: expected `i32`, found `bool`
}
```

**Solution**: Ensure the expression evaluates to the expected type. Use type conversions if necessary.

## Symbol Resolution Errors

### UnknownType

**Description**: Referenced type name is not defined in scope.

**Example**:

```rust
fn test(x: UndefinedType) -> i32 {  // Error: unknown type `UndefinedType`
    return 42;
}
```

**Solution**: Define the type before using it, or check for typos in the type name.

### UnknownIdentifier

**Description**: Variable or identifier is used before declaration.

**Example**:

```rust
fn test() {
    let y = x + 10;  // Error: use of undeclared variable `x`
}
```

**Solution**: Declare the variable before use, or check for typos in the variable name.

### UndefinedFunction

**Description**: Function is called but not defined.

**Example**:

```rust
fn test() {
    let result = unknown_function(42);  // Error: call to undefined function `unknown_function`
}
```

**Solution**: Define the function, import it, or check for typos in the function name.

### UndefinedMethod

**Description**: Method is called on a type but the method doesn't exist.

**Example**:

```rust
struct Point {
    x: i32,
    y: i32,
}

fn test() {
    let p = Point { x: 10, y: 20 };
    let result = p.distance();  // Error: undefined method `distance` on type `Point`
}
```

**Solution**: Define the method in an `impl` block for the type, or check for typos.

## Visibility Errors

### VisibilityViolation

**Description**: Attempting to access a private symbol from outside its defining scope.

**Context Variants**:
- `Function { name }`
- `Struct { name }`
- `Enum { name }`
- `Field { struct_name, field_name }`
- `Method { type_name, method_name }`
- `Import { path }`

**Examples**:

```rust
// Private function
mod internal {
    fn private_func() {}
}

fn test() {
    internal::private_func();  // Error: function `private_func` is private
}

// Private struct
mod internal {
    struct PrivateStruct {}
}

fn test() {
    let s = internal::PrivateStruct {};  // Error: struct `PrivateStruct` is private
}

// Private field
struct Point {
    x: i32,  // Private by default
    y: i32,
}

fn test() {
    let p = Point { x: 10, y: 20 };
    let x = p.x;  // Error: field `x` of struct `Point` is private
}

// Private method
struct Counter {
    value: i32,
}

impl Counter {
    fn internal_increment(&self) {}  // Private method
}

fn test() {
    let c = Counter { value: 0 };
    c.internal_increment();  // Error: method `internal_increment` on type `Counter` is private
}
```

**Solution**: Make the symbol public with `pub` or access it from within its defining scope.

## Function and Method Errors

### ArgumentCountMismatch

**Description**: Function or method called with wrong number of arguments.

**Variants**:
- `Function { function_name, expected, found }`
- `Method { type_name, method_name, expected, found }`

**Examples**:

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn test() {
    let result = add(42);  // Error: function `add` expects 2 arguments, found 1
}

struct Calculator {}

impl Calculator {
    fn multiply(&self, a: i32, b: i32) -> i32 {
        return a * b;
    }
}

fn test() {
    let calc = Calculator {};
    let result = calc.multiply(5, 10, 15);  // Error: method `multiply` on type `Calculator` expects 2 arguments, found 3
}
```

**Solution**: Provide the correct number of arguments to the function or method call.

### MethodCallOnNonStruct

**Description**: Attempting to call a method on a non-struct type.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    x.some_method();  // Error: cannot call method `some_method` on non-struct type `i32`
}
```

**Solution**: Methods can only be called on struct instances. Use functions for primitive types.

## Operator Errors

### UnsupportedUnaryOperator

**Description**: Unary operator applied to incompatible type.

**Examples**:

```rust
fn test() {
    let x: bool = true;
    let neg = -x;  // Error: unsupported unary operator `-` for type `bool`
}

fn test() {
    let x: u32 = 10;
    let neg = -x;  // Error: unsupported unary operator `-` for type `u32` (unsigned)
}

fn test() {
    let x: i32 = 42;
    let not = !x;  // Error: unsupported unary operator `!` for type `i32` (logical NOT requires bool)
}
```

**Operator Requirements**:
- `!` (logical NOT): Requires `bool`
- `-` (negation): Requires signed integer types (i8, i16, i32, i64)
- `~` (bitwise NOT): Requires any integer type

**Solution**: Ensure the operand type matches the operator requirements.

### BinaryOperatorTypeMismatch

**Description**: Binary operator applied to incompatible types.

**Examples**:

```rust
fn test() {
    let result = 10 + "hello";  // Error: binary operator `+` cannot be applied to types `i32` and `string`
}

fn test() {
    let x: bool = true;
    let y: i32 = 42;
    let result = x && y;  // Error: binary operator `&&` expects `bool` operands, found `bool` and `i32`
}
```

**Operator Requirements**:
- Arithmetic (`+`, `-`, `*`, `/`, `%`, `**`): Both operands must be numeric and same type
- Comparison (`<`, `<=`, `>`, `>=`): Both operands must be numeric and same type
- Equality (`==`, `!=`): Both operands must be same type
- Logical (`&&`, `||`): Both operands must be `bool`
- Bitwise (`&`, `|`, `^`, `<<`, `>>`): Both operands must be integer and same type

**Solution**: Ensure both operands are compatible with the operator.

### DivisionByZero

**Description**: Compile-time detected division by zero.

**Example**:

```rust
fn test() -> i32 {
    return 42 / 0;  // Error: division by zero
}
```

**Solution**: Use a non-zero divisor. Runtime division checks should be handled separately.

## Import Errors

### ImportPathNotFound

**Description**: Import path doesn't resolve to a valid module or symbol.

**Example**:

```rust
use std::nonexistent::Module;  // Error: import path `std::nonexistent::Module` not found
```

**Solution**: Verify the import path is correct and the module exists.

### AmbiguousImport

**Description**: Multiple symbols with the same name are imported.

**Example**:

```rust
use module_a::Function;
use module_b::Function;  // Error: ambiguous import: `Function` is imported from multiple sources

fn test() {
    Function();  // Which Function?
}
```

**Solution**: Use aliases to disambiguate:

```rust
use module_a::Function as FunctionA;
use module_b::Function as FunctionB;
```

### CircularImport

**Description**: Module imports create a circular dependency.

**Example**:

```rust
// module_a.inf
use module_b::B;

// module_b.inf
use module_a::A;  // Error: circular import detected: module_a → module_b → module_a
```

**Solution**: Refactor to remove circular dependencies. Extract common types to a shared module.

### GlobImportFailure

**Description**: Glob import failed to resolve.

**Example**:

```rust
use undefined_module::*;  // Error: glob import from `undefined_module` failed: module not found
```

**Solution**: Verify the module exists and is accessible.

## Registration Errors

### RegistrationFailed

**Description**: Failed to register a symbol (type, struct, enum, function, etc.) in the symbol table.

**Example**:

```rust
fn test() {}
fn test() {}  // Error: registration failed: function `test` is already defined
```

**Solution**: Ensure symbol names are unique within their scope.

### DuplicateSymbol

**Description**: Symbol is defined multiple times in the same scope.

**Example**:

```rust
struct Point {}
struct Point {}  // Error: duplicate symbol: `Point` is already defined in this scope
```

**Solution**: Rename one of the symbols or remove the duplicate definition.

### DuplicateField

**Description**: Struct has multiple fields with the same name.

**Example**:

```rust
struct Point {
    x: i32,
    x: i64,  // Error: duplicate field: `x` is already defined in struct `Point`
}
```

**Solution**: Rename the duplicate field.

### DuplicateEnumVariant

**Description**: Enum has multiple variants with the same name.

**Example**:

```rust
enum Color {
    Red,
    Green,
    Red,  // Error: duplicate enum variant: `Red` is already defined in enum `Color`
}
```

**Solution**: Rename the duplicate variant.

## Structural Errors

### FieldNotFound

**Description**: Struct field doesn't exist.

**Example**:

```rust
struct Point {
    x: i32,
    y: i32,
}

fn test() {
    let p = Point { x: 10, y: 20 };
    let z = p.z;  // Error: struct `Point` has no field `z`
}
```

**Solution**: Use an existing field name or add the field to the struct definition.

### MemberAccessOnNonStruct

**Description**: Attempting to access a field on a non-struct type.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    let y = x.value;  // Error: cannot access field `value` on non-struct type `i32`
}
```

**Solution**: Member access is only valid on struct types.

### ArrayIndexOnNonArray

**Description**: Attempting to index a non-array type.

**Example**:

```rust
fn test() {
    let x: i32 = 42;
    let y = x[0];  // Error: cannot index non-array type `i32`
}
```

**Solution**: Array indexing is only valid on array types.

### ArrayIndexTypeMismatch

**Description**: Array index is not a numeric type.

**Example**:

```rust
fn test() {
    let arr: [i32; 5] = [1, 2, 3, 4, 5];
    let x = arr[true];  // Error: array index must be numeric, found `bool`
}
```

**Solution**: Use a numeric type (i32, u32, etc.) for array indices.

### ArraySizeMismatch

**Description**: Array literal has wrong number of elements.

**Example**:

```rust
fn test() {
    let arr: [i32; 3] = [1, 2, 3, 4, 5];  // Error: array size mismatch: expected 3 elements, found 5
}
```

**Solution**: Ensure the array literal has the correct number of elements matching the type annotation.

### EmptyArrayWithoutType

**Description**: Empty array literal without type annotation.

**Example**:

```rust
fn test() {
    let arr = [];  // Error: cannot infer type of empty array without type annotation
}
```

**Solution**: Provide a type annotation:

```rust
fn test() {
    let arr: [i32; 0] = [];
}
```

### InvalidEnumVariant

**Description**: Enum variant doesn't exist.

**Example**:

```rust
enum Color {
    Red,
    Green,
    Blue,
}

fn test() {
    let c = Color::Yellow;  // Error: enum `Color` has no variant `Yellow`
}
```

**Solution**: Use an existing variant or add the variant to the enum definition.

### TypeMemberAccessOnNonEnum

**Description**: Attempting to access a variant on a non-enum type.

**Example**:

```rust
struct Point {}

fn test() {
    let x = Point::SomeVariant;  // Error: cannot access variant on non-enum type `Point`
}
```

**Solution**: Type member access (`::`) is only valid for enum variants or associated functions.

### ConditionMustBeBool

**Description**: Condition in if/while/loop must be boolean.

**Example**:

```rust
fn test() {
    if 42 {  // Error: condition must be `bool`, found `i32`
        // ...
    }
}
```

**Solution**: Use a boolean expression:

```rust
fn test() {
    if 42 != 0 {
        // ...
    }
}
```

### InvalidSelfReference

**Description**: `self` used outside method context.

**Example**:

```rust
fn test() {
    return self.value;  // Error: `self` can only be used in methods
}
```

**Solution**: `self` is only valid inside method definitions.

## Error Context Details

### TypeMismatchContext

Provides specific context for where the type mismatch occurred:

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

This context helps pinpoint the exact location and nature of the type error.

### VisibilityContext

Provides specific context for visibility violations:

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

### RegistrationKind

Identifies what kind of symbol failed to register:

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

## Error Recovery

The type checker implements error recovery to collect multiple errors before failing:

```rust
fn test() -> i32 {
    let x: bool = 42;        // Error 1: type mismatch in variable definition
    let y = undefined_var;   // Error 2: use of undeclared variable
    return "string";         // Error 3: type mismatch in return
}

// All three errors reported:
// "type mismatch in variable definition: expected `bool`, found `i32`;
//  use of undeclared variable `undefined_var`;
//  type mismatch in return: expected `i32`, found `string`"
```

This allows developers to fix multiple issues in a single iteration.

## Error Deduplication

The type checker deduplicates errors to avoid repeated reports:

```rust
fn test() {
    let x = undefined_var;  // Error reported once
    let y = undefined_var;  // Not reported again (same variable)
    let z = undefined_var;  // Not reported again (same variable)
}

// Only one error: "use of undeclared variable `undefined_var`"
```

## Location Information

All errors include precise source location:

```rust
pub struct Location {
    pub start: Position,  // Line and column
    pub end: Position,
}

pub struct Position {
    pub line: usize,
    pub column: usize,
}
```

Error messages include location:
```
file.inf:10:15: type mismatch in return: expected `i32`, found `bool`
```

## Best Practices for Error Handling

### 1. Check for Multiple Errors

The type checker collects multiple errors before failing. Always handle the possibility of multiple error messages:

```rust
match TypeCheckerBuilder::build_typed_context(arena) {
    Ok(completed) => {
        let typed_context = completed.typed_context();
        // Success path
    }
    Err(e) => {
        // Error may contain multiple messages separated by "; "
        eprintln!("Type checking failed with {} error(s):",
            e.to_string().split("; ").count());

        for (idx, error_msg) in e.to_string().split("; ").enumerate() {
            eprintln!("  [{}] {}", idx + 1, error_msg);
        }
    }
}
```

### 2. Provide User-Friendly Messages

Format errors for human readability with context and suggestions:

```rust
match TypeCheckerBuilder::build_typed_context(arena) {
    Err(e) => {
        eprintln!("Type checking failed:");
        eprintln!("{}", e);
        eprintln!("\nPlease fix the errors above and try again.");
        eprintln!("Tip: Read error messages from top to bottom - later errors");
        eprintln!("     may be consequences of earlier ones.");
    }
    Ok(completed) => { /* ... */ }
}
```

### 3. Log Errors for Debugging

Use structured logging for programmatic error analysis:

```rust
match TypeCheckerBuilder::build_typed_context(arena) {
    Err(e) => {
        log::error!("Type check failed: {}", e);

        // Count errors for metrics
        let error_count = e.to_string().split("; ").count();
        log::info!("Total errors: {}", error_count);

        // Continue or abort based on context
        return Err(e);
    }
    Ok(completed) => {
        log::info!("Type checking succeeded");
        // ...
    }
}
```

### 4. Extract Location Information

While the current error format includes locations in the message, you can parse them if needed:

```rust
fn parse_error_location(error_msg: &str) -> Option<(usize, usize)> {
    // Error format: "line:column: message"
    let parts: Vec<&str> = error_msg.splitn(2, ": ").collect();
    if parts.len() < 2 {
        return None;
    }

    let location = parts[0];
    let coords: Vec<&str> = location.split(':').collect();
    if coords.len() != 2 {
        return None;
    }

    let line = coords[0].parse::<usize>().ok()?;
    let column = coords[1].parse::<usize>().ok()?;
    Some((line, column))
}
```

### 5. Categorize Errors for IDEs

Build structured error reports for editor integration:

```rust
#[derive(Debug)]
struct Diagnostic {
    line: usize,
    column: usize,
    severity: Severity,
    message: String,
}

#[derive(Debug)]
enum Severity {
    Error,
    Warning,
}

fn extract_diagnostics(error: anyhow::Error) -> Vec<Diagnostic> {
    error.to_string()
        .split("; ")
        .filter_map(|msg| {
            let location = parse_error_location(msg)?;
            Some(Diagnostic {
                line: location.0,
                column: location.1,
                severity: Severity::Error,
                message: msg.to_string(),
            })
        })
        .collect()
}
```

### 6. Handle Partial Results

Since the type checker fails on first unrecoverable error, consider saving partial progress:

```rust
fn incremental_type_check(source: &str) -> Result<TypedContext, Vec<String>> {
    let arena = parse_source(source)
        .map_err(|e| vec![format!("Parse error: {}", e)])?;

    match TypeCheckerBuilder::build_typed_context(arena) {
        Ok(completed) => Ok(completed.typed_context()),
        Err(e) => {
            // Extract individual errors
            let errors: Vec<String> = e.to_string()
                .split("; ")
                .map(|s| s.to_string())
                .collect();
            Err(errors)
        }
    }
}
```

## Common Error Patterns

### Pattern 1: Undefined Symbol Cascade

```rust
fn test() {
    let x = undefined;  // Error: use of undeclared variable
    let y = x + 1;      // x has no type, but no additional error
    return y;           // y inferred from context
}
```

The type checker tries to continue after undefined symbols to collect more errors.

### Pattern 2: Type Mismatch Propagation

```rust
fn test() -> i32 {
    let x: bool = true;
    return x;  // Error: type mismatch (bool vs i32)
}
```

Type mismatches don't propagate - each occurrence is checked independently.

### Pattern 3: Visibility Cascade

```rust
mod internal {
    struct PrivateStruct {
        field: i32,
    }
}

fn test() {
    let s = internal::PrivateStruct { field: 42 };  // Error: struct is private
    let f = s.field;  // If struct was accessible, field access would be checked separately
}
```

## Related Documentation

- [API Guide](./api-guide.md) - How to handle errors in code
- [Architecture](./architecture.md) - Error recovery implementation
- [Type System](./type-system.md) - Type checking rules
