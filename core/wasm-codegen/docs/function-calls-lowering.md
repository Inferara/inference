# Function Calls Lowering

This document describes how Inference function calls are compiled to WebAssembly `call`
instructions, covering the forward-reference pre-scan, the interlock between parameter
indices and body-local indices, the call lowering pipeline, drop emission rules, and known
limitations.

## Prerequisites

Readers should be familiar with:

- The WebAssembly binary format — specifically function indices, type signatures, and the
  `call` instruction (see
  [WebAssembly spec, section 5.4.1](https://webassembly.github.io/spec/core/binary/instructions.html))
- Inference function syntax (see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec))
- The overall compilation pipeline described in `core/wasm-codegen/README.md`
- Local variable lowering described in `docs/local-variables-lowering.md`

## Why Forward References Require a Pre-Scan

In WebAssembly, the `call` instruction takes a function index — an integer that identifies
the callee by its position in the WASM function section. The function section is ordered by
definition order in source.

Inference allows forward references: a caller can appear before its callee in the source
file. A single-pass compiler that emits `call` instructions as it encounters calls would
not yet know the index of a callee defined later.

The compiler solves this with a dedicated pre-scan in `lib.rs`:

```rust
fn traverse_t_ast_with_compiler(typed_context: &TypedContext, compiler: &mut Compiler) {
    for source_file in &typed_context.source_files() {
        let func_defs = source_file.function_definitions();
        // Pre-scan: build function name-to-index map so that forward references
        // (callee defined after caller in source) resolve correctly at call sites.
        compiler.build_func_name_to_idx(&func_defs);
        for func_def in func_defs {
            compiler.visit_function_definition(&func_def, typed_context);
        }
    }
}
```

`build_func_name_to_idx` assigns each function its WASM index — the same ordering used
during `visit_function_definition`. This guarantees that when `lower_function_call` looks
up a callee name, it finds the correct index regardless of whether the callee was already
compiled.

### Diagram

```text
traverse_t_ast_with_compiler
        |
        +---> build_func_name_to_idx(func_defs)
        |         |
        |         | Enumerate funcs in source order
        |         | func_name_to_idx["foo"] = 0, ["bar"] = 1, ...
        |         v
        |     func_name_to_idx populated for ALL functions
        |
        +---> visit_function_definition(func_defs[0])  // "foo"
        +---> visit_function_definition(func_defs[1])  // "bar"
        +---> ...
                  |
                  | lower_function_call("bar") can look up index 1
                  | even if called from "foo" (index 0, defined first)
```

## How Parameter Indices Interlock with Local Indices

WebAssembly represents function parameters as the first locals in a function body. A
function with signature `(i32, i64) -> i32` has:

- Local 0: first `i32` parameter
- Local 1: `i64` parameter
- Locals 2, 3, ...: additional locals declared in the body

The compiler implements this by processing parameters first, before `pre_scan_locals`:

```text
visit_function_definition
        |
        +---> Process parameters: populate locals_map[param_name] = (0..param_count, vt)
        |         local_idx starts at 0 and increments for each param
        |
        +---> param_count = local_idx  (save watermark)
        |
        +---> pre_scan_locals(body, locals_map, local_idx)
        |         local_idx continues from param_count (no reset)
        |         body locals get indices param_count, param_count+1, ...
        |
        +---> Function::new(local_declarations)
                  only declares locals with index >= param_count
                  (params are implicit from the type signature)
```

This means that within `locals_map`, parameters and body locals share the same namespace
and can be accessed uniformly via `local.get <index>` — the WASM VM handles the
distinction transparently.

### Example

```inference
fn first_of_two(a: i32, b: i32) -> i32 {
    let tmp: i32 = a;
    return tmp;
}
```

`locals_map` after pre-scan:

- `"a"` → (0, I32)
- `"b"` → (1, I32)
- `"tmp"` → (2, I32)

`Function::new` receives only `[(1, I32)]` for `tmp` (index 2, but declared as count=1 of
that type). `a` and `b` are implicit from the type signature.

Generated body:

```text
local.get 0   ; a
local.set 2   ; tmp = a
local.get 2   ; tmp
return
```

## The Call Lowering Pipeline

`lower_function_call` in `compiler.rs` handles the three steps needed to emit a `call`:

```text
lower_function_call(fce, ctx, func, locals_map)
        |
        1. Check callee kind: only Expression::Identifier accepted
        |     Other kinds → Err(CodegenError::UnsupportedCalleeKind)
        |
        2. Lower arguments in positional order
        |     for (label, expr) in fce.arguments:
        |         lower_expression(expr, ...)  // pushes arg onto WASM stack
        |     labels are ignored (WASM is purely positional)
        |
        3. Resolve callee index and emit call
              func_idx = func_name_to_idx[fce.name()]
              func.instruction(&Instruction::Call(func_idx))
```

Argument labels (if present in source) are discarded at the WASM level because WebAssembly
has no concept of named arguments. The type-checker validates label correctness and
argument count before codegen runs.

### Code Path

```rust
fn lower_function_call(&self, fce, ctx, func, locals_map) -> Result<(), CodegenError> {
    let Expression::Identifier(_) = &fce.function else {
        return Err(CodegenError::UnsupportedCalleeKind);
    };
    cov_mark::hit!(wasm_codegen_emit_function_call);
    if let Some(arguments) = &fce.arguments {
        for (_label, expr_ref) in arguments {
            self.lower_expression(&expr_ref.borrow(), ctx, func, locals_map);
        }
    }
    let func_name = fce.name();
    let func_idx = self.func_name_to_idx.get(&func_name).copied()
        .ok_or(CodegenError::UnknownFunction(func_name))?;
    func.instruction(&Instruction::Call(func_idx));
    Ok(())
}
```

## Drop Emission Rules

WebAssembly is a stack machine. A value-returning function call leaves its return value on
the operand stack. When the call appears as a standalone expression statement (rather than
being consumed by `local.set` or another expression), that value must be explicitly dropped
to keep the stack balanced.

The `Statement::Expression` arm in `lower_statement` determines whether to emit `drop`
after evaluating an expression:

```rust
Statement::Expression(expression) => {
    self.lower_expression(&expression, ctx, func, locals_map);
    let expr_produces_value = ctx.get_node_typeinfo(expression.id())
        .is_some_and(|ti| !matches!(ti.kind, TypeInfoKind::Unit));
    if expr_produces_value {
        let is_block_result = statements_iterator.peek().is_none()
            && parent_blocks_stack.last()
                .is_some_and(|b| b.is_non_det() && !b.is_void());
        if !is_block_result {
            func.instruction(&Instruction::Drop);
        }
    }
}
```

### Decision Table

| Call return type | Position in block | Drop emitted? | Reason |
|-----------------|-------------------|---------------|--------|
| `unit` (void) | anywhere | No | No value on stack |
| non-void | middle of block | Yes | Value not consumed; stack must be balanced |
| non-void | last stmt of non-det block | No | Value is the block's result, consumed by enclosing context |
| non-void | RHS of `let` | No | `local.set` consumes the value (different code path) |
| non-void | RHS of `return` | No | `return` consumes the value |

## Supported vs Unsupported Callee Kinds

Only plain identifier callees are currently supported:

```inference
// Supported: plain identifier
let x = foo(1, 2);
return bar();

// Not yet supported: method call
obj.method();       // → CodegenError::UnsupportedCalleeKind → todo!()

// Not yet supported: associated function
MyType::func();     // → CodegenError::UnsupportedCalleeKind → todo!()

// Not yet supported: higher-order / function pointer
let f = foo;
f(1);               // → CodegenError::UnsupportedCalleeKind → todo!()
```

The `CodegenError` enum in `errors.rs` encodes these distinctions:

```rust
pub(crate) enum CodegenError {
    UnsupportedCalleeKind,      // → todo!()  (planned future work)
    UnknownFunction(String),    // → panic!() (type-checker inconsistency)
}
```

## Known Limitations

### Recursion

Direct or indirect recursion is explicitly forbidden in Inference (Power of 10, Rule 1).
The analysis pass that detects recursive call graphs has not yet been implemented. At
codegen time, a recursive call is not specially detected — it would generate a valid `call`
instruction. The analysis pass must be added to reject recursive programs before they reach
codegen.

### Uzumaki Arguments

Passing `@` (uzumaki) as a function argument (e.g., `foo(@)`) triggers a type-checker
panic because the type-checker does not yet propagate the expected parameter type onto the
`@` expression. This is a gap in the type-checker, not in codegen.

### Method and Associated Function Calls

`obj.method()` and `Type::assoc()` call forms require member access resolution and
dispatch logic not yet implemented. They produce `todo!()` via
`CodegenError::UnsupportedCalleeKind`.

### Multi-File Calls

`build_func_name_to_idx` is invoked per source file. Cross-file function calls cannot be
resolved until multi-file compilation is implemented (currently `todo!()` in `codegen()`).

## Coverage Marks

| Mark | Count | Meaning |
|------|-------|---------|
| `wasm_codegen_emit_function_params` | 7 | 7 parameters across all functions in `fn_params.inf` |
| `wasm_codegen_emit_function_call` | 5 | 5 call sites in `fn_calls.inf` |

The `fn_params_test` verifies `wasm_codegen_emit_function_params` fires exactly 7 times
(matching `fn_params.inf`: 1+1+1+2+2 params). The `fn_calls_test` verifies
`wasm_codegen_emit_function_call` fires exactly 5 times.

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `build_func_name_to_idx`, `visit_function_definition`, `lower_function_call`
- `core/wasm-codegen/src/errors.rs` — `CodegenError` enum
- `core/wasm-codegen/src/lib.rs` — `traverse_t_ast_with_compiler` (where pre-scan is called)
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `core/wasm-codegen/docs/local-variables-lowering.md` — Local variable lowering (prerequisite)
- `tests/test_data/codegen/wasm/base/fn_params/fn_params.inf` — Parameter test fixture
- `tests/test_data/codegen/wasm/base/fn_calls/fn_calls.inf` — Function call test fixture
