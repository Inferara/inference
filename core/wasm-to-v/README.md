# inference-wasm-to-v-translator

WebAssembly to Rocq (Coq) translator for the Inference programming language compiler.

## Overview

This crate translates WebAssembly bytecode into Rocq (formerly Coq) formal verification code, enabling mathematical verification of compiled Inference programs. It serves as the final phase in Inference's verification pipeline, bridging the gap between executable WebAssembly code and formal Rocq proofs.

The translator converts WASM binary format into equivalent Rocq definitions that preserve program semantics and can be formally verified using the Rocq proof assistant.

## Key Features

- **Complete WASM module translation**: Functions, types, imports, exports, tables, memory, globals, data segments, and elements
- **Custom name section support**: Preserves function and local variable names from WASM debug information
- **Expression tree reconstruction**: Converts linear WASM instructions into structured Rocq expressions
- **Non-deterministic instruction support**: Handles Inference's extended WASM instructions (forall, exists, uzumaki, assume, unique)
- **Partial output on error**: Section entries that fail to translate are skipped rather than aborting the entire module
- **Zero-copy parsing**: Efficiently processes WASM bytecode using streaming parser

## Quick Start

### Basic Usage

```rust
use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
use rustc_hash::FxHashMap;

let wasm_bytes = std::fs::read("output.wasm")?;
let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
let rocq_code = translate_bytes("my_module", &wasm_bytes, &empty)?;
std::fs::write("output.v", rocq_code)?;
```

Pass an empty map to source the per-spec indices from the WASM custom
section (`inference.spec_funcs`) that `wasm-codegen` embeds in `proof`
mode. Pass a populated `FxHashMap<String, Vec<u32>>` to override or
supplement the embedded section; the translator surfaces
`WasmToVError::EmbeddedSpecMismatch` if both are present and disagree.

### Integration with Inference Compiler

The translator is invoked as the final phase of the Inference compilation pipeline. The full multi-phase pipeline (source → AST → typed AST → WASM → Rocq) lives in the `inference` orchestrator crate; this snippet picks up after `inference::codegen` has produced a `CodegenOutput`:

```rust
use inference::{codegen, parse, type_check, wasm_to_v};

let source = std::fs::read_to_string("input.inf")?;
let arena = parse(&source)?;
let typed_context = type_check(arena)?;
let codegen_output = codegen(&typed_context)?;
let rocq_output = wasm_to_v(
    "module_name",
    codegen_output.wasm(),
    codegen_output.spec_func_indices_by_spec(),
)?;
```

See the [`inference`](../inference/README.md) crate for complete pipeline documentation.

## Architecture

The translator uses a two-phase approach for converting WASM bytecode to Rocq:

```
Phase 1: Parse              Phase 2: Translate
WASM bytes     →            WasmParseData     →            Rocq code
(binary format)             (structured data)              (text format)
  streaming                   in-memory                      generation
```

### Phase 1: Parsing (`wasm_parser.rs`)

The parser makes a single forward pass through WASM bytecode sections, populating a `WasmParseData` structure without loading the entire module into memory. Sections are processed in WASM specification order:

- **Type Section**: Function signatures stored as `RecGroup` entries
- **Import Section**: External function, memory, table, and global imports
- **Function Section**: Maps function indices to their type indices
- **Table Section**: Indirect call table definitions with limits and element types
- **Memory Section**: Linear memory definitions with size limits
- **Global Section**: Global variable definitions with initialization expressions
- **Export Section**: Exported functions, memories, tables, and globals
- **Start Section**: Optional module entry point function index
- **Element Section**: Table element initialization segments
- **Data Count Section**: Number of data segments (WebAssembly bulk memory proposal)
- **Data Section**: Memory initialization data segments
- **Code Section**: Function bodies with local variables and instructions
- **Custom Section**: Debug information including function and local variable names

Component model sections (Module, Instance, ComponentType, etc.) are recognized but generate empty stubs.

### Phase 2: Translation (`translator.rs`)

The translator converts structured `WasmParseData` into Rocq code strings. Section entries that cannot be translated are skipped and the accumulated output is returned regardless:

