# Function Calls Lowering

This document describes how Inference function calls are compiled to WebAssembly `call`
instructions, covering the forward-reference pre-scan, the interlock between parameter
indices and body-local indices, the call lowering pipeline (including method and associated
function calls), drop emission rules, and known limitations.

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

The compiler solves this with a two-stage pre-scan in `lib.rs`: first top-level functions
are indexed, then struct methods are indexed with mangled names.

Stage 1 registers all top-level functions:

```text
build_func_name_to_idx(arena, func_def_ids, ctx)
    func_name_to_idx["foo"] = 0, ["bar"] = 1, ...
```

Stage 2 registers methods under mangled names (`"{StructName}.{method_name}"`):

```text
build_method_name_to_idx(arena, method_defs, ctx, base_idx)
    func_name_to_idx["Point.new"]       = base_idx + 0
    func_name_to_idx["Point.translate"] = base_idx + 1
    method_mangled_names[("Point", "new")]       = "Point.new"
    method_mangled_names[("Point", "translate")] = "Point.translate"
```

Both stages run before any body is compiled, so all callee names resolve correctly
regardless of definition order in the source file.

### Diagram

```text
traverse_t_ast_with_compiler
        |
        +---> build_func_name_to_idx(func_def_ids)
        |         func_name_to_idx["foo"] = 0, ["bar"] = 1, ...
        |
        +---> build_method_name_to_idx(method_defs, base_idx=N)
        |         func_name_to_idx["Point.new"] = N+0, ...
        |         method_mangled_names[("Point","new")] = "Point.new"
        |
        +---> visit_function_definition(func_def_ids[0], None)  // "foo"
        +---> visit_function_definition(func_def_ids[1], None)  // "bar"
        +---> ...
        +---> visit_function_definition(method_def_ids[0], Some("Point"))  // "Point.new"
                  |
                  | lower_function_call can look up any index
                  | regardless of definition order
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

Before emitting arguments or a `call` instruction, the compiler resolves which function to
call. This resolution is centralised in `resolve_function_callee`, which returns a
`ResolvedCallee` enum:

```text
ResolvedCallee
  ├── Function(name)                        — plain identifier call: foo()
  ├── AssociatedFunction { mangled_name, .. } — Type::method() syntax
  └── InstanceMethod { mangled_name, .. }    — receiver.method() syntax
```

For `AssociatedFunction` and `InstanceMethod`, the mangled name is formed by joining the
struct name and the method name with a dot (`"Point.new"`, `"Point.translate"`). This
matches the name inserted by `build_method_name_to_idx`, so the same `func_name_to_idx`
lookup works for all three callee kinds.

`lower_function_call` in `compiler.rs` handles the steps needed to emit a `call`:

```text
lower_function_call(fce, ctx)
        |
        1. resolve_function_callee → ResolvedCallee (name + optional receiver)
        |
        2. For InstanceMethod: push receiver (self pointer) as first argument
        |
        3. Lower remaining arguments in positional order
        |     for (label, expr) in fce.arguments:
        |         lower_expression(expr, ...)
        |     labels are ignored (WASM is purely positional)
        |
        4. Resolve callee index and emit call
              func_idx = func_name_to_idx[callee_name]
              func.instruction(&Instruction::Call(func_idx))
```

Argument labels (if present in source) are discarded at the WASM level because WebAssembly
has no concept of named arguments. The type-checker validates label correctness and
argument count before codegen runs.

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

Three callee forms are now supported:

```inference
// Supported: plain identifier
let x = foo(1, 2);
return bar();

// Supported: associated function call
let p = Point::new(1, 2);

// Supported: instance method call
let result = p.translate(5, 10);

// Not yet supported: higher-order / function pointer
let f = foo;
f(1);               // → todo!()
```

The `CodegenError` enum in `errors.rs` covers the remaining error cases:

```rust
pub(crate) enum CodegenError {
    UnknownFunction(String),           // → panic!() (type-checker inconsistency)
    UnsupportedSretReturnExpression,   // → panic!() (unexpected sret return form)
}
```

## Method Name Mangling

Methods are emitted as ordinary WASM functions. To avoid name collisions between methods
on different structs and between methods and top-level functions, the compiler mangles
method names by joining the struct name and method name with a dot:

```text
struct_name + "." + method_name
```

Examples:
- `Point::new` → `"Point.new"`
- `Counter::increment` → `"Counter.increment"`

The dot separator is chosen because it matches Zig's convention and is a standard WASM
name-section idiom. Since `.` is a syntax token in Inference (member access operator), it
cannot appear in any user-defined identifier, making accidental collisions impossible.

The `assert!` in `build_method_name_to_idx` guards against collisions with top-level
function names. A top-level function named `Point.something` would trigger this assertion
at startup, before any code is emitted.

Methods are never emitted as WASM exports, regardless of their declared visibility in
Inference source. Only top-level `pub fn` declarations produce WASM exports.

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

### Multi-File Calls

`build_func_name_to_idx` is invoked per source file. Cross-file function calls cannot be
resolved until multi-file compilation is implemented (currently `todo!()` in `codegen()`).

## Coverage Marks

| Mark | Count | Meaning |
|------|-------|---------|
| `wasm_codegen_emit_function_params` | 7 | 7 parameters across all functions in `fn_params.inf` |
| `wasm_codegen_emit_function_call` | 5 | 5 call sites in `fn_calls.inf` |
| `wasm_codegen_emit_self_copy_on_entry` | varies | `mut self` frame copy emitted for each method with mutable receiver |

The `fn_params_test` verifies `wasm_codegen_emit_function_params` fires exactly 7 times
(matching `fn_params.inf`: 1+1+1+2+2 params). The `fn_calls_test` verifies
`wasm_codegen_emit_function_call` fires exactly 5 times.

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `build_func_name_to_idx`, `build_method_name_to_idx`, `resolve_function_callee`, `lower_function_call`
- `core/wasm-codegen/src/errors.rs` — `CodegenError` enum
- `core/wasm-codegen/src/lib.rs` — `traverse_t_ast_with_compiler` (where pre-scan is called)
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `core/wasm-codegen/docs/local-variables-lowering.md` — Local variable lowering (prerequisite)
- `tests/test_data/codegen/wasm/base/fn_params/fn_params.inf` — Parameter test fixture
- `tests/test_data/codegen/wasm/base/fn_calls/fn_calls.inf` — Function call test fixture
- `tests/test_data/codegen/wasm/base/method_assoc/method_assoc.inf` — Associated function call fixture
- `tests/test_data/codegen/wasm/base/method_instance/method_instance.inf` — Instance method call fixture
