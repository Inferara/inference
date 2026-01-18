# Type Checker Architecture

This document provides an in-depth look at the type checker's internal architecture, design decisions, and implementation patterns.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    TypeCheckerBuilder                        │
│  (Typestate Pattern: InitState → CompleteState)             │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                       TypeChecker                            │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 1: process_directives()                        │  │
│  │  - Register import statements in scope tree           │  │
│  │  - Build import dependency graph                      │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 2: register_types()                            │  │
│  │  - Collect type aliases (type X = Y)                  │  │
│  │  - Register struct definitions with fields            │  │
│  │  - Register enum definitions with variants            │  │
│  │  - Register spec definitions                          │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 3: resolve_imports()                           │  │
│  │  - Bind import paths to symbols                       │  │
│  │  - Handle glob imports (use path::*)                  │  │
│  │  - Handle partial imports (use path::{A, B})          │  │
│  │  - Validate visibility of imported symbols            │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 4: collect_function_and_constant_definitions() │  │
│  │  - Register function signatures                       │  │
│  │  - Register methods on structs                        │  │
│  │  - Register constants                                 │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 5: infer_variables() [for each function]      │  │
│  │  - Type-check function body statements                │  │
│  │  - Infer expression types                             │  │
│  │  - Validate assignments and returns                   │  │
│  │  - Check visibility and access control                │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                      TypedContext                            │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Arena (original AST)                               │    │
│  │  - Source files                                     │    │
│  │  - All AST nodes with unique IDs                    │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  node_types: FxHashMap<NodeID, TypeInfo>           │    │
│  │  - Maps AST node IDs to inferred types              │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  SymbolTable (hierarchical scopes)                  │    │
│  │  - Type definitions                                 │    │
│  │  - Function signatures                              │    │
│  │  - Variable bindings                                │    │
│  │  - Import resolutions                               │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Phase-by-Phase Walkthrough

### Phase 1: Process Directives

**Goal**: Register all import statements without resolving them yet.

**Input**: AST with `use` directives

**Output**: Symbol table with raw import records

**Why separate from resolution?** We need to know what imports exist before we can resolve circular import dependencies or handle glob imports that depend on module structure.

```rust
// Example AST
use std::io::File;
use std::collections::*;
use math::{sin, cos as cosine};

// After Phase 1
SymbolTable {
    imports: [
        Import { path: ["std", "io", "File"], kind: Plain },
        Import { path: ["std", "collections"], kind: Glob },
        Import {
            path: ["math"],
            kind: Partial([
                ImportItem { name: "sin", alias: None },
                ImportItem { name: "cos", alias: Some("cosine") }
            ])
        }
    ]
}
```

### Phase 2: Register Types

**Goal**: Collect all type definitions into the symbol table.

**Input**: Type aliases, struct definitions, enum definitions, spec definitions

**Output**: Symbol table populated with type information

**Why before functions?** Functions reference types in their signatures, so types must be registered first.

```rust
// Example AST
type MyInt = i32;

struct Point {
    x: i32,
    y: i32,
}

enum Color {
    Red,
    Green,
    Blue,
}

// After Phase 2
SymbolTable {
    types: {
        "MyInt": TypeAlias(TypeInfo { kind: Number(I32), ... }),
        "Point": Struct(StructInfo {
            name: "Point",
            fields: {
                "x": StructFieldInfo { type_info: i32, visibility: Private },
                "y": StructFieldInfo { type_info: i32, visibility: Private }
            },
            visibility: Private,
            ...
        }),
        "Color": Enum(EnumInfo {
            name: "Color",
            variants: {"Red", "Green", "Blue"},
            visibility: Private,
            ...
        })
    }
}
```

### Phase 3: Resolve Imports

**Goal**: Bind import paths to actual symbols in the symbol table.

**Input**: Raw import records from Phase 1 + registered types from Phase 2

**Output**: Resolved imports with symbol references

**Challenges**:
- **Glob imports**: Must enumerate all public symbols in target module
- **Circular imports**: Module A imports B, B imports A
- **Visibility**: Only resolve imports to public symbols from external scopes