1. **Module header**: Generates required Rocq imports from standard libraries
2. **Helper definitions**: Creates convenience constructors (`Vi32`, `Vi64`, `Mt`, `Mm`, `Mg`, `Mi`, `Me`, `Ma`)
3. **Section translations**: Converts each WASM section to Rocq list definitions
4. **Function translations**: Transforms function bodies into Rocq expression sequences
5. **Module record**: Assembles all components into a final Rocq `module` record

The translator prioritizes correctness and readability over optimization, generating well-formatted Rocq code with preserved names from WASM debug information.

### Core Data Structures

```
WasmParseData<'a>
    ├─ mod_name: String                         → Module identifier
    ├─ function_types: Vec<RecGroup>            → Type signatures
    ├─ function_type_indexes: Vec<u32>          → Function → Type mapping
    ├─ function_bodies: Vec<FunctionBody<'a>>   → Code with locals
    ├─ imports: Vec<Import<'a>>                 → External dependencies
    ├─ exports: Vec<Export<'a>>                 → Public interface
    ├─ tables: Vec<Table<'a>>                   → Indirect call tables
    ├─ memory_types: Vec<MemoryType>            → Linear memory specs
    ├─ globals: Vec<Global<'a>>                 → Global variables
    ├─ data: Vec<Data<'a>>                      → Memory initialization
    ├─ elements: Vec<Element<'a>>               → Table initialization
    ├─ start_function: Option<u32>              → Entry point
    ├─ func_names_map: Option<HashMap<...>>     → Function names (debug)
    └─ func_locals_name_map: Option<HashMap...> → Local names (debug)
```

## Translation Mapping

### WASM Types → Rocq Types

| WASM Type | Rocq Type |
|-----------|-----------|
| `i32` | `T_num T_i32` |
| `i64` | `T_num T_i64` |
| `f32` | `T_num T_f32` |
| `f64` | `T_num T_f64` |
| `v128` | `T_vec T_v128` |
| `funcref` | `T_ref T_funcref` |
| `externref` | `T_ref T_externref` |

### WASM Instructions → Rocq Expressions

The translator converts WASM's linear instruction sequence into structured Rocq expressions:

```rust
// WASM instruction sequence
local.get 0
local.get 1
i32.add

// Becomes Rocq expression (simplified)
BI_const (Vi32 0) ::
BI_const (Vi32 1) ::
BI_binop (Binop_i BOI_add) ::
nil
```

### Module Structure

Every translated module produces a Rocq `module` record:

```coq
Definition my_module : module := {|
  mod_types := ...;      (* Function type signatures *)
  mod_funcs := ...;      (* Function definitions *)
  mod_tables := ...;     (* Indirect call tables *)
  mod_mems := ...;       (* Linear memory *)
  mod_globals := ...;    (* Global variables *)
  mod_elems := ...;      (* Table elements *)
  mod_datas := ...;      (* Memory data *)
  mod_start := ...;      (* Optional start function *)
  mod_imports := ...;    (* External imports *)
  mod_exports := ...;    (* Public exports *)
|}.
```

## Expression Translation

WASM uses a stack-based instruction model, while Rocq uses structured expressions. The translator reconstructs control flow from linear instruction sequences.

### Stack-Based to Structured

Linear WASM instructions are converted to Rocq expression lists:

```wasm
local.get 0
local.get 1
i32.add
```

Becomes:

```coq
BI_get_local 0%N ::
BI_get_local 1%N ::
BI_binop (Binop_i BOI_add) ::
nil
```

### Block Structures

WASM block instructions create lexical scopes with optional result types:

```wasm
block (result i32)
  i32.const 1
  i32.const 2
  i32.add
end
```

The translator generates nested Rocq block expressions with proper scope and result type handling.

### Conditional Branches

WASM if-then-else instructions translate to Rocq conditional constructs:

```wasm
local.get 0
if (result i32)
  i32.const 1
else
  i32.const 2
end
```

The translator creates Rocq if expressions with type-checked arms matching the declared result type.

### Loops

WASM loop instructions are translated to Rocq loop constructs. Branch instructions (`br`, `br_if`) that target loop labels maintain their break and continue semantics in the generated Rocq code.

## Name Preservation

The translator extracts and preserves debug information from WASM's custom name section:

**WASM Custom Section:**
```
name section:
  module name: "MyModule"
  function names:
    0: "add"
    1: "multiply"
  local names:
    0: {0: "a", 1: "b"}
    1: {0: "x", 1: "y"}
```

