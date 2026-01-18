# inference-wasm-to-v-translator

WebAssembly to Rocq (Coq) translator for formal verification in the Inference programming language compiler.

## Overview

This crate translates WebAssembly bytecode into Rocq (formerly Coq) formal verification language. It enables mathematical verification of compiled Inference programs by converting WASM binary format into equivalent Rocq definitions that can be formally proven correct.

The translator is a critical component of Inference's verification pipeline, bridging the gap between executable code (WASM) and formal proofs (Rocq).

## Key Features

- **Complete WASM module translation**: Functions, types, imports, exports, tables, memory, globals, data segments, and elements
- **Custom name section support**: Preserves function and local variable names from WASM debug information
- **Expression tree reconstruction**: Converts linear WASM instructions into structured Rocq expressions
- **Non-deterministic instruction support**: Handles Inference's extended WASM instructions (forall, exists, uzumaki, assume, unique)
- **Error recovery**: Collects multiple translation errors before failing
- **Zero-copy parsing**: Efficiently processes WASM bytecode using streaming parser

## Quick Start

### Basic Usage

```rust
use inference_wasm_to_v_translator::wasm_parser::translate_bytes;

// Read WASM bytecode
let wasm_bytes = std::fs::read("output.wasm")?;

// Translate to Rocq
let rocq_code = translate_bytes("my_module", &wasm_bytes)?;

// Write Rocq output
std::fs::write("output.v", rocq_code)?;
```

### Integration with Inference CLI

The translator is typically invoked through the main `inference` crate as part of the compilation pipeline:

```rust
use inference::wasm_to_v;

let rocq_output = wasm_to_v(&wasm_bytes, "module_name")?;
```

## Architecture

The translator uses a two-phase approach:

```
Phase 1: Parse        Phase 2: Translate
WASM bytes     →      WasmParseData     →      Rocq code
(streaming)           (structured)             (string)
```

### Phase 1: Parsing (wasm_parser.rs)

The parser streams through WASM bytecode sections and populates a `WasmParseData` structure:

- **Type Section**: Function signatures stored as `RecGroup` entries
- **Import Section**: External function, memory, table, and global imports
- **Function Section**: Maps function indices to their type indices
- **Table Section**: Table definitions with limits and element types
- **Memory Section**: Linear memory definitions with size limits
- **Global Section**: Global variable definitions with initialization expressions
- **Export Section**: Exported functions, memories, tables, and globals
- **Start Section**: Optional start function index
- **Element Section**: Table element initialization
- **Data Section**: Memory initialization data
- **Code Section**: Function bodies with local variables and instructions
- **Custom Section**: Name mappings for functions and local variables (debug info)

### Phase 2: Translation (translator.rs)

The translator converts `WasmParseData` into Rocq definitions:

1. **Module header**: Generates required Rocq imports and helper definitions
2. **Type translations**: Converts WASM types to Rocq type constructors
3. **Function translations**: Transforms function bodies into Rocq expression trees
4. **Module record**: Assembles all components into a Rocq `module` record

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

WASM uses a stack-based instruction model, while Rocq uses structured expressions. The translator reconstructs control flow from linear instructions:

### Block Structures

```wasm
block (result i32)
  i32.const 1
  i32.const 2
  i32.add
end
```

Translates to a nested Rocq expression with proper scope and result type handling.

### Conditional Branches

```wasm
local.get 0
if (result i32)
  i32.const 1
else
  i32.const 2
end
```

Translates to Rocq if-then-else with type-checked arms.

### Loops

WASM loops are translated to Rocq loop constructs with break and continue semantics preserved.

## Name Preservation

The translator preserves debug information from WASM's custom name section:

```rust
// WASM custom section "name"
Function names:
  0: "add"
  1: "multiply"
Local names:
  0: {0: "a", 1: "b"}
  1: {0: "x", 1: "y"}

// Results in Rocq definitions
Definition add : module_func := ...
Definition multiply : module_func := ...
```

This improves readability of generated Rocq code and aids in verification.

## Error Handling

The translator implements comprehensive error collection:

```rust
pub fn translate(&mut self) -> anyhow::Result<String> {
    let mut errors = Vec::new();

    // Collect errors from each translation phase
    for import in &self.imports {
        match translate_module_import(import) {
            Ok(translated) => { /* ... */ }
            Err(e) => errors.push(e),
        }
    }

    // Continue translating other sections...

    // Return first error if any occurred
    if let Some(error) = errors.into_iter().next() {
        return Err(error);
    }

    Ok(result)
}
```

This approach collects errors from all sections before failing, providing better diagnostics.

