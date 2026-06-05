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

The compiler solves this with a three-stage index registration pass in `lib.rs`
(`register_function_indices`). Importantly, **imported functions occupy the lowest indices
first**, so all local-function indices must be offset by the import count.

### Stage 0 — Import reservation (`register_imports`)

`external fn` declarations bound to a source module via `use … from <module>` are
emitted as WASM function imports. They are registered before any local function so
they occupy indices `0..N`. `set_local_func_base(N)` then seeds the local-function
index counter past the imports.

```text
register_imports(arena, extern_def_ids, ctx)
    extern_name_to_idx["sum"] = 0   (import at index 0)
    extern_name_to_idx["neg"] = 1   (import at index 1)
    returns N = 2  (import count)

set_local_func_base(2)              (locals now start at 2)
```

### Stage 1 — Top-level function registration (`build_func_name_to_idx`)

Local top-level functions are assigned indices starting at `N` (the import count
returned by Stage 0, passed as `base_idx`):

```text
build_func_name_to_idx(arena, func_def_ids, ctx, base_idx=N)
    func_name_to_idx["foo"] = N+0, ["bar"] = N+1, ...
```

### Stage 2 — Method registration (`build_method_name_to_idx`)

Struct methods are indexed under mangled names (`"{StructName}.{method_name}"`) starting
after all top-level functions:

```text
build_method_name_to_idx(arena, method_defs, ctx, base_idx=N+toplevel_count)
    func_name_to_idx["Point.new"]       = N + toplevel + 0
    func_name_to_idx["Point.translate"] = N + toplevel + 1
    method_mangled_names[("Point", "new")]       = "Point.new"
    method_mangled_names[("Point", "translate")] = "Point.translate"
```

All three stages run before any body is compiled, so all callee names resolve correctly
regardless of definition order in the source file and regardless of whether the callee
is an import or a local.

### Diagram

```text
register_function_indices
        |
        +---> register_imports(extern_def_ids)         // Stage 0
        |         extern_name_to_idx["sum"] = 0, ...
        |         returns N = import_count
        |
        +---> set_local_func_base(N)                   // seeds func_idx = N
        |
        +---> build_func_name_to_idx(func_def_ids, base_idx=N)   // Stage 1
        |         func_name_to_idx["foo"] = N+0, ["bar"] = N+1, ...
        |
        +---> build_method_name_to_idx(method_defs, base_idx=N+toplevel)  // Stage 2
        |         func_name_to_idx["Point.new"] = N+toplevel+0, ...
        |         method_mangled_names[("Point","new")] = "Point.new"
        |
        +---> visit_function_definition(func_def_ids[0], None)  // "foo"
        +---> visit_function_definition(func_def_ids[1], None)  // "bar"
        +---> ...
        +---> visit_function_definition(method_def_ids[0], Some("Point"))  // "Point.new"
                  |
                  | lower_function_call / lower_extern_call can look up any index
                  | regardless of definition order or import vs local
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

## Extern Function Calls

An `external fn` declaration bound to a source module is an import: it has no
local body to compile, but it does have a WASM function index (assigned by Stage
0) and a WASM type signature derived from the declared Inference parameter and
return types.

When `lower_function_call` resolves the callee and finds it in
`extern_name_to_idx`, it emits `call <import_idx>` via the same
`Instruction::Call` path used for local functions. The only difference is which
index table the lookup hits.

### Example

```inference
external fn sum(a: i32, b: i32) -> i32;
use { sum } from arith;

pub fn add_three(x: i32) -> i32 {
    return sum(x, 3);
}
```

After Stage 0, `sum` is at import index `0`; after Stage 1, `add_three` is at
local index `1`. The generated WAT:

```wat
(module
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (import "arith" "sum" (func (;0;) (type 0)))
  (func $add_three (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 3
    call 0
    return
    unreachable)
  (export "add_three" (func 1)))
```

### Import Section Emission

The import section is emitted in `finish_and_take` between the Type section and
the Function section (the WASM section ordering mandate). It is guarded by
`cov_mark::hit!(wasm_codegen_emit_import_section)` and omitted entirely when
there are no externs. Each entry carries the logical module name, the export
field name, and the type index from `intern_type`.

### Type Deduplication

`intern_type` deduplicates function signatures before assigning a type index: an
import and a local function (or two imports) with the same parameter and result
types share one type entry in the type section. This keeps the type section
compact even when multiple externs share a common signature.

## Supported vs Unsupported Callee Kinds

Four callee forms are now supported:

```inference
// Supported: plain identifier (local)
let x = foo(1, 2);
return bar();

// Supported: extern call
let y = sum(x, 3);  // sum is an external fn

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
| `wasm_codegen_emit_import_section` | 1+ | Import section emitted (fires whenever at least one `external fn` is present) |
| `wasm_codegen_emit_extern_call` | 1+ | Extern call lowered to `call <import_idx>` (fires in `single_import_test`) |

The `fn_params_test` verifies `wasm_codegen_emit_function_params` fires exactly 7 times
(matching `fn_params.inf`: 1+1+1+2+2 params). The `fn_calls_test` verifies
`wasm_codegen_emit_function_call` fires exactly 5 times. The `single_import_test` checks
both import-section marks together.

## Related Files

- `core/wasm-codegen/src/compiler.rs` — `register_imports`, `build_func_name_to_idx`, `build_method_name_to_idx`, `resolve_function_callee`, `lower_function_call`, `finish_and_take` (import section emission)
- `core/wasm-codegen/src/lib.rs` — `register_function_indices`, `traverse_t_ast_with_compiler`, `collect_emittable_functions` (extern fn routed to imports bucket)
- `core/wasm-codegen/src/errors.rs` — `CodegenError` enum
- `core/wasm-codegen/README.md` — Crate-level overview and compilation phases
- `core/wasm-codegen/docs/local-variables-lowering.md` — Local variable lowering (prerequisite)
- `core/wasm-linker/README.md` — How the linked output is produced from the import-bearing intermediate module
- `tests/test_data/codegen/wasm/extern_import/single_import/single_import.inf` — Minimal one-import fixture
- `tests/test_data/codegen/wasm/extern_import/multi_import/multi_import.inf` — Two imports, index shift
- `tests/test_data/codegen/wasm/extern_import/import_with_locals/import_with_locals.inf` — Import plus two local functions
- `tests/test_data/codegen/wasm/extern_import/import_dedup/import_dedup.inf` — Two same-signature imports sharing one type
- `tests/src/codegen/wasm/extern_import.rs` — Structural and golden tests for import emission
- `tests/test_data/codegen/wasm/base/fn_params/fn_params.inf` — Parameter test fixture
- `tests/test_data/codegen/wasm/base/fn_calls/fn_calls.inf` — Function call test fixture
- `tests/test_data/codegen/wasm/base/method_assoc/method_assoc.inf` — Associated function call fixture
- `tests/test_data/codegen/wasm/base/method_instance/method_instance.inf` — Instance method call fixture