**Generated Rocq Code:**
```coq
Definition add : module_func := {|
  (* Parameters a and b are preserved *)
  modfunc_locals := nil;
  modfunc_body := ...
|}.

Definition multiply : module_func := {|
  (* Parameters x and y are preserved *)
  modfunc_locals := nil;
  modfunc_body := ...
|}.

Definition MyModule : module := ...
```

This dramatically improves readability of generated Rocq code and makes verification work more intuitive by preserving original source-level names.

## Error Handling

The parser phase (Phase 1) fails fast on malformed WASM bytecode. The translator phase (Phase 2) currently silently skips individual section entries that fail to translate, always returning whatever output has been accumulated. Errors from section translation (imports, exports, tables, globals, etc.) are collected into an internal `Vec` that is not yet acted upon.

### Parser Errors

The parser returns an `anyhow::Result` and propagates the first error encountered. Common causes:

- WASM magic bytes or version mismatch
- Invalid section data or truncated module
- Malformed instruction sequences

### Translator Errors

The translator (`WasmParseData::translate`) matches each section item result and pushes failures into an error accumulator, but the accumulator is never checked before returning `Ok`. In practice this means:

- Failed imports, exports, tables, globals, data, or element entries are silently omitted from the generated Rocq output
- A failed function body translation causes that function to be omitted from the `mod_funcs` list
- The returned `String` is always syntactically valid Rocq, but may be semantically incomplete

### Error Categories

Errors that may be silently dropped during translation include:

- **Unsupported WASM features**: Unknown reference types
- **Unimplemented instructions**: Opcodes not yet handled in `translate_basic_operator`
- **Type mismatches**: Inconsistent type information between sections

The tag section (exception handling) and component model sections are silently ignored by the parser itself rather than producing errors.

## Non-Deterministic Instructions

Inference extends WebAssembly with custom instructions for non-deterministic computation and formal verification. These extensions enable explicit representation of non-deterministic choices in the binary format.

| Instruction | Binary Encoding | Purpose | Rocq Translation |
|-------------|-----------------|---------|------------------|
| `forall` | `0xfc 0x3a` | Begin universal quantification block | Forall block construct |
| `exists` | `0xfc 0x3b` | Begin existential quantification block | Exists block construct |
| `assume` | `0xfc 0x3c` | Filter execution paths by constraint | Assume statement |
| `unique` | `0xfc 0x3d` | Assert exactly one execution path exists | Unique constraint |
| `i32.uzumaki` | `0xfc 0x31` | Generate non-deterministic i32 value | Uzumaki constructor |
| `i64.uzumaki` | `0xfc 0x32` | Generate non-deterministic i64 value | Uzumaki constructor |

These instructions are parsed by the forked `inf-wasmparser` dependency and translated to corresponding Rocq constructs that enable formal reasoning about non-deterministic programs.

### Example

**Inference Source:**
```inference
forall {
    let x = @i32;  // uzumaki - all possible i32 values
    assume(x > 0);
    return x;
}
```

**Generated WASM (pseudo):**
```wasm
forall.start
  uzumaki.i32
  local.set 0
  local.get 0
  i32.const 0
  i32.gt_s
  assume
  local.get 0
forall.end
```

**Translated Rocq:**
```coq
(* Rocq representation with forall block and uzumaki *)
```

## Testing

The crate includes comprehensive test coverage using WASM test modules in `test_data/`.

### Running Tests

```bash
# Run all translator tests
cargo test -p inference-wasm-to-v-translator

# Run integration test that processes all test data
cargo test -p inference-wasm-to-v-translator test_parse_test_data

# Run with verbose output to see per-file results
cargo test -p inference-wasm-to-v-translator -- --nocapture
```

### Test Structure

