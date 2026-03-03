# Loops Lowering

This document describes how Inference `loop` and `break` statements are compiled to
WebAssembly structured control flow, including depth tracking across nested blocks and
interaction with non-deterministic blocks and array frame layouts.

## Prerequisites

Readers should be familiar with:

- WebAssembly structured control flow — specifically the `block`/`loop`/`br`/`br_if`
  instruction encoding (see
  [WebAssembly spec, section 5.4.1](https://webassembly.github.io/spec/core/binary/instructions.html))
- Inference `loop`/`break` syntax (see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec))
- The overall compilation pipeline described in `core/wasm-codegen/README.md`
- Local variable lowering described in `docs/local-variables-lowering.md`
- Conditional lowering described in `docs/conditionals-lowering.md`

## Encoding a Conditional Loop

### Pattern

```inference
loop i < n {
    // body
    i = i + 1;
}
```

The compiler lowers this as a `block`+`loop` double-nesting:

```text
block $exit                   ;; forward branch target for exit
  loop $continue              ;; backward branch target for back-edge
    <lower condition>         ;; leaves i32 on stack
    i32.eqz                   ;; invert: 0 means "condition false"
    br_if 1                   ;; exit to $exit when condition is false
    <lower body statements>
    br 0                      ;; unconditional back-edge to $continue
  end
end
```

The outer `block` provides the forward branch target for loop exit. `br_if 1` targets depth
1 (the `block`), not depth 0 (the `loop`). The inner `loop` provides the backward branch
target: `br 0` jumps back to the top of the `loop`, re-evaluating the condition.

### Why double-nesting?

WASM `loop` instructions branch **backward** (to the loop header) when targeted by `br`.
To exit a loop, a **forward** branch target is needed — hence the outer `block`. This is
the standard pattern used by all WASM compilers (LLVM, Binaryen, wasm-tools).

## Encoding an Infinite Loop

### Pattern

```inference
loop {
    // body
    if done { break; }
}
```

The compiler emits the same double-nesting but without a condition check:

```text
block $exit                   ;; forward branch target for break
  loop $continue
    <lower body statements>   ;; break inside body targets $exit
    br 0                      ;; unconditional back-edge
  end
end
```

Without a `break`, the loop runs forever (the `br 0` unconditionally jumps back).

## Break Statement

`break` lowers to `br <depth>` where `depth` is computed from the `LoopContext`:

```rust
br_depth = wasm_block_depth - exit_depth - 1
```

- `wasm_block_depth` — current nesting depth of all WASM structured blocks (block, loop,
  if, non-det blocks)
- `exit_depth` — the `wasm_block_depth` at the time the enclosing loop's outer `block` was
  opened (recorded in `loop_exit_depths`)

### Example: break inside if inside loop

```inference
loop i < n {
    if i > 5 { break; }
    i = i + 1;
}
```

```text
block $exit           ;; depth 0 at entry, exit_depth = 0
  loop $continue      ;; wasm_block_depth = 2 after block+loop
    <condition check>
    br_if 1
    local.get $i
    i32.const 5
    i32.gt_s
    if                ;; wasm_block_depth = 3
      br 2            ;; 3 - 0 - 1 = 2, targets $exit
    end               ;; wasm_block_depth = 2
    <i = i + 1>
    br 0
  end
end
```

## LoopContext

The `LoopContext` struct tracks two pieces of state across the entire function:

```rust
struct LoopContext {
    wasm_block_depth: u32,
    loop_exit_depths: Vec<u32>,
}
```

- **`wasm_block_depth`** — incremented for every WASM structured block opened (`block`,
  `loop`, `if`, and non-deterministic blocks like `forall`/`exists`/`assume`/`unique`),
  decremented when each block closes.
- **`loop_exit_depths`** — a stack of saved `wasm_block_depth` values, one per enclosing
  loop. When entering a loop, the current depth is pushed; when exiting, it is popped.
  `break` reads the top of this stack to compute its `br` target.

The context is reset to default at the start of each function in `visit_function_definition`.

### Why non-det blocks matter

Non-deterministic blocks (`forall`, `exists`, `assume`, `unique`) also open WASM structured
blocks (custom opcodes followed by `0x40` block type and `0x0b` end). They increment
`wasm_block_depth` just like `if` and `loop` blocks. If a `break` appears after a non-det
block inside a loop, the depth computation must account for those blocks having opened and
closed:

```inference
loop i < n {
    forall {
        let mut x: i32 = @;
        assume { x = x; }
    }
    i = i + 1;
    if i > 5 { break; }
}
```

The `forall` block increments depth by 1 (and the `assume` inside it by another 1), but
both are decremented back when they close. The subsequent `break` inside `if` sees the
correct depth because `wasm_block_depth` tracks all opens and closes.

## Local Variables Inside Loops

Local variables declared inside loop bodies are collected by `pre_scan_locals` before any
instructions are emitted:

```rust
Statement::Loop(loop_statement) => {
    Self::pre_scan_locals(&loop_statement.body, ctx, locals_map, local_idx);
}
```

This follows the same scope-flattening strategy used for `if`/`else` arms. See
`docs/local-variables-lowering.md` for the full explanation.

Similarly, `collect_array_slots` recurses into loop bodies to discover array declarations
for frame layout computation:

```rust
Statement::Loop(loop_stmt) => {
    Self::collect_array_slots(&loop_stmt.body, ctx, array_offsets, current_offset);
}
```

## Early Return from Loop with Arrays

When a function has array variables (active frame layout), every `return` statement must
emit the stack epilogue before returning. This includes returns inside loop bodies:

```inference
pub fn loop_return_array(n: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    let mut i: i32 = 0;
    loop i < 4 {
        if arr[i] > n { return arr[i]; }
        i = i + 1;
    }
    return 0;
}
```

The `return arr[i]` inside the loop body emits:
1. Load `arr[i]` from memory
2. Stack epilogue (restore `__stack_pointer`)
3. `return` instruction

This is handled uniformly by the `Statement::Return` arm in `lower_statement`, which
checks for `frame_layout` on every return path regardless of nesting depth.

## Nested Loops

Nested loops push multiple entries onto `loop_exit_depths`. `break` always targets the
innermost enclosing loop (the last entry):

```inference
loop i < 3 {
    loop {
        if done { break; }  // targets inner loop's exit block
    }
    i = i + 1;
}
```

Each `lower_loop_statement` call pushes its own exit depth and pops it on return, so
depths are always correctly paired.

## Coverage Marks

| Mark | Location | Hit when |
|------|----------|----------|
| `wasm_codegen_emit_loop_statement` | `lower_loop_statement` | Any loop statement |
| `wasm_codegen_emit_loop_conditional` | `lower_loop_statement` | Loop with condition |
| `wasm_codegen_emit_loop_infinite` | `lower_loop_statement` | Loop without condition |
| `wasm_codegen_emit_break` | `lower_statement` (Break arm) | Break statement |

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `LoopContext`, `lower_loop_statement`,
  `lower_statement` (Break arm), `pre_scan_locals` (Loop arm)
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `core/wasm-codegen/docs/conditionals-lowering.md` — If/else lowering (shares
  `wasm_block_depth` tracking)
- `core/wasm-codegen/docs/local-variables-lowering.md` — Scope flattening for loop body
  locals
- `core/wasm-codegen/docs/arrays-and-memory.md` — Stack frame prologue/epilogue for
  functions with arrays
- `tests/src/codegen/wasm/loops.rs` — All loop codegen and execution tests
- `tests/test_data/codegen/wasm/loops/` — Loop test fixtures
