# Assignment Statement Lowering

This document describes how assignment statements (`x = expr;`) are compiled to WebAssembly,
covering the supported target forms, the expression lowering pipeline, and local index resolution.

## Prerequisites

Readers should be familiar with:

- The WebAssembly stack machine execution model and local variable operations
- Inference assignment syntax and the `mut` keyword (see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec))
- How variables are pre-scanned and indexed during compilation (see
  [local-variables-lowering.md](local-variables-lowering.md))
- The overall compilation pipeline described in `core/wasm-codegen/README.md`

## Overview

An assignment statement has the form:

```inference
x = expression;
```

where `x` is a mutable variable (declared with `let mut x: Type = ...;`) or a mutable function
parameter (`fn foo(mut a: i32) { a = ...; }`).

### WASM Encoding

The assignment is lowered to:

```text
lower_expression(right_hand_side)    // Push the value onto the operand stack
local.set <local_idx>                // Store into the target variable's local
```

The `local_idx` is looked up in the `locals_map`, which was populated during the `pre_scan_locals`
phase (see [local-variables-lowering.md](local-variables-lowering.md)).

### Conceptual Example

```inference
pub fn update() -> i32 {
    let mut x: i32 = 5;
    x = 10;
    return x;
}
```

compiles to (pseudo-WASM):

```wasm
(local $x i32)
i32.const 5
local.set $x          ;; Initialize x
i32.const 10
local.set $x          ;; Update x to 10
local.get $x
return
```

## Lowering Implementation

### The `lower_assign_statement` Function

Located in `core/wasm-codegen/src/compiler.rs`, this function handles assignment lowering:

```rust
fn lower_assign_statement(
    &self,
    assign_stmt: &AssignStatement,
    ctx: &TypedContext,
    func: &mut Function,
    locals_map: &FxHashMap<String, (u32, ValType)>,
)
```

#### Parameters

- `assign_stmt` - The `AssignStatement` AST node containing `left` (target) and `right` (value expression)
- `ctx` - The typed context containing type information
- `func` - The `wasm_encoder::Function` builder for emitting instructions
- `locals_map` - The pre-populated map from variable names to (local_idx, ValType) pairs

#### Algorithm

1. **Match on target form** - Borrow the `left` expression and pattern match on its kind
2. **Identifier path** - If the target is an `Expression::Identifier`:
   - Look up the variable name in `locals_map`
   - Retrieve the local index
   - Lower the right-hand side expression via `lower_expression`
   - Emit `Instruction::LocalSet(local_idx)`
3. **Array index path** - If the target is an `Expression::ArrayIndexAccess`: compute element address (base + index * elem_size) and emit a store instruction via `lower_array_index_write`
4. **Member access path** - If the target is an `Expression::MemberAccess`: compute field address (struct_ptr + field_offset) and emit a store instruction via `lower_member_access_write`
5. **Unsupported paths** - Any other target form is refused, not lowered: `lower_assign_statement` records `CodegenError::UnsupportedConstruct` naming "an assignment to a target that is not a variable, an array element or a field" and stops. The type checker admits exactly the three shapes above and reports anything else as an invalid assignment target, so this is the backstop for a caller that skipped it (see [Fail-Closed Code Generation](type-checker-guarded-panics.md))

### Local Index Resolution

Variable names are globally collected during `pre_scan_locals`. This means:

- A mutable local declared at function scope is visible throughout the function
- A mutable local declared inside a `forall { }` or `if { }` block shares the same index pool
- An assignment target must refer to a variable that was declared in a previous `let mut` or function parameter

If a variable is not found in `locals_map`, the code calls `expect()`, which panics. That
stays a panic deliberately: `pre_scan_locals` walks the whole body and enters every binding
before any instruction is emitted, so a miss is a divergence between the pre-scan and the
emission pass rather than a property of the source. There is nothing to tell a user, and a
refusal would report a compiler bug as a source-level diagnostic. Refusals are reserved for
statements about the *program*; see [Fail-Closed Code Generation](type-checker-guarded-panics.md)
for the distinction.

### Type Information

The `ValType` stored in `locals_map` for each variable is the WebAssembly type (i32, i64, etc.).
The right-hand side expression is lowered to its corresponding WASM type, and `local.set` performs
the store operation. Type checking is performed by the type-checker phase before code generation.