```
core/wasm-to-v/
├─ src/
│  ├─ lib.rs              → Public API and integration tests
│  ├─ wasm_parser.rs      → WASM parsing logic
│  └─ translator.rs       → Rocq code generation
└─ test_data/
   ├─ comments.*.wasm         → Comment handling tests
   ├─ custom.*.wasm           → Custom section tests
   ├─ fac.*.wasm              → Factorial function test
   ├─ forward.*.wasm          → Forward reference tests
   ├─ func_ptrs.*.wasm        → Function pointer tests
   ├─ inline-module.*.wasm    → Inline module tests
   ├─ memory_*.wasm           → Memory section tests
   ├─ ref_*.wasm              → Reference type tests
   ├─ start.*.wasm            → Start section tests
   ├─ table.*.wasm            → Table section tests
   ├─ table-sub.*.wasm        → Table subtyping tests
   ├─ table_get.*.wasm        → table.get instruction tests
   ├─ table_set.*.wasm        → table.set instruction tests
   ├─ table_size.*.wasm       → table.size instruction tests
   ├─ token.*.wasm            → Token parsing tests
   ├─ type.*.wasm             → Type section tests
   └─ unreached-valid.*.wasm  → Unreachable code validity tests
```

### Test Behavior

The `test_parse_test_data` test in `lib.rs` (a `#[cfg(test)]` module test):

1. Discovers all `.wasm` files in `test_data/`
2. Attempts to translate each file to Rocq
3. Catches panics from unimplemented features (using `panic::catch_unwind`)
4. Reports success/failure statistics with categories:
   - **Successful**: Translation completed without errors
   - **Failed (errors)**: Translation returned an `Err` result
   - **Failed (panics)**: Translation panicked (usually unimplemented features)

This test serves as both a regression test suite and a feature coverage indicator.

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Parse WASM module | O(n) | Single pass through bytecode |
| Translate types | O(t) | t = number of type definitions |
| Translate functions | O(f × i) | f = functions, i = avg instructions per function |
| Name lookup | O(1) | HashMap-based name resolution |
| Overall | O(n) | Linear in WASM file size |

## Dependencies

This crate has minimal dependencies to keep the build fast and secure:

- **anyhow** (`workspace`): Error handling with context propagation
- **inf-wasmparser** (`workspace`): Fork of `wasmparser` with Inference non-deterministic instruction support
- **inference-wasm-codegen** (`workspace`): Source of the `SPEC_FUNCS_SECTION_NAME` and `SPEC_FUNCS_SECTION_VERSION` wire-format constants; this crate consumes them at the decode boundary so encoder and decoder share one source of truth
- **rustc-hash** (`workspace`): `FxHashMap` for the `spec_funcs_by_spec` API type
- **thiserror** (`workspace`): Derive macro for the `WasmToVError` enum in `errors.rs`

The `inf-wasmparser` fork is critical for parsing Inference's custom WASM instruction extensions. See [`tools/inf-wasmparser/`](../../tools/inf-wasmparser/README.md) for details.

## Limitations and Known Issues

### Current Limitations

