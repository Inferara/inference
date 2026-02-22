# Local Variables Lowering

This document describes how `let` variable definitions are compiled to WebAssembly locals,
covering the two-pass design that the WebAssembly binary format requires, the expression kinds
that are accepted as initializers, and the type-dispatch logic inside `lower_literal`.

## Prerequisites

Readers should be familiar with:

- The WebAssembly binary format and its function body structure (see
  [WebAssembly spec, section 5.4.9](https://webassembly.github.io/spec/core/binary/instructions.html))
- Inference `let` and `const` syntax (see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec))
- The overall compilation pipeline described in `core/wasm-codegen/README.md`

## Why Two Passes Are Required

WebAssembly mandates that local variable declarations appear at the very beginning of a function
body, before any instructions. The binary format encodes a function body as:

```text
function body:
  local_count: u32
  locals:      [(count: u32, type: ValType)] * local_count
  code:        instruction*
  end:         0x0b
```

Inference functions can declare `let` and `const` locals anywhere inside a function body,
including inside nested blocks such as `forall { }` or `exists { }`. Because the declarations
must physically precede the instructions in the binary, a single-pass approach — declaring a
local at the point where the `let` statement is encountered — is not possible.

The compiler therefore operates in two passes per function:

```text
Pass 1 — pre_scan_locals
  Walk the entire function body (recursively into nested blocks).
  For every VariableDefinition or ConstantDefinition statement encountered,
  assign a sequential local index and record (name -> (index, ValType))
  in locals_map.

Pass 2 — lower_statement
  Walk the function body again to emit instructions.
  When a VariableDefinition or ConstantDefinition is encountered,
  look up its pre-assigned index in locals_map and emit
  the appropriate push instruction followed by local.set <index>.
```

The local declarations collected during Pass 1 are handed to `wasm_encoder::Function::new()`
before any instructions are emitted, satisfying the binary format requirement.

### Diagram

```text
visit_function_definition
        |
        +---> pre_scan_locals(body)
        |         |
        |         | Recursively walks all statements (including nested blocks)
        |         | Assigns monotonically increasing local indices
        |         v
        |     locals_map: { "x" -> (0, i32), "y" -> (1, i64), ... }
        |
        +---> Function::new(local_declarations)   <-- wasm-encoder
        |         |
        |         | local_declarations built from locals_map, sorted by index
        |
        +---> lower_statement(body, ...)
                  |
                  | Emits push + local.set for each VariableDefinition /
                  | ConstantDefinition using indices from locals_map
```

### Scope Flattening

The pre-scan intentionally flattens all nested scopes into a single WASM local pool. A
local declared inside a `forall { }` block shares the same pool as one declared at the
top of the function. This is consistent with how WebAssembly defines locals: they are
function-scoped, not block-scoped. The Inference type-checker is responsible for enforcing
lexical scoping rules at the language level.

## Supported Initializer Expression Kinds

The `Statement::VariableDefinition` arm inside `lower_statement` accepts three expression
kinds as the right-hand side of a `let` binding. Any other expression kind currently
results in a `todo!` panic, indicating it is not yet implemented.

### Literal

A compile-time constant written directly in source. The compiler calls `lower_literal` to
push the value onto the operand stack, then emits `local.set`.

```inference
let x: i32 = 42;
let flag: bool = true;
let n: i64 = -9223372036854775808;
```

Generated instructions (for `let x: i32 = 42`):

```text
i32.const 42
local.set 0
```

### Identifier

A reference to a previously declared local. The compiler looks up both the source and
destination indices in `locals_map`, emits `local.get <src>`, then `local.set <dst>`.

```inference
let x: i32 = 10;
let y: i32 = x;
```

Generated instructions (for `let y: i32 = x`):

```text
local.get 0   ; x is at index 0
local.set 1   ; y is at index 1
```

### Uzumaki (`@`)

Non-deterministic value generation. The compiler emits the custom `0xfc`-prefixed uzumaki
instruction for the appropriate WASM type, then `local.set`.

```inference
let a: i32 = @;
let b: i64 = @;
```

Generated instructions (for `let a: i32 = @`):

```text
0xfc 0x31     ; i32.uzumaki
local.set 0
```

Generated instructions (for `let b: i64 = @`):

```text
0xfc 0x32     ; i64.uzumaki
local.set 1
```

The uzumaki opcode is selected by consulting `TypedContext::is_node_i64`. If the node is
typed as `i64` or `u64`, `UZUMAKI_I64_OPCODE (0x32)` is used; otherwise
`UZUMAKI_I32_OPCODE (0x31)` is used.

## The `lower_literal` Type-Dispatch Logic

`lower_literal` is a shared helper called by both `ConstantDefinition` and
`VariableDefinition` arms. It takes a `&Literal` and emits the corresponding WASM const
instruction. The type information comes from `TypedContext::get_node_typeinfo`, which
carries the `TypeInfoKind` resolved by the type-checker.

### Bool

`bool` literals emit `i32.const 0` for `false` and `i32.const 1` for `true`. WebAssembly
has no native boolean type; by convention, booleans are represented as `i32` with 0/1
encoding.

### Number — Sub-i32 Types

Sub-i32 types require special handling because their source-level values may not fit inside
a Rust `i32` without conversion. WebAssembly stores them all as `i32` on the stack, so the
conversion must preserve the bit pattern expected by the calling convention:

| Inference type | Rust parse target | Widening to `i32`   | WASM instruction |
|----------------|-------------------|---------------------|------------------|
| `i8`           | `i32::parse`      | sign-extended by parser (negative values parse to negative i32) | `i32.const` |
| `i16`          | `i32::parse`      | same as i8          | `i32.const` |
| `i32`          | `i32::parse`      | identity            | `i32.const` |
| `u8`           | `u8::parse`       | `i32::from(u8)` — zero-extends | `i32.const` |
| `u16`          | `u16::parse`      | `i32::from(u16)` — zero-extends | `i32.const` |
| `u32`          | `u32::parse`      | `.cast_signed()` — reinterprets bit pattern | `i32.const` |

The distinction between `i8`/`i16`/`i32` and `u8`/`u16`/`u32` is necessary because:

- Signed sub-i32 types are parsed as `i32` directly. The Inference parser emits the literal
  text including any negative sign, so `i32::parse` already produces the correctly
  sign-extended value (e.g., `-128` parses to `-128_i32`).
- Unsigned sub-i32 types are parsed as their own unsigned Rust type first (`u8`, `u16`,
  `u32`) to validate the source-level range (e.g., `255` is valid for `u8`, but `-1` is
  not). They are then widened to `i32` using `i32::from` (zero-extension for `u8`/`u16`)
  or `.cast_signed()` (bit-reinterpretation for `u32`, where `4294967295_u32` becomes
  `-1_i32`).

### Number — i64 and u64

`i64` literals parse to Rust `i64` and emit `i64.const`. `u64` literals parse to Rust `u64`
and use `.cast_signed()` to obtain the `i64` bit pattern before emitting `i64.const`. This
ensures that `18446744073709551615` (which is `u64::MAX`) is emitted as `i64.const -1`,
the correct 2's-complement bit pattern.

### Unsupported literal kinds

`Array`, `String`, and `Unit` literals are not yet implemented and will produce a `todo!`
panic at compile time if encountered.

## Relationship to `ConstantDefinition`

`const` definitions use an identical code path for their values because `const` in Inference
only accepts literal initializers. Both `VariableDefinition` (for `let`) and
`ConstantDefinition` (for `const`) call `lower_literal` to push the value onto the stack,
then emit `local.set <index>`. The index in both cases comes from `locals_map` populated
during `pre_scan_locals`.

The difference is that `ConstantDefinition` does not need to handle `Identifier` or
`Uzumaki` initializers, because the grammar does not allow them as `const` values.

## Coverage Marks

The following `cov_mark` identifiers are used to verify that the described code paths are
exercised in tests:

| Mark | Location | Meaning |
|------|----------|---------|
| `wasm_codegen_emit_variable_definition` | `lower_statement`, `VariableDefinition` arm | A `let` statement was lowered |
| `wasm_codegen_variable_definition_uzumaki_i32` | uzumaki branch, i32 path | `let x: i32 = @` was lowered |
| `wasm_codegen_variable_definition_uzumaki_i64` | uzumaki branch, i64 path | `let x: i64 = @` was lowered |
| `wasm_codegen_emit_constant_definition` | `lower_statement`, `ConstantDefinition` arm | A `const` statement was lowered |

The `local_variables_test` in `tests/src/codegen/wasm/base.rs` checks that
`wasm_codegen_emit_variable_definition` fires exactly 14 times, matching the 14 `let`
bindings in `tests/test_data/codegen/wasm/base/local_variables/local_variables.inf`.

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `pre_scan_locals`, `lower_statement`, `lower_literal`
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `tests/test_data/codegen/wasm/base/local_variables/local_variables.inf` — Comprehensive `let` test fixture
- `tests/test_data/codegen/wasm/base/local_variables_exec/local_variables_exec.inf` — Executable test fixture
- `tests/src/codegen/wasm/base.rs` — `local_variables_test` and `local_variables_execution_test`
