# Type Checker

Bidirectional type inference and checking for the Inference programming language.

## Overview

The `inference-type-checker` crate implements a multi-phase type checker that validates and infers types throughout an abstract syntax tree (AST). It supports primitive types, user-defined structs and enums, generic type parameters, method resolution, import systems with visibility checking, and comprehensive error recovery.

## Key Features

- **Bidirectional Type Checking**: Combines type synthesis (inferring types from expressions) and type checking (validating expressions against expected types)
- **Multi-Phase Analysis**: Processes code in distinct phases to handle forward references and circular dependencies
- **Scope-Aware Symbol Table**: Hierarchical scope management with proper symbol resolution
- **Import System**: Full support for plain, glob, and partial imports with path resolution
- **Visibility Control**: Enforces access control for functions, structs, enums, fields, and methods
- **Generic Type Parameters**: Type parameter inference and substitution for generic functions
- **Comprehensive Error Recovery**: Collects multiple errors before failing, with detailed error messages
- **Operator Support**: Type checking for arithmetic, logical, comparison, bitwise, and unary operators

## Quick Start

```rust
use inference_ast::arena::Arena;
use inference_type_checker::TypeCheckerBuilder;

// Parse source code into an AST arena
let arena: Arena = parse_source(source_code);

// Run type checking
let typed_context = TypeCheckerBuilder::build_typed_context(arena)?
    .typed_context();

// Query type information for AST nodes
if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
    println!("Node {} has type: {}", node_id, type_info);
}
```

## Architecture

### Type Checking Phases

The type checker runs in five sequential phases:

```
1. Process Directives    → Register raw import statements
2. Register Types        → Collect struct, enum, spec, and type alias definitions
3. Resolve Imports       → Bind import paths to symbols in the symbol table
4. Register Functions    → Collect function and method signatures
5. Infer Variables       → Type-check function bodies and variable declarations
```

This ordering ensures that types are available before functions reference them, and imports are resolved before symbol lookup.

### Core Components

```
TypeCheckerBuilder
    ├─ TypedContext         → Stores AST arena + type annotations
    │   ├─ Arena            → Original parsed AST
    │   ├─ node_types       → Map: NodeID → TypeInfo
    │   └─ SymbolTable      → Hierarchical scope management
    │
    └─ TypeChecker          → Main type inference engine
        ├─ SymbolTable      → Type and function definitions
        ├─ errors           → Accumulated type errors
        └─ Inference Logic  → Expression and statement checking
```

## Module Documentation

- [`type_info`] - Type representation system with `TypeInfo` and `TypeInfoKind`
- [`typed_context`] - Storage for type annotations on AST nodes
- [`errors`] - Comprehensive error types with 29 distinct variants
- `symbol_table` (internal) - Hierarchical scope and symbol management
- `type_checker` (internal) - Core type inference implementation

## Supported Types

### Primitive Types

Primitive types are represented in the AST using `Type::Simple(SimpleTypeKind)`, a lightweight enum that avoids heap allocation for builtin types.

```rust
// Numeric types
i8, i16, i32, i64     // Signed integers
u8, u16, u32, u64     // Unsigned integers

// Other primitives
bool                   // Boolean
unit                   // Unit type (like void)
```

**Internal Representation**: The AST's `SimpleTypeKind` enum provides an efficient, value-based representation that the type checker converts to `TypeInfoKind` for semantic analysis. This design eliminates unnecessary allocations for the most common types in Inference programs.

### Compound Types

```rust
// Arrays with fixed size
[i32; 10]
[[bool; 5]; 3]         // Nested arrays

// Structs
struct Point {
    x: i32,
    y: i32,
}

// Enums (unit variants only)
enum Status {
    Active,
    Inactive,
}
```

### Generic Types

```rust
// Generic function with type parameter T
fn identity<T>(x: T) -> T {
    return x;
}

// Type parameter inference at call site
let result = identity(42);  // T inferred as i32
```

## Type Checking Examples

### Basic Type Inference

```rust
fn example() -> i32 {
    let x = 42;           // x inferred as i32
    let y: bool = true;   // y explicitly typed as bool
    return x;
}
```

### Method Resolution

```rust
struct Counter {
    value: i32,
}

impl Counter {
    fn increment(&self) -> i32 {
        return self.value + 1;
    }
}

fn test() {
    let c = Counter { value: 10 };
    let result = c.increment();  // Method call type-checked
}
```

### Operator Type Checking

```rust
fn operators() {
    let a: i32 = 10;
    let b: i32 = 20;

    // Arithmetic operators (require numeric types)
    let sum = a + b;
    let diff = a - b;
    let prod = a * b;
    let quot = a / b;     // Division operator

    // Unary operators
    let neg = -a;         // Negation (signed integers only)
    let bitnot = ~b;      // Bitwise NOT

    // Logical operators (require bool)
    let x: bool = true;
    let y: bool = false;
    let and_result = x && y;
    let or_result = x || y;
    let not_result = !x;
}
```

### Import System

```rust
// Plain import
use std::collections::HashMap;

// Glob import
use std::io::*;

// Partial import with aliases
use std::fs::{File, read_to_string as read_file};
```

## Error Handling

The type checker provides detailed error messages with source locations:

```rust
// Type mismatch error
fn test() -> i32 {
    return true;  // Error: expected `i32`, found `bool`
}

// Undefined symbol
fn test() {
    let x = unknown_var;  // Error: use of undeclared variable `unknown_var`
}

// Visibility violation
mod internal {
    fn private_fn() {}
}

fn test() {
    internal::private_fn();  // Error: function `private_fn` is private
}
```

### Error Recovery

The type checker continues after encountering errors to collect all issues:

```rust
fn multiple_errors() -> i32 {
    let x: bool = 42;        // Error 1: type mismatch
    let y = undefined_var;   // Error 2: undefined variable
    return "string";         // Error 3: wrong return type
}
// All three errors reported together
```

## Type Information API

The `TypedContext` provides methods to query type information:

```rust
// Check specific types
typed_context.is_node_i32(node_id);
typed_context.is_node_i64(node_id);

// Get full type information
if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
    // Type checking
    if type_info.is_number() { /* ... */ }
    if type_info.is_bool() { /* ... */ }
    if type_info.is_struct() { /* ... */ }
    if type_info.is_array() { /* ... */ }

    // Generic type handling
    if type_info.is_generic() { /* ... */ }
    if type_info.has_unresolved_params() { /* ... */ }
}
```

## Testing

The crate includes comprehensive test coverage:

```bash
# Run all type checker tests
cargo test -p inference-tests type_checker

# Run specific test modules
cargo test -p inference-tests type_checker::coverage
cargo test -p inference-tests type_checker::array_tests
```

Test organization:
- `tests/src/type_checker/type_checker.rs` - Core type inference tests
- `tests/src/type_checker/array_tests.rs` - Array type checking
- `tests/src/type_checker/coverage.rs` - Comprehensive coverage tests

## Recent Changes

### Issue #86 Enhancements

**Operator Support**:
- **Division operator** (`/`) type checking for numeric types
- **Unary negation operator** (`-`) type checking for signed integers (i8, i16, i32, i64)
- **Bitwise NOT operator** (`~`) type checking for all integer types

**Visibility Parsing**:
- Comprehensive visibility support in type checker for functions, structs, enums, constants, and type aliases
- Proper handling of `pub` modifiers throughout the symbol table and type checking phases
- Visibility checking enforced during imports and symbol access

**Implementation Improvements**:
- Expression inference now uses immutable references for better performance
- Atomic counter integration for deterministic node ID generation from AST

### Issue #54 Initial Implementation

**Core Type Checking System**:
- Bidirectional type inference with synthesis and checking modes
- Multi-phase type checking (directives → types → imports → functions → variables)
- Scope-aware symbol table with hierarchical scope management
- Import system with registration and resolution phases
- Generic type parameter inference and substitution

**Type System Features**:
- Full support for primitive types (bool, string, unit, i8-i64, u8-u64)
- Array types with fixed sizes and element type checking
- Struct types with field visibility and member access validation
- Enum types with variant access validation
- Method resolution for instance methods and associated functions

**Error Handling**:
- Comprehensive error system with 29 distinct error variants
- Error recovery to collect multiple errors before failing
- Error deduplication to avoid repeated reports
- Detailed error messages with context and location information

## Implementation Details

### Symbol Table

The symbol table uses a tree structure for scopes:

```
Root Scope
├─ Module A
│  ├─ Function foo
│  │  └─ Local variables
│  └─ Struct Bar
└─ Module B
   └─ Function baz
```

Symbol lookup walks up the tree from the current scope to find matching symbols.

### Type Substitution

Generic type parameters are substituted during function calls:

```rust
fn generic<T>(x: T) -> [T; 2] {
    return [x, x];
}

// Call with i32
let result = generic(42);
// T → i32, return type [T; 2] → [i32; 2]
```

### Visibility Rules

- `pub` items are visible from any scope
- Private items are only visible from their definition scope and child scopes
- Imports respect the visibility of imported symbols

## Design Rationale

### Why Bidirectional?

Bidirectional type checking combines the best of both worlds:
- **Synthesis** (bottom-up): Infers types from expressions without context
- **Checking** (top-down): Validates expressions against expected types

This approach provides better error messages and handles polymorphic types more naturally.

### Why Multi-Phase?

The multi-phase design handles forward references and mutual recursion:
- Functions can reference types defined later in the file
- Imports can reference symbols from other modules
- Types can refer to each other in their definitions

### Why Error Recovery?

Collecting multiple errors before failing improves developer experience:
- Fix multiple issues in one edit cycle
- See all type errors at once, not just the first one
- Better understanding of cascading errors

## Documentation

Detailed documentation is available in the `docs/` directory:

- [Architecture Guide](./docs/architecture.md) - Internal design, phase walkthrough, and implementation patterns
- [API Guide](./docs/api-guide.md) - Practical examples and usage patterns for the type checker API
- [Type System Reference](./docs/type-system.md) - Complete type system rules, operators, and type inference
- [Error Reference](./docs/errors.md) - Comprehensive catalog of all 29 error types with examples

## Related Documentation

- [AST Arena Documentation](../ast/README.md) - Understanding the AST structure
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Inference language reference
- [CONTRIBUTING.md](../../CONTRIBUTING.md) - Development guidelines

## Future Work

Current limitations and planned improvements:

- Multi-file support: Currently expects single source file
- Trait system: Not yet implemented
- Type inference for closures: Under development
- Exhaustiveness checking for enums: Planned
- Const generics: Future consideration

## License

This crate is part of the Inference compiler project. See the repository root for license information.