## Supported Target Forms

### Identifier Targets

**Fully supported.** An assignment to a variable name:

```inference
pub fn demo() -> i32 {
    let mut x: i32 = 0;
    x = 42;
    return x;
}
```

Covered test cases in `tests/test_data/codegen/wasm/base/assign/assign.inf`:

- Simple assignment: `x = 42;`
- Assignment from expression: `x = 1 + 2;`
- Assignment from parameter: `x = a;`
- Multiple assignments: `x = 2; x = 3;`
- Assignment from function call: `x = get_three();`
- Assignment inside conditionals: `if x > 0 { result = x; }`
- Assignment to mutable parameters: `fn foo(mut a: i32) { a = 99; }`
- Assignments across all numeric types: i32, i64, bool

### Array Index Targets

**Supported.** Assignments like `arr[i] = value;` are handled by `lower_array_index_write`.
The element address is computed as `base_ptr + index * elem_size` using the same three-case
constant-folding specialization as array reads (zero index, constant non-zero index, runtime
variable index). See [arrays-and-memory.md](arrays-and-memory.md) for full details.

### Member Access Targets

**Supported.** Assignments like `p.x = value;` are handled by `lower_member_access_write`.
The field address is computed as `struct_ptr + field_offset`, where `field_offset` is resolved
from the precomputed `FrameLayout::struct_offsets` map (O(1)) or recomputed via
`compute_struct_field_layout` for parameters and complex expressions. See
[arrays-and-memory.md](arrays-and-memory.md) for full details.

## Coverage Markers

The assignment lowering emits the following `cov_mark` for testing:

- `wasm_codegen_emit_assign_identifier` - Incremented for each assignment to an identifier target

Test suite expects exactly 10 hits for `tests/test_data/codegen/wasm/base/assign/assign.inf`:
- 1 in `assign_simple_i32()`
- 1 in `assign_simple_i64()`
- 1 in `assign_from_expr()`
- 1 in `assign_from_param()`
- 2 in `assign_multiple()` (two assignments to the same variable)
- 1 in `assign_from_call()`
- 1 in `assign_bool()`
- 1 in `assign_in_if()`
- 1 in `assign_param_mut()` (assignment to a mutable parameter)

## Related Features

### Non-Deterministic Blocks

Assignments can appear inside `forall`, `exists`, `assume`, or `unique` blocks:

```inference
pub fn example() {
    forall {
        let mut x: i32 = 0;
        x = @;     // Assignment of uzumaki value
    }
}
```

The lowering is unchanged; the right-hand side expression (`@`) is lowered normally and the
result is stored via `local.set`. The non-deterministic semantics come from the expression
itself, not the assignment operation. See `tests/test_data/codegen/wasm/base/assign_nondet/`.

### Interaction with Control Flow

Assignments are permitted inside `if` and `else` blocks:

```inference
pub fn conditional_update(flag: bool) -> i32 {
    let mut result: i32 = 0;
    if flag {
        result = 100;
    }
    return result;
}
```

The `if` block is lowered with `BlockType::Empty` (see [conditionals-lowering.md](conditionals-lowering.md)),
so any side effects from assignments are preserved correctly.

## Limitations and Future Work

- **Tuple unpacking** - Pattern-based destructuring assignments are not yet supported
- **Validation** - The compiler does not validate that an assignment target is mutable at the codegen
  level; this is enforced by the type-checker (`AssignToImmutable` error) before codegen runs

## Examples

See test file `tests/test_data/codegen/wasm/base/assign/assign.inf` for comprehensive examples.

Example of the generated WASM (for `assign_simple_i32`):

```wasm
(func $assign_simple_i32 (result i32)
  (local $x i32)
  (i32.const 0)
  (local.set $x)
  (i32.const 42)
  (local.set $x)
  (local.get $x)
  (return))
```

## Related Documentation

- [local-variables-lowering.md](local-variables-lowering.md) - Local variable declaration and indexing
- [conditionals-lowering.md](conditionals-lowering.md) - `if`/`else` control flow
- [function-calls-lowering.md](function-calls-lowering.md) - Function call expressions (used in assignment RHS)
