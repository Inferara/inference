# inference-wasm-to-v-translator

WebAssembly to Rocq (Coq) translator for the Inference programming language compiler.

## Overview

This crate translates WebAssembly bytecode into Rocq (formerly Coq) formal verification code, enabling mathematical verification of compiled Inference programs. It serves as the final phase in Inference's verification pipeline, bridging the gap between executable WebAssembly code and formal Rocq proofs.

The translator converts WASM binary format into equivalent Rocq definitions that preserve program semantics and can be formally verified using the Rocq proof assistant.

## Key Features

- **Complete WASM module translation**: Functions, types, imports, exports, tables, memory, globals, data segments, and elements
- **Custom name section support**: Preserves function and local variable names from WASM debug information
- **Expression tree reconstruction**: Converts linear WASM instructions into structured Rocq expressions
- **Specification-to-obligation translation**: A `forall`/plain `spec` function is omitted from the module record entirely and its logical content becomes a `hassert` verification obligation; an `exists`/`unique` spec function is retained with a vanilla body and its obligation is a `reachability_spec` record consumed by the `ValidExistsSpec`/`ValidUniqueSpec` predicates (see [Non-Deterministic Instructions](#non-deterministic-instructions) and [`ROCQ_CONTRACT.md`](./ROCQ_CONTRACT.md))
- **Fail-closed translation**: a section entry that cannot be translated fails the whole module rather than being dropped from the output — a `.v` is a proof artifact, so a partial one must never be returned as success (see [Rejection Policy](#rejection-policy))
- **Zero-copy parsing**: Efficiently processes WASM bytecode using streaming parser

## Quick Start

### Basic Usage

```rust
use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
use rustc_hash::FxHashMap;

let wasm_bytes = std::fs::read("output.wasm")?;
let empty_specs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
let empty_hspecs = inference_hassert::HSpecMap::default();
let rocq_code = translate_bytes("my_module", &wasm_bytes, &empty_specs, &empty_hspecs)?;
std::fs::write("output.v", rocq_code)?;
```

Pass empty maps to source both the per-spec indices and the `hassert`
obligations from the WASM custom sections (`inference.spec_funcs` and
`inference.hspecs`) that `wasm-codegen` embeds in `proof` mode. Pass a
populated map to override or supplement the embedded section; the
translator surfaces `WasmToVError::EmbeddedSpecMismatch` /
`WasmToVError::EmbeddedHspecsMismatch` if the explicit and embedded sides
disagree.

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
    codegen_output.hspecs(),
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

The translator converts structured `WasmParseData` into Rocq code strings. Every section is translated before any error is reported, so the failure a caller sees is the first in the translator's section traversal order (imports, exports, tables, memories, globals, data, elements, then function bodies) — not the module's binary section order; but if any section failed, the assembled module is discarded and that error is returned:

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
| `f32` | rejected — `UnsupportedFeature` |
| `f64` | rejected — `UnsupportedFeature` |
| `v128` | rejected — `UnsupportedFeature` |
| `funcref` | `T_ref T_funcref` |
| `externref` | `T_ref T_externref` |

The proof model's `number_type` has exactly two constructors, `T_i32` and `T_i64`, and it declares no vector type at all, so `f32`, `f64`, and `v128` have nothing to map to. `translate_value_type` is the single chokepoint for every position a type can occupy — function parameters and results, locals, globals, and block result types — so a float in a *signature* is refused even when no float instruction appears in any body. The message names the position ("… in a function parameter") because a `.wasm` carries no source locations.

### WASM Instructions → Rocq Expressions

The translator converts WASM's linear instruction sequence into structured Rocq expressions:

```rust
// WASM instruction sequence
local.get 0
local.get 1
i32.add

// Becomes Rocq expression (simplified)
BI_local_get 0%N ::
BI_local_get 1%N ::
BI_binop T_i32 (Binop_i BOI_add) ::
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
BI_local_get 0%N ::
BI_local_get 1%N ::
BI_binop T_i32 (Binop_i BOI_add) ::
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

Both phases fail closed. The parser phase (Phase 1) fails fast on malformed WASM bytecode. The translator phase (Phase 2) collects errors from every section into an accumulator so that one failure does not mask later ones, but it checks that accumulator before returning: if any section failed, the assembled module is discarded and the first error is returned. A `.v` is a proof artifact, so a partial translation is never returned as success.

### Parser Errors

The parser returns an `anyhow::Result` and propagates the first error encountered. Common causes:

- WASM magic bytes or version mismatch
- Invalid section data or truncated module
- Malformed instruction sequences

### Translator Errors

The translator (`WasmParseData::translate`) matches each section item result and pushes failures into an error accumulator, then checks it before emitting the obligation definitions. In practice this means:

- A failed import, export, table, global, data, or element entry fails the whole translation; it is never silently omitted from the generated Rocq output
- A failed function body likewise fails the translation rather than dropping the function from `mod_funcs` and shifting every later index
- An `Ok(String)` is therefore a complete translation of the whole module, not a best effort

Accumulating before failing is deliberate: translating every section first means the error a caller sees is the first *in module order*, not the first the walk happened to reach.

### Error Categories

Recoverable `WasmToVError`s the translator returns:

- **`UnsupportedFeature`**: a construct outside the subset the wasm-verifier proof contract covers — any floating-point, SIMD/vector, or conversion instruction; an `f32`/`f64`/`v128` value type in any position; a non-deterministic instruction in any body the emitted module retains; `memory64`, shared, or custom-page-size memories; atomics; and the proposal families (GC, exception handling, stack switching, tail calls, wide arithmetic, typed references) the contract does not cover. See [Rejection Policy](#rejection-policy).
- **`WasmParse`**: malformed bytes, surfaced by the parser phase
- **Identifier errors**: a module or function name that cannot be rendered as a legal Rocq identifier

The tag section (exception handling) and component model sections are silently ignored by the parser itself rather than producing errors.

### Rejection Policy

The translator emits only what the vendored WasmCert proof stub in `rocq-stub/` declares. Anything else is refused with `UnsupportedFeature` naming the construct — never a `.v` that fails `coqc` downstream, and never a panic. Concretely, the following are rejected rather than translated:

| Construct | Reason the message gives |
|-----------|--------------------------|
| Any `f32`/`f64` instruction | the wasm-verifier proof contract covers no floating-point surface |
| Any SIMD/vector instruction | SIMD proposal — the wasm-verifier proof contract covers no vector types |
| Any conversion instruction naming a float on either side (`trunc`, `trunc_sat`, `convert`, `demote`, `promote`, `reinterpret`) | the contract declares no floating-point number type, so a conversion naming one has no lowering |
| `f32`, `f64`, or `v128` as a value type, in any position | as above, per type, plus the position it occupies |
| GC, exception handling, stack switching, tail calls, wide arithmetic, typed references, `memory.discard`, segment-indexed table ops — at the **instruction** surface | no lowering under the wasm-verifier proof contract |
| GC struct/array and `cont` types, declared subtyping (`sub`, non-final), and shared composites — at the **type-section** surface | worded apart from the instruction arms above, so a fixture carrying both keeps testing both |
| `table64`, shared tables, and tables with an element initializer | the emitted `Mt` carries limits and an element type only |
| A memory index other than `0`, on a load/store `memarg` or on `memory.init`/`memory.copy`/`memory.fill` | the model has one linear memory, so the index has nowhere to go |
| The tag section, any unrecognised section id, and component-model sections | content the emitted `.v` could not account for |

A proposal family appearing in this table twice is the point, not duplication. GC and stack switching each reach the module through *two* surfaces — an instruction and a type-section entry — and rejecting only the instruction left the type surface emitting a dangling `::` into `mod_types`. Any future family added here needs the same question asked of it: which surfaces can carry it?

Structural contradictions inside the binary are rejected as `WasmParse` rather than `UnsupportedFeature`, because the input is malformed rather than merely unmodelled: a repeated or out-of-order core section, a data count disagreeing with the data section, function and code sections of different lengths, operators after a body's terminating `end`, a truncated locals vector or operator stream, and a declared locals count above the limit WebAssembly engines share.

The **integer-to-integer** width conversions are not on this list. `i32.wrap_i64` and `i64.extend_i32_s/u` translate to `BI_cvtop` with the contract's `CVO_wrap`/`CVO_extend`, and the five sign-extension operators (`i32.extend8_s`, `i32.extend16_s`, `i64.extend8_s`, `i64.extend16_s`, `i64.extend32_s`) translate to `BI_unop t (Unop_extend n)` — the contract classifies sign-extension as a *unop*, not a conversion, so the WASM mnemonics group them misleadingly. `Unop_extend`'s argument is the source width in **bits**; a byte count would type-check and denote a constant-zero extension, so the emitter's spelling is pinned by byte comparison rather than left to `coqc`.

No Inference program can reach any of the rejected constructs: the language has no floating-point types, no vectors, and emits no conversion instruction (it narrows sub-`i32` values with shifts and masks), so `coqc` gating over Inference sources can never cover them. They are reachable only through foreign bytes — the external linking path (`infc -L` / `--wasm-dep`) and the public `translate_bytes` API — which is exactly why the refusal has to be explicit. This is the second layer of a two-layer defense: `core/wasm-linker` already refuses float, SIMD, float-naming conversion, and tail-call content in external modules, so on the CLI path the linker's mnemonic-bearing diagnostic normally fires first.

Two consequences worth stating plainly. Rejecting on a *value type* means a module carrying an unused float signature stops translating even with no float instruction anywhere — correct, because the type-section entry is emitted wholesale and would be ill-typed regardless. And translation stops at the first offending construct, so a module with many unsupported constructs reports them one at a time.

The one construct rejected for a translator-side reason rather than a model-side one is `select t`: the stub does declare a typed `BI_select`, but no lowering is wired for it, and the message says so.

## Non-Deterministic Instructions

Inference extends WebAssembly with custom instructions for non-deterministic computation, in the `0xfc` prefix space:

| Instruction | Binary Encoding | Purpose |
|-------------|-----------------|---------|
| `forall` | `0xfc 0x3a` | Begin universal quantification block |
| `exists` | `0xfc 0x3b` | Begin existential quantification block |
| `assume` | `0xfc 0x3c` | Filter execution paths by constraint |
| `unique` | `0xfc 0x3d` | Assert exactly one execution path exists |
| `i32.uzumaki` | `0xfc 0x31` | Generate non-deterministic i32 value |
| `i64.uzumaki` | `0xfc 0x32` | Generate non-deterministic i64 value |

This crate's parser recognizes all six via the forked `inf-wasmparser`
dependency, but **none of them are translated into Rocq instructions any
more**. The consumer this crate targets, wasm-verifier (a private
Inferara repository; [`ROCQ_CONTRACT.md`](./ROCQ_CONTRACT.md) is the
in-repo statement of its interface, and the vendored signature stub in
`rocq-stub/` declares the subset of that interface this crate can emit),
sits on vanilla WasmCert-Coq, which has no
`BI_forall`/`BI_exists`/`BI_assume`/`BI_unique`/`BI_uzumaki_num`
counterparts to translate into.

Instead, translation is a kind-dependent split, all parts enforced
fail-closed:

1. **A `forall`-quantified (or plain) `spec` function's body is never
   emitted as instructions at all.** Its WASM function index is omitted
   from the module record — no `Definition`, no `mod_funcs` entry — and
   every surviving reference (calls, exports, elements, the start
   function) is renumbered past the gap. Its logical content is instead
   carried, out of band, as one `hassert` value per function, built
   AST-side during codegen (`core/wasm-codegen/src/hassert/`) and
   serialized into the `inference.hspecs` custom section. `wasm-to-v`
   reads that section, resolves each obligation's applied function
   symbols against the final (post-link) module layout, and prints one
   `Definition <mod>__<Spec>_hspec{k} : hassert := …` per obligation plus
   a `Theorem valid_<mod>__<Spec> : ValidSpec <mod> <mod>__<Spec>_specs`.
2. **An `exists`/`unique`-quantified `spec` function is retained in the
   module record** with a vanilla body — codegen appends one hidden
   trailing *choice parameter* per scalar `@` and compiles
   `assume`/`assert` to trap-on-false filters, so the body carries no
   non-deterministic opcode. Its obligation arrives through the same
   `inference.hspecs` section, kind-tagged, and is printed as a
   `Definition <mod>__<Spec>_exspec{k} : reachability_spec` (or
   `_uqspec{k}`) record plus a
   `Theorem valid_exists_<mod>__<Spec> : ValidExistsSpec …` (or
   `valid_unique_…`/`ValidUniqueSpec`) — the downstream judgment reduces
   the retained body, so the function must stay in `mod_funcs`, while
   the reference sites (calls, exports, elements, start) reject it: a
   retained spec function is the subject of its obligation, not a
   callable. See [`ROCQ_CONTRACT.md`](./ROCQ_CONTRACT.md) for the full
   translation scheme and complete worked examples of both shapes.
3. **A non-deterministic instruction reaching any body the emitted
   module retains is a translate error**
   (`WasmToVError::UnsupportedFeature`). Inference's own analysis rule
   A042 rejects non-det syntax anywhere outside a `spec` declaration at
   compile time, and the reachability lowering is vanilla WASM by
   construction, so this path is unreachable from Inference-compiled
   code; the rejection is defense-in-depth against a foreign or
   hand-crafted `.wasm`.

### Narrow-Typed Domain Constraints

A scalar uzumaki draw always produces a full-width `i32`/`i64` value. When the
declared type is narrower — `i8`/`u8`/`i16`/`u16`/`bool`/an enum — codegen
emits a short domain-mapping sequence between the draw and the `local.set` in
the *WASM* instruction stream (mask for `u8`/`u16`, `shl`+`shr_s` for
`i8`/`i16`, `and 1` for `bool`, `rem_u <variant count>` for a non-empty enum;
see the `wasm-codegen` README). In a `forall`/plain spec function that
sequence never reaches the emitted `.v` — the body is omitted from the module
record. In a retained `exists`/`unique` body the draw is instead a read of the
`@`'s choice parameter, and the same domain-mapping sequence *does* appear in
the emitted `.v`, keeping the choice in its declared domain during the
reachability reduction. The corresponding `hassert` obligation currently binds
the drawn variable as an unconstrained universal slot, existential binder, or
(reachability) frame slot; carrying the declared type's domain into the
*universal* obligation (`HA_has_type`-style range antecedents) is a tracked
follow-up of the wasm-verifier contract work.

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

The `test_parse_test_data` test in `lib.rs` (a `#[cfg(test)]` module test) discovers all `.wasm` files in `test_data/`, translates each, and reports statistics under `panic::catch_unwind`.

> [!WARNING]
> **This test is currently inert and measures nothing.** Every fixture's module name is its file stem (`token.2`, `unreached-valid.0`), and the illegal `.`/`-` characters fail `validate_rocq_identifier` before a single operator is reached. The run reports 0 successful / 125 errors / 0 panics and *passes*, so it reads as coverage while providing none. Reviving it — deriving a legal Rocq module name from the stem — is tracked in [Future Work](#future-work). Do not remove the `catch_unwind` harness before the corpus is revived; stripping the guard from an inert test and then reviving the corpus unguarded is the wrong order.

The "failed (panics)" category is a historical artifact. It counted `todo!()` arms in `translate_basic_operator`, of which there are now none: every operator either translates or returns a recoverable error. Panic-freedom for the operator surface is covered by the WAT-driven rejection matrix in `lib.rs` (`mod unsupported_surface`) instead.

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

2. **Exception Handling**: not supported, rejected
   - The tag section is silently ignored during parsing; every exception-handling instruction — modern (`throw`, `throw_ref`, `try_table`) and legacy (`try`/`catch`/`rethrow`) — is refused with `UnsupportedFeature`
   - See [WebAssembly Exception Handling proposal](https://github.com/WebAssembly/exception-handling)

3. **Reference Types**: `funcref` and `externref` only
   - Typed function references (`ref.as_non_null`, `br_on_null`, `call_ref`) and the GC reference types are rejected, not translated
   - See [WebAssembly Reference Types proposal](https://github.com/WebAssembly/reference-types)

4. **Floating point and SIMD**: not supported, rejected
   - `f32`/`f64`/`v128` value types and every float, vector, and float-naming conversion instruction are refused with `UnsupportedFeature`; see [Rejection Policy](#rejection-policy) for why
   - The integer-to-integer width conversions and the five sign-extension operators *are* supported, because the contract declares `CVO_wrap`/`CVO_extend` and `Unop_extend`
   - Supporting the rest means growing the wasm-verifier proof contract first; the translator's grouped rejection arms are one arm per class, so the eventual change is localized
   - See [WebAssembly SIMD proposal](https://github.com/WebAssembly/simd)

5. **Bulk Memory**: partially supported
   - `memory.init`, `data.drop`, `memory.copy`, and `memory.fill` translate; the segment-indexed table operations (`table.init`, `elem.drop`, `table.copy`) are rejected — they have no lowering under the wasm-verifier proof contract
   - See [WebAssembly Bulk Memory proposal](https://github.com/WebAssembly/bulk-memory-operations)

### Known Issues

- **One error at a time**: only one error surfaces per run — a function body stops at its first offending construct, and the module-level walk reports only the first error it accumulated — so a module with several unsupported constructs has to be fixed (or refused) one at a time
- **Debug names, not mnemonics**: the float, vector, and rejected-conversion messages name the operator in its `wasmparser` debug form (`F32Add`), not its wat mnemonic (`f32.add`). Unambiguous, but not the spelling a reader of the `.wat` sees. Value types are the exception: they are spelled `f32`/`f64`/`v128`
- **Control flow complexity**: Some complex control flow patterns (deeply nested blocks, unusual branch targets) may generate suboptimal or incorrect Rocq code
- **Large data segments**: Memory initialization with large data segments produces verbose output that may be difficult to work with in Rocq
- **Name conflicts**: Generated Rocq identifiers may conflict with reserved keywords in edge cases

## Future Work

Planned improvements for future releases:

1. **Revive the `test_data` corpus**: derive a legal Rocq module name from dotted file stems so the 125 upstream fixtures exercise the translator again (today every one of them fails name validation first, so the suite measures nothing)
2. **Optimization**: Generate more compact Rocq expressions by recognizing common patterns and idioms
3. **Validation**: Add semantic validation beyond syntactic translation to catch invalid WASM constructs earlier
4. **Component Model**: Full WebAssembly component model translation support for modern WASM applications
5. **Source Maps**: Preserve mapping from Inference source → WASM → Rocq for better error reporting and debugging
6. **Incremental Translation**: Support translating modified modules efficiently for faster development iteration
7. **Proof Scaffolding**: Generate proof templates and lemmas for common verification tasks
8. **Better Diagnostics**: Include WASM byte offsets and section names in error messages
9. **Name Sanitization**: Automatically handle Rocq keyword conflicts in generated identifiers
10. **Optimized Data Segments**: Represent large data segments more compactly in generated Rocq code
11. **Float and SIMD support**: requires the wasm-verifier proof model to grow those surfaces first; until then the translator refuses them — and the float-naming conversions that depend on them — rather than emitting terms the model cannot type

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
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier.

(* Helper definitions *)
Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).
Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).
(* ... more helpers ... *)

(* Function definition *)
Definition add : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N ::
    BI_local_get 1%N ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    nil;
|}.

(* Module record *)
Definition my_module : module := {|
  mod_types :=
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    nil;
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
    BI_local_get 0%N ::
    BI_local_get 1%N ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_if (BT_valtype (Some (T_num T_i32)))
      (BI_local_get 0%N :: nil)
      (BI_local_get 1%N :: nil) ::
    nil;
|}.
```

The if-then-else structure is preserved with proper type annotations for the result type.

## Related Documentation

- [Rocq Output Contract](./ROCQ_CONTRACT.md) - The external Rocq predicates the generated `.v` files depend on, and the proof-skeleton shape the translator emits
- [Vendored Rocq Stub](./rocq-stub/README.md) - The two-namespace signature stub the `coqc` type-check gate compiles generated modules against
- [`core/hassert`](../hassert/) - The `HAssert`/`HTerm` verification-obligation IR and the `inference.hspecs` custom-section codec shared by codegen, the linker, and this crate
- [WASM Codegen Documentation](../wasm-codegen/README.md) - WebAssembly code generation
- [Language Specification](https://github.com/Inferara/inference-language-spec) - Inference language reference
- [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
- [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

## Contributing

See the main project [CONTRIBUTING.md](../../CONTRIBUTING.md) guide.

## License

This crate is part of the Inference compiler project. See the repository root for license information.
