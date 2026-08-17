# inference-wasm-linker

Static-merge linker for the Inference compiler: folds external `.wasm` function
bodies into the main module so no cross-module imports remain in the output.

## Overview

When an Inference program declares `external fn` bindings and calls them, the
compiler emits the main module with `(import …)` entries — one per external
function — at the lowest function indices. `inference-wasm-linker` consumes that
intermediate module plus the resolved external `.wasm` binaries and produces a
**single self-contained module** with those imports satisfied and removed.
The result has no dangling cross-module imports and flows directly into
`wasm-to-v` for Rocq translation.

This approach mirrors `wasm-ld`: compile first, link second. Keeping the link
pass in a separate crate makes it testable in isolation and reusable for the
C-library half of issue #9.

## How the Merge Works

For each import in the main module the linker performs these steps:

```text
1. Find which external module exports a function of that name
2. Compute the transitive closure of that export inside its source module
         (the functions it calls, recursively, plus any helpers)
3. Classify the closure's feasibility tier (A, B, or C — see below)
4. Dedup the closure's function types into the output type section
5. Append the closure's bodies after the main module's local functions,
         rewriting every index-bearing instruction into the unified index space
6. Remove the satisfied import and redirect the main module's calls
         from the old import index onto the merged body's new index
```

### Index Space After Merging

The output module defines a single function index space:

```text
[0 .. main_local_count)          main module's local functions (imports removed)
[main_local_count .. total)      merged external functions, in closure order
```

Every `call`, `ref.func`, and `call_indirect` type index in all copied bodies is
rewritten through the `rewrite` module to land in this space.

### Operator Re-encoding

The `rewrite` module walks each copied body's operator stream and re-encodes only
the index-bearing operators (`Call`, `ReturnCall`, `RefFunc`, `CallIndirect`,
`ReturnCallIndirect`, block/loop/if when carrying a function type index). Every
other operator is copied verbatim from the source bytes, so the output is
byte-identical to the input wherever no index changes.

### Type Deduplication

Two functions with identical signatures share one type entry in the output type
section. The deduplication key is a byte-packed encoding of the parameter and
result value types. This prevents the type section from growing with duplicate
entries as more external closures are merged in. Only **type-section entries**
(signature declarations) are deduplicated — function bodies are never
deduplicated or dropped by this step. Unreachable functions are excluded earlier
by the transitive closure walk, before any output index is committed.

### Name Section

The linker preserves the `name` custom section so the Rocq translator emits
named `Definition`s rather than opaque `func_<uuid>` placeholders:

- Main module local functions keep their source debug names (re-indexed onto the
  import-free output space).
- Every merged external function is named under its source's logical module,
  using a `module.field` form:
  - A merged closure **root** is named `<module>.<import field>` — a closure that
    satisfies import `sum` bound under logical module `mathlib` becomes
    `mathlib.sum`.
  - A merged **inner callee** the source module named keeps that name, prefixed:
    `mathlib.helper`.
  - A **nameless** inner callee (an external stripped of its name section) is
    given a deterministic fallback derived from its output index, prefixed the
    same way: `mathlib.func_<idx>`.
- If no function carries a name, the name section is omitted entirely.

The module prefix is collision-free by construction: two externals bound under
different logical modules may export — and internally call — functions of the
same field, and an unprefixed scheme would let those names collide in the name
section, forcing the Rocq translator down its index-suffix disambiguation
(`sum` vs `sum_2`), which is index-dependent and shifts across merges. The `.`
separator matches Inference's `Type.method` naming convention. The Rocq
translator (`core/wasm-to-v/src/rocq_names.rs`) sanitizes every non-alphanumeric
to `_`, so `mathlib.sum` reads as `Definition mathlib_sum` in the `.v`. A residual
name collision after sanitization (e.g. two distinct logical modules that
sanitize to the same identifier) is still disambiguated by the translator's index
suffix; the module prefix removes the common case rather than every possible one.

## Proof-Mode Custom Sections

Two more custom sections carry proof-mode verification metadata across the
link: `inference.spec_funcs` (per-spec WASM function indices) and
`inference.hspecs` (per-spec `hassert` verification obligations, decoded via
the shared `inference-hassert` crate — the same codec `wasm-codegen` writes
and `wasm-to-v` reads). Both are handled identically at a high level, driven
by the parsed module's role:

- **External modules never decode either section.** `ParsedModule::parse_with_role`
  skips both at the custom-section dispatch point for `ModuleRole::External`,
  without even attempting to decode them — a malformed one in an irrelevant
  external can never fail the link, because only the executable closure of the
  satisfied export crosses the merge; an external's own spec obligations are
  never part of the output.
