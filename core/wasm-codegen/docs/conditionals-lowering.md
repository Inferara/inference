# Conditionals Lowering

This document describes how Inference `if`/`else` statements are compiled to WebAssembly
structured control flow, and why the compiler emits an `unreachable` instruction before
the `end` of every non-void function body.

## Prerequisites

Readers should be familiar with:

- WebAssembly structured control flow — specifically the `if`/`else`/`end` instruction
  encoding and `BlockType` (see
  [WebAssembly spec, section 5.4.1](https://webassembly.github.io/spec/core/binary/instructions.html))
- WebAssembly stack typing and the polymorphic stack (see
  [WebAssembly spec, appendix A](https://webassembly.github.io/spec/core/appendix/algorithm.html))
- Inference `if`/`else` syntax (see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec))
- The overall compilation pipeline described in `core/wasm-codegen/README.md`
- Local variable lowering described in `docs/local-variables-lowering.md`

## Encoding an `if` Statement

### If-only (no else arm)

```inference
if x > 0 {
    return 1;
}
return 0;
```

The compiler lowers this as:

```text
lower_expression(condition)   // leaves i32 on stack (0 = false, non-zero = true)
If(BlockType::Empty)          // 0x04 0x40
  lower statements in if_arm
End                           // 0x0b
```

`BlockType::Empty` (`0x40`) is correct here because Inference `if` is a statement, not an
expression — it does not produce a value on the operand stack. The condition is consumed by
the `If` instruction itself (it pops one `i32`).

### If/else

```inference
if x > 0 {
    return 1;
} else {
    return 0;
}
```

The compiler emits an additional `Else` instruction between the two arms:

```text
lower_expression(condition)   // leaves i32 on stack
If(BlockType::Empty)          // 0x04 0x40
  lower statements in if_arm
Else                          // 0x05
  lower statements in else_arm
End                           // 0x0b
```

Both arms are lowered the same way as any other block: by calling `lower_statement` for
each statement in the arm.

### Nesting

Nested `if` statements simply recurse. Each call to `lower_if_statement` emits its own
`If`/`End` pair. The `parent_blocks_stack` is passed through unchanged so that drop
emission logic for expression statements sees the correct enclosing context at any depth.

```inference
if x > 0 {
    if y > 0 {
        return 2;
    }
    return 1;
}
return 0;
```

```text
local.get x
i32.const 0
i32.gt_s
If(Empty)           // outer if
  local.get y
  i32.const 0
  i32.gt_s
  If(Empty)         // inner if
    i32.const 2
    return
  End               // inner end
  i32.const 1
  return
End                 // outer end
i32.const 0
return
unreachable         // see next section
```

## Local Variables Inside If Arms

Local variables declared inside `if` or `else` arms are collected by `pre_scan_locals`
before any instructions are emitted. `pre_scan_locals` recurses into both arms of an
`IfStatement`:

```rust
Statement::If(if_statement) => {
    Self::pre_scan_locals(&if_statement.if_arm, ctx, locals_map, local_idx);
    if let Some(else_arm) = &if_statement.else_arm {
        Self::pre_scan_locals(else_arm, ctx, locals_map, local_idx);
    }
}
```

The result is that locals declared in `if` and `else` arms are pooled with all other
function-scoped locals. See `docs/local-variables-lowering.md` for the full explanation of
scope flattening.

### Example

```inference
if x > 0 {
    let a: i32 = x;   // pre-scan assigns a -> (1, I32)
    return a;
} else {
    let b: i32 = x;   // pre-scan assigns b -> (2, I32)
    return b;
}
```

`Function::new` declares two extra locals (indices 1 and 2), both `i32`. At runtime, only
one of `a` or `b` is ever written — the other slot is initialised to zero by the WASM
runtime but never used.

## The `unreachable` Sentinel at Function End

### The Problem

The WASM validator is a linear-time stack type-checker. It verifies every textual position
in the instruction sequence, including positions after unconditional branches. When a
non-void function's `if`/`else` block covers all control-flow paths via explicit `return`
instructions, there is no value on the stack at the function's `end` instruction — yet the
validator requires one:

```wat
(func $if_else_branch (param $x i32) (result i32)
  local.get $x
  i32.const 0
  i32.gt_s
  if
    i32.const 1
    return        ;; exits the function
  else
    i32.const 0
    return        ;; exits the function
  end
  ;; <-- stack is empty here; validator expects i32
)
```

Without any instruction after `end`, the module fails validation with a type mismatch.

### The Solution

The compiler emits `unreachable` immediately before the function's `end`:

```wat
(func $if_else_branch (param $x i32) (result i32)
  local.get $x
  i32.const 0
  i32.gt_s
  if
    i32.const 1
    return
  else
    i32.const 0
    return
  end
  unreachable   ;; makes the operand stack polymorphic
)
```

The `unreachable` instruction is
[stack-polymorphic per the WASM specification](https://webassembly.github.io/spec/core/valid/instructions.html):
after it, the validator enters a polymorphic state in which any required type is trivially
satisfied. This means the `result i32` at function `end` is accepted without an actual
`i32` on the stack.

The `unreachable` is emitted for every non-void function, regardless of whether all paths
actually return:

```rust
if has_return_value {
    func.instruction(&Instruction::Unreachable);
}
func.instruction(&Instruction::End);
```

For void functions (`unit` return type), no value is expected at `end`, so the
`unreachable` is omitted.

### Runtime Behaviour

When the program is correct — all control paths return — the `unreachable` is dead code
and never executes. The WASM runtime's JIT compiler will eliminate it after the first
unconditional branch.

When the program has a bug — a path falls through without returning — the `unreachable`
traps at runtime with a deterministic `RuntimeError: unreachable executed` error and a
stack trace. This is strictly safer than returning a default zero value.

### Relation to Return-Path Analysis

Emitting `unreachable` as a runtime safety net is **not** a substitute for compile-time
return-path analysis. The `core/analysis/` crate (planned) will enforce that every
non-void function returns on all paths, producing a compile-time error for violations. The
`unreachable` sentinel serves as defense-in-depth: if the analysis has a bug or is
bypassed, the program traps rather than silently misbehaving.

This mirrors the strategy used by rustc, LLVM/Clang, GCC, Zig, and Binaryen, all of which
enforce returns in the front end **and** emit traps in codegen. For a detailed treatment of
this design decision and WASM spec rationale, see
`book/unreachable-emission-in-codegen.md`.

## Coverage Marks

The two marks are hit every time `lower_if_statement` executes. Each test checks its own
expected count independently via `cov_mark::check_count!`:

| Test | Mark | Expected count |
|------|------|----------------|
| `if_else_test` | `wasm_codegen_emit_if_statement` | 7 (one per `if` in `if_else.inf`) |
| `if_else_test` | `wasm_codegen_emit_if_with_else` | 2 (two `if`/`else` pairs in `if_else.inf`) |
| `if_bool_exprs_test` | `wasm_codegen_emit_if_statement` | 16 |
| `if_bool_exprs_test` | `wasm_codegen_emit_if_with_else` | 5 |
| `if_nondet_test` | `wasm_codegen_emit_if_statement` | 1 (one `if` inside `forall`) |

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `lower_if_statement`, `pre_scan_locals` (if arm),
  `visit_function_definition` (unreachable emission)
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `core/wasm-codegen/docs/local-variables-lowering.md` — Scope flattening for if/else
  arm locals
- `book/unreachable-emission-in-codegen.md` — Detailed rationale for the `unreachable`
  sentinel, WASM spec references, and comparison with other compilers
- `tests/test_data/codegen/wasm/base/if_else/if_else.inf` — if/else test fixture
- `tests/test_data/codegen/wasm/base/if_bool_exprs/if_bool_exprs.inf` — boolean
  condition test fixture
- `tests/test_data/codegen/wasm/base/if_nondet/if_nondet.inf` — if inside forall fixture
- `tests/src/codegen/wasm/base.rs` — `if_else_test`, `if_else_exec_test`,
  `if_nondet_test`, `if_bool_exprs_test`, `if_bool_exprs_exec_test`