1. **Component Model**: WebAssembly component model sections are recognized but generate empty stubs
   - `ModuleSection`, `InstanceSection`, `ComponentSection`, etc. are parsed but not translated
   - See [WebAssembly Component Model proposal](https://github.com/WebAssembly/component-model)

2. **Exception Handling**: Tag section (exception handling) is not supported
   - The tag section is silently ignored during parsing; exception handling instructions in function bodies may cause panics or incorrect output
   - See [WebAssembly Exception Handling proposal](https://github.com/WebAssembly/exception-handling)

3. **Reference Types**: Limited support for complex reference types
   - `funcref` and `externref` are supported
   - Typed function references and GC reference types are not yet implemented
   - See [WebAssembly Reference Types proposal](https://github.com/WebAssembly/reference-types)

4. **SIMD Operations**: Vector operations (v128) are partially supported
   - Some SIMD instructions may not translate correctly
   - See [WebAssembly SIMD proposal](https://github.com/WebAssembly/simd)

5. **Bulk Memory**: Bulk memory operations require additional validation
   - `memory.copy`, `memory.fill`, `table.copy`, `table.init` need testing
   - See [WebAssembly Bulk Memory proposal](https://github.com/WebAssembly/bulk-memory-operations)

### Known Issues

- **Silent translation errors**: Errors during section translation (invalid imports, unsupported reference types, unimplemented opcodes) are collected into an internal accumulator that is never checked. The translator always returns `Ok`, so callers cannot detect incomplete output
- **Control flow complexity**: Some complex control flow patterns (deeply nested blocks, unusual branch targets) may generate suboptimal or incorrect Rocq code
- **Large data segments**: Memory initialization with large data segments produces verbose output that may be difficult to work with in Rocq
- **Name conflicts**: Generated Rocq identifiers may conflict with reserved keywords in edge cases

## Future Work

Planned improvements for future releases:

1. **Error propagation**: Check the translator's error accumulator before returning and surface failures to callers instead of silently producing incomplete output
2. **Optimization**: Generate more compact Rocq expressions by recognizing common patterns and idioms
3. **Validation**: Add semantic validation beyond syntactic translation to catch invalid WASM constructs earlier
4. **Component Model**: Full WebAssembly component model translation support for modern WASM applications
5. **Source Maps**: Preserve mapping from Inference source → WASM → Rocq for better error reporting and debugging
6. **Incremental Translation**: Support translating modified modules efficiently for faster development iteration
7. **Proof Scaffolding**: Generate proof templates and lemmas for common verification tasks
8. **Better Diagnostics**: Include WASM byte offsets and section names in error messages
9. **Name Sanitization**: Automatically handle Rocq keyword conflicts in generated identifiers
10. **Optimized Data Segments**: Represent large data segments more compactly in generated Rocq code
11. **SIMD Support**: Complete translation of all WebAssembly SIMD instructions

## Integration with Inference Compiler

The translator is invoked as the final phase of the Inference compilation pipeline:

```
Inference source code
    ↓ (parsing)
Tree-sitter AST
    ↓ (semantic analysis)
Typed AST
    ↓ (type checking)
Type-checked AST
    ↓ (wasm-encoder codegen)
WebAssembly bytecode
    ↓ (this crate)
Rocq formal verification code
```

The generated Rocq code can then be used with the Rocq proof assistant to formally verify properties of the compiled program.

## Examples

### Example 1: Simple Addition Function

**Inference Source:**
```inference
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

**WASM (WAT format for clarity):**
```wasm
(module
  (func $add (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add
  )
  (export "add" (func $add))
)
```

**Generated Rocq Code (simplified):**
```coq
Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes.
From Wasm Require Import numerics.
From Wasm Require Import datatypes.

(* Helper definitions *)
Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).
Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).
(* ... more helpers ... *)

(* Function definition *)
Definition add : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_get_local 0%N ::
    BI_get_local 1%N ::
    BI_binop (Binop_i BOI_add) ::
    nil;
|}.

(* Module record *)
Definition my_module : module := {|
  mod_types :=
    {| recType_types := [Tf (T_num T_i32 :: T_num T_i32 :: nil)
                             (T_num T_i32 :: nil)] |} :: nil;
  mod_funcs := add :: nil;
  mod_tables := nil;
  mod_mems := nil;
  mod_globals := nil;
  mod_elems := nil;
  mod_datas := nil;
  mod_start := None;
  mod_imports := nil;
  mod_exports := Me "add" (MED_func 0%N) :: nil;
|}.
```

### Example 2: Conditional Logic

**Inference Source:**
```inference
fn max(x: i32, y: i32) -> i32 {
    if x > y {
        return x;
    } else {
        return y;
    }
}
```

**WASM (WAT format):**
```wasm
(func $max (param i32 i32) (result i32)
  local.get 0
  local.get 1
  i32.gt_s
  if (result i32)
    local.get 0
  else
    local.get 1
  end
)
```

**Generated Rocq Code:**

The translator reconstructs the control flow and generates Rocq if-then-else constructs:

```coq
Definition max : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_get_local 0%N ::
    BI_get_local 1%N ::
    BI_relop (Relop_i (ROI_gt SX_S)) ::
    BI_if (Tf nil (T_num T_i32 :: nil))
      (BI_get_local 0%N :: nil)
      (BI_get_local 1%N :: nil) ::
    nil;
|}.
```

The if-then-else structure is preserved with proper type annotations for the result type.

## Related Documentation

- [Rocq Output Contract](./ROCQ_CONTRACT.md) - The external Rocq predicates the generated `.v` files depend on, and the proof-skeleton shape the translator emits
- [WASM Codegen Documentation](../wasm-codegen/README.md) - WebAssembly code generation
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Inference language reference
- [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
- [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

## Contributing

See the main project [CONTRIBUTING.md](../../CONTRIBUTING.md) guide.

## License

This crate is part of the Inference compiler project. See the repository root for license information.
