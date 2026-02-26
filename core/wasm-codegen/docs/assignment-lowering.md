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
3. **Unsupported paths** - Any other target form (member access, array index, etc.) → `todo!()`

### Local Index Resolution

Variable names are globally collected during `pre_scan_locals`. This means:

- A mutable local declared at function scope is visible throughout the function
- A mutable local declared inside a `forall { }` or `if { }` block shares the same index pool
- An assignment target must refer to a variable that was declared in a previous `let mut` or function parameter

If a variable is not found in `locals_map`, the code calls `expect()`, which panics with a
diagnostic message. This is a compile-time safety check (the type-checker should have verified
the variable exists and is mutable).

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

### Member Access and Array Index Targets

**Not yet implemented.** Assignments like:

```inference
obj.field = value;     // todo!()
arr[idx] = value;      // todo!()
```

currently emit a `todo!()` panic. These require:

- Member access resolution (struct/object field lookup)
- Array bounds tracking and index computation

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

- **Complex targets** - Member access (`obj.field = x`) and array index (`arr[i] = x`) targets are not yet supported
- **Tuple unpacking** - Pattern-based destructuring assignments are not yet supported
- **Validation** - The compiler does not yet validate that an assignment target is actually mutable;
  type-checker enforcement is relied upon

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