- **The main module decodes and validates both up front.** A malformed
  payload, or a **second** `inference.spec_funcs`/`inference.hspecs` section
  in the main module, is a hard `LinkError::Parse` rather than a silent
  last-wins overwrite — both sections are verification deliverables the merge
  must not drop.
- **`inference.spec_funcs` is remapped.** Its `spec_name -> [func_idx]` payload
  is expressed in the *pre-link* function-index space, so `Plan::remap_spec_funcs`
  rewrites every recorded index through the exact same `map_main_func` map
  that repoints the main module's own `call` targets and exports, then
  `crate::spec_funcs::encode` re-emits it canonically. `map_main_func` bounds-checks
  each index here too — a garbage or out-of-range spec index is rejected with a
  `LinkError` instead of being silently remapped onto the wrong (or a
  nonexistent) function and reaching the Rocq proof obligation.
- **`inference.hspecs` needs no remap.** Its obligations reference callees
  *symbolically*, by function name, not by index, so the merge carries the
  decoded map through unchanged in content — it is simply re-encoded
  canonically (`inference_hassert::encode`) once the merge is otherwise
  complete. Every symbol stays resolvable post-link because the main module's
  own function names survive the rebuilt name section verbatim (only merged
  external names are synthesized), so nothing in the payload needs updating.

Both sections are emitted only when the main module actually carried one — a
self-contained module (or one with no `spec` blocks at all) produces neither
`inference.*` section in the linked output, matching the executable-code-only
output of a plain merge. When present, they are emitted in that order,
directly after the rebuilt `name` section.

## Feasibility Tiers

Whether an external function can be merged depends on what its transitive closure
touches. The tier model ships the common cases first and gates the hard case
behind a clear error rather than attempting an unsound merge.

### Tier A — Pure Functions

No memory accesses, no globals, no data segments, no tables. Examples: `sum`,
`sub`, `abs`, any function that only reads its parameters and does arithmetic.

Merge cost: copy the body, dedup the type, rewrite `call` targets. No address
relocation needed.

```wat
;; Tier A: pure arithmetic — trivially mergeable
(func $sum (param i32 i32) (result i32)
  local.get 0
  local.get 1
  i32.add)
```

### Tier B — Memory Through Caller-Passed Pointers

The closure loads or stores through addresses the caller supplies, but defines no
static data of its own, no mutable globals, and no table or element entries.
Examples: `sort(ptr, len)`, `memcpy(dst, src, n)`.

Merge cost: same as Tier A. The function shares the single linear memory the main
module owns; no address relocation is required because all addresses are
caller-supplied at runtime.

```wat
;; Tier B: writes to a caller-supplied address — mergeable
(func $store_at (param $addr i32) (param $val i32)
  local.get $addr
  local.get $val
  i32.store)
```

### Tier C — Own Static Data, Globals, or Tables

The closure carries its own baked-in data segments (lookup tables, string
constants), defines or accesses module globals (per-module mutable state), or
uses table and element entries for indirect calls. Merging these without
relocation metadata would silently produce an incorrect module because the
absolute addresses and per-module state would alias unpredictably with the main
module.

The linker rejects Tier-C inputs with `LinkError::RequiresRelocatableBuild` and
a list of specific reasons. Build the external module with a
relocatable/position-independent toolchain to enable future Tier-C support.

```text
error: external function `lookup` requires a relocatable build:
         defines or initializes its own static data segments
```

### Classification Logic

The `tier` module collects "Tier-C reasons" by inspecting the parsed module
structure and the closure's `ClosureEffects`:

| Signal | Tier-C reason |
|--------|---------------|
| `module.data_count > 0` or closure uses `memory.init` / `data.drop` | own static data segments |
| `!module.globals.is_empty()` or closure uses `global.get` / `global.set` | defines or accesses module globals |
| `!module.tables.is_empty()` or `module.element_count > 0` or closure uses `call_indirect` / `table.*` / `ref.func` / `elem.drop` | uses a table or element segment |

If no Tier-C reasons are collected, the closure is Tier B when any body accesses
memory (load/store/copy/fill/size/grow), and Tier A otherwise.

## Entry Point

```rust
use inference_wasm_linker::{link, LinkError};

let unified: Vec<u8> = link(
    main_wasm,
    &[("arith", arith_wasm), ("crypto", crypto_wasm)],
)?;
```

`link` takes the main module bytes and a slice of `(logical_module, bytes)`
pairs — each external is tagged with the logical module name codegen emitted for
it. It returns the unified module bytes, or a `LinkError` if any module fails to
parse, a merged closure reaches a transitive host import, or a closure is Tier C.

Every import in the main module must be satisfiable by one of the supplied
external modules. The match is by **both** the logical module name and the export
field name: `find_export` (in `src/merge.rs`) only considers externals whose
`logical_module` equals the import's module, then matches the field. So an import
`("arith", "sum")` binds to the `sum` export of the external tagged `arith` — not
to a same-named `sum` exported by a different module.

