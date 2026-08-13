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
│  │  - Handle file imports (use a::b)                     │  │
│  │  - Handle item imports (use a::b::{A, B})             │  │
│  │  - Validate visibility of imported symbols            │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 4: collect_function_and_constant_definitions() │  │
│  │  - Register function signatures                       │  │
│  │  - Register methods on structs                        │  │
│  │  - Register constants (value type + scope variable)   │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 4b (after resolve_imports):                    │  │
│  │  - renormalize_signatures(): re-resolve param/return  │  │
│  │    types so an item-imported struct type becomes      │  │
│  │    `Struct`, matching what call sites infer           │  │
│  │  - check_definition_cycles() then                     │  │
│  │    check_const_initializers(): const initializers are │  │
│  │    type-checked here so a `const` may reference a      │  │
│  │    cross-file `const`; a value cycle reports only      │  │
│  │    CircularDefinition                                 │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Phase 5: infer_variables() [for each function]      │  │
│  │  - Type-check function body statements                │  │
│  │  - Infer expression types                             │  │
│  │  - Validate assignments and returns                   │  │
│  │  - Check visibility and access control                │  │
│  │  - Bare-name lookups honor the file boundary: a       │  │
│  │    non-entry file cannot see the entry file's         │  │
│  │    private items by bare name                         │  │
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

**Why separate from resolution?** We need every file's symbols registered before binding imports, so an item import can resolve against a file that appears later in canonical order, and re-export (`pub use`) chains can be traversed across files.

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
                "x": StructFieldInfo { type_info: i32 },
                "y": StructFieldInfo { type_info: i32 }
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

**Signature type validation** runs in its own pass *after* import resolution, not
during registration. Registration keeps any unresolved `Custom` type name as-is;
the later `validate_signatures` pass enters each file's scope and checks every
parameter and return type against the symbol table, so an item-imported type
(`use a::b::{T};`) is recognized in a signature position exactly as in a `let`
binding. A type that still does not resolve is reported as an unknown type.

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

#### Check Mode: The Expected Type

Every expression is inferred by `infer_expression_expecting(expr_id, expected, ctx)`, where `expected: Option<Expected>` is what the surrounding position requires of the expression. `infer_expression` is the shim that passes `None`, and it is what the positions with nothing to require — an expression statement, a condition, an array index — still call.

`Expected` pairs the required type with the position requiring it:

```rust
struct Expected<'a> {
    ty: &'a TypeInfo,
    source: &'a TypeMismatchContext,
}
```

The two travel together deliberately. The type is what an integer literal denotes when the expectation reaches a leaf; the `source` is what a diagnostic needs in order to say *why* the literal has that type, because the type is written somewhere the literal is not. Splitting them would let a descent forward one without the other and leave a literal typed with no explanation. `Expected` is `Copy` and borrowed, so the transparent forms forward it unchanged and a literal under `-( 1 + 2 )` still reports the position that typed the whole expression.

The positions that construct an `Expected` are the ones that know a declared type: an annotated `let`/`const` initializer, the right-hand side of an assignment, a struct-literal field value, an element of an array literal, a call argument (free, associated, and method calls alike), and the operand of `return`. Each then runs its ordinary post-inference mismatch check, which reports the single diagnostic when the value cannot denote the required type.

The number-literal arm is where an expectation *terminates* — the one place an expected type becomes a recorded type. It takes an expected *integer* type in preference to any type already recorded for the node, because the recorded one may be its own `i32` fallback from an earlier visit — the generic-argument pre-pass reaches argument literals before any expected type exists — and the position is the authority on which type the literal denotes. Every other arm either forwards an expectation or decides what to forward:

- **Literal-closed predicate** — `is_literal_closed(arena, expr_id)` decides, from syntax alone, whether an expression is built entirely out of integer literals: a literal is closed, and `( e )`, `-e`, `~e` and the arithmetic/bitwise/shift operators preserve closure. `!` and the comparison, equality and logical operators do not, because their operands' types are unconstrained by the type of the whole.
- **Transparent descent** — `Parenthesized`, `PrefixUnary(Neg)` and `PrefixUnary(BitNot)` forward `expected` unchanged to their operand; each still runs its own signedness or numeric check on the type that comes back, so `-e` under an unsigned expected type is still rejected. `ArrayLiteral` consumes an expected `[T; N]` by expecting `T` of every element, recursively, so a nested initializer types all the way down.
- **Peer-first binary typing** — `infer_binary_operands` inspects both operands' closure. With exactly one closed, the *other* operand is inferred first and, when its type is an integer type, is expected of the closed one. With both closed, the type expected of the whole expression descends into both, but only for an operator that yields its operands' type. With neither closed, both operands already carry their own types and nothing is expected of either. Peer typing runs for every operator, including comparisons and the shift count — code generation picks the shift opcode from the left operand alone and requires both stack operands to match.

