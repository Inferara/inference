# inference-wasm-codegen

WebAssembly code generation for the Inference compiler.

## Overview

This crate compiles Inference's typed AST directly to WebAssembly binary bytecode using the `wasm-encoder` crate. It supports standard WebAssembly instructions plus custom extensions for non-deterministic operations required for formal verification.

## Architecture

The compilation is performed entirely in-process with no external tool invocation:

```text
Typed AST (TypedContext)
        ↓
    Compiler  ← wasm-encoder
        ↓
  WASM Module (.wasm)
```

### Compilation Phases

1. **AST Traversal** - Walk the typed AST across all source files in canonical order (entry
   file first, then imported files sorted lexicographically by module path). Each file's
   definitions are visited in order; non-entry files contribute internal functions whose
   names are file-qualified (see phase 2).
2. **Import reservation + function index pre-scan** - Build the complete WASM function
   index space before any body is compiled, in two stages that each run across all files:
   (a) `register_imports` assigns indices `0..N` to every `external fn` declaration bound
   via `use … from <module>`, populating `extern_import_idx` — keyed by the declaring
   `DefId` rather than by the extern's name, since two files may declare the same name and
   bind it to different modules — and recording the `(logical_module, export_field,
   type_idx)` tuple needed for the import section. Declarations that bind the same
   `(module, field)` at the same signature share one import;
   (b) a two-pass local scan first registers all top-level functions from every source file
   under their mangled `FnKey` names, then registers all struct methods under their mangled
   names (`"{StructName}.{method_name}"`). Functions from the entry file use unqualified
   names (`add`, `main`); functions from non-entry files use file-qualified flat names
   joining module path segments and the item name with `.` (`lib.arith.add`,
   `lib.arith.Point.new`). This ensures all callee indices — imports, locals, and methods
   across every file — are known before the first `call` instruction is emitted.
   See [docs/function-calls-lowering.md](docs/function-calls-lowering.md).
3. **Compound Frame Layout** - For functions with array- or struct-typed variables or parameters,
   compute a stack frame layout by walking the entire function body and collecting array and struct
   declarations and parameter types. This pre-computation determines memory offsets for each compound
   value and allocates a synthetic `__frame_ptr` WASM local. Struct fields are laid out with C-compatible
   natural alignment (`compute_struct_field_layout`). A compound parameter earns a slot only when the
   body can write through it — see `compound_param_is_by_reference`; a parameter that is neither
   assigned through nor forwarded to an `external fn` is read straight from the caller's pointer. A
   function whose parameters and bindings all turn out to need no memory gets no frame at all: no
   `__frame_ptr`, no prologue, no epilogue, and no `__stack_pointer` mutation.
   See [docs/arrays-and-memory.md](docs/arrays-and-memory.md).
4. **Local Pre-scan** - Walk the entire function body once to collect all `let` and `const`
   declarations and assign them sequential WASM local indices before any instructions are
   emitted. This step is mandatory because the WebAssembly binary format requires all local
   declarations to appear at the very start of a function body, before the instruction
   sequence. See [docs/local-variables-lowering.md](docs/local-variables-lowering.md) for a
   detailed explanation.