```rust
// Before resolution
Import { path: ["std", "collections", "HashMap"], kind: Plain }

// After resolution
ResolvedImport {
    local_name: "HashMap",
    symbol: Struct(StructInfo { name: "HashMap", ... }),
    definition_scope_id: 42  // Scope where HashMap is defined
}

// Glob import resolution
Import { path: ["std", "io"], kind: Glob }
// Resolves to multiple ResolvedImport entries, one for each public symbol in std::io
```

### Phase 4: Register Functions

**Goal**: Collect function signatures (name, parameters, return type, type parameters).

**Input**: Function and method definitions

**Output**: Symbol table with function signatures

**Why after imports?** Functions may reference imported types in their signatures.

```rust
// Example AST
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn identity<T>(x: T) -> T {
    return x;
}

// After Phase 4
SymbolTable {
    functions: {
        "add": FuncInfo {
            name: "add",
            type_params: [],
            param_types: [i32, i32],
            return_type: i32,
            visibility: Private,
            definition_scope_id: 0
        },
        "identity": FuncInfo {
            name: "identity",
            type_params: ["T"],
            param_types: [Generic("T")],
            return_type: Generic("T"),
            visibility: Private,
            definition_scope_id: 0
        }
    }
}
```

### Phase 5: Infer Variables

**Goal**: Type-check function bodies and infer expression types.

**Input**: Function bodies with statements and expressions

**Output**: TypedContext with type information for every AST node

**This is the most complex phase**, involving:
- Variable type inference
- Expression type synthesis
- Statement type checking
- Generic type parameter substitution
- Method resolution
- Visibility enforcement

```rust
// Example function
fn example() -> i32 {
    let x = 42;           // Infer x: i32
    let y: bool = true;   // Check true is bool
    return x;             // Check x matches return type i32
}

// After Phase 5
TypedContext {
    node_types: {
        <literal 42>: TypeInfo { kind: Number(I32) },
        <variable x>: TypeInfo { kind: Number(I32) },
        <literal true>: TypeInfo { kind: Bool },
        <variable y>: TypeInfo { kind: Bool },
        <identifier x in return>: TypeInfo { kind: Number(I32) },
        ...
    }
}
```

## Symbol Table Design

### Scope Tree Structure

Scopes form a tree that mirrors the lexical structure of the code:

```
Root Scope (ID: 0)
├─ Module: std (ID: 1)
│  ├─ Module: io (ID: 2)
│  │  ├─ Struct: File
│  │  └─ Function: read_to_string
│  └─ Module: collections (ID: 3)
│     └─ Struct: HashMap
├─ Function: main (ID: 4)
│  ├─ Variable: x
│  └─ Block (ID: 5)
│     └─ Variable: y
└─ Struct: MyStruct (ID: 6)
   └─ Method: new (ID: 7)
      └─ Variable: self
```

### Symbol Lookup Algorithm

```rust
fn lookup_symbol(name: &str, current_scope_id: u32) -> Option<Symbol> {
    let mut scope = current_scope_id;
    loop {
        // Check current scope
        if let Some(symbol) = scopes[scope].symbols.get(name) {
            return Some(symbol);
        }

        // Check resolved imports in current scope
        if let Some(import) = scopes[scope].resolved_imports.get(name) {
            return Some(import.symbol);
        }

        // Move to parent scope
        if let Some(parent) = scopes[scope].parent_id {
            scope = parent;
        } else {
            return None;  // Reached root, symbol not found
        }
    }
}
```

### Visibility Checking

Visibility is enforced during symbol lookup:

```rust
fn is_accessible(symbol_scope: u32, access_scope: u32, visibility: Visibility) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => {
            // Private symbols accessible only from definition scope and descendants
            access_scope == symbol_scope || is_descendant(access_scope, symbol_scope)
        }
    }
}
```

## Type Information Representation

### Two-Level Type System

The type checker uses a two-level type representation strategy:

**Level 1 - AST Types** (`Type` enum in `inference_ast`):
- Source-level representation parsed from code
- Uses `Type::Simple(SimpleTypeKind)` for primitive builtins
- `SimpleTypeKind` is a lightweight enum without heap allocation
- Efficient for the parser and AST construction

**Level 2 - Type Information** (`TypeInfo` in `inference_type_checker`):
- Semantic representation for type checking and inference
- Uses `TypeInfoKind` with rich semantic information
- Supports type parameter substitution and unification

### TypeInfo Structure

```rust
pub struct TypeInfo {
    pub kind: TypeInfoKind,
    pub type_params: Vec<String>,
}

pub enum TypeInfoKind {
    // Primitives
    Unit,
    Bool,
    String,
    Number(NumberType),  // I8, I16, I32, I64, U8, U16, U32, U64

    // Compound types
    Array(Box<TypeInfo>, u32),  // Element type + size
    Struct(String),
    Enum(String),

    // Generic and qualified types
    Generic(String),            // Type parameter (e.g., T)
    QualifiedName(String),      // module::Type
    Function(String),           // Function type signature

    // Other
    Custom(String),             // User-defined type
    Qualified(String),          // Qualified identifier
    Spec(String),               // Specification type
}
```

### SimpleTypeKind in the AST

```rust
// In inference_ast::nodes
pub enum SimpleTypeKind {
    Unit,
    Bool,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
}
```

The `SimpleTypeKind` enum provides:
- **Zero-cost representation**: Stack-allocated enum, no heap allocation
- **Type safety**: Compile-time guarantee that only valid primitive types exist
- **Efficient comparison**: Direct enum comparison without string matching
- **Pattern matching**: Exhaustive compile-time checking of all cases

### Type Substitution for Generics

When calling a generic function, type parameters are substituted:

```rust
// Generic function
fn identity<T>(x: T) -> T { return x; }

// Call site
let result = identity(42);

// Type parameter substitution
// Before: T
// After:  i32
// Substitution map: { "T" -> TypeInfo { kind: Number(I32) } }

let return_type = function_return_type.substitute(&substitutions);
// Generic("T").substitute({ "T" -> i32 }) = i32
```

## Expression Type Inference

### Bidirectional Type Checking

The type checker uses bidirectional inference:

**Synthesis (infer)**: Infer type from expression structure
```rust
infer_expression(expr: &Expression) -> TypeInfo {
    match expr {
        Expression::Literal(lit) => infer_literal_type(lit),
        Expression::Binary(bin) => {
            let left_type = infer_expression(bin.left);
            let right_type = infer_expression(bin.right);
            check_operator_types(bin.operator, left_type, right_type)
        }
        // ...
    }
}
```

**Checking (check)**: Validate expression against expected type
```rust
check_expression(expr: &Expression, expected: TypeInfo) -> Result<()> {
    let actual = infer_expression(expr);
    if actual != expected {
        return Err(TypeMismatch { expected, actual });
    }
    Ok(())
}
```

### Operator Type Rules

**Arithmetic operators** (`+`, `-`, `*`, `/`, `%`, `**`):
- Both operands must be numeric
- Result type is the same as operand type
- Division operator (`/`) added in recent updates

**Comparison operators** (`==`, `!=`, `<`, `<=`, `>`, `>=`):
- Both operands must be numeric
- Result type is always `bool`

**Logical operators** (`&&`, `||`):
- Both operands must be `bool`
- Result type is `bool`

**Bitwise operators** (`&`, `|`, `^`, `<<`, `>>`):
- Both operands must be numeric (integer types)
- Result type is the same as operand type

**Unary operators**:
- `!` (logical NOT): Operand must be `bool`, result is `bool`
- `-` (negation): Operand must be signed integer, result is same type
- `~` (bitwise NOT): Operand must be integer, result is same type

## Method Resolution

Methods are resolved in two steps:

1. **Find method on type**: Look up the method in the type's method table
2. **Check visibility**: Verify the method is accessible from call site