Peer-first is what keeps the diagnostic for the ordinary mistake at the binding: `let a: i32 = 3; let x: i64 = a + 1;` reports a variable-definition mismatch pointing at `x`, rather than an operand mismatch pointing at `+`. Its one visible consequence is ordering — when the left operand is literal-closed and both operands independently produce diagnostics, the right operand's are reported first.

The `Binary` and `ArrayLiteral` arms keep their memo early-return **unconditionally**: a recorded type is returned without re-deriving it, even under an expected type. Re-deriving would re-run every check in the subtree and report its diagnostics a second time, and it could not change an outcome — the only visit that records an interior type before any expected type exists is the generic-argument pre-pass, and wherever what it recorded disagrees with what is later expected it has already pushed `ConflictingTypeInference`.

**Provenance side table**: when the literal arm consumes an expected type it also records the position in `TypedContext::literal_type_sources`, keyed by the literal's `ExprId`. Analysis rule A022 reads it to append a note naming that position to an out-of-range diagnostic:

```
literal `300` is out of range for type `u8` (valid range: 0..=255)
note: the literal is typed `u8` by the type expected in return statement
```

Without it, a range error against a type written elsewhere reads as action at a distance. The table is **diagnostics-only**: `node_types` remains the single source of truth for what a literal denotes, and no backend (`wasm-codegen`, `hassert`, `wasm-to-v`) may consult it, because nothing about how a program compiles depends on which position supplied the type.

**Range validation** is not the type checker's: it records the literal's type but never parses its value. Analysis rule A022 (`LiteralOutOfRange`) reads the recorded type and validates the value against it, so it follows contextual typing automatically.

#### Array Element and Uzumaki Propagation

An expected `[T; N]` reaching an array literal is `T` expected of every element, so `let arr: [i8; 3] = [10, 20, 30];` types all three literals `i8` and code generation emits each at the right width. Because the expectation is consumed in the `ArrayLiteral` arm rather than at one statement handler, it applies wherever an array type is expected — a `let`, a `const`, an assignment, and each level of a nested initializer such as `[[i64; 2]; 2]`.

The same propagation applies to uzumaki (`@`) leaves inside array literals. When the declared array element type is known, a `@` element receives that declared type, allowing constructs such as `let a: [i32; 2] = [0, @];` to type-check and reach codegen. Propagation recurses through nested array literals so that every `@` leaf in a multi-dimensional array literal is typed. A struct- or array-typed `@` element is typed by the same mechanism but is subsequently rejected by analysis rule A040 (`UzumakiOnCompoundArrayElement`), which enforces the codegen restriction that only scalar and enum elements may use uzumaki.

Struct literal fields follow an analogous pattern: when a field's declared type is known, a `@` value for that field receives the field's type. A compound-typed field `@` (struct or array) is typed but then rejected by analysis rule A038 (`UzumakiOnCompoundField`).

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

### Custom Type Resolution

When a type is declared using a custom name (like a struct or enum), the type checker must resolve the name to determine whether it refers to a `Struct`, `Enum`, or something else. This is handled by the `resolve_custom_type()` method in the symbol table.

**Why is resolution needed?** When constructing `TypeInfo` from an AST `Type`, the parser may create `TypeInfoKind::Custom(name)` because the type definition hasn't been processed yet. During variable definition, function parameter registration, and function call validation, we need to resolve these custom names to their actual definitions.

**Resolution Algorithm**:

```rust
pub fn resolve_custom_type(&self, mut ti: TypeInfo) -> TypeInfo {
    match &ti.kind {
        TypeInfoKind::Custom(name) => {
            // Resolve from the current scope, capturing the type's canonical key
            // (its defining-file identity) so a same-named type from another file
            // is a distinct type. The `Struct`/`Enum` kinds carry `(bare_name, key)`.
            let from_scope = self.current_scope_id().unwrap_or(0);
            if let Some((_, key)) = self.resolve_struct_in_scope(name, from_scope) {
                ti.kind = TypeInfoKind::Struct(name.clone(), key);
            } else if let Some((_, key)) = self.resolve_enum_in_scope(name, from_scope) {
                ti.kind = TypeInfoKind::Enum(name.clone(), key);
            }
            // Falls through to Custom if not found (forward reference)
            ti
        }
        TypeInfoKind::Array(elem, size) => {
            // Recursively resolve element types
            let resolved_elem = self.resolve_custom_type(*elem.clone());
            ti.kind = TypeInfoKind::Array(Box::new(resolved_elem), *size);
            ti
        }
        _ => ti,  // Other types need no resolution
    }
}
```

**When is it called?**
- During function parameter registration (Phase 4)
- During function call validation (Phase 5)
- When registering variable and constant definitions

**Example**:

```rust
// Define a struct
struct Point {
    x: i32,
    y: i32,
}

// Use in array type
fn test(coords: [Point; 10]) {
    // ...
}

// Process:
// 1. AST Type -> TypeInfo::Custom("Point")
// 2. Lookup "Point" -> Found StructInfo
// 3. Resolution -> TypeInfo::Struct("Point")
// 4. Argument validation: signature expects Struct("Point"), argument has Struct("Point") ✓
```

**Forward References**: If a custom type name is not found in the symbol table, resolution falls back to leaving it as `Custom(name)`. This handles forward references in nested modules and allows the compiler to report a more precise error later (during later phases that expect a resolved type).
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

Both directions run in a single traversal of one function. An expected type descends into the expression; a synthesized type ascends out of it.

```rust
fn infer_expression_expecting(
    &mut self,
    expr_id: ExprId,
    expected: Option<Expected<'_>>,
    ctx: &mut TypedContext,
) -> Option<TypeInfo>;

// The synthesis-only form: nothing is required of the expression.
fn infer_expression(&mut self, expr_id: ExprId, ctx: &mut TypedContext) -> Option<TypeInfo> {
    self.infer_expression_expecting(expr_id, None, ctx)
}
```

**Synthesis** is what every arm does with its subexpressions: a binary expression infers both operands and derives its own type from them, an identifier looks its type up, a call takes its function's return type.

**Checking** is not a separate traversal. A position that knows the type it requires passes it as `expected` and compares the synthesized type against it afterwards, reporting one `TypeMismatch` with that position's `TypeMismatchContext`. Between those two steps, `expected` is what lets an integer literal in the subexpression denote the required type in the first place — see [Check Mode: The Expected Type](#check-mode-the-expected-type).

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

## Argument Type Validation

When a function or method is called, the type checker validates that each argument matches the corresponding parameter type. This is essential for catching type mismatches early.

Arguments bind by position: argument `i` binds parameter `i`. An argument label does not
select a parameter — where labels are present they are checked for agreement with the
declaration, and a call that disagrees is rejected rather than reordered.

**Validation Process**:

1. Look up the function signature (parameter types and names, return type, type parameters)
2. If any argument is labelled, validate the labels:
   - All arguments must be labelled, or none of them
   - Each label must name a parameter of the callee
   - Each label must name the parameter declared at that argument's position
3. For each argument in the call:
   - Infer the argument's type
   - Compare it against the corresponding parameter type
   - If types don't match, record a type mismatch error with detailed context

**Example**:

```rust
fn add(x: i32, y: i32) -> i32 {
    return x + y;
}

fn test() {
    add(1, 2);      // OK: both i32
    add(1, true);   // Error: arg[1] type mismatch (expected i32, found bool)
    add(1);         // Error: missing required argument
}
```

**Error Information**:

When a type mismatch is detected in a function argument, the error includes:
- Expected type (from the parameter signature)
- Found type (from argument inference)
- Argument index (position in parameter list, 0-based)
- Argument name (automatically generated as `arg0`, `arg1`, etc.)
- Function or method name
- Source location of the call

