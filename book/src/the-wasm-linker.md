# The WASM Linker

The language-level constructs — `external fn`, `use … from`, resolution priority —
are documented in
[External Functions and WASM Linking](external-functions-and-wasm-linking.md).
This chapter is the subsystem deep-dive: what `inference-wasm-linker` actually does
to the bytes, why each decision was made, and where the approach diverges from
conventional linkers.

## Motivation

Inference programs can call pre-compiled `.wasm` library functions, but verification
requires a single self-contained module. Imports are the enemy of that goal: a
module with a dangling `(import "arith" "sum" …)` cannot be fully verified, because
the Rocq translator has no body to reason about. Dynamic linking trades one problem
for another — it moves the unverifiable part from the import section to runtime.

The solution is a **static whole-body merge**: after codegen emits an
import-bearing module, the linker copies the needed function bodies in, rewrites
every index reference into a unified index space, and removes the import section
entirely. The output has no dangling imports, no relocation step, and no runtime
loader. Verification then covers the actual deployed artifact, not a stand-in.

## Where the Linker Fits in the Pipeline

Codegen produces an intermediate module whose external calls lower to
`(import …)` entries. The linker consumes that module plus the resolved `.wasm`
binaries and produces the self-contained module that flows into `wasm-to-v`:

```text
.inf source
    │
    ▼
  parse ──▶ type-check ──▶ analyze
                                  │
                                  ▼
                            codegen (wasm-codegen)
    │
    ▼  main.wasm (import-bearing)
    │                               arith.wasm ──┐
    │                               sortlib.wasm ─┤
    └─────────────────────────────────────────────┤
                                                  ▼
                                       inference-wasm-linker
                                                  │
                                    unified.wasm (no imports)
                                                  │
                                    ┌─────────────┤
                                    ▼             ▼
                                  .out         wasm-to-v
                                             (Rocq .v file)
```

The linker is provided by the `inference-wasm-linker` crate (`core/wasm-linker/`).
Its public API is a single function:

```rust
use inference_wasm_linker::{link, LinkError};

let unified: Vec<u8> = link(
    main_wasm,
    &[("arith", arith_wasm), ("sortlib", sortlib_wasm)],
    None,
)?;
```