```rust
// Method lookup algorithm
fn resolve_method(
    type_info: &TypeInfo,
    method_name: &str,
    call_site_scope: u32
) -> Option<MethodInfo> {
    // Get struct info from symbol table
    let struct_info = symbol_table.lookup_struct(type_info)?;

    // Find method by name
    let method = struct_info.methods.get(method_name)?;

    // Check visibility
    if !is_accessible(method.scope_id, call_site_scope, method.visibility) {
        return None;
    }

    Some(method)
}
```

### Instance Methods vs Associated Functions

Methods are distinguished by whether they take `self`:

```rust
impl Counter {
    // Instance method (has self)
    fn increment(&self) -> i32 {
        return self.value + 1;
    }

    // Associated function (no self)
    fn new() -> Counter {
        return Counter { value: 0 };
    }
}

// Usage
let c = Counter::new();      // Associated function call
let v = c.increment();        // Instance method call
```

In the symbol table:
```rust
MethodInfo {
    signature: FuncInfo { name: "increment", ... },
    has_self: true,   // Instance method
    ...
}

MethodInfo {
    signature: FuncInfo { name: "new", ... },
    has_self: false,  // Associated function
    ...
}
```

## Error Recovery Strategy

The type checker continues after errors to collect multiple issues:

```rust
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<TypeCheckError>,           // Accumulate errors
    reported_error_keys: FxHashSet<String>,  // Deduplicate errors
    ...
}

impl TypeChecker {
    fn infer_types(&mut self, ctx: &mut TypedContext) -> anyhow::Result<SymbolTable> {
        // Run all phases even if some fail
        self.process_directives(ctx);
        self.register_types(ctx);
        self.resolve_imports();
        self.collect_function_and_constant_definitions(ctx);

        // Inference phase continues with errors
        for source_file in ctx.source_files() {
            for def in &source_file.definitions {
                match def {
                    Definition::Function(func) => {
                        self.infer_variables(func.clone(), ctx);
                        // Errors added to self.errors, continue to next function
                    }
                    // ...
                }
            }
        }

        // Report all errors at the end
        if !self.errors.is_empty() {
            bail!("Type checking failed: {}", format_errors(&self.errors))
        }

        Ok(self.symbol_table)
    }
}
```

### Error Deduplication

Errors are deduplicated using a key-based system:

```rust
fn report_error(&mut self, error: TypeCheckError) {
    let key = error.deduplication_key();
    if !self.reported_error_keys.contains(&key) {
        self.reported_error_keys.insert(key);
        self.errors.push(error);
    }
}
```

This prevents reporting the same error multiple times when an incorrect symbol is used in multiple places.

## Performance Considerations

### Arena Allocation

The AST uses arena allocation for efficient memory management:
- All nodes allocated in contiguous memory
- No individual node deallocations
- Cache-friendly traversal
- ID-based references instead of pointers

### Hash Map Usage

The type checker uses `FxHashMap` from `rustc-hash` for better performance:
- Faster than `std::collections::HashMap` for integer and string keys
- Used for symbol tables, type maps, and scope lookups

### Scope Reference Counting

Scopes use `Rc<RefCell<Scope>>` for shared ownership:
- Multiple child scopes can reference parent
- Interior mutability for adding symbols during type checking
- No cycles in scope tree, so `Rc` is safe

### SimpleTypeKind for Primitives

Primitive types use `SimpleTypeKind` enum instead of heap-allocated nodes:
- **Memory efficiency**: No `Rc` allocation for common types (i32, bool, unit, etc.)
- **Cache performance**: Enum values are stack-allocated and compact
- **Type checking speed**: Direct pattern matching without indirection
- **Default values**: Easy to construct default return type `Type::Simple(SimpleTypeKind::Unit)`

This design recognizes that primitive types are the most frequently used types in typical programs, so optimizing their representation has significant impact on overall compiler performance.

## Design Trade-offs

### Multi-Phase vs Single-Pass

**Choice**: Multi-phase

**Trade-off**:
- **Pro**: Handles forward references and mutual recursion naturally
- **Pro**: Clear separation of concerns
- **Con**: Multiple traversals of the AST
- **Con**: More complex state management