```rust
TypeCheckError::TypeMismatch {
    expected: TypeInfo { kind: Number(I32) },
    found: TypeInfo { kind: Bool },
    context: TypeMismatchContext::MethodArgument {
        type_name: "Counter".to_string(),
        method_name: "set_value".to_string(),
        arg_name: "arg0".to_string(),
        arg_index: 0,
    },
    location: Location { /* ... */ },
}
```

**With Generic Types**:

For generic functions, type parameters are substituted before comparison:

```rust
fn identity<T>(x: T) -> T {
    return x;
}

fn test() {
    // T inferred as i32 from call argument
    let result = identity(42);      // T = i32, arg: i32 ✓
    let result = identity(true);    // T = bool, arg: bool ✓
}
```

The substitution happens during type parameter inference, so the argument validation sees the concrete types, not the generic type variables.

## Error Recovery Strategy

The type checker continues after errors to collect multiple issues:

```rust
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<TypeCheckError>,                     // Accumulate errors
    reported_errors: FxHashSet<(DedupKind, String)>, // Deduplicate by (kind, name)
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
fn push_error_dedup(&mut self, error: TypeCheckError) {
    if let Some(key) = error.dedup_key() {
        if !self.reported_errors.insert(key) {
            return;
        }
    }
    self.errors.push(error);
}
```

The key is a `(DedupKind, String)` tuple where `DedupKind` is a small enum
listing the variants that participate in deduplication and the `String` is
the offending symbol's name (or a composite key for variants like
`SpecFunctionShadowsTopLevel`). This prevents reporting the same error
multiple times when an incorrect symbol is used in multiple places.

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

Primitive types use the `SimpleTypeKind` enum instead of heap-allocated nodes, providing significant performance benefits:

**Memory Efficiency**:
- No `Rc` allocation for common types (i32, bool, unit, etc.)
- Zero-cost representation: stack-allocated enum values
- Smaller AST memory footprint for typical programs

**Performance Benefits**:
- Cache-friendly: compact enum values improve cache locality
- Fast type checking: direct pattern matching without pointer indirection
- Efficient equality: discriminant comparison instead of string matching

**Ease of Use**:
- Type-safe: compile-time guarantee that only valid primitive types exist
- Easy construction: `Type::Simple(SimpleTypeKind::Unit)` for default return type
- Exhaustive pattern matching: compiler enforces handling all cases

**Design Rationale**:
This design recognizes that primitive types are the most frequently used types in typical Inference programs (appearing in 70-90% of type annotations). Profiling showed that the previous string-based representation created unnecessary allocations and hash lookups. The new `SimpleTypeKind` enum eliminates these costs while maintaining type safety and clarity.

**Impact on Type Checking**:
The `validate_type()` method no longer needs symbol table lookups for primitive types. The pattern match on `Type::Simple(_)` immediately recognizes these as valid builtin types, simplifying the validation logic and improving performance.

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

## Assignment Mutability Validation

When an assignment targets an array index (`arr[i] = x`) or a struct field (`p.x = 42`), the type checker validates that the root variable is mutable. This requires extracting the root variable name from the left-hand side expression, which may be arbitrarily nested.

**Root Variable Name Extraction**:

```rust
fn extract_root_variable_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.clone()),
        Expression::ArrayIndexAccess(access) => {
            // Recursively extract from nested accesses: arr[i][j] -> arr
            Self::extract_root_variable_name(&access.array.borrow())
        }
        Expression::MemberAccess(access) => {
            // Recursively extract from struct field access: p.x -> p
            Self::extract_root_variable_name(&access.expr.borrow())
        }
        _ => None,  // Non-identifier bases (function calls, etc.)
    }
}
```

**Assignment Validation Process**:

1. When an `AssignStatement` is encountered, call `extract_root_variable_name` on the left-hand side
2. Look up the variable in the symbol table to check if it's mutable
3. If the variable is immutable, report `AssignToImmutable` error

This single unified call handles plain variable assignment (`x = 1`), array element assignment (`arr[i] = x`), struct field assignment (`p.x = 42`), and combinations (`arr[i].field = x`).

**Example**:

```rust
// Array element assignment requires mut
let arr: [i32; 10] = [0; 10];
arr[0] = 42;  // Error: cannot assign to immutable variable `arr`

let mut arr: [i32; 10] = [0; 10];
arr[0] = 42;  // OK

// Struct field assignment also requires the struct variable to be mut
struct Point { x: i32, y: i32 }

let p = Point { x: 1, y: 2 };
p.x = 10;  // Error: cannot assign to immutable variable `p`

let mut p = Point { x: 1, y: 2 };
p.x = 10;  // OK
```