## Error Reference

| Error | Meaning |
|-------|---------|
| `LinkError::Parse(msg)` | A module's bytes could not be parsed as valid WASM. Also covers a malformed, out-of-range-indexed, or duplicate main-module `inference.spec_funcs`/`inference.hspecs` section (both are never even decoded for an external) |
| `LinkError::UnsatisfiedImport { field }` | No external module exports a function named `field` |
| `LinkError::TransitiveHostImport { module, field }` | A body inside the merged closure calls one of the external module's own imports; there is no body to copy for it |
| `LinkError::RequiresRelocatableBuild { field, reasons }` | The closure for `field` is Tier C; `reasons` lists the specific signals |
| `LinkError::UnsupportedConstruct(msg)` | A body contains an unmergeable construct: any floating-point instruction (diagnosed with the exact mnemonic, e.g. `floating-point instruction 'f32.add' is not supported`), a float or `v128` value type in a merged signature/local/block type, a reference-typed value, a tail call (`return_call`/`return_call_indirect`), a segment-indexed table op (`table.init`/`elem.drop`/`table.copy`), a verification-only non-det or uzumaki opcode in an external body, or the external module importing its environment (non-function imports). Also raised when the main module carries a section the merge cannot preserve: a start function, a table section, non-function imports, or data/element segments. The message names the specific construct. |
| `LinkError::UnsupportedWasmFeature { module, details }` | The external module is well-formed WASM but uses a feature outside the supported subset: any floating-point type or instruction, saturating float-to-int, reference types, SIMD, atomics, exceptions, `memory64`, multi-memory, multi-value, GC, or tail calls. The `details` field carries the validator's feature-named diagnostic. |

## Supported Subset

The linker accepts only the following WebAssembly feature set (see `SUPPORTED_WASM_FEATURES` in `src/lib.rs`):

- Integer core: `i32`/`i64` value types, all integer arithmetic, comparisons, and loads/stores, plus the integer-to-integer width conversions (`i32.wrap_i64`, `i64.extend_i32_s/u`). No conversion naming a float on either side.
- Mutable globals, bulk memory (`memory.copy`/`memory.fill`), and sign-extension (`i32.extend8_s`, …).

Rejected at the feature gate (external modules using any of these produce `UnsupportedWasmFeature`):

- **Floats** — `f32`/`f64` value types in any signature, local, or global; any float instruction. The Inference language has no `f32`/`f64` types and the Rocq translator models none.
- **Saturating float-to-int** (`i32.trunc_sat_f32_s`, etc.) — its operands are floats, and the Rocq translator has no lowering.
- Reference types, SIMD, atomics/threads, exceptions, `memory64`, multi-memory, multi-value, GC, tail calls.

The safety allow-list (`src/safety.rs`) provides an independent per-opcode backstop. It additionally rejects, as `UnsupportedConstruct`:

- Tail calls (`return_call`/`return_call_indirect`) — the Rocq translator has no lowering.
- Segment-indexed table ops (`table.init`/`elem.drop`/`table.copy`) — carry element segments the merge cannot relocate, and the Rocq translator has no lowering.
- Float instructions that reach the allow-list from the main-module re-encode path (which bypasses the feature gate), diagnosed with the exact mnemonic. This includes every conversion naming a float — `trunc`, `trunc_sat`, `convert`, `demote`, `promote`, `reinterpret` — because the Rocq translator declares no float number type for such a term to mention.
- Verification-only constructs (`forall`/`exists`/`assume`/`unique` blocks, `i32.uzumaki`/`i64.uzumaki`) in an external body — they have no executable semantics.

## Current Limitations

- Only Tier-A and Tier-B external functions merge. Tier-C inputs produce a clear
  `RequiresRelocatableBuild` error until a follow-on adds relocation metadata
  support.
- An external module that itself imports its host environment (non-function
  imports — memory, global, tag) is rejected as `UnsupportedConstruct`. A module
  importing only other functions from its host is rejected as
  `TransitiveHostImport` when the closure reaches one of those imports.
- Reference-typed values (`funcref`, `externref`) and `v128` in merged signatures or bodies
  are rejected as `UnsupportedConstruct`. The Inference codegen output uses only `i32`/`i64`,
  so this limit does not affect Inference-generated main modules.
- The main module must not declare a start function, a table section, data or element
  segments, or non-function imports — the static merge does not preserve these sections, so
  each is rejected up front rather than silently dropped. Inference codegen emits none of
  them; the guards apply to hand-built or third-party main modules fed to the public `link()`.