## Non-Deterministic Instructions

Inference extends WASM with non-deterministic instructions for formal verification:

| Instruction | Binary Encoding | Purpose |
|-------------|-----------------|---------|
| `forall.start` | `0xfc 0x3a` | Universal quantification start |
| `exists.start` | `0xfc 0x3b` | Existential quantification start |
| `uzumaki.i32` | `0xfc 0x3c` | Non-deterministic i32 value |
| `uzumaki.i64` | `0xfc 0x3d` | Non-deterministic i64 value |
| `assume` | `0xfc 0x3e` | Assume constraint |
| `unique` | `0xfc 0x3f` | Uniqueness constraint |

These instructions translate to corresponding Rocq constructs that can be formally reasoned about.

## Testing

The crate includes comprehensive test coverage using WASM files from the `test_data/` directory:

```bash
# Run all translator tests
cargo test -p inference-wasm-to-v-translator

# Test translation of specific WASM files
cargo test -p inference-wasm-to-v-translator test_parse_test_data
```

### Test Structure

```
core/wasm-to-v/
├─ src/
│  ├─ lib.rs              → Public API and integration tests
│  ├─ wasm_parser.rs      → WASM parsing logic
│  └─ translator.rs       → Rocq code generation
└─ test_data/
   ├─ example1.wasm       → Test WASM modules
   ├─ example2.wasm
   └─ ...
```

The integration test in `lib.rs` automatically discovers and translates all `.wasm` files in `test_data/`, reporting success rates and failure details.

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Parse WASM module | O(n) | Single pass through bytecode |
| Translate types | O(t) | t = number of type definitions |
| Translate functions | O(f × i) | f = functions, i = avg instructions per function |
| Name lookup | O(1) | HashMap-based name resolution |
| Overall | O(n) | Linear in WASM file size |

## Dependencies

- **anyhow**: Error handling with context
- **inf-wasmparser**: Fork of `wasmparser` with non-deterministic instruction support
- **uuid**: Unique identifier generation for Rocq definitions

## Limitations

### Current Limitations

1. **Component model**: WebAssembly component sections are parsed but not translated (empty stubs)
2. **Tag section**: Exception handling tags are not supported
3. **Reference types**: Limited support for complex reference types beyond funcref and externref
4. **SIMD**: Vector operations (v128) are partially supported
5. **Bulk memory**: Bulk memory operations require additional validation

### Known Issues

- Some complex control flow patterns may generate suboptimal Rocq code
- Error messages could provide more context about location in WASM module
- Memory initialization with large data segments may produce verbose output

## Future Work

Planned improvements:

1. **Optimization**: Generate more compact Rocq expressions
2. **Validation**: Add semantic validation beyond syntactic translation
3. **Component model**: Full WebAssembly component translation support
4. **Source maps**: Preserve mapping from Inference source → WASM → Rocq
5. **Incremental translation**: Support translating modified modules efficiently
6. **Proof scaffolding**: Generate proof templates for common verification tasks

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
    ↓ (LLVM codegen)
LLVM IR
    ↓ (LLVM → WASM)
WebAssembly bytecode
    ↓ (this crate)
Rocq formal verification code
```

The generated Rocq code can then be used with the Rocq proof assistant to formally verify properties of the compiled program.

## Examples

### Example 1: Simple Addition Function

WASM input:
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

Rocq output (simplified):
```coq
Definition add : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_get_local 0%N ::
    BI_get_local 1%N ::
    BI_binop (Binop_i BOI_add) ::
    nil;
|}.

Definition my_module : module := {|
  mod_types := ...;
  mod_funcs := add :: nil;
  mod_exports := Me "add" (MED_func 0%N) :: nil;
  (* ... other fields ... *)
|}.
```

### Example 2: Conditional Logic

WASM input:
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

The translator reconstructs the control flow and generates appropriate Rocq if-then-else constructs.

## Related Documentation

- [WASM Codegen Documentation](../wasm-codegen/README.md) - LLVM IR to WASM compilation
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Inference language reference
- [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
- [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

## Contributing

When modifying the translator:

1. Update parsing logic in `src/wasm_parser.rs` for new WASM sections
2. Update translation logic in `src/translator.rs` for new operators or constructs
3. Add test WASM files to `test_data/` for regression testing
4. Update this documentation to reflect changes
5. Ensure all tests pass: `cargo test -p inference-wasm-to-v-translator`

See the main project [CONTRIBUTING.md](../../CONTRIBUTING.md) for general guidelines.

## License

This crate is part of the Inference compiler project. See the repository root for license information.
