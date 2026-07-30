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
  Walk the entire function body (recursively into nested blocks and if/else arms).
  For every VariableDefinition or ConstantDefinition statement encountered,
  assign a sequential local index and record (name -> (index, ValType))
  in locals_map.

Pass 2 — lower_statement
  Walk the function body again to emit instructions.
  When a VariableDefinition or ConstantDefinition is encountered,
  look up its pre-assigned index in locals_map and emit
  the appropriate push instruction followed by local.set <index>.
```

The local declarations collected during Pass 1 satisfy the binary format requirement — but
they are not the *complete* declaration list a function may need, and they are not handed to
`wasm_encoder::Function::new()` immediately. See
[Local Declarations Are Finalized After the Body Is Built](#local-declarations-are-finalized-after-the-body-is-built)
below for why.

### Diagram

```text
visit_function_definition
        |
        +---> pre_scan_locals(body)
        |         |
        |         | Recursively walks all statements (including nested blocks
        |         | and both arms of if/else statements)
        |         | Assigns monotonically increasing local indices
        |         v
        |     locals_map: { "x" -> (0, i32), "y" -> (1, i64), ... }
        |
        +---> Function::new([])   <-- wasm-encoder; body starts with NO
        |         |                  declarations, see note below
        |
        +---> lower_statement(body, ...)
        |         |
        |         | Emits push + local.set for each VariableDefinition /
        |         | ConstantDefinition using indices from locals_map.
        |         | Memory lowerings (region fill/copy) may allocate
        |         | further scratch locals from `RegionEmit` as they go.
        |
        +---> take_completed_function()
                  |
                  | Prefixes the already-encoded body with the complete
                  | declaration list: locals_map's declarations followed by
                  | one entry per allocated scratch local.
```

### Local Declarations Are Finalized After the Body Is Built

Pass 1's `locals_map` covers every named `let`/`const` local, plus the eagerly reserved
frame-pointer, bounds-check, and narrow-division temporaries — everything computable by
walking the AST without emitting any instructions. It does **not** cover the scratch i32
locals (`RegionEmit`) that the region fill and region copy lowerings allocate on demand
while the body is being emitted, for zero-initializing a stack frame or copying a compound
value between two addresses without a bulk-memory instruction (a build that permits bulk
memory emits the instruction and allocates no scratch local at all — see
[docs/arrays-and-memory.md](arrays-and-memory.md#region-fill-and-copy-lowering)). Whether a
function needs those, and how many, depends on what gets lowered — not something Pass 1 can
predict without duplicating the emission logic.

To reconcile this with the requirement that declarations precede instructions, the compiler
builds the body into a `wasm_encoder::Function` created with an *empty* locals vector
(`Function::new([])`). Once the body is fully emitted, `Compiler::take_completed_function`
takes the raw encoded body, strips its (empty) locals-vector prefix, and re-encodes it behind
the real declaration list — the names from `locals_map` followed by one `(1, ValType::I32)`
entry per allocated scratch slot. A function that allocates no scratch locals is
byte-identical to one built with the final declarations from the start.

### Scope Flattening

The pre-scan intentionally flattens all nested scopes into a single WASM local pool. A
local declared inside a `forall { }` block, an `if` arm, an `else` arm, or a `loop` body
shares the same pool as one declared at the top of the function. This is consistent with
how WebAssembly defines locals: they are function-scoped, not block-scoped. The Inference
type-checker enforces lexical scoping for nested (ancestor) shadowing; analysis rule A041
additionally rejects a name declared more than once across disjoint sibling blocks, since
such names are individually well-typed but would otherwise collide in this flat pool.

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

For a declared type narrower than `i32` (`i8`/`u8`/`i16`/`u16`/`bool`/an enum), the
draw is followed by a domain-constraint sequence before the `local.set`, so the
drawn value is confined to the declared type's value set instead of the full
32-bit draw range. For example, `let c: u8 = @;` lowers to:

```text
0xfc 0x31     ; i32.uzumaki
i32.const 255 ; 0xFF
i32.and
local.set 2
```

`i8`/`i16` use `shl`+`shr_s` (sign-narrow) instead of `and`; `bool` uses `and 1`;
a non-empty enum uses `rem_u <variant count>` (an empty enum is uninhabited and
left unconstrained, since `rem_u 0` would trap). `i32`/`u32`/`i64`/`u64` draws —
including both examples above — need no constraint, since their value set
already spans the full draw width. See `emit_uzumaki_domain_constraint` in
`compiler.rs`.

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
| `wasm_codegen_uzumaki_domain_narrow_int` | `emit_uzumaki_domain_constraint`, sub-i32 arm | An `i8`/`u8`/`i16`/`u16` draw was mask/shift-narrowed |
| `wasm_codegen_uzumaki_domain_bool` | `emit_uzumaki_domain_constraint`, `bool` arm | A `bool` draw was constrained via `and 1` |
| `wasm_codegen_uzumaki_domain_enum` | `emit_uzumaki_domain_constraint`, non-empty enum arm | A non-empty enum draw was constrained via `rem_u <variant count>` |
| `wasm_codegen_uzumaki_domain_enum_empty` | `emit_uzumaki_domain_constraint`, empty enum arm | A variantless enum draw was left unconstrained |
| `wasm_codegen_emit_constant_definition` | `lower_statement`, `ConstantDefinition` arm | A `const` statement was lowered |

The `local_variables_test` in `tests/src/codegen/wasm/base.rs` checks that
`wasm_codegen_emit_variable_definition` fires exactly 14 times, matching the 14 `let`
bindings in `tests/test_data/codegen/wasm/base/local_variables/local_variables.inf`.

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `pre_scan_locals`, `lower_statement`, `lower_literal`, `take_completed_function`
- `core/wasm-codegen/src/memory.rs` — `RegionEmit`
- `core/wasm-codegen/docs/arrays-and-memory.md` — Region fill/copy lowering that uses `RegionEmit`
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `tests/test_data/codegen/wasm/base/local_variables/local_variables.inf` — Comprehensive `let` test fixture
- `tests/test_data/codegen/wasm/base/local_variables_exec/local_variables_exec.inf` — Executable test fixture
- `tests/src/codegen/wasm/base.rs` — `local_variables_test` and `local_variables_execution_test`