## Variable Shadowing Prohibition

Inference prohibits variable shadowing: a variable declared in an inner scope may not share a name with any variable visible in an enclosing scope. This is a hard type-check error.

**Rationale**: Shadowing is a common source of subtle bugs. Safety-critical coding standards such as MISRA C Rule 5.3 and NASA's Power of 10 prohibit it for the same reason.

**Implementation**:

Before registering a new `let` binding in the current scope, the type checker calls `lookup_variable_in_parent_scopes()` on the symbol table to search all ancestor scopes for a variable with the same name. If one is found, a `VariableShadowed` error is emitted rather than registering the new binding.

**Example**:

```rust
fn test() {
    let x: i32 = 1;
    {
        let x: i32 = 2;  // Error: variable `x` shadows a binding from an outer scope
    }
}
```

Variables in sibling scopes (different branches of an `if/else`, separate `{...}` blocks at the same nesting level) are not considered shadowing because neither is in the other's ancestor chain.

## Testing Strategy

The type checker has comprehensive test coverage across multiple dimensions:

### Test Organization
- `type_checker.rs` - Core type inference tests
- `array_tests.rs` - Array-specific type checking
- `struct_tests.rs` - Struct type checking (literals, field access, mutability, sret restrictions, shadowing, empty struct/unused self errors)
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

**Trait System**:
- Interface-based polymorphism with trait definitions
- Trait bounds on generic type parameters
- Default implementations and associated types
- Coherence checking for trait implementations

**Type Inference Improvements**:
- Let-polymorphism for local variables
- Better error messages with type inference hints
- Partial type annotations (infer some parameters)

**Const Generics**:
- Array sizes as generic parameters: `fn foo<const N: usize>(arr: [i32; N])`
- Const expressions in type positions
- Const generic bounds and where clauses

**Exhaustiveness Checking**:
- Verify all enum variants are handled in match expressions
- Detect unreachable patterns
- Suggest missing patterns in error messages

### Known Limitations

**Module System**:
- Single-file only: multi-file support under development
- No module-level visibility scoping beyond current file
- Import resolution limited to single compilation unit

**Type System**:
- No higher-ranked types: polymorphism limited to function definitions
- No associated types: only concrete type parameters supported
- No type-level computation beyond simple substitution

**Const Evaluation**:
- Array sizes must be numeric literals
- No const functions or const expressions
- No compile-time computation of array bounds

**Numeric Literal Range Validation**:
- Out-of-range literals are rejected. For example, `let a: i8 = 200;` produces a `LiteralOutOfRange` error. The expected type (see [Check Mode: The Expected Type](#check-mode-the-expected-type)) is what gives the range checker a type to validate against.
- A minus sign written *against* the digits is part of the literal token, so `-200` is one literal and is checked as `-200`. A minus sign separated from them is a `Neg` expression instead; the expected type descends into its operand, so `let a: i8 = - 100;` type-checks here. The range check runs on the *un-negated* literal, which would make each signed type's minimum unreachable in that spelling — `- 128` at `i8` measured as `128`. Rather than teach the range check to look through a negation, the separated spelling is rejected outright by analysis rule A046, and A022 skips the literals A046 claims so it never reports a magnitude the author did not write. The glued form is the one canonical spelling of a negative literal and is checked correctly.

**Pattern Matching**:
- No destructuring of structs or arrays
- No guard expressions in patterns
- No exhaustiveness checking for enums

## Related Components

- **AST (`inference_ast`)**: Provides the arena and node structures
- **Parser (`tree-sitter-inference`)**: Generates the AST from source
- **Code Generator (`inference_wasm_codegen`)**: Consumes typed context for WASM generation

## References

- [Bidirectional Type Checking (Pierce & Turner)](https://www.cs.cmu.edu/~fp/papers/pldi04.pdf)
- [Type Systems for Programming Languages (Pierce)](https://www.cis.upenn.edu/~bcpierce/tapl/)
- [Rust Compiler Symbol Table](https://rustc-dev-guide.rust-lang.org/symbol-resolution.html)