5. **Instruction Emission** - Lower functions, statements, and expressions to WASM
   instructions. `let` definitions are lowered via a push instruction followed by
   `local.set`; `const` definitions use the same path. Supported initializer expression
   kinds are literals, identifiers, uzumaki (`@`) expressions, function calls, array
   literals, struct literals, and enum variant accesses. Enum variant access
   (`Color::Red`) is lowered via `Expr::TypeMemberAccess`: the type name is resolved via
   `ctx.lookup_enum()`, the variant name is looked up using `EnumInfo::variant_index()`,
   and an `i32.const <tag>` instruction is emitted. Enum values carry no linear memory
   footprint and are treated identically to `i32` scalars in locals, parameters, return
   values, and array elements (using the same `i32.load`/`i32.store` as `u32`).
   Array and struct variables automatically get frame
   allocation code (prologue) and deallocation code (epilogue). During variable
   initialization (`let` and `const`), array and struct literal elements whose value is
   syntactically zero (the literal `0`, `-0`, `false`, or any of these wrapped in
   parentheses or a unary negation) are not stored to linear memory; the function
   prologue's zero-fill already guarantees those bytes are zero. This
   zero-store elision applies only at initialization time — assignment statements always
   emit stores regardless of value because the destination may hold non-zero data from
   a prior operation. The elision is controlled by the `init_zero_elision` flag on
   `Compiler` and the `skip_zero_stores` parameter on `lower_struct_literal_fields`.
   See [docs/arrays-and-memory.md](docs/arrays-and-memory.md) for details.
   Array index access
   (read/write) compiles to load/store instructions with computed addresses. Struct field
   access (`p.x`) compiles to a load at `struct_pointer + field_offset`; struct field
   assignment (`p.x = v`) compiles to a store at the same address. Compound fields
   (nested structs and array-typed struct fields) use pointer semantics during member
   access: the member access expression pushes the field's base address rather than
   loading a scalar. `lower_struct_literal_fields` handles nested struct literal
   initialization and array-field initialization with element-wise stores or a region copy
   depending on whether the RHS is a literal or an identifier. Arrays of structs are
   supported: each element occupies `struct_total_size` bytes, addressed by
   `base + index * elem_size`; element field reads/writes use the same dispatch as plain
   struct member access. Function calls push arguments in positional order and emit a
   `call <func_idx>` instruction. A struct-typed parameter is copied into the callee's
   frame on entry when the body assigns through it or forwards it to an `external fn`
   (value semantics); otherwise it is read through the caller's pointer.
   Assignment statements (`x = value;` where `x` is declared `mut`) are lowered by
   evaluating the right-hand side expression and emitting `local.set` to store the result.
   Array index assignment (`arr[i] = value;`) computes the element address and emits a store
   instruction. `if`/`else` statements emit WASM structured `if`/`else`/`end` blocks with
   `BlockType::Empty` because Inference `if` is a statement, not an expression.
   Loop statements emit the standard WASM `block`+`loop` double-nesting pattern with a
   `br_if` exit check for conditional loops and `br 0` unconditional back-edge; `break`
   statements lower to `br <depth>` targeting the enclosing loop's exit block.
   See [docs/loops-lowering.md](docs/loops-lowering.md).
   Function calls are dispatched through `resolve_function_callee` which classifies the
   callee as a plain `Function`, an `AssociatedFunction` (called via `Type::method()`
   syntax), or an `InstanceMethod` (called via `receiver.method()` syntax). All three
   resolve to a mangled WASM function name that is looked up in `func_name_to_idx` before
   emitting `call <idx>`. Instance methods receive `self` as an implicit first argument
   (an `i32` pointer). A receiver is a parameter and obeys the same rule as any other: it
   is copied into the callee's frame on entry (value semantics — mutations do not reach
   the caller) when the body assigns through it, or when the body forwards `self` or a
   projection of it to an `external fn` — a linked external shares the program's linear
   memory and can store through the pointer it is handed, which would reach the caller's
   own memory. A receiver that does neither is read directly through the pointer,
   whether it is written `self` or `mut self`. Methods are never exported as WASM
   exports regardless of Inference visibility.
   See [docs/function-calls-lowering.md](docs/function-calls-lowering.md).
   Non-void functions emit an `unreachable` instruction before the function `end` to
   satisfy the WASM validator when all paths exit through explicit `return` instructions.
   See [docs/conditionals-lowering.md](docs/conditionals-lowering.md).
6. **Module Assembly** - Assemble sections in WASM-required order into a complete binary:
   TypeSection first, then ImportSection (only if at least one `external fn` is present;
   sits between Type and Function per WASM spec), FunctionSection, MemorySection and
   GlobalSection (only when at least one function uses arrays or structs), ExportSection,
   CodeSection, NameSection, and (proof mode, when non-empty) the `inference.spec_funcs` and
   `inference.hspecs` custom sections. The import section placement is
   mandatory because imported functions occupy the lowest indices and the section ordering
   is enforced by the binary format.

### Entry Parameter Normalization

An exported function is the module's WebAssembly ABI boundary, where a host may pass any
i32 bit pattern for a narrow scalar parameter. Each exported function (an entry-file
top-level `pub fn`, including `main`) canonicalizes its narrow parameters in the entry
prologue before the body runs: `u8`/`u16` take the argument's low bits (zero-extend),
`i8`/`i16` sign-extend from the low bits, and `bool` normalizes by truthiness (`p != 0`, so
any nonzero host value is `true`). In-language callers always pass canonical values and each
normalization is a fixed point on them, so in-domain behavior is unchanged and non-exported
functions are byte-identical. Enum-typed and compound parameters are not normalized.

### Shift Counts

A shift count is taken modulo the operand type's bit width. WebAssembly's `ishl`/`ishr`
already mask the count modulo 32/64, which is exactly the type width for `i32`/`u32`/`i64`/
`u64`, so those types emit nothing extra. Narrow types (`i8`/`u8`/`i16`/`u16`) promote to
i32, where wasm's mod-32 mask would produce a non-monotonic cliff, so a narrow-typed shift
masks the count (`& 7` / `& 15`) before the wasm shift, extending wasm's own mod-width
semantics to the declared type. The mask applies to both `<<` and `>>`.

### WebAssembly Feature Level