The third argument, `contracts: Option<&[ImportWriteSet]>`, is the mode that
decides whether [The Declared Write-Set Check](#the-declared-write-set-check)
runs at all: `None` skips it (the mode for hand-written `.wasm` fixtures with
no Inference declaration behind them, such as the linker's own tests), while
`Some(list)` — the mode `infc`/`infs` uses — holds every satisfied import to a
declared write set built from the program's `external fn` declarations.

Each external is tagged with the logical module name codegen recorded for it. The
merge resolves each import by matching both the logical module name **and** the
export field name, so two libraries exporting the same field name under different
logical modules are never conflated.

## The Merge Algorithm

For each import in the main module:

1. Find which external module exports a function of that name under the right
   logical module.
2. Compute the **transitive closure** of that export inside its source module via
   breadth-first search — the functions it calls recursively, plus any unexported
   helpers.
3. **Classify** the closure's feasibility tier (A, B, or C — see below).
4. **Dedup** the closure's function types into the output type section (two
   functions with identical signatures share one type entry; the key is a
   byte-packed encoding of the parameter and result value types).
5. **Append** the closure's bodies after the main module's local functions,
   rewriting every index-bearing instruction into the unified index space.
6. **Remove** the satisfied import and redirect the main module's calls from the
   old import index onto the merged body's new index.

### Index Space After Merging

The output defines one function index space, with no import section:

```text
[0 .. main_local_count)       main module's local functions (imports removed)
[main_local_count .. total)   merged external functions, in closure order
```

### Operator Re-encoding

The `rewrite` module walks each copied body's operator stream and re-encodes
only the index-bearing operators: `call`, `return_call`, `ref.func`,
`call_indirect`, `return_call_indirect`, and `block`/`loop`/`if` when carrying a
function type index. Every other operator is copied verbatim from the source
bytes. The main module's own bodies are re-encoded for the same reason: removing
imports shifts their local-function indices downward.

### Dead-Code Exclusion

The transitive closure walk collects only the functions actually reachable from
each satisfied export. An `unused` function that the source module exports but
that no satisfied import calls is never pulled into the closure, so it does not
appear in the merged output.

### The Example from the Repository

The `scratch/linker-e2e/` demo links three external modules simultaneously. The
Inference.toml declares them:

```toml
[wasm-dependencies]
arith   = { path = "libs/arith.wasm"   }
memlib  = { path = "libs/memlib.wasm"  }
sortlib = { path = "libs/sortlib.wasm" }
```

The source uses all three:

```inference
external fn sum(a: i32, b: i32) -> i32;
external fn neg(a: i32) -> i32;
use { sum, neg } from arith;

external fn store_at(mut ptr: i32, val: i32);
external fn load_at(ptr: i32) -> i32;
use { store_at, load_at } from memlib;

external fn sort_pair(mut ptr: i32);
use { sort_pair } from sortlib;
```

`arith` is pure arithmetic (Tier A). `memlib` and `sortlib` access memory only
through the caller-supplied pointer (Tier B). `sort_pair` transitively calls a
non-exported `swap` helper — the closure walk drags that helper in automatically,
and both functions merge. After linking, the output has no imports and one
reconciled linear memory shared by all three modules' bodies.

## Feasibility Tiers

Not all external functions can be merged without relocation metadata. The linker
classifies each closure:

| Tier | What the closure may touch | Merged? | Admission condition |
|------|---------------------------|---------|---------------------|
| A | No memory, no global or table access, no data segments — pure arithmetic | Yes | None beyond the operator allow-list |
| B | Linear memory **only through caller-supplied pointers**; no own data segments, no global or table access | Yes | Provenance proof: every memory address is parameter-derived (see below); the closure's may-write set is covered by the `external fn` declaration's `mut` parameters (see [The Declared Write-Set Check](#the-declared-write-set-check)) |
| C | Own static data segments, global access, or table/element use | No | Rejected with `LinkError::RequiresRelocatableBuild` |

### What Each Tier May Touch

The classification logic inspects the parsed module structure and the closure's
`ClosureEffects`, collected as a side effect of the operator allow-list scan:

| Signal | Forces Tier C |
|--------|---------------|
| `module.data_count > 0` or closure uses `memory.init` / `data.drop` | owns static data segments |
| `module.element_count > 0` or closure uses `call_indirect` / `table.*` / `ref.func` / `elem.drop` | uses a table or element segment |

Reading or writing a module global is **not** a Tier-C signal: the closure's
globals are merged into the output alongside the main module's, with every
`global.get` / `global.set` remapped onto the merged index space. A global used
to *address* memory is still rejected, because the address it produces is not
parameter-derived.

If no Tier-C signals are present, the closure is Tier B when any body accesses
linear memory (load, store, copy, fill, size, or grow), and Tier A otherwise.

Globals and table *use* are gated on **use**; data and element segments on
**declaration**. A global no body reads or writes, and a table with no element
segment that no instruction names, are inert — and they are exactly what real
toolchains emit unconditionally (lld puts a `__stack_pointer` global into every
`wasm32-unknown-unknown` artifact, and an empty `(table 1 1 funcref)` into every
`std` one), so rejecting on their declaration would exclude every such artifact.

The two declaration-gated signals rest on different arguments. An *active* data
segment writes memory at instantiation whether or not any instruction names it,
so an unreferenced one still changes program behavior — a correctness argument.
An *element* segment is rejected as conservatism: dropping one is unobservable,
since the merged output declares no table for it to initialize, but it marks a
module built around indirect dispatch and admitting it would silently discard a
construct the author wrote.

Dropping an admitted external's globals and tables is sound because
`ClosureEffects` is closure-scoped: a closure admitted with no global or table
effect contains no operator naming either index space. That matters most for
globals — the merge re-emits main's global section, so a leaked `global.get 0`
would rebind to main's first global and, the types agreeing, still pass
post-merge validation. A leaked *table* operator is fail-safe by comparison: no
table section is emitted, so validation rejects it as an unknown table.

Clearing the tier gate is not the same as linking. A stock artifact also declares
a multi-page memory that the merge will not reconcile against an Inference main's
fixed one-page `(memory 1 1)`; see the memory reconciliation rules below.

A Tier-A function carries no shared-memory surface at all: it reads its parameters,
does arithmetic, and returns. Merge cost is a body copy, a type dedup, and an
index rewrite.

A Tier-B function shares the single linear memory the main module owns. No address
relocation is needed because every address is caller-supplied at runtime. However,
the linker must *prove* this property — it cannot assume it from the section
structure alone.

### Tier-C Rejection

A Tier-C closure is rejected with `LinkError::RequiresRelocatableBuild` listing
the specific reasons:

```text
error: external function `lookup` requires a relocatable build:
         defines or initializes its own static data segments
```

The `reasons` field is a `Vec<String>` so a closure that simultaneously has its
own globals and uses `memory.init` reports both signals in one diagnostic.

## Tier-B Provenance Analysis

The provenance analysis is the most novel part of the linker. Tier B's contract
is that a merged external touches shared linear memory *only through addresses the
caller passes in*. A function that fabricates an address from a constant or reads
one from its own global would alias the host program's own linear memory at a
fixed offset — a silent miscompile the section-inspection tier check cannot detect,
because the function validates cleanly and its export signature matches.

The linker proves the contract with a **sound, flow-sensitive, interprocedural
abstract interpretation** over the whole closure.

### The Provenance Lattice

Every operand-stack slot and every local carries one of three provenance tags:

| Tag | Meaning |
|-----|---------|
| `Prov::Param(mask)` | The value provably derives from one or more of this function's parameters, through operations that cannot cancel the caller's pointer. `mask` is a 64-bit bitset recording which parameters. |
| `Prov::Const` | The value is a compile-time constant (`*.const` literal, or `add`/`sub` of two `Const`s). Caller-independent — never a valid memory address on its own, but a valid offset to add to a `Param` base. |
| `Prov::NotParam` | Any other source: a global, a call result, a parameter-cancelling operator, or anything the analysis cannot prove parameter-derived. The fail-closed default. |

The lattice join is a **must-join**: a value stays `Param` only when it is
`Param` on *every* incoming control-flow path. The mask at a join point is the
**union** of the per-path masks (on every path it derives from *some* parameter,
so on the merged path it derives from one of the union). A value arriving as
`Const` on one path and `Param` on another widens to `NotParam`.

### Why `Const` Is a Separate Tag

`Const` exists so a `Param + Const` expression (a struct-field or
array-element offset: `base_ptr + 8`) can stay `Param` while a `Param + NotParam`
expression cannot. A `NotParam` addend means *not provably parameter-derived*, not
*constant*. It may hold `C - p` (a constant minus a parameter), and
`(C - p) + p == C` is a caller-independent absolute address. Restricting the
non-`Param` addend to a proven `Const` closes that cancellation attack.

For the same reason `sub` propagates `Param` only from the minuend when the
subtrahend is `Const` (`caller_base - fixed_offset` is still caller-relative), and
`Param - Param` demotes to `NotParam` (since `b - b == 0`, a caller-independent
constant).

Every other binary operator — multiply, divide, bitwise, shift, rotate,
comparison — and every unary operator produces `NotParam`. The analysis is
deliberately conservative: it cannot distinguish `param << 0` (value-preserving)
from `param & 0` (value-destroying), so it treats all such operators uniformly.

### Bulk-Memory Extent Operands

For `memory.fill` and `memory.copy`, the **size/extent** operand carries the same
caller-derivation requirement as the address operand. The operation touches the
contiguous region `[address, address + size)`, so a caller-bounded start is not
enough — a constant or global `size` would let the operation clobber or read an
unbounded span above a caller pointer (`memory.fill(base, v, 0x8000)` scorches
host memory the caller never exposed). A `Param` size (a caller-supplied `len`)
is admitted; a `Const` or `NotParam` size fails the subset check.

### Interprocedural Fixpoint

Each function in the closure is summarised once by seeding parameter `i` with
`Prov::Param({i})`. The summary records, per function, the provenance mask of
every memory access, and per call site, the argument mask of every argument in
the calling function's own parameter terms.

A greatest-fixpoint pass over the call graph then computes, for every function
`g`, the set `trusted[g]` — the subset of `g`'s parameters that are provably
caller-derived:

- The closure root's parameters are all trusted (the host that calls the exported
  function supplies them).
- A parameter `j` of a non-root function `g` is trusted if and only if, at
  **every** internal call site `f → g`, the argument in position `j` has a
  non-empty mask that is a subset of `trusted[f]`.

The iteration starts from "all parameters trusted" and removes any parameter
contradicted at a reachable call site, converging in at most `(slot count + 1)`
rounds. The fixpoint handles self- and mutual recursion correctly. A function
reachable only through the table (no direct call site) starts with the empty
trusted set — a dereference of its parameter is rejected.

Finally, every recorded memory access is verified: its address mask must be
non-empty and a subset of its function's trusted set. A failure at any point
rejects the whole closure as Tier C, never Tier B.

```text
root_params = {ptr, len}        (trusted by the external caller)

inner(addr, count):
  addr derived from ptr → trusted
  count derived from len → trusted
  memory.fill(addr, 0, count) → both masks ⊆ trusted → Tier B ✓

inner2(addr, count):
  count is i32.const 0x8000 → Const, mask = {} → mask ⊄ trusted → Tier C ✗
```

### The Declared Write-Set Check

Tier-B admission proves that every memory address a closure computes *derives
from* a caller-supplied parameter. It does not say *which* parameter licenses
a given store, and it does not have to — until the closure is also held to a
declared write set.

`mut` on an `external fn` parameter is the Inference-side declaration of that
licence: it states that the foreign body may store through the address that
parameter denotes. The linker checks the claim, keyed on `(module, field)` —
the same pair an import is satisfied on — against the merged bytes, in one of
two ways depending on what the declaration says:

- **No parameter marked `mut`** (an empty declared set) is checked
  *structurally*: the merged closure must record no `Store` access anywhere,
  not merely an attributed write set that happens to come out empty. A caller
  relying on "this closure writes nothing, anywhere" then inherits none of the
  attribution pass's own assumptions.
- **Some parameter marked `mut`** is checked against a second, forward
  least-fixpoint attribution pass — distinct from the greatest-fixpoint
  `trusted[g]` computation above — that computes, for every function `g` in
  the closure, the set of the *root export's* parameters each of `g`'s own
  parameters may derive from, seeded at the root and propagated through every
  call site's argument dependencies. Every `Store` access's address is then
  attributed to the union of root parameters its dependencies may derive from.
  The attributed set must be a subset of the declared one, or the link fails
  with `LinkError::UndeclaredExternWrite`, naming the offending parameter by
  index and, where the declaration gave it a name, by name.

An import the checked mode's declaration list does not mention is treated as
declaring **no** `mut` parameter, never skipped — see
[Where the Linker Fits in the Pipeline](#where-the-linker-fits-in-the-pipeline)
for the two `contracts` modes that decide whether the check runs at all, and
for which callers use each one.

**This check licenses derivation-to-a-parameter, not containment within it** —
the same limit [Tier-B Provenance Analysis](#tier-b-provenance-analysis)
describes. `W ⊆ D` bounds which parameter a store's *address* derives from; it
says nothing about which *bytes* the store touches, or whether they stay
inside the region the caller meant that parameter to grant. A declaration
`external fn writer(mut a: [i32; 2], b: [i32; 2]);` whose body stores at `a +
8` — past `a`'s own two elements — links cleanly, because the address still
derives from parameter 0. Closing that gap needs the numeric/interval domain
issue #420 tracks; the write-set check narrows *which* parameter a store may
be attributed to, not *where* within it the store lands.

That narrower guarantee is nonetheless enough to license one thing safely:
`core/wasm-codegen`'s by-reference optimization elides a compound parameter's
entry copy only when **every** external it reaches declares no `mut`
parameter at all. An empty declared set forces the structural "no `Store`
access anywhere" fact above, and a closure that never stores cannot disturb a
caller's elided copy — no containment property is needed for that narrower
claim. A *mixed* declaration (`writer(mut a, b)`) still costs every parameter
it reaches a copy, including `b`'s: per-position elision is deliberately not
implemented, because it would need the containment property this check does
not have. See `core/wasm-codegen/docs/arrays-and-memory.md` for the codegen
side of this decision.

The attributed set can also be broader than "this parameter is itself a
written-through address" suggests, for a reason distinct from the
containment gap above: the attribution is affine, and it counts every
parameter that may *contribute* to a store's address, not only the one
playing the role of base pointer. The ordinary scaled-index write `mem[ptr +
(idx << 2)] = val` attributes to both `ptr` and `idx` — `idx` never denotes
an address on its own, but it scales one, and the affine form the
attribution tracks does not distinguish that role from a base pointer's. A
declaration `external fn set_elem(ptr: i32, idx: i32, val: i32);` bound to a
body that writes this way must therefore mark `idx`, not only `ptr`, `mut` —
`set_elem(mut ptr: i32, mut idx: i32, val: i32);` — or the link fails naming
parameter 1, even though nothing about `idx`'s own value is a location the
body writes *to*. This runs in the same conservative direction as the
containment gap: attributing too broadly can force a wider declaration than
the informal reading of `mut` suggests, never let an actual write through
undeclared.

The read/write split above is what keeps this from over-reaching further.
`memory.copy(dest, src, size)`'s `src` operand is recorded as a `Load`, never
a `Store`, so it never contributes to any `Store` access's dependency and is
never pulled into the attributed write set — even though the same
instruction also writes `dest`. A pure source pointer stays correctly
unforced; it is specifically a *store's own* address computation whose
affine contributors all get pulled in, `idx`-style scaling parameters
included.

## Proof-Only Stripping

Inference non-deterministic blocks (`forall`, `exists`, `assume`, `unique`) and
uzumaki rvalues (`i32.uzumaki`, `i64.uzumaki`) are **proof-only constructs**:
they have meaning solely in the Rocq lowering and no executable runtime semantics.
A function that is merged into the output is part of an executable binary, so a
proof-only opcode inside such a body would yield a non-executable output.

The operator allow-list (`src/safety.rs`) **rejects** every proof-only opcode from
an external body with `LinkError::UnsupportedConstruct`:

```text
unsupported WASM construct for static merge: non-deterministic block `forall`
    has no executable semantics and cannot be merged into an executable binary
```

The main module's own proof scaffolding (its `spec` blocks and non-det opcodes)
is **preserved and passed through** verbatim — the re-encoder recognises those
opcodes and copies them intact onto the main-module bodies, which are not subject
to the allow-list. Those bodies flow through `wasm-to-v` as the proof obligations
the user wrote.

## Floating-Point Exclusion

The Inference language has no `f32`/`f64` types. The Rocq translator models no
float instruction. The linker enforces this at two gates:

1. **Feature gate** (`SUPPORTED_WASM_FEATURES` in `src/lib.rs`): every external
   module is structurally validated before any body is touched. The feature set
   deliberately omits `FLOATS`, so a float-using external is rejected upfront with
   `LinkError::UnsupportedWasmFeature` naming the exact feature.

2. **Operator allow-list** (`src/safety.rs`): the main-module re-encode path does
   not pass through the feature gate, so the allow-list is the backstop. Every
   float instruction — comparisons, arithmetic, conversions, reinterprets,
   loads/stores, and constants — is rejected with `LinkError::UnsupportedConstruct`
   naming the exact mnemonic (e.g. `floating-point instruction 'f32.add' is not
   supported`).

Saturating float-to-int (`i32.trunc_sat_f32_s`, etc.) is also excluded: its
operands are floats, and the Rocq translator declares no float number type.

Sign-extension (`i32.extend8_s`, `i64.extend32_s`, etc.) is *not* excluded,
though Inference codegen still emits none of it. The Rocq translator lowers all
five opcodes to `BI_unop t (Unop_extend n)` — the proof model classifies
sign-extension as a unop, beside `clz`/`ctz`/`popcnt`, not as a conversion — so
an external compiled by a real toolchain can carry them.

## Name Preservation

The linker preserves the WASM `name` custom section so the Rocq translator emits
named `Definition`s rather than opaque `func_<idx>` placeholders:

- Main module local functions keep their source debug names, re-indexed onto the
  import-free output space.
- Every merged external function is named under its source's logical module,
  joined with `::`:
  - A closure **root** satisfying import `sum` bound under logical module `mathlib`
    becomes `mathlib::sum`.
  - An internal callee the source module named keeps that name behind an internal
    mark: `mathlib::#helper`.
  - A **nameless** inner callee (an external with no name section) receives a
    deterministic fallback derived from its output index: `mathlib::#func_<idx>`.

The section is one namespace with two halves, and `::` is the boundary. A
compiled function's name-section symbol is built from Inference identifiers
joined by `.`, so it can never carry a `:`: a source module named `mathlib` and a
linked logical module named `mathlib` are free to coexist. Within the merged
half, the module prefix keeps two libraries that export the same field apart, and
the internal mark keeps a module's private callees from shadowing its own roots —
an inner debug name comes from the foreign module and is unconstrained, so it may
be exactly an export field.

A WASM name map holds one name per function index. When one foreign body
satisfies two imports (an export bound under two fields), the least of its root
names is recorded and an obligation over the other is rewritten onto it, so both
declarations still describe the body they name.

The Rocq translator (`core/wasm-to-v/src/rocq_names.rs`) maps every byte outside
`[A-Za-z0-9_]` to `_` and collapses the runs, so `mathlib::sum` becomes
`Definition mathlib_sum` in the `.v` file — the same identifier the dotted form
produced. A residual collision after sanitization (two modules that sanitize to
one identifier) is still disambiguated by the translator's index suffix; the
scheme removes the common cases rather than every possible one.

## Error Reference

| Error | Trigger |
|-------|---------|
| `LinkError::Parse(msg)` | A module's bytes could not be parsed as valid WASM, or a module that passed structural validation contains a malformed section (over-declared locals count, invalid LEB128, etc.). Under adoption it also covers a malformed or duplicated `inference.spec_funcs`/`inference.hspecs` section in a *linked library*, with the logical module named; under the other two settings those bytes are not read at all |
| `LinkError::UnsatisfiedImport { field }` | No external module tagged with the right logical module name exports a function named `field` |
| `LinkError::TransitiveHostImport { module, field }` | A body inside the merged closure calls one of the external module's own imports; there is no body to copy for it |
| `LinkError::RequiresRelocatableBuild { field, reasons }` | The closure for `field` is Tier C; `reasons` lists each signal (e.g. "defines or initializes its own static data segments") |
| `LinkError::UnsupportedConstruct(msg)` | A body contains an unmergeable construct: any floating-point instruction (with the exact mnemonic), a proof-only non-det or uzumaki opcode in an external body, a tail call (`return_call` / `return_call_indirect`), a segment-indexed table op (`table.init` / `elem.drop` / `table.copy`), a float or `v128` value type in a merged signature or local, multi-memory access, or a main module section the merge cannot preserve (start function, table section, non-function imports, data/element segments) |
| `LinkError::UnsupportedWasmFeature { module, details }` | The external module is well-formed WASM but uses a feature beyond the supported subset (floats, saturating float-to-int, reference types, SIMD, atomics, exceptions, `memory64`, multi-memory, multi-value, GC, or tail calls); `details` carries the validator's feature-named diagnostic |
| `LinkError::AmbiguousImport { module, field }` | More than one supplied external exports a function of the same field name the import requests under the same logical module; the body to merge is ambiguous |
| `LinkError::IncompatibleMemory { field, reason }` | The linear memory requirements of the main module and the Tier-B external cannot be reconciled into one shared output memory |
| `LinkError::InvalidMergedModule(msg)` | The post-merge structural validator rejected the merged output; this is a guard against allow-list gaps — it converts a potential silent miscompile into a clean diagnostic |
| `LinkError::UndeclaredExternWrite { module, field, param_index, param_name }` | A Tier-B closure's attributed write set is not covered by its `external fn` declaration's `mut` parameters (checked mode only — see [The Declared Write-Set Check](#the-declared-write-set-check)); the message names the offending parameter and, when the declaration uses an unnamed form, says to name it before it can be marked `mut` |
| `LinkError::UndescribedExternWrite { module, field, param_index }` | A Tier-B closure may store through a parameter, and the checked-mode contract list supplied for this link holds no entry at all for this import — named or unnamed. An import nothing describes is held to writing nothing, so the store is refused rather than admitted unchecked |
| `LinkError::DuplicateWriteContract { module, field }` | The checked-mode contract list holds more than one entry for the same `(module, field)` import; the linker has no basis to choose which one governs |
| `LinkError::UnresolvedObligationSymbol { symbol, merged_roots }` | A function symbol the main module's `inference.hspecs` obligations apply is carried by no function of the merged output. `merged_roots` lists every `<module>::<field>` the merge did satisfy, so the message can say what was on offer. |
| `LinkError::AmbiguousObligationSymbol { symbol, carriers }` | Two or more functions of the merged output carry one applied obligation symbol. `carriers` says where each came from — the program's own code, a satisfied import, or a linked module's private function. |
| `LinkError::AdoptedSpecUnlisted { module, spec }` | Adoption only: the library ships obligations under a specification its own `inference.spec_funcs` section does not list, so its two verification sections disagree with each other |
| `LinkError::AdoptedSpecNameInvalid { module, spec, key, reason }` | Adoption only: the name an adopted specification would take is not one the proof translation can spell as an identifier; `reason` is the structural clause that rules it out |
| `LinkError::AdoptedSpecNameCollision { spec, module, contender }` | Adoption only: the name an adopted specification would take is already claimed — by a specification the program declares (`contender: None`) or by another library's adopted specification |
| `LinkError::AdoptedObligationSymbolUnresolved { module, spec, symbol }` | Adoption only: an adopted obligation applies a function symbol no function of its own library's `name` section carries |
| `LinkError::AdoptedObligationSymbolAmbiguous { module, spec, symbol, carriers }` | Adoption only: several functions of the library carry the symbol an adopted obligation applies; `carriers` lists their indices in the library |
| `LinkError::AdoptedObligationUnmergedSymbol { module, spec, symbol, imported }` | Adoption only: an adopted obligation applies a function of its library this merge did not fold in — one outside the export closure (`imported: false`) or one of the library's own imports (`imported: true`) |
| `LinkError::AdoptedSpecSymbolCollision { module, spec, symbol }` | Adoption only: the merged-namespace symbol an adopted specification function would take is one a function of the merged output already carries |

## Supported WASM Subset

The linker accepts only the following feature set (`SUPPORTED_WASM_FEATURES`
in `src/lib.rs`):

- Integer core: `i32`/`i64` value types, all integer arithmetic, comparisons,
  loads/stores, and the three integer width conversions (`i32.wrap_i64`,
  `i64.extend_i32_s/u`).
- Mutable globals, bulk memory (`memory.copy`/`memory.fill`), and sign-extension
  (`i32.extend8_s`, `i32.extend16_s`, `i64.extend8_s`, `i64.extend16_s`,
  `i64.extend32_s`).

Everything else is rejected at the feature gate or the operator allow-list before
any body is copied.

## Formal Implications

Once merged, every external function becomes an ordinary local function in the
output module. The Rocq translator processes that module without knowing which
functions originated externally: each merged body becomes a Rocq `Definition` in
the `.v` file, exactly as a locally-defined function does.

Verification therefore covers the actual deployed merged artifact, not a
stand-in. A proof that `sort_demo` returns the expected value reasons about
the real `mathlib_sort` and `memlib_store_at` bodies — not their declared
signatures.

Two custom sections carry the program's proof obligations, and the linker treats
them differently because their payloads are shaped differently:

- `inference.spec_funcs` (`src/spec_funcs.rs`) records, per spec, the WASM
  **function indices** that make up that spec — its membership list, not its
  obligations. The Rocq translator reads it to decide which functions leave the
  emitted module record (a `forall` or plain spec function becomes a downstream
  contract, so its body is dropped) and which stay in it (an `exists` or
  `unique` function's body is what its reachability judgment reduces), and to
  disambiguate which function a reachability obligation judges. The merge
  removes imports and shifts every function index, so an index recorded pre-link
  is stale afterward; the linker decodes the section, remaps each index through
  the unified index space via `Plan::map_main_func`, and re-emits it
  canonically.
- `inference.hspecs` records the `hassert` obligations an `assert(...)` inside
  a spec compiles to, referencing the functions they apply by **symbolic
  name** — a `name`-section string, not an index — so no index remap applies.
  The merge re-emits it otherwise unchanged, editing exactly one thing: when a
  merged body ends up recorded in the output under a different alias than the
  one an obligation names (the aliased-export case in
  [Name Preservation](#name-preservation) above), the affected symbol is
  rewritten onto the alias the output's name section actually
  carries. A merge whose obligations still fail to resolve after that rewrite
  is rejected — see `LinkError::UnresolvedObligationSymbol` and
  `LinkError::AmbiguousObligationSymbol` in the [Error Reference](#error-reference)
  below.

Both of the above describe the **main module's own** sections. An **external**
module's `inference.spec_funcs` and `inference.hspecs` — the spec membership
and `hassert` obligations a linked *library* recorded about its own code —
describe a module the output is not: only the executable closure of a satisfied
export crosses the merge, and a library's specification functions are never in
it. What the link does with them is an explicit input,
`LinkOptions::external_specs`, with three settings:

- **`Warn`** (the default, and what a build that writes a `.v` uses). Neither
  section is decoded — a malformed one in a library nothing needed still cannot
  fail a link — and a `LinkWarning` names each library whose `inference.hspecs`
  the output does not carry, together with both spellings of the opt-in. The
  emitted bytes are exactly what they were.
- **`Ignore`**. The same bytes, and nothing said. A build that writes no `.v`
  uses it: no proof artifact exists for the obligations to reach, and the
  report would fire on every compile of every program that links a proof-mode
  library.
- **`Adopt`** (`infc --adopt-external-specs`, or `adopt-external-specs = true`
  under the manifest's `[verification]` table). Each contributing library's
  verification sections are decoded and its **universal** (`forall`)
  obligations are written into the merged module's own sections under
  `<logical module>_<library's spec name>` — `mathlib` plus `DoubleSpec` gives
  `mathlib_DoubleSpec`, and `a::b` gives `a_b_DoubleSpec`. Every function
  symbol such an obligation applies is resolved in the *library's own* `name`
  section, required to be one of the bodies this merge actually folded in, and
  rewritten to the exact string the output's `name` section carries for that
  body — so an adopted obligation names the body its author wrote it about, and
  never a same-named function of the program. The adopted spec has no
  specification function of its own in the output, so its
  `inference.spec_funcs` entry lists no index, which is what a universal
  obligation needs: `ValidSpec` is discharged denotationally and never reduces
  a spec body.

`exists` and `unique` obligations are **not** adopted under any setting, and
adoption reports each one it left behind. Their judgments are evaluated against
the frame an execution of the specification function reaches, and that function
is precisely what does not cross the merge. A library whose obligations are all
reachability obligations contributes no spec at all rather than an empty one: a
`ValidSpec` over an empty list is true of every module and would state nothing
about the program.

Adoption fails closed. A spec name colliding with one the program declares or
with another library's, a name the proof translation cannot spell as an
identifier, a symbol the library's own name section carries on no function or
on several, an obligation over a function this merge did not need, and a
library whose two verification sections disagree with each other are each a
hard `LinkError` naming the library, the specification and the symbol — see the
[Error Reference](#error-reference).

Adoption carries obligations, not proofs: each arrives as a `ValidSpec` theorem
with an unfilled proof, to be discharged against the merged module. That is
what makes it sound across everything the merge changes about the library's
environment — one shared linear memory, remapped globals, renumbered calls.

## Comparison with Traditional Linkers

Traditional `wasm-ld` (LLVM's WebAssembly linker) supports relocatable object
files: each compiled translation unit emits relocation metadata (symbol tables,
reloc sections), and `wasm-ld` patches absolute addresses and index references at
link time. That model handles Tier-C inputs (static data, globals, indirect-call
tables) without a provenance proof, because the relocation metadata describes
exactly what needs patching.

Inference cannot use `wasm-ld` for two reasons:

1. **Verification needs a relocation-free module.** `wasm-ld` produces a module
   that is self-contained at runtime but which was assembled from relocatable
   pieces. The Rocq translator expects to reason about the final module; it has no
   model for relocation metadata or the toolchain decisions it encodes.

2. **External libraries are not necessarily compiled with Inference.** They may
   come from any WASM toolchain and need not carry `wasm-ld`-compatible relocation
   sections. The static merge works on any conforming WASM binary — no toolchain
   cooperation is required for Tier-A and Tier-B inputs.

The provenance analysis fills the gap: instead of relocation metadata, the linker
*proves* that the merged function cannot produce an address the host program did
not supply. A function passing that proof needs no relocation, because its memory
accesses are bounded to whatever region the caller chose to expose.

| Concern | `wasm-ld` | Inference static merge |
|---------|-----------|------------------------|
| Tier-A/B inputs | Requires reloc sections | Any conforming WASM binary |
| Tier-C inputs | Supported via relocation | Rejected (`RequiresRelocatableBuild`) |
| Address safety | Relocation metadata | Interprocedural provenance proof |
| Verification | Reloc artifact not translatable | Merged module translates directly |
| Runtime loader | Not required (static) | Not required (static) |

Tier-C support via relocation metadata is a stated future direction. The current
linker explicitly gates on Tier A and B rather than risk a silent miscompile from
an unproven address.

## Related Resources

- [External Functions and WASM Linking](external-functions-and-wasm-linking.md) —
  the language-level feature: `external fn`, `use … from`, resolution priority,
  and the Tier A/B/C overview
- [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md) —
  configuring `[wasm-dependencies]` and building with the project-aware CLI
- [Compilation Targets](compilation_targets.md) — compile vs. proof modes;
  the merged module flows through the same `-v` proof path as a locally-compiled module
- `core/wasm-linker/README.md` — merge algorithm, tier classification,
  index space, name section, testing, and fuzzing
- `core/wasm-linker/src/provenance.rs` — full source of the abstract interpreter
  (module-level doc comment is a complete specification)
- [WebAssembly binary format](https://webassembly.github.io/spec/core/binary/index.html) —
  section ordering, index spaces, and the name custom section