**Rationale**: Forward references are common in real code, and the performance cost of multiple passes is acceptable for the improved error messages and flexibility.

### Bidirectional vs Unification-Based

**Choice**: Bidirectional type checking

**Trade-off**:
- **Pro**: Simpler implementation than full unification
- **Pro**: Better error messages (know expected type)
- **Pro**: More predictable for developers
- **Con**: Less powerful type inference than Hindley-Milner
- **Con**: Some cases require type annotations

**Rationale**: Bidirectional checking provides a good balance of inference power and implementation complexity for a statically-typed language targeting WebAssembly.

### Error Recovery vs Fail-Fast

**Choice**: Error recovery with multiple error reporting

**Trade-off**:
- **Pro**: Better developer experience (fix multiple issues at once)
- **Pro**: See all type errors, not just first one
- **Con**: More complex error handling logic
- **Con**: Need to handle invalid state carefully

**Rationale**: Collecting multiple errors dramatically improves the edit-compile-test cycle, especially for large codebases.

### SimpleTypeKind vs Heap-Allocated Types

**Choice**: Value-based `SimpleTypeKind` enum for primitives

**Trade-off**:
- **Pro**: Zero heap allocations for most common types
- **Pro**: Smaller AST memory footprint
- **Pro**: Faster type equality checks (enum discriminant comparison)
- **Pro**: Simpler default value construction (e.g., unit return type)
- **Con**: Two representations to maintain (AST vs TypeInfo)
- **Con**: Conversion overhead between representations

**Rationale**: Profiling showed that primitive types dominate typical Inference programs. The previous design using `Rc<SimpleType>` created unnecessary allocations and indirections. The new design using `SimpleTypeKind` eliminates these costs while maintaining type safety. The conversion overhead to `TypeInfoKind` is negligible compared to the memory and cache benefits.

**Impact on Type Checking**: The validate_type method no longer needs to look up primitive types in the symbol table. The pattern match on `Type::Simple(_)` immediately recognizes these as valid builtin types, simplifying the validation logic.

## Testing Strategy

The type checker has comprehensive test coverage across multiple dimensions:

### Test Organization
- `type_checker.rs` - Core type inference tests
- `array_tests.rs` - Array-specific type checking
- `coverage.rs` - Comprehensive operator and statement coverage

### Test Categories
1. **Positive tests**: Valid code that should type-check
2. **Negative tests**: Invalid code that should produce specific errors
3. **Edge cases**: Boundary conditions and corner cases
4. **Regression tests**: Previously-fixed bugs

### Testing Pattern
```rust
#[test]
fn test_feature() {
    let source = r#"fn test() { /* test code */ }"#;
    let typed_context = try_type_check(source)
        .expect("Type checking should succeed");

    // Query type information using filter_nodes
    let nodes = typed_context.filter_nodes(|node| /* predicate */);

    // Assertions
    assert!(typed_context.get_node_typeinfo(node_id).is_some());
}
```

## Future Enhancements

### Planned Features
- **Trait system**: Interface-based polymorphism
- **Type inference improvements**: Let-polymorphism for local variables
- **Const generics**: Array sizes as generic parameters
- **Exhaustiveness checking**: Ensure all enum variants handled

### Known Limitations
- **Single-file only**: Multi-file support under development
- **No higher-ranked types**: Polymorphism limited to function definitions
- **No associated types**: Only concrete type parameters supported
- **Limited const evaluation**: Array sizes must be literals

## Related Components

- **AST (`inference_ast`)**: Provides the arena and node structures
- **Parser (`tree-sitter-inference`)**: Generates the AST from source
- **Code Generator (`inference_wasm_codegen`)**: Consumes typed context for WASM generation

## References

- [Bidirectional Type Checking (Pierce & Turner)](https://www.cs.cmu.edu/~fp/papers/pldi04.pdf)
- [Type Systems for Programming Languages (Pierce)](https://www.cis.upenn.edu/~bcpierce/tapl/)
- [Rust Compiler Symbol Table](https://rustc-dev-guide.rust-lang.org/symbol-resolution.html)