- One `.wasm` library version per logical name. Multi-version resolution is
  deferred to the manifest layer (issue #96).

## Module Organization

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | Public API (`link`, `LinkError`), crate-level documentation |
| `src/parse.rs` | `ParsedModule` — section-by-section owned representation; `ParsedModule::parse` |
| `src/closure.rs` | `compute` — transitive closure via BFS; `ClosureEffects` for tier classification |
| `src/tier.rs` | `classify` — Tier A/B/C feasibility decision |
| `src/merge.rs` | `Plan::build` + `Plan::emit` — the full merge pass; index allocation, type dedup, body re-encoding, name section, `inference.spec_funcs` remap, `inference.hspecs` re-encode |
| `src/rewrite.rs` | `reencode_body` — operator-level re-encoding under a new index space |
| `src/spec_funcs.rs` | Codec for the `inference.spec_funcs` custom section — mirrors `inference_wasm_codegen`'s encoder as a self-contained copy rather than a cross-crate dependency (the sibling `inference.hspecs` section, by contrast, shares its codec via the `inference-hassert` crate) |
| `tests/link.rs` | Integration tests: Tier A, Tier B, Tier C rejection, transitive closure, type dedup, name section, multiple externals, diamond closure |

## Testing

The integration tests in `tests/link.rs` build all fixtures from inline WAT via
the `wat` crate and assert on the linked module structure via `inf-wasmparser`:

```bash
cargo test -p inference-wasm-linker
```

Test coverage includes:

- **Tier A** — two pure functions (`sum`, `sub`) merged from one external
- **Tier A call targets** — `call` operands in the main body repoint to merged indices
- **Name section** — merged closure roots named after satisfied import fields; main names survive
- **Type dedup** — shared `(i32,i32)->i32` signature collapses to one type entry
- **Transitive closure** — `sum` delegates to an unexported `add_impl`; both are merged
- **Dead-code exclusion** — an unreferenced `unused` function is not merged
- **Tier B** — `store_at` writes to a caller address; merge succeeds; memory export survives
- **Tier C (data segment)** — `lookup` using `memory.init` is rejected with a data-segment reason
- **Tier C (global)** — `counter` accessing a module global is rejected with a global reason
- **Tier C (indirect call)** — `call_indirect` use is rejected with a table/element reason
- **Multiple externals** — `sum` from one library and `sub` from another; both satisfied
- **Unsatisfied import** — missing `sub` fails with `UnsatisfiedImport`
- **No-import passthrough** — self-contained module links without modification
- **Transitive host import** — a closure body that calls its own module's import is rejected
- **Body re-encoding** — locals, value-typed blocks, mixed types, `return_call`, `call_indirect`
- **Diamond closure** — two roots sharing one internal callee; merged exactly once
- **Main globals** — main module globals and global exports survive the merge
- **Environment import** — external module importing its host environment is rejected
- **Adversarial robustness** — a hand-seeded corpus of malformed/adversarial
  externals (one per confirmed Issue #9 robustness-audit defect) plus a
  deterministic byte-mutation sweep is fed through `link` by
  `adversarial_corpus_never_panics_and_only_emits_valid_modules`, asserting the
  contract on every input: `link` returns `Err` **or** a validator-clean module,
  and never panics, hangs, or emits a silently-invalid artifact

## Fuzzing

A coverage-guided `cargo-fuzz` target over `link` lives in `fuzz/`, a crate
detached from the main workspace (so `cargo build`/`cargo test` never touch it).
`cargo-fuzz` and nightly are not part of the default build; where they are
available:

```bash
cargo install cargo-fuzz
cargo +nightly fuzz run link
```

The deterministic property test above mirrors the fuzzer's invariant and seed
corpus, so the seam is exercised under stable `cargo test` even without
`cargo-fuzz`. See `fuzz/README.md` for details.

## Related Resources

- `core/wasm-codegen` — emits the intermediate module with `(import …)` entries consumed by this crate, plus the `inference.spec_funcs`/`inference.hspecs` sections this crate remaps/carries through
- `core/hassert` — the `HAssert`/`HTerm` IR and `inference.hspecs` codec shared by `wasm-codegen`, this crate, and `wasm-to-v`
- `core/wasm-to-v/ROCQ_CONTRACT.md` — how the Rocq translator consumes both sections downstream of this crate
- `core/inference/src/lib.rs` — driver entry points (`codegen`, `link`, `wasm_to_v`)
- Master plan: `.claude/docs/issues/9/master_plan.md` — design decisions and phase scope
- [WebAssembly binary format](https://webassembly.github.io/spec/core/binary/index.html) — section ordering, index spaces
- [WASM name custom section](https://github.com/WebAssembly/extended-name-section/blob/main/proposals/extended-name-section/Overview.md) — function debug names