By default every emitted function body, and the module as a whole, stays within the
WebAssembly 1.0 (MVP) instruction set. Whole-region memory operations that would
naturally reach for the bulk-memory proposal's `memory.fill`/`memory.copy` — frame
zero-initialization in the prologue, array/struct parameter copies, sret returns, and
struct-to-struct assignment — are instead lowered to plain loads and stores: a
straight-line sequence for small regions, an index-driven loop for large ones. See
[docs/arrays-and-memory.md](docs/arrays-and-memory.md#region-fill-and-copy-lowering).

A build may opt back into the instructions by setting `EmitFeatures { bulk_memory: true }`
in the `CodegenOptions` passed to `codegen()` (`infc --wasm-features bulk-memory`), which restores the single
`memory.fill`/`memory.copy` at each of those sites. The feature set applies identically
in both compilation modes, so the Rocq translation always describes the same program as
the emitted binary, and the `Soroban` target rejects it because its runtime's acceptance
of the opcodes is unverified.

## Non-Deterministic Extensions

Inference supports non-deterministic constructs for formal verification through custom WebAssembly instructions in the `0xfc` prefix space, emitted via `wasm_encoder::Function::raw()`.

One family of proof-mode bodies deliberately emits none of them: an
`exists`/`unique`-quantified spec function is *reachability-lowered* to
vanilla WASM — each scalar `@` becomes a hidden trailing choice parameter
(name-section label `__choice{k}`) read with `local.get` (the narrow-type
domain sequence below is kept, applied to the loaded choice), nested
`exists`/`assume` block wrappers are suppressed, and `assume`/`assert`
filters compile to trap-on-false checks — because the downstream
reachability judgment reduces the retained body under vanilla WASM
semantics (see `hassert/reach.rs` and
[`core/wasm-to-v/ROCQ_CONTRACT.md`](../wasm-to-v/ROCQ_CONTRACT.md)). The
opcode encodings below cover every other non-det position:

### Uzumaki (`@`)

Non-deterministic value generation. Represents a variable that can hold any value of its type.

```inference
pub fn example() -> i32 {
    return @;  // Returns any i32 value
}
```

**Custom opcodes:**
- `i32.uzumaki` → `0xfc 0x31`
- `i64.uzumaki` → `0xfc 0x32`

The draw always produces a full-width value, so a declared type narrower than
`i32`/`i64` gets a domain-constraint sequence emitted immediately after the
draw, mapping the raw draw onto the type's value set:

| Declared type | Emitted after the draw | Resulting domain |
|---|---|---|
| `i8`, `i16` | `shl <32-width>`, `shr_s <32-width>` (sign-narrow) | full signed sub-range |
| `u8`, `u16` | `and <width bitmask>` | full unsigned sub-range |
| `bool` | `and 1` | `{0, 1}` |
| enum, N ≥ 1 variants | `rem_u N` | tags `0..N-1` |
| enum, 0 variants | none (uninhabited; `rem_u 0` would trap) | — |
| `i32`, `u32`, `i64`, `u64` | none | full 32/64-bit width |

This reuses the same mask / `shl`+`shr_s` shapes used to normalize sub-i32
arithmetic results at their producing instruction (binops, unary negation,
unary bitwise-not), rather than `i32.extend8_s`/`extend16_s`, which the
`wasm-to-v` translator does not implement. A compound (array/struct) uzumaki
leaf gets the same `bool`/enum constraint before its store; narrow-int
compound leaves need no constraint, since the element's `store8`/`store16`
truncation plus its sign/zero-extending typed load already realize the
domain. See [docs/local-variables-lowering.md](docs/local-variables-lowering.md)
and [docs/arrays-and-memory.md](docs/arrays-and-memory.md).

In proof mode a compound `@` also has to be given a meaning in the specification
function's *obligation*, and there the two quantifiers part ways: a `forall`
specification is a claim about values, so an aggregate can become as many logical
variables as it has scalar leaves; an `exists`/`unique` specification is a claim
about one actual run of its own compiled body, where an aggregate is a single
pointer into memory and each choice arrives as one scalar parameter — so there,
aggregates must still be taken apart by hand. See
[docs/specification-obligations.md](docs/specification-obligations.md) for the
leaf encoding, the slot-allocation rule, and the diagnostics that draw the line.

### Forall Block

Universal quantification - all execution paths inside the block must be reachable.

```inference
pub fn example() {
    forall {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3a` + `0x40` (block type) + body + `0x0b` (end)

### Exists Block

Existential quantification - at least one execution path inside the block must be reachable.

```inference
pub fn example() {
    exists {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3b` + `0x40` (block type) + body + `0x0b` (end)

### Assume Block

Precondition assumption - filters execution paths based on assumptions.

```inference
pub fn example() {
    assume {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3c` + `0x40` (block type) + body + `0x0b` (end)

### Unique Block

Uniqueness constraint - exactly one execution path is reachable inside the block.

```inference
pub fn example() {
    unique {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3d` + `0x40` (block type) + body + `0x0b` (end)

## Type Mapping

Inference types map to WebAssembly types:

| Inference Type | WASM Type | Notes                                      |
|----------------|-----------|--------------------------------------------|
| `unit`         | -         | No value produced                          |
| `bool`         | i32       |                                            |
| `i8`, `u8`     | i32       |                                            |
| `i16`, `u16`   | i32       |                                            |
| `i32`, `u32`   | i32       |                                            |
| `i64`, `u64`   | i64       |                                            |
| `[T; N]`       | i32       | Pointer to shadow-stack frame              |
| `struct S`     | i32       | Pointer to shadow-stack frame              |
| `enum E`       | i32       | Zero-based variant tag; no heap allocation |

WebAssembly only supports `i32`, `i64`, `f32`, and `f64` as value types. Smaller integer types use `i32` with appropriate truncation and extension during operations. Arrays and structs are represented as i32 pointers to linear memory; the compiler manages a shadow stack and emits prologue/epilogue code for frame allocation. Struct fields are laid out with C-compatible natural alignment. Enum values are pure scalars — a variant is compiled to its zero-based index (`Red = 0`, `Green = 1`, `Blue = 2`) and stored directly in an `i32` local or register without touching linear memory.

## WebAssembly Execution Model

Inference uses the **reactor model** rather than the command model:

### Command Model (Typical for WASI)

Languages like Rust and Zig targeting `wasm32-wasi` generate a `_start` entry point:

```text
_start() → runtime initialization → main() → exit
```

Execution: `wasmtime module.wasm`

### Reactor Model (Inference)

Inference produces reactor-style modules where the entry file's `pub` functions are exported and callable individually:

```text
// In src/main.inf (entry file)
pub fn main() → exported as "main"
pub fn foo()  → exported as "foo"
fn bar()      → not exported (private)

// In src/lib/arith.inf (non-entry file)
pub fn add()  → NOT exported (pub is intra-project visibility only)
```

Execution: `wasmtime --invoke main module.wasm`

**Why Reactor Model?**
- **Simplicity** - No runtime initialization overhead
- **Flexibility** - Multiple entry points; caller chooses which function to invoke
- **Embedding** - Better suited for embedding in host applications
- **Verification** - Functions are verified individually in formal verification

> **Planned:** When a `pub fn main` is present, the compiler will also emit a `_start` entry point so that the module can be executed as a regular program (e.g., `wasmtime module.wasm` without `--invoke`). The `has_main` detection is already implemented.

## Usage

```rust
use inference_wasm_codegen::{CodegenOptions, CompilationMode, OptLevel, Target, codegen};
use inference_type_checker::typed_context::TypedContext;

fn compile(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    // Generate WASM bytecode from typed AST
    let output = codegen(
        typed_context,
        "module",
        CodegenOptions {
            target: Target::Wasm32,
            mode: CompilationMode::Compile,
            opt_level: OptLevel::O2,
            ..Default::default()
        },
    )?;
    Ok(output.wasm().to_vec())
}
```

The `codegen` function:
1. Creates a compiler instance
2. Traverses the typed AST and emits WASM instructions for function definitions
3. Assembles the WASM module sections
4. Returns a `CodegenOutput` carrying the WASM bytecode (`.wasm()`) plus target/mode
   metadata and, in proof mode, the per-spec `hassert` obligation map

## Current Limitations

- **Top-level constructs** - Only function definitions are compiled. Top-level `const` declarations do not reach codegen (analysis rule A032 / issue #171); cross-file `const` type-checking works and will be extended when #171 lands
- **Control flow** - `loop` and `break` statements are now supported (conditional loops, infinite loops, nested loops, and break from any nesting depth). Assignment statements (`x = value;`) are supported for identifier targets, array index targets, and struct field targets (`p.x = v`).
- **Expression types** - Fixed-size arrays with scalar and enum element types are supported, including array-returning functions via the sret calling convention. Enum types are fully supported: variant access (`Color::Red`), enum-typed locals and parameters, enum return values, enum fields inside structs, enums in arrays, equality/inequality comparisons (`==`, `!=`), reassignment, and uzumaki initialization. Arithmetic operations and ordering comparisons on enum values are rejected by the type checker. Structs with scalar, enum, and compound fields are supported: struct literals with nested struct and array fields, member access read/write for both scalar and compound fields, struct parameters (copied on entry when the body may write them, otherwise passed by reference), struct-returning functions via sret, associated function calls (`Type::func()`), and instance method calls (`obj.method()`). Arrays of structs are supported: element reads, element field reads/writes, copy semantics, and struct-array parameters via sret. Nested structs (one level deep) and structs with array fields (one level deep) are supported; nesting beyond one level is rejected by analysis rule A026. Constructing a struct literal whose field is an array of structs (`Grid { cells: [Point { … }, … ] }`) or a multi-dimensional array (`Foo { grid: [[1, 2, 3], [4, 5, 6]] }` for a `[[i32; 3]; 2]` field, including arrays of structs such as `[[Point; 2]; 2]`) lowers element-by-element, with chained reads and writes through the field (`g.cells[i].x`, `g.grid[i][j]`). Multidimensional arrays (`[[i32; 3]; 2]`) are also supported for uzumaki initialization within non-deterministic blocks. Partial initialization syntax and mutable array parameters are not yet implemented. Higher-order function calls (function pointers) are not yet implemented.
- **Type system** - Generic types and function types are not yet fully implemented
- **Recursion with compound types** - Functions using arrays or structs cannot currently recurse (no stack overflow analysis). Recursion detection and stack bounds checking are future work.
- **Return-path analysis** - The analysis pass (rule A007) detects non-void functions missing a `return` on all paths and emits a compile-time error before codegen is reached. An `unreachable` trap is also emitted as a defence-in-depth runtime safety net; see [docs/conditionals-lowering.md](docs/conditionals-lowering.md).
- **Specification obligations** - A specification names aggregates by their ordered scalar leaves, over the same surface the executable aggregate `@` supports: arrays of scalars at any rank, and structs whose fields are scalars or one-dimensional scalar arrays. Arrays of structs (A028) and structs with struct or multidimensional-array fields (A027) are rejected on every specification path — `@`, parameters, literals. One access chain may carry one non-constant index; a spec function may quantify 64 scalar leaves in total (P013). Compound `@` and compound parameters are rejected inside `exists`/`unique`-quantified functions, where each choice arrives as one scalar parameter of the run the obligation talks about, and so is a nested `forall` (P008/P004/P007) — inside a `forall`/plain function the same nesting emits a real universal binder. Reassignment, `loop`/`break`, `**`, string literals, and aggregate call arguments and compound call results stay rejected. Memory-content assertions (addresses, points-to, iterated heaps) are out of scope. See [docs/specification-obligations.md](docs/specification-obligations.md).

## Documentation

Detailed design documents live in `docs/`:

- [docs/local-variables-lowering.md](docs/local-variables-lowering.md) - The two-pass
  approach for lowering `let`/`const` locals, supported initializer kinds, and the
  `lower_literal` type-dispatch logic for sub-i32 types.
- [docs/assignment-lowering.md](docs/assignment-lowering.md) - How assignment statements
  (`x = expr;`) are lowered to WASM local.set instructions, local index resolution, and
  current limitations on target forms.
- [docs/function-calls-lowering.md](docs/function-calls-lowering.md) - Three-stage index
  pre-scan (import reservation, top-level functions, methods), import section emission,
  extern call lowering, parameter index interlock with locals, the call lowering pipeline,
  drop emission rules, and known limitations.
- [docs/conditionals-lowering.md](docs/conditionals-lowering.md) - How `if`/`else`
  statements are lowered to WASM structured control flow and why `unreachable` is emitted
  before the `end` of every non-void function.
- [docs/arrays-and-memory.md](docs/arrays-and-memory.md) - Stack allocation and shadow
  stack infrastructure for fixed-size arrays and structs, including frame layout computation,
  prologue/epilogue emission, load/store instruction selection, when an array or struct
  parameter is copied on entry versus passed by reference,
  struct field layout (`compute_struct_field_layout`),
  `CompoundFieldLayout` for nested struct and array fields, member access lowering for scalar
  and compound fields, struct literal lowering with nested dispatch, arrays of structs, struct
  uzumaki with array fields, and the region fill/copy lowering (`RegionEmit`,
  `emit_memcpy_via_locals`) that keeps every whole-region memory operation within the
  WebAssembly 1.0 instruction set unless the build opts into bulk memory.
- [docs/loops-lowering.md](docs/loops-lowering.md) - How `loop`/`break` statements are
  lowered to WASM structured control flow (`block`/`loop`/`br`), `LoopContext` depth
  tracking, and interaction with non-det blocks, if-statements, and array frames.
- [docs/specification-obligations.md](docs/specification-obligations.md) - What a `spec`
  function becomes in proof mode: aggregates as ordered scalar leaves, the slot-allocation
  rule that fixes every `T_local` index in an emitted goal, one fully expanded obligation,
  the definedness rule for `a[i]` (and the both-bounds requirement a signed index carries),
  quantifier alternation, the caps, and the table of kept rejections.

## Module Organization

- `lib.rs` - Public API, multi-file AST traversal in canonical arena order, two-stage index pre-scan across all files (imports → top-level functions → methods), root-only export policy (`should_export`), file-qualified spec name emission (`qualified_spec_name` with `_` join for non-entry specs), `SpecNameCollision` backstop
- `compiler.rs` - WASM instruction emission, module assembly, and array frame layout computation
- `memory.rs` - Shadow stack infrastructure: `FrameLayout`, `ArraySlot`, `StructSlot`, `StructFieldSlot`, `CompoundFieldLayout`, `compute_struct_field_layout`, `type_byte_size`, `natural_alignment_for_type`, `emit_ptr_offset_addr`, prologue/epilogue emission, load/store instruction selection, `emit_struct_param_copy`, `RegionEmit` and the region fill/copy lowering (`emit_memcpy_via_locals`, `emit_memcpy_via_stack`) that avoids bulk-memory instructions unless the build permits them
- `errors.rs` - `CodegenError` enum for function call lowering failures, spec-name validation, and proof-mode `hassert` translation failures (`UntranslatableSpec`, `HspecTreeTooDeep`)
- `output.rs` - `CodegenOutput` containing WASM bytes, metadata, and (proof mode only) the per-spec `hassert` obligation map (`hspecs()`)
- `target.rs` - Compilation target definitions (`Wasm32`, `Soroban`) and the requestable post-MVP instruction families (`EmitFeatures`)
- `hassert/` - Proof-mode-only pass translating each `spec` free function into a `hassert` verification obligation — kind-tagged, so a `forall`/plain body yields a `ValidSpec` payload and an `exists`/`unique` body a reachability payload with its entry arity and source-visible slots — read-only over the typed AST (`mod.rs`: `translate_spec_fns` entry point and callee resolution index; `translate.rs`: the right-folded statement/term translator with its `Univ`/`UnivLvl`/`Exist`/`Reach` modes, the `AggValue` leaf tree that aggregate values translate to, and the pinned-witness machinery short-circuit operators and non-constant indices share; `reach.rs`: the reachability pre-scan whose `ChoicePlan` maps each scalar `@` to its appended choice parameter, shared by signature registration, body lowering, and payload translation; `diag.rs`: the `P001`–`P014` diagnostic registry). See [`docs/specification-obligations.md`](docs/specification-obligations.md) for the obligation shapes a specification author reads, and [`core/wasm-to-v/ROCQ_CONTRACT.md`](../wasm-to-v/ROCQ_CONTRACT.md) for the full translation scheme
- `hspecs_section.rs` - Encodes the obligation map into the `inference.hspecs` custom WASM section (via the shared `inference-hassert` codec) and the fail-closed pre-encode depth guard

## Testing

Tests are located in `tests/src/codegen/wasm/`:

```bash
# Run all codegen tests
cargo test -p inference-tests

# Run specific test
cargo test -p inference-tests trivial_test
```

Test data includes:
- `trivial.inf` - Simple function returning a constant
- `const.inf` - Constant definitions
- `nondet.inf` - Non-deterministic constructs (uzumaki, forall, exists, assume, unique)
- `local_variables.inf` - All `let` binding forms: every numeric type, bool, uzumaki, and
  identifier initializers (validated against `inf_wasmparser` and compared byte-for-byte)
- `local_variables_exec.inf` - Wasmtime execution tests that verify the correct WASM value
  is returned for each `let` binding form
- `fn_params.inf` - Functions with typed parameters (i32, i64, bool, multi-param); verifies
  parameter-to-local-index mapping and WASM type signatures
- `fn_calls.inf` - Function call scenarios including no-arg calls, arg passing, forward
  references, and `let`-from-call; validated and executed via wasmtime
- `if_else.inf` - `if`-only, `if`/`else`, nested `if`, locals inside arms, and void
  `if`; validated against `inf_wasmparser` and byte-compared, executed via wasmtime
- `if_nondet.inf` - `if` nested inside a `forall` non-deterministic block
- `if_bool_exprs.inf` - Comprehensive boolean condition coverage: direct bool params,
  comparison + logical operators, complex nested conditions, boolean locals, boolean
  equality/inequality, and if/else with complex conditions; validated and executed via
  wasmtime
- `assign.inf` - Assignment statement tests including simple assignments (i32, i64, bool),
  assignments from expressions, parameters, function calls, multiple assignments, and
  assignments inside `if` blocks; validated against `inf_wasmparser` and executed via
  wasmtime
- `assign_nondet.inf` - Assignment statements inside non-deterministic blocks (forall,
  exists, assume, unique)
- `algo_bitwise.inf` - 10 functions implementing bitwise algorithms (popcount, power-of-2
  checks, bit manipulation, rotation, byte swapping) demonstrating combined use of binary
  operations, prefix unary expressions, conditionals, and iterative loops
- `algo_i64_mixed.inf` - Demonstrates i64 operations in context of classic algorithms
  (factorial, fibonacci, GCD) with variable definitions and iterative loops
- `algo_converge.inf` - Convergence algorithms using i32/i64 arithmetic with iterative loops
- `array_literal.inf` - Fixed-size array literal declarations with i32 and bool element types,
  including single-element and multi-array cases; validated against `inf_wasmparser` and
  executed via wasmtime
- `array_zero_literal.inf` - Array literal initialization where all or some elements are
  syntactic zeros (`0`, `false`, `(0)`, `-0`, `-(0)`): verifies that zero-valued stores are
  elided during `let` initialization (the frame is already zeroed by the prologue's
  zero-fill), that mixed-value arrays emit stores only for non-zero elements, that sret
  returns via `return [0, 0, 0]` always store (no elision), and that `[true, false, true]`
  emits stores for the `true` elements but not `false`; validated and executed via wasmtime
- `array_index.inf` - Array index read access (both constant and variable indices) with i32
  and bool arrays, including reading array elements for use in conditions; validated and
  executed via wasmtime
- `array_assign.inf` - Array element write operations including simple writes, multiple
  assignments, element swapping, writes with computed indices, and bool array mutations;
  also includes `reassign_zeros` which verifies that zero stores are emitted during
  assignment (not elided, because the destination may hold non-zero values); validated and
  executed via wasmtime
- `array_params.inf` - Array-typed function parameters: copy-on-entry for the parameter that
  is written, value semantics verification (callee mutations don't affect caller's array),
  multi-parameter functions, and bool array parameters; the read-only parameters here are
  passed by reference, so `mutate_copy`'s is the only parameter with a frame slot; validated
  and executed via wasmtime
- `array_nondet.inf` - Arrays inside non-deterministic blocks (forall, exists) and
  non-deterministic array initialization (`@`) inside blocks; validated against `inf_wasmparser`
  (non-det modules skip WAT comparison)
- `struct_literal.inf` - Struct literal initialization: simple structs with i32 fields, single-field
  structs, and mixed-type structs (`bool`, `i64`); also includes structs initialized entirely with
  zero fields and structs where only some fields are zero — verifies that zero fields are elided
  during initialization and that non-zero fields are still stored; validated against
  `inf_wasmparser` and executed via wasmtime
- `struct_access.inf` - Struct field read access (`p.x`, `p.y`, `p.x + p.y`) for i32, bool, and
  i64 field types; validated and executed via wasmtime
- `struct_assign.inf` - Assignment to struct fields on mutable struct variables (`p.x = 42`,
  field swapping, bool field mutation); also includes `reassign_zeros` which assigns an all-zero
  struct literal to an already-initialized variable, verifying that zero stores are not elided
  during assignment (because the destination already holds non-zero data); validated and executed
  via wasmtime
- `struct_params.inf` - Struct-typed function parameters: copy-on-entry value semantics for
  the written parameter (callee mutations don't affect caller's struct), mixed-type struct
  params, multiple struct params; the read-only parameters are passed by reference, so
  `modify_no_effect`'s is the only parameter with a frame slot; validated and executed via wasmtime
- `struct_return.inf` - Functions returning struct types via sret convention: return of struct
  literal, return of a variable, chained calls (`return make_point()`), and mixed-type struct
  returns; validated and executed via wasmtime
- `struct_copy.inf` - Struct-to-struct copy (`let b = a;`) preserving value semantics:
  modifications to the copy do not affect the original; validated and executed via wasmtime
- `method_assoc.inf` - Associated function calls (`Type::func()`) including zero-arg constructors,
  multi-arg builders, cross-struct calls, and associated functions returning struct types; validated
  and executed via wasmtime
- `method_instance.inf` - Instance method calls (`obj.method()`) including `self` reads, multiple
  fields, chained independent calls, and methods with parameters; validated and executed via wasmtime
- `method_self_mutate.inf` - Methods with `mut self` receiver: verifies that mutations inside the
  method body do not affect the caller's copy (value semantics); executed via wasmtime
- `method_cross_call.inf` - Methods that call top-level functions and vice versa; validates that
  mangled method names and top-level function names coexist in `func_name_to_idx`
- `method_three_fields.inf` - Struct with three fields; exercises field offset computation inside
  method bodies; validated and executed via wasmtime
- `method_i64_fields.inf` - Methods on structs with `i64` fields; validates that 8-byte loads/stores
  work correctly inside method bodies; validated and executed via wasmtime
- `method_multi_struct.inf` - Two separate structs each with methods; validates that mangled names
  for different struct types do not collide; validated and executed via wasmtime
- `method_array_return.inf` - Associated functions and methods returning array types via the sret
  calling convention; validated and executed via wasmtime
- `method_return_struct.inf` - Methods returning struct types via the sret calling convention;
  covers associated constructors and instance methods returning structs; validated and executed
  via wasmtime
- `nested_struct.inf` - Structs containing other structs as fields: nested struct literal
  initialization, reading inner fields via copy (`let i: Inner = o.inner`), writing inner
  fields on the copy, passing nested structs as function parameters, returning nested structs
  via sret, and instance methods that access the inner struct; validated and executed via wasmtime
- `struct_with_array.inf` - Structs with fixed-size array fields: literal initialization
  with array field values, reading array elements from a struct field (`s.arr[0]`), writing
  to a struct array field element (`s.arr[1] = 99`), passing structs with array fields as
  parameters, returning them via sret, and instance methods that traverse the array field;
  validated and executed via wasmtime
- `array_of_structs.inf` - Fixed-size arrays where each element is a struct: initialization
  with per-element struct literals, reading a field from an indexed element (`pts[1].x`),
  writing a field on an indexed element (`pts[0].x = 99`), copying a whole element to a
  variable (`let p: Point = pts[1]`), writing a whole element (`pts[0] = replacement`),
  passing arrays of structs as parameters, and calling methods on extracted elements;
  validated and executed via wasmtime
- `nested_struct_with_array.inf` - A struct (`HasArray`) that contains an array field,
  itself nested inside a second struct (`Deep`): chained member + index access
  (`d.inner.arr[1]`), sum of array field elements, parameter passing, and sret return of the
  outer struct; validated and executed via wasmtime
- `multidim_array_uzumaki.inf` - Multidimensional arrays (`[[i32; 3]; 2]`, `[[i64; 2]; 2]`)
  initialized with uzumaki (`@`) inside `forall` blocks, with subsequent element reads;
  validated against `inf_wasmparser` (non-det modules skip WAT comparison)
- `struct_array_field_nondet.inf` - Structs with array fields initialized with uzumaki (`@`)
  inside `forall` blocks: structs with a single array field, structs with mixed i64 array
  and i32 fields, and structs with two separate array fields; validated against
  `inf_wasmparser`
- `enum_variant.inf` - Basic enum variant access (`Color::Red`, `Color::Green`,
  `Color::Blue`): assigns a variant to a local and returns it; verifies zero-based tag
  assignment and correct i32 return value; validated and executed via wasmtime
- `enum_multi.inf` - Two independent enum types in the same module (`Direction`, `Shape`):
  verifies that each enum's variant indices are independent and do not collide; validated
  and executed via wasmtime
- `enum_params.inf` - Enum-typed function parameters: pass-through, comparison inside
  `if`, and reassignment from a parameter; validated and executed via wasmtime
- `enum_compare.inf` - Equality and inequality comparisons on enum values (`==`, `!=`)
  and comparison against a literal variant; validated and executed via wasmtime
- `enum_assign.inf` - Reassignment of a `mut` enum local and assignment from a parameter;
  validated and executed via wasmtime
- `enum_array.inf` - Fixed-size array of enum values (`[Color; 3]`): initialization with
  variant literals, index read with equality check, and returning an enum element from an
  array; validated and executed via wasmtime
- `enum_in_struct.inf` - Enum-typed struct field: struct literal with an enum field,
  reading the field and comparing it to a variant; validated and executed via wasmtime
- Multi-file golden fixtures in `tests/test_data/codegen/wasm/multi_file_golden/`
  (tests in `tests/src/codegen/wasm/multi_file_golden.rs` and `multi_file.rs`):
  - `two_file` - Entry calls a function in one imported file; verifies file-qualified
    internal name and unqualified export
  - `re_export_chain` - Three-file chain (`main → math → lib/arith`); `math` re-exports
    `arith` via `pub use`; `main` reaches `math::arith::add`
  - `item_import` - Braced item import (`use lib::arith::{add};`); item used bare at call
    site without namespace prefix
  - `root_only_export` - Non-entry `pub fn` is NOT a WASM export; only entry `pub fn`s
    are exported; verified by WAT inspection
  - `method_mangling` - Cross-file method call; method name mangled as `lib.arith.Point.new`;
    entry-file method stays unqualified
  - `dup_struct` - Two files each defining a private `struct Buffer` with different field
    layouts; codegen resolves to the correct per-file layout at every access site
  - `cross_file_struct` - Struct defined in a non-entry file, constructed and passed across
    a file boundary; verifies canonical type key resolution in codegen
  - `cross_file_method` - Methods on a struct defined in a non-entry file; verifies that
    `FnKey::Method` qualifies by the struct's defining file
  - `single_via_project` - Single-file program compiled through `parse_project`; verifies
    byte-identical output to the direct `parse` path (golden-file regression)
  - `proof_specs` - Non-entry file `lib/checks.inf` carries a spec; proof-mode `.v` output
    contains `main__lib_checks_LibSpec` (underscore-joined, not dot-joined); entry spec
    stays bare (`main__EntrySpec`)
  - 6 execution smoke tests in `multi_file.rs` driven through `parse_project` directly,
    including cross-file call correctness verified via Wasmtime
- Extern import test fixtures in `tests/test_data/codegen/wasm/extern_import/`
  (tests in `tests/src/codegen/wasm/extern_import.rs`):
  - `single_import.inf` - One `external fn` bound to a module via `use … from`; verifies
    import occupies index 0 and the local function shifts to index 1; golden WAT validates
    import section content and call target
  - `multi_import.inf` - Two externs from the same module; both imports at indices 0 and 1;
    the local function shifts to index 2; verifies nested call order in the body
  - `import_with_locals.inf` - One import and two local functions; all locals shift past the
    import; verifies that cross-local calls use local indices and the extern call uses the
    import index
  - `import_dedup.inf` - Two externs with an identical `(i32) -> i32` signature share one
    type entry; verifies import-against-import type deduplication
- By-reference parameter fixtures in `tests/test_data/codegen/wasm/param_by_ref/`
  (tests in `tests/src/codegen/wasm/param_by_ref.rs`). Each fixture pins the *decision*
  with `check_count!` on `wasm_codegen_param_by_reference` /
  `wasm_codegen_param_written_in_body` / `wasm_codegen_param_escapes_to_extern`, not only
  the emitted bytes:
  - `read_only_params` - The headline shape: struct and array parameters that are read and
    never written, in functions needing no frame for anything else, so each compiles
    frameless
  - `written_param` - The written twin; one assignment restores the slot, the copy and the
    parameter rebind
  - `mut_never_written` - `mut` parameters that are never assigned; the half of the domain
    where the body scan decides something the `mut` marker cannot
  - `extern_forward` - The escape gate: parameters forwarded to an `external fn` keep their
    copies while a sibling that is not forwarded does not; covers the struct and array
    lowerings separately
  - `alias_same_argument` - The same caller variable passed to two parameters, one written
    and copied, the other by reference
  - `alias_whole_and_part` - One parameter's region strictly contains the other's
  - `alias_receiver` - The receiver channel, which never appears in argument position;
    immutable and `mut self` receivers aliasing a parameter
  - `alias_sret` - A compound return writes the caller's destination while a by-reference
    parameter is read, including a destination pointer forwarded down a call chain
- Receiver escape fixtures in `tests/test_data/codegen/wasm/self_extern_escape/`: a method
  that hands `self`, a projection of it, or a scalar drawn from it to an `external fn` —
  from a nested block, a nested expression, or a sub-object position — allocates the
  receiver a frame slot and copies on entry; `no_escape_self` is the control that stays
  frameless, `mut_self_extern` covers the mutable receiver, and `escape_with_param` pins
  the receiver slot's frame offset against a co-resident written-parameter slot
- Loop test fixtures in `tests/test_data/codegen/wasm/loops/`:
  - `simple_loop.inf` - Basic conditional loops (`loop COND { body }`) with counter patterns
  - `infinite_loop_break.inf` - Infinite loops (`loop { body }`) with `break` exit
  - `nested_loop.inf` - Nested conditional and infinite loops with inner break
  - `loop_with_if.inf` - Conditional loops with if/else bodies
  - `loop_accumulator.inf` - Accumulator patterns (sum, factorial, power)
  - `loop_break_early.inf` - Conditional loop with early break from if
  - `break_nested_if.inf` - Break inside nested if conditions
  - `void_loop.inf` - Void-returning function with loop
  - `loop_zero_iters.inf` - Loop with always-false condition (zero iterations)
  - `loop_with_array.inf` - Loops with array variables (frame layout + prologue/epilogue)
  - `loop_in_nondet.inf` - Loops inside forall/exists non-deterministic blocks
  - `nondet_then_break.inf` - Non-det block inside loop followed by break
  - `loop_return_array.inf` - Early return from loop with active array frame layout

## Related Resources

- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)

## License

See the [repository license](https://github.com/Inferara/inference#license) for details.
