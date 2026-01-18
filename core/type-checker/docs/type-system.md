# Type System Reference

Complete reference for the Inference language type system as implemented by the type checker.

## Table of Contents

- [Type Categories](#type-categories)
- [Primitive Types](#primitive-types)
- [Compound Types](#compound-types)
- [Generic Types](#generic-types)
- [Type Inference Rules](#type-inference-rules)
- [Operator Type Rules](#operator-type-rules)
- [Method Resolution](#method-resolution)
- [Visibility and Access Control](#visibility-and-access-control)

## Type Categories

The Inference type system organizes types into several categories:

```
Types
├── Primitives
│   ├── Unit
│   ├── Bool
│   ├── String
│   └── Numbers (i8, i16, i32, i64, u8, u16, u32, u64)
├── Compound
│   ├── Arrays [T; N]
│   ├── Structs
│   └── Enums
├── Generic
│   └── Type Parameters (T, U, etc.)
└── Special
    ├── Functions
    ├── Qualified Names (module::Type)
    └── Custom Types
```

## Primitive Types

Primitive types use an efficient representation in the AST through the `SimpleTypeKind` enum. This provides a value-based representation without heap allocation, which is then converted to `TypeInfoKind` by the type checker for semantic analysis.

**AST Representation**: `Type::Simple(SimpleTypeKind)` - Lightweight enum value
**Type Checker Representation**: `TypeInfoKind` - Semantic analysis format

### Unit Type

The unit type represents the absence of a value, similar to `void` in other languages.

```rust
fn do_something() {
    // Implicitly returns unit
}

fn explicit_unit() -> unit {
    return;  // Explicit unit return
}
```

**AST Representation**: `Type::Simple(SimpleTypeKind::Unit)`
**Type Checker Representation**: `TypeInfoKind::Unit`

### Boolean Type

Boolean values are either `true` or `false`.

```rust
fn test() -> bool {
    let x: bool = true;
    let y: bool = false;
    return x && y;
}
```

**AST Representation**: `Type::Simple(SimpleTypeKind::Bool)`
**Type Checker Representation**: `TypeInfoKind::Bool`

**Operations**:
- Logical: `&&`, `||`, `!`
- Comparison: `==`, `!=`
- Conditions: `if`, `while`, `loop`

### String Type

UTF-8 encoded strings.

**Note**: String is not currently part of `SimpleTypeKind` as it's not yet fully implemented as a primitive type in the compiler. String support is under development.

```rust
fn greet(name: string) -> string {
    return name;
}
```

**Type Checker Representation**: `TypeInfoKind::String`

**Operations**:
- Comparison: `==`, `!=`

### Numeric Types

Eight numeric types with different sizes and signedness:

| Type | Size | Range | Signed |
|------|------|-------|--------|
| `i8` | 8 bits | -128 to 127 | Yes |
| `i16` | 16 bits | -32,768 to 32,767 | Yes |
| `i32` | 32 bits | -2^31 to 2^31-1 | Yes |
| `i64` | 64 bits | -2^63 to 2^63-1 | Yes |
| `u8` | 8 bits | 0 to 255 | No |
| `u16` | 16 bits | 0 to 65,535 | No |
| `u32` | 32 bits | 0 to 2^32-1 | No |
| `u64` | 64 bits | 0 to 2^64-1 | No |

**AST Representation**: `Type::Simple(SimpleTypeKind::{I8, I16, I32, I64, U8, U16, U32, U64})`
**Type Checker Representation**: `TypeInfoKind::Number(NumberType)`

```rust
// In the AST (inference_ast crate)
enum SimpleTypeKind {
    Unit, Bool,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
}

// In the type checker (inference_type_checker crate)
enum NumberType {
    I8, I16, I32, I64,
    U8, U16, U32, U64,
}
```

**Examples**:

```rust
fn test_numbers() {
    let a: i8 = 127;
    let b: i16 = 32767;
    let c: i32 = 2147483647;
    let d: i64 = 9223372036854775807;

    let e: u8 = 255;
    let f: u16 = 65535;
    let g: u32 = 4294967295;
    let h: u64 = 18446744073709551615;
}
```

**Operations**:
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `**` (power)
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Bitwise: `&`, `|`, `^`, `<<`, `>>`, `~`
- Unary: `-` (signed only), `~`

## Compound Types

### Arrays

Fixed-size arrays with homogeneous element types.

**Syntax**: `[ElementType; Size]`

```rust
fn test_arrays() {
    let arr1: [i32; 5] = [1, 2, 3, 4, 5];
    let arr2: [bool; 3] = [true, false, true];

    // Nested arrays
    let matrix: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
}
```

**Representation**: `TypeInfoKind::Array(Box<TypeInfo>, u32)`

**Type Rules**:
1. All elements must have the same type
2. Size must match the number of elements
3. Empty arrays require type annotation

```rust
// Valid
let arr: [i32; 3] = [1, 2, 3];

// Error: size mismatch
let arr: [i32; 3] = [1, 2, 3, 4, 5];

// Error: type mismatch
let arr: [i32; 3] = [1, 2, true];

// Error: cannot infer type
let arr = [];

// Valid: empty array with type
let arr: [i32; 0] = [];
```

**Array Indexing**:

```rust
fn test_indexing() {
    let arr: [i32; 5] = [10, 20, 30, 40, 50];

    let x = arr[0];   // x: i32
    let y = arr[2];   // y: i32

    // Index must be numeric
    let i: i32 = 1;
    let z = arr[i];   // Valid
}
```

**Nested Array Indexing**:

```rust
fn test_nested() {
    let matrix: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

    let row = matrix[0];      // row: [i32; 3]
    let element = matrix[0][1];  // element: i32
}
```

### Structs

User-defined composite types with named fields.

```rust
struct Point {
    x: i32,
    y: i32,
}

struct Person {
    name: string,
    age: i32,
    active: bool,
}
```

**Representation**: `TypeInfoKind::Struct(String)`

**Struct Literals**:

```rust
fn test_structs() {
    let p = Point { x: 10, y: 20 };
    let person = Person {
        name: "Alice",
        age: 30,
        active: true,
    };
}
```

**Field Access**:

```rust
fn test_field_access() {
    let p = Point { x: 10, y: 20 };

    let x_val = p.x;  // x_val: i32
    let y_val = p.y;  // y_val: i32
}
```

**Type Rules**:
1. All fields must be initialized in struct literals
2. Field types must match struct definition
3. Field access respects visibility rules

### Enums

Enumerated types with unit variants.

```rust
enum Color {
    Red,
    Green,
    Blue,
}

enum Status {
    Active,
    Inactive,
    Pending,
}
```

**Representation**: `TypeInfoKind::Enum(String)`

**Variant Access**:

```rust
fn test_enums() {
    let c = Color::Red;
    let s = Status::Active;
}
```

**Type Rules**:
1. Variants must be defined in the enum
2. Variant access uses `::` syntax
3. Currently only unit variants (no associated data)

## Generic Types

### Type Parameters

Generic functions can be parameterized over types.

```rust
fn identity<T>(x: T) -> T {
    return x;
}

fn first<T>(arr: [T; 2]) -> T {
    return arr[0];
}
```

**Representation**: `TypeInfoKind::Generic(String)`

### Type Parameter Inference

Type parameters are inferred at call sites:

```rust
fn identity<T>(x: T) -> T {
    return x;
}

fn test() {
    let x = identity(42);      // T inferred as i32
    let y = identity(true);    // T inferred as bool
    let z = identity("hello"); // T inferred as string
}
```

### Type Substitution

When calling generic functions, type parameters are substituted:

```rust
// Generic function definition
fn swap<T>(arr: [T; 2]) -> [T; 2] {
    return [arr[1], arr[0]];
}

// Call site
let result = swap([1, 2]);

// Type substitution:
// T → i32
// [T; 2] → [i32; 2]
// Return type: [i32; 2]
```

**Substitution Algorithm**:
1. Match argument types to parameter types
2. Build substitution map: `{ "T" -> concrete_type }`
3. Apply substitutions to return type and body
4. Verify no unresolved parameters remain

### Generic Arrays

Arrays can be generic over element type:

```rust
fn sum_array<T>(arr: [T; 3]) -> T {
    // T must be numeric for + operator
    return arr[0] + arr[1] + arr[2];
}
```

## Type Inference Rules

### Expression Type Inference

The type checker uses bidirectional inference:

**1. Literals**: Type inferred from syntax
```rust
42          → i32
true        → bool
"hello"     → string
```

**2. Variables**: Type from declaration or initializer
```rust
let x: i32 = 42;      // x: i32 (explicit)
let y = 42;           // y: i32 (inferred from literal)
let z = x + y;        // z: i32 (inferred from operation)
```

**3. Binary Operations**: Type from operands
```rust
let a: i32 = 10;
let b: i32 = 20;
let sum = a + b;      // sum: i32
let equal = a == b;   // equal: bool (comparison result)
```

**4. Function Calls**: Type from return type
```rust
fn get_value() -> i32 {
    return 42;
}

let x = get_value();  // x: i32
```

**5. Array Indexing**: Type from array element type
```rust
let arr: [i32; 5] = [1, 2, 3, 4, 5];
let elem = arr[0];    // elem: i32
```

**6. Field Access**: Type from field definition
```rust
struct Point {
    x: i32,
    y: i64,
}

let p = Point { x: 10, y: 20 };
let x = p.x;          // x: i32
let y = p.y;          // y: i64
```

### Statement Type Checking

**1. Variable Definitions**: Check initializer against declared type
```rust
let x: i32 = 42;      // OK: 42 is i32
let y: bool = 42;     // Error: type mismatch
```

**2. Assignments**: Check value against variable type
```rust
let x: i32 = 10;
x = 20;               // OK: 20 is i32
x = true;             // Error: type mismatch
```

**3. Return Statements**: Check value against function return type
```rust
fn test() -> i32 {
    return 42;        // OK: 42 is i32
}

fn test2() -> i32 {
    return true;      // Error: type mismatch
}
```

**4. Conditions**: Must be boolean
```rust
if true { }           // OK
if 42 { }             // Error: condition must be bool
while x > 0 { }       // OK: x > 0 is bool
```

## Operator Type Rules

### Arithmetic Operators

**Operators**: `+`, `-`, `*`, `/`, `%`, `**`

**Type Rules**:
- Both operands must be numeric
- Both operands must be the same type
- Result type is the same as operand type

```rust
let a: i32 = 10;
let b: i32 = 20;
let sum = a + b;      // sum: i32
let diff = a - b;     // diff: i32
let prod = a * b;     // prod: i32
let quot = a / b;     // quot: i32
let rem = a % b;      // rem: i32
let pow = a ** b;     // pow: i32

// Error: type mismatch
let x: i32 = 10;
let y: i64 = 20;
let z = x + y;        // Error: cannot add i32 and i64
```

**Division Operator**: Recent addition to support division operations.

```rust
fn divide(a: i32, b: i32) -> i32 {
    return a / b;     // Division operator
}
```

### Comparison Operators

**Operators**: `==`, `!=`, `<`, `<=`, `>`, `>=`

**Type Rules**:
- Both operands must be the same type
- Numeric types: all comparisons allowed
- Non-numeric types: only `==` and `!=`
- Result type is always `bool`

```rust
let x: i32 = 10;
let y: i32 = 20;

let eq = x == y;      // eq: bool
let ne = x != y;      // ne: bool
let lt = x < y;       // lt: bool
let le = x <= y;      // le: bool
let gt = x > y;       // gt: bool
let ge = x >= y;      // ge: bool

// String comparison
let s1: string = "hello";
let s2: string = "world";
let equal = s1 == s2; // equal: bool
let not_equal = s1 != s2; // not_equal: bool
```

### Logical Operators

**Operators**: `&&`, `||`, `!`

**Type Rules**:
- All operands must be `bool`
- Result type is `bool`

```rust
let a: bool = true;
let b: bool = false;

let and_result = a && b;  // and_result: bool
let or_result = a || b;   // or_result: bool
let not_result = !a;      // not_result: bool

// Error: type mismatch
let x: i32 = 42;
let y = x && true;        // Error: && expects bool operands
```

### Bitwise Operators

**Operators**: `&`, `|`, `^`, `<<`, `>>`, `~`

**Type Rules**:
- Operands must be integer types
- Both operands must be the same type
- Result type is the same as operand type
- Unary `~` requires integer type

```rust
let a: i32 = 0b1010;
let b: i32 = 0b1100;

let and = a & b;      // and: i32 (0b1000)
let or = a | b;       // or: i32 (0b1110)
let xor = a ^ b;      // xor: i32 (0b0110)
let shl = a << 2;     // shl: i32 (shift left)
let shr = a >> 1;     // shr: i32 (shift right)
let not = ~a;         // not: i32 (bitwise NOT)
```

### Unary Operators

**Logical NOT**: `!`

**Type Rule**: Operand must be `bool`, result is `bool`

```rust
let x: bool = true;
let y = !x;           // y: bool (false)

// Error
let z: i32 = 42;
let w = !z;           // Error: ! expects bool
```

**Negation**: `-`

**Type Rule**: Operand must be signed integer (i8, i16, i32, i64), result is same type

```rust
let x: i32 = 42;
let y = -x;           // y: i32 (-42)

let a: i64 = 100;
let b = -a;           // b: i64 (-100)

// Error: unsigned types
let u: u32 = 10;
let v = -u;           // Error: negation not supported on unsigned types

// Error: non-numeric types
let b: bool = true;
let c = -b;           // Error: negation not supported on bool
```

**Bitwise NOT**: `~`

**Type Rule**: Operand must be integer type (signed or unsigned), result is same type

```rust
let x: i32 = 0b1010;
let y = ~x;           // y: i32 (bitwise complement)

let u: u32 = 0xFF;
let v = ~u;           // v: u32 (bitwise complement)
```

**Summary Table**:

| Operator | Name | Operand Type | Result Type |
|----------|------|--------------|-------------|
| `!` | Logical NOT | `bool` | `bool` |
| `-` | Negation | Signed int (i8/i16/i32/i64) | Same as operand |
| `~` | Bitwise NOT | Integer (signed or unsigned) | Same as operand |

## Method Resolution

### Instance Methods

Methods that take `self` as the first parameter.

```rust
struct Counter {
    value: i32,
}

impl Counter {
    fn increment(&self) -> i32 {
        return self.value + 1;
    }

    fn get_value(&self) -> i32 {
        return self.value;
    }
}

fn test() {
    let c = Counter { value: 10 };
    let next = c.increment();     // next: i32
    let val = c.get_value();      // val: i32
}
```

### Associated Functions

Functions in an `impl` block that don't take `self`.

```rust
struct Counter {
    value: i32,
}

impl Counter {
    fn new() -> Counter {
        return Counter { value: 0 };
    }
}

fn test() {
    let c = Counter::new();  // Associated function call
}
```

### Method Lookup Algorithm

1. Check if receiver is a struct type
2. Find the struct definition in symbol table
3. Look up method by name in struct's method table
4. Check visibility of method
5. Verify argument count and types
6. Return method signature with return type

### Method Type Checking

```rust
struct Calculator {
    base: i32,
}

impl Calculator {
    fn add(&self, x: i32, y: i32) -> i32 {
        return x + y + self.base;
    }
}

fn test() {
    let calc = Calculator { base: 100 };

    // Type check method call:
    // 1. calc is Calculator (struct)
    // 2. add is a method on Calculator
    // 3. Arguments: (5, 10) match (i32, i32)
    // 4. Return type: i32
    let result = calc.add(5, 10);  // result: i32
}
```

## Visibility and Access Control

### Visibility Modifiers

- `pub`: Public, accessible from any scope
- (default): Private, accessible only from defining scope and descendants

```rust
pub struct PublicStruct {
    pub public_field: i32,
    private_field: i32,  // Private by default
}

struct PrivateStruct {
    field: i32,
}

pub fn public_function() {}
fn private_function() {}
```

### Visibility Rules

**1. Type Visibility**: Controls who can name the type

```rust
pub struct Point { x: i32, y: i32 }  // Can be used anywhere
struct Internal { data: i32 }         // Can only be used in this module
```

**2. Field Visibility**: Controls who can access fields

```rust
struct Point {
    pub x: i32,     // Public field
    y: i32,         // Private field
}

fn test() {
    let p = Point { x: 10, y: 20 };
    let x = p.x;    // OK: public field
    let y = p.y;    // OK: same module
}

// In another module:
fn test2() {
    let p = Point { x: 10, y: 20 };
    let x = p.x;    // OK: public field
    let y = p.y;    // Error: private field
}
```

**3. Method Visibility**: Controls who can call methods

```rust
impl Point {
    pub fn distance(&self) -> i32 {
        // Public method
    }

    fn internal_method(&self) {
        // Private method
    }
}
```

**4. Function Visibility**: Controls who can call functions

```rust
pub fn public_function() {}
fn private_function() {}
```

### Visibility Checking Algorithm

```rust
fn is_accessible(symbol_scope: u32, access_scope: u32, visibility: Visibility) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => {
            // Private symbols accessible from defining scope and descendants
            access_scope == symbol_scope || is_descendant(access_scope, symbol_scope)
        }
    }
}
```

## Type Equivalence

### Structural Equivalence

Types are equivalent if they have the same structure:

```rust
// These are the same type
let x: i32 = 42;
let y: i32 = 100;

// These are the same type
let arr1: [i32; 5] = [1, 2, 3, 4, 5];
let arr2: [i32; 5] = [6, 7, 8, 9, 10];
```

### Nominal Equivalence

Structs and enums use nominal equivalence (name-based):

```rust
struct Point1 { x: i32, y: i32 }
struct Point2 { x: i32, y: i32 }

// Point1 and Point2 are different types, even with same structure
let p1: Point1 = Point1 { x: 10, y: 20 };
let p2: Point2 = p1;  // Error: type mismatch
```

## Type Compatibility

### Exact Match Required

The Inference type system does not perform implicit conversions:

```rust
let x: i32 = 42;
let y: i64 = x;       // Error: no implicit conversion from i32 to i64

let a: u32 = 10;
let b: i32 = a;       // Error: no implicit conversion from u32 to i32
```

### Array Size Must Match

```rust
let arr: [i32; 3] = [1, 2, 3];
let arr2: [i32; 5] = arr;  // Error: [i32; 3] != [i32; 5]
```

### Generic Type Constraints

Currently, there are no trait-based constraints on generic types. Type parameters are unconstrained:

```rust
fn identity<T>(x: T) -> T {
    return x;  // OK: no constraints on T
}

fn add<T>(a: T, b: T) -> T {
    return a + b;  // Error: + requires numeric type, T is unconstrained
}
```

## Future Type System Features

### Planned

- **Trait system**: Interface-based polymorphism with trait constraints
- **Type inference improvements**: Let-polymorphism for better local inference
- **Const generics**: Array sizes as generic parameters
- **Associated types**: Types associated with traits

### Under Consideration

- **Implicit conversions**: Numeric type widening (i32 → i64)
- **Type aliases with generics**: `type List<T> = [T; 10]`
- **Union types**: `i32 | i64`
- **Optional types**: `Option<T>`
- **Result types**: `Result<T, E>`

## Related Documentation

- [API Guide](./api-guide.md) - Using the type checker API
- [Architecture](./architecture.md) - Type checker internals
- [Error Reference](./errors.md) - Complete error catalog
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Official language spec
