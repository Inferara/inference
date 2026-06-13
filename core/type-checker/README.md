# Type Checker

Bidirectional type inference and checking for the Inference programming language.

## Overview

The `inference-type-checker` crate implements a multi-phase type checker that validates and infers types throughout an abstract syntax tree (AST). It supports primitive types, user-defined structs and enums, generic type parameters, method resolution, import systems with visibility checking, and comprehensive error recovery.

## Key Features

- **Bidirectional Type Checking**: Combines type synthesis (inferring types from expressions) and type checking (validating expressions against expected types)
- **Multi-Phase Analysis**: Processes code in distinct phases to handle forward references and circular dependencies
- **Per-File Scope Model**: Each source file gets its own named child scope under the program root; qualified-name resolution (`a::b::fn()`) walks the scope tree from the calling file's scope
- **File-Based Import Resolution**: `use a::b;` binds a namespace reference; `use a::b::{x, y};` binds individual items; `pub use …` re-exports bindings for transitive access; glob imports are rejected at the parser and removed from the resolver
- **Cross-File Visibility**: `pub` items are accessible from importing files; private items are visible only within their defining file (and that file's specs). Fields inherit their struct's visibility — there is no per-field `pub`
- **Canonical Type Keys**: Every struct and enum has a file-qualified canonical key (`file_path::TypeName`) so same-named types in different files resolve distinctly at every access site
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
- [`errors`] - Comprehensive error types with 46 distinct variants
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

```inference
// File import — binds the name `arith` in the importing file
use lib::arith;

// Item import — binds `add` and `sub` bare in the importing file
use lib::arith::{add, sub};

// Re-export — makes `arith` part of the importing file's public surface
pub use lib::arith;

// Cross-file call via namespace access
pub fn main() {
    let r: i32 = arith::add(1, 2);
}
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

// Visibility violation — cross-file access of a private item
// (error names both the use site and the definition site with a "add pub" hint)
fn test() {
    lib::helper();  // Error: `helper` is private in `lib`
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

// Look up an enum by name; returns None if the name is unknown
if let Some(enum_info) = typed_context.lookup_enum("Color") {
    // Variants are in declaration order, giving zero-based tag indices
    if let Some(tag) = enum_info.variant_index("Green") {
        println!("Green = {}", tag);  // 1
    }
}
```

### Public API Surface

The following types are re-exported from `inference_type_checker` for downstream crates:

- `StructInfo` — struct field information
- `StructFieldInfo` — individual field name and type
- `EnumInfo` — enum variant list and `variant_index()` helper
- `MethodMetadata` — method parameter types, return type, and `has_self` flag

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
- `tests/src/type_checker/struct_tests.rs` - Struct type checking (literals, field access, mutability, sret restrictions, shadowing, empty struct/unused self errors)
- `tests/src/type_checker/associated_functions.rs` - Distinguishing instance methods from associated functions; verifies `InstanceMethodCalledAsAssociated` and `AssociatedFunctionCalledAsMethod` errors
- `tests/src/type_checker/features.rs` - Feature-level tests: enum operator constraints, import resolution without project context
- `tests/src/type_checker/coverage.rs` - Comprehensive coverage tests
- `tests/src/type_checker/multi_file.rs` - 20 smoke tests for the multi-file type checker: per-file scopes, namespace access, item imports, re-export chains, visibility diagnostics, const cycles
- `tests/src/type_checker/multi_file_matrix.rs` - 85-case comprehensive matrix crossing item kinds (fn, struct, enum, const, type) × import forms × visibility, including same-named private structs resolving distinctly, spec cross-file access, and dual-location diagnostics

## Recent Changes

### File-Based Module System (Issue #63)

**Per-file scope model**:
- Each `SourceFileData` in the arena is assigned a named child scope under the program root scope, keyed by its `::` -joined module path (e.g. `lib::arith`). The entry file's scope is the root itself, so its items are unqualified.
- All definitions from a file register inside its scope. Qualified-name resolution (`a::b::fn()`) walks existing scope-tree logic unchanged.

**Import resolution** (glob machinery removed):
- `use a::b;` binds a namespace reference to scope `a::b` under the importing file's scope, with the last segment as the local name.
- `use a::b::{x, y};` looks up each item in scope `a::b`, verifies it exists and is `pub`, and binds it as a resolved import usable bare.
- `pub use …` marks the binding as re-exported so importers of the current file can traverse through it. Intermediate hops in a re-export chain must all be `pub use` for the path to be accessible.
- Empty item import lists (`use a::b::{};`) are rejected with `EmptyImportList`.
- `ImportKind::Glob`, `resolve_glob_import`, and `get_public_symbols_from_scope` have been deleted; the `Glob` arm was also removed from the external-import resolution path.

**Cross-file visibility rule**:
- An item is accessible from another file if and only if it is `pub` and reached via an import chain whose intermediate hops are all `pub use`.
- Within a file (and that file's spec scopes), all items are accessible regardless of visibility.
- Struct fields have no per-field visibility — a field is accessible whenever its struct is accessible.
- `PrivateAccessViolation` (and `ImportedItemPrivate`) carry a second `Location` pointing at the definition site, with a "`add pub`" hint — a dual-location diagnostic requiring no new rendering infrastructure.

**Function visibility bug fixed**:
- `register_function` previously hard-coded `Visibility::Private` via `..` destructuring; the `vis` field is now extracted and forwarded. The same audit was applied to const, type-alias, and enum registration paths.

**Canonical type keys**:
- Every struct and enum is stored under a file-qualified key (e.g. `lib_arith::Point`).
- `TypedContext::canonical_struct_key` / `canonical_enum_key` expose these for codegen.
- The bare-name `lookup_struct_anywhere` / `lookup_enum_anywhere` escape hatches are no longer on the `TypedContext` consumer path; they survive only as the deliberate spec-collision existence check inside the type checker.
- Spec-inner types key by their enclosing file, so single-file programs produce bare keys and existing golden files stay valid.

**`CircularDefinition` check**:
- A dependency graph is built over const initializers and type aliases (intra- and cross-file); cycles are reported as a hard `CircularDefinition` error naming the cycle.
- File import cycles are explicitly allowed (the scope tree is built before any lookup).

**Specs**:
- A spec sees its own file's private items via parent-chain lookup (unchanged behavior).
- Cross-file references from inside a spec obey the same `pub` + import rule as regular code.
- `pub spec` is rejected by the parser before the type checker is reached.

### Core Type Checking System (Issues #54, #86)

**Multi-Phase Type Checking**:
- Bidirectional type inference combining synthesis and checking modes
- Five-phase analysis: directives → types → imports → functions → variables
- Scope-aware symbol table with hierarchical scope management
- Import system with registration and resolution phases
- Generic type parameter inference and substitution

**Type System Features**:
- Full support for primitive types using efficient `SimpleTypeKind` enum representation
- Primitive types (bool, unit, i8-i64, u8-u64) without heap allocation
- Array types with fixed sizes and element type checking
- Struct types with field visibility and member access validation
- Enum types with variant access validation
- Method resolution for instance methods and associated functions

**Operator Support**:
- Arithmetic operators: `+`, `-`, `*`, `/`, `%`, `**`
- Comparison operators: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical operators: `&&`, `||`, `!`
- Bitwise operators: `&`, `|`, `^`, `<<`, `>>`, `~`
- Unary operators: `-` (signed integers), `!` (boolean), `~` (all integers)

**Visibility and Access Control**:
- Comprehensive visibility support for functions, structs, enums, constants, and type aliases
- Proper handling of `pub` modifiers throughout symbol table and type checking phases
- Visibility checking enforced during imports and symbol access
- Private-by-default with explicit `pub` for public items

**Error Handling**:
- Comprehensive error system with detailed error variants
- Error recovery to collect multiple errors before failing
- Error deduplication to avoid repeated reports
- Detailed error messages with context and location information
- Precise source locations for all errors

### Struct Type Support (Issue #149)

**Struct Literal Validation**:
- Struct literals (`Point { x: 10, y: 20 }`) are validated against the struct definition: each field name must exist (`UnknownStructField`), no field may appear twice (`DuplicateStructField`), and no required field may be omitted (`MissingStructField`)
- Field value types are checked against declared field types; type mismatches produce `TypeMismatch` errors with `VariableDefinition` or `ArrayElement` context
- Struct literal field values are type-checked against the struct field types

**Struct Field Access and Mutation**:
- Member access (`p.x`) validates that the receiver is a struct type (`ExpectedStructType`) and that the named field exists (`FieldNotFound`)
- Assignment to a struct field (`p.x = v`) validates that the root variable is declared `mut` (`AssignToImmutable`) using the unified `extract_root_variable_name` helper that handles arbitrarily nested access chains (`arr[i].field`, `p.x.y`, etc.)

**Struct Parameters and Return Types**:
- Struct-typed function parameters are registered with `TypeInfoKind::Struct(name)` after `resolve_custom_type()` resolves the AST `Custom` node
- Functions returning a struct are registered and their return type is tracked for downstream use by the analysis pass (which enforces codegen restrictions on compound-returning calls)

**Struct Definition Validation**:
- `DuplicateStructFieldDefinition`: struct field names must be unique within the definition
- `RecursiveStructDefinition`: struct fields must not create a size cycle (direct or through arrays/aliases)

**Variable Shadowing in Struct Contexts**:
- The shadowing prohibition applies to struct variable bindings: a `let p: Point = ...` in an inner scope shadows an outer `p` and emits `VariableShadowed`

### Method Call Type Checking (Issue #162)

**Instance Method vs Associated Function Dispatch**:
- Methods with a `self` parameter must be called on a receiver using `instance.method()` syntax;
  calling them as `Type::method()` emits `InstanceMethodCalledAsAssociated`
- Methods without `self` are associated functions and must be called as `Type::func()`;
  calling them on a receiver emits `AssociatedFunctionCalledAsMethod`
- `TypedContext::lookup_method(type_name, method_name)` returns `MethodMetadata` with
  `has_self`, `param_types`, `return_type`, and `visibility` fields — the public projection
  of the type-checker's internal `MethodInfo`

**Compound-Returning Method Restrictions**:
- These restrictions are now enforced by the analysis pass (A016–A018). The type checker tracks compound return types and exposes them through `TypedContext` for the analysis pass to query.

### Enum Codegen Support (Issue #179)

**`EnumInfo` made public**:
- `EnumInfo`, its fields (`name`, `variants`, `visibility`, `definition_scope_id`), and `variant_index()` are now `pub` (previously `pub(crate)`)
- `EnumInfo` is re-exported from the crate root alongside `StructInfo` and `StructFieldInfo`
- `variants` changed from `FxHashSet<String>` to `Vec<String>` to guarantee deterministic declaration-order iteration; this preserves stable zero-based tag assignment for WASM codegen across compilations

**New `TypedContext` methods**:
- `lookup_enum(name)` — looks up an enum by name and returns its `EnumInfo`; returns `None` if the name is not registered. Variants in the returned `EnumInfo` are in declaration order, which determines their zero-based integer tags in WASM.
- `register_test_enum(name, variants)` — available under the `test-utils` feature for unit tests in downstream crates that need a populated `TypedContext` without running the full pipeline

**Operator constraints on enum values**:
- Arithmetic operators (`+`, `-`, `*`, `/`, `%`) are rejected when applied to enum values (`InvalidBinaryOperand`)
- Ordering comparisons (`<`, `<=`, `>`, `>=`) are rejected on enum values
- Equality comparisons (`==`, `!=`) are accepted for enum values

### Bug Fixes and Validation Hardening

**New `TypeCheckError` variants**:
- `DuplicateStructFieldDefinition` — two fields with the same name in a struct `struct S { x: i32, x: i32 }` is rejected
- `RecursiveStructDefinition` — field type creates a size cycle, including cycles through arrays (`struct A { items: [A; 3] }`) and type aliases
- `InvalidAssignmentTarget` — left-hand side of an assignment is not a valid lvalue (identifier, array index, or struct field)
- `ArrayLiteralSizeMismatch` — array literal element count does not match the declared array size
- `DivisionByZero` — literal zero in the divisor position of `/` or `%`
- `DuplicateEnumVariant` — two variants with the same name in an enum definition

**Validation fixes**:
- Undeclared types referenced in variable definitions are now validated (previously missed in some positions)
- Top-level `const` initializers are type-checked (previously unchecked)
- Type lookup is now case-sensitive; `I32` no longer resolves to `i32`
- External function parameter types are now correctly parsed by the AST builder

### Fixed-Size Array Support (Issue #148)

**Array Type Resolution**:
- Custom type names (structs and enums) are now resolved to their concrete definitions
- `resolve_custom_type()` method in symbol table handles resolution recursively for array element types
- Array types with custom element types (e.g., `[MyStruct; 10]`) are fully supported
- Forward references are handled gracefully (custom types fall through to `Custom` variant if not found)

**Array Element Type Propagation**:
- When an array literal initializes a variable with explicit array type annotation, element types are propagated to all numeric literals in the array
- Enables the code generator to emit correct WASM instructions for each element
- Example: `let arr: [i8; 3] = [10, 20, 30];` propagates `i8` type to all three literals

**Array Assignment Validation**:
- Assignment to array indices (e.g., `arr[i] = x;`) now validates immutability
- Extracting the root array name from nested index expressions (e.g., `arr[i][j]`)
- Proper error reporting when assigning to immutable array elements

**Argument Type Validation**:
- Function and method calls now validate argument types against parameter signatures
- Type mismatches in arguments produce detailed error messages with argument index and name
- Works with generic type parameters and custom types

## Implementation Details

### Symbol Table

The symbol table uses a tree structure for scopes:

```
Root Scope
├─ File: lib/arith       (scope name: "lib::arith")
│  ├─ Function add       (pub)
│  └─ Struct Buffer      (private)
├─ File: math            (scope name: "math")
│  ├─ Namespace import: arith → lib::arith   (pub use → re-exported)
│  └─ Function foo       (pub)
└─ File: (entry)         (scope = root)
   ├─ Namespace import: math → math
   └─ Function main      (pub)
```

Symbol lookup walks up the tree from the current scope to find matching symbols. Qualified-name resolution (`math::arith::add`) walks down from the importing file's scope through namespace bindings to reach the target scope, then looks up the final name locally. The existing `resolve_qualified_name` algorithm is reused without modification.

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

- `pub` items are visible to any file that imports them (directly or via a `pub use` re-export chain)
- Private items are visible only within their defining file and that file's spec scopes
- Struct fields have no per-field visibility; a field is accessible whenever its struct is accessible
- Only the entry file's top-level `pub fn`s become WASM exports; `pub` in non-entry files is intra-project visibility only

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
- [Error Reference](./docs/errors.md) - Comprehensive catalog of all 50 error variants with examples

## Related Documentation

- [AST Arena Documentation](../ast/README.md) - Understanding the AST structure
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Inference language reference
- [CONTRIBUTING.md](../../CONTRIBUTING.md) - Development guidelines

## Current Limitations and Future Work

### Current Limitations

- **No higher-ranked types**: Polymorphism limited to function definitions
- **No associated types**: Only concrete type parameters supported
- **Limited const evaluation**: Array sizes must be literals
- **No exhaustiveness checking**: Enum pattern matching completeness not verified
- **No import aliasing**: `use a::b as c;` is not yet supported; last-segment name collisions are hard errors
- **`pub use … from M;` re-export is inert**: The `pub` visibility on an external WASM import is accepted but does not re-export the binding to other source files; wrap the external in a `pub fn` instead

### Planned Features

- **Type inference improvements**: Let-polymorphism for better local inference
- **Const generics**: Array sizes as generic parameters
- **Exhaustiveness checking**: Ensure all enum variants are handled in match expressions

## License

This crate is part of the Inference compiler project. See the repository root for license information.
