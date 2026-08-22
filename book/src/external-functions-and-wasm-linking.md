# External Functions and WASM Linking

Inference programs can call functions from pre-compiled `.wasm` libraries using
two cooperating language constructs: `external fn` and `use … from`. The
compiler emits the calls as WebAssembly imports, and a separate link step
(provided by `inference-wasm-linker`) folds the external function bodies into the
output so the final `.wasm` and `.v` files are self-contained.

## Declaring an External Function

Use `external fn` to declare a function whose body lives in another `.wasm`
module. The declaration looks like an ordinary function signature without a body:

```inference
external fn sum(a: i32, b: i32) -> i32;
```

A named parameter may be declared `mut`:

```inference
external fn store_at(mut ptr: i32, val: i32);
```

`mut` on an `external fn` parameter declares that the foreign body may
**store** through the address that parameter denotes — for a compound
parameter that is the pointer the caller hands over, for a scalar it is the
integer's own value. This is a claim about the linked library, not a request:
the linker derives, from the merged `.wasm` bytes, which parameters the body
actually stores through, and rejects the link when that set is not covered by
the declaration. An external whose body stores through an *undeclared*
parameter fails to link with `UndeclaredExternWrite`; a parameter declared
`mut` that the body never writes through links anyway, since `mut` here is a
permission the body is not obliged to exercise. The linker's side of this
check — including exactly what it does and does not prove about *where* a
declared store lands — is the subject of
[The WASM Linker](the-wasm-linker.md).

The set this check requires you to declare is looser than "the parameter
itself denotes the address" suggests. Attribution is affine: a store's
address is attributed to *every* parameter that may contribute a term to it,
not only the one playing the role of base pointer. The ordinary scaled-index
write `mem[ptr + (idx << 2)] = val` attributes to both `ptr` and `idx`, so
`external fn set_elem(ptr: i32, idx: i32, val: i32);` bound to a body that
writes this way must declare `set_elem(mut ptr: i32, mut idx: i32, val:
i32);` — `idx` included, even though `idx` never itself denotes a location
the body writes to; it only scales `ptr`'s. Narrowing the attribution to the
one operand that "looks like" a base pointer would need the linker to tell a
base from an offset, which an affine form alone does not carry; the safe
direction to err in is attributing too broadly, since that can only demand a
wider declaration than the informal reading of `mut` suggests, never let an
undeclared write through. This over-attribution is specific to a *store's*
own address computation: a *read*-only pointer, such as the source of a
`memory.copy(dest, src, size)`, contributes to no store's dependency and so
is never forced `mut` merely for being touched by the same instruction that
writes `dest`.

`mut` is required on every parameter a linked external's body stores through,
compound or scalar alike — there is no exemption for a pointer-shaped `i32`.
Passing a *compound* argument (a struct or an array) to a `mut` position
additionally requires the argument to be rooted at a `mut` binding, enforced
at the call site by analysis rule A047 (see
[Static Analysis](static-analysis.md)): exactly the same requirement an
ordinary assignment through that binding would carry, since a foreign store
through a `mut` parameter is otherwise the one write in the language invisible
at the call site. A *scalar* argument is not checked by A047 — it passes a
value, not a region, so there is no binding at the call site for the rule to
root — which keeps the check honest rather than complete: a foreign store
through an undeclared caller-supplied integer is still possible and is closed
only by a future containment analysis (issue #420).

Parameter names are optional in the declaration, for a parameter no merged
body writes through:

```inference
external fn sum(i32, i32) -> i32;
```

The unnamed form and the named form are equivalent **only when nothing is
written through the parameter**. Neither `ArgKind::Ignored` (`_: i32`) nor
`ArgKind::TypeOnly` (a bare type) carries a mutability field, and the grammar
gives neither a slot for `mut`, so a parameter a linked body stores through
cannot be declared in the unnamed form at all: `external fn sort_pair([i32;
2]);` bound to a library that writes through it has an empty declared write
set by construction and fails to link with `UndeclaredExternWrite`, whose
message says to name the parameter first. Give every parameter of a writing
external a name.

The type signature must match the exported function in the external module exactly.
If the types disagree, the validation step (`validate_extern`, run by the link
driver when it resolves each binding against the real `.wasm` bytes) reports a
`SignatureMismatch` error and no linked module is produced. A related
resolution-time check: two files that each declare and bind the same
`(module, field)` must agree on which parameters are `mut` — a disagreement is
rejected as `ConflictingWriteSet`, naming both files, since the linker checks
the merged body once and cannot honor two different declarations of it.

## Binding an External Function to a Module

An `external fn` declaration is not tied to a particular module until a `use`
directive names the source:

```inference
use { sum } from arith;
```

The name after `from` is a **logical module reference**, not a file path. The
compiler resolves it at build time by searching:

1. The `[wasm-dependencies]` table in `Inference.toml` (highest priority).
2. Directories passed via `-L` / `--wasm-lib-dir` on the command line.
3. Directories listed in the `INFERENCE_WASM_LIB_PATH` environment variable
   (a `PATH`-style list, separated by `:` on Unix and `;` on Windows).

A `::` separator is used for namespaced logical names:

```inference
use { sha256 } from crypto::digest;
```

This resolves to `crypto/digest.wasm` in one of the search directories (using the
platform's path separator at resolution time, so the source stays portable across
operating systems).

Multiple names from the same module are grouped in one `use` directive:

```inference
external fn sum(a: i32, b: i32) -> i32;
external fn neg(a: i32) -> i32;
use { sum, neg } from arith;
```

## Calling an External Function

Once declared and bound, an external function is called exactly like a local one:

```inference
external fn sum(a: i32, b: i32) -> i32;
use { sum } from arith;

pub fn add_three(x: i32) -> i32 {
    return sum(x, 3);
}
```

The type-checker validates the call site (argument types, return type) using the
declared signature. If the call passes type checking, codegen emits `call 0` — the
import index — identically to how it would emit a call to a local function.

## What the Compiler Emits (Intermediate Form)

Before linking, the compiled module contains a WASM import section. The single-import
example above produces:

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

Imported functions occupy the lowest WASM function indices. The local `add_three`
is shifted to index 1 (after the one import at index 0). The call target `call 0`
is the import index, resolved statically during the pre-scan phase: the compiler
resolves the callee name to the `external fn` declaration in scope where the call
is written — its file, and the `spec` block enclosing it — and takes the import
that declaration reserved. Identity is the declaration, not the name, so two files
may each declare `scale` and bind it to a different module, and each file's calls
reach its own.

## The Link Step

`inference-wasm-linker` consumes the intermediate module and the resolved external
`.wasm` binaries, and produces a single self-contained module with the imports
satisfied and removed. The external function bodies are merged in and every index
reference is rewritten into the unified index space.

```text
main.wasm (with imports) ──┐
arith.wasm ────────────────┼──▶ inference-wasm-linker ──▶ unified.wasm
                           │                                     │
                                                          wasm-to-v
                                                                 ↓
                                                          unified.v
```

After linking:
- No `(import …)` referencing `arith` remains in the output.
- The bodies of `sum` (and any functions it calls transitively) are appended after
  `add_three` and called by index.
- The unified module passes validation and flows into `wasm-to-v` as an ordinary
  module whose merged functions translate to Rocq `Definition`s.

For the merge algorithm itself — transitive-closure computation, index re-encoding,
the Tier-B provenance proof, and the full link-error taxonomy — see
[The WASM Linker](the-wasm-linker.md).

## Memory-Merge Feasibility

Not all external functions can be merged. The linker classifies each closure:

| Tier | What the function touches | Merged? |
|------|--------------------------|---------|
| A | No memory, no global or table access, no data — pure arithmetic | Yes |
| B | Memory only through caller-supplied pointers (e.g., `sort(ptr, len)`) | Yes |
| C | Own static data, global access, or indirect-call tables | No — requires a relocatable build |

Tiers A and B turn on what the closure *uses*. A global the function never reads
or writes, and a table with no element segment that nothing names, do not force
Tier C — so the `__stack_pointer` global lld puts in every
`wasm32-unknown-unknown` artifact no longer rejects it on sight.

That is a necessary step toward linking stock toolchain output, not a sufficient
one. Such an artifact also declares a multi-page linear memory, and the merge
never relaxes the anchor module's declared bound, so against an Inference main —
which emits a fixed one-page `(memory 1 1)` — it now clears the tier gate and
fails at memory reconciliation instead. Configurable linear memory is a separate
change.

A Tier-C function produces a clear error at link time:

```text
error: external function `lookup` requires a relocatable build:
         defines or initializes its own static data segments
```

Build the library with a relocatable/position-independent toolchain to enable
Tier-C support in a future release.

## Current Restrictions

- External functions that themselves import their host environment (memory, globals)
  are rejected with a clear error: a static merge cannot reconstruct that environment.
- Analysis rule A024 (`ExternFunctionCall`) is scope-aware: a call to a *bound*
  external (one named by a `use { … } from <module>;` in scope) is allowed and
  flows through the codegen + link path. Only a call to an *unbound* bare
  `external fn` — one with no `use` binding — is rejected, since codegen emits no
  import for it and so cannot compile the call.
- Only one version of each logical module is resolved per build. Multi-version
  dependency resolution is deferred to a future manifest update.
- A `mut` scalar parameter's argument is not checked by analysis rule A047: a
  scalar carries no region for the rule to root a binding in, so a call
  passing a plain `i32` has nothing at the call site to reject. This is an
  accepted, documented gap, not an absence of risk — the shadow stack occupies
  `[0, stack_size)` of the same linear memory a scalar `i32` addresses, and
  under the default layout (one page, entirely stack) a caller's own frame
  sits just below address 65536, so `store_at(65528, 7)` overwrites that
  caller's own locals through a plain `i32.store`, admitted by the linker
  because Tier-B admission proves only that the address *derives from* a
  parameter, not where it lands — see
  [The WASM Linker](the-wasm-linker.md) for what that proof does and does not
  bound. Closing it is tracked in issue #420.

## Example: Two Libraries, One Module

```inference
external fn sort(mut ptr: i32, len: i32);
external fn checksum(ptr: i32, len: i32) -> i32;
use { sort } from collections;
use { checksum } from crypto;

pub fn process(ptr: i32, len: i32) -> i32 {
    sort(ptr, len);
    return checksum(ptr, len);
}
```

The compiler emits two imports (indices 0 and 1), the local `process` at index 2.
The linker searches both `collections.wasm` and `crypto.wasm`, computes the closure
of each export, and merges the bodies into a single output module.

## Related Resources

- [The WASM Linker](the-wasm-linker.md) — the subsystem deep-dive: merge algorithm, feasibility tiers, the Tier-B provenance proof, and the link-error taxonomy
- [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md) — declaring external `.wasm` modules in `Inference.toml` under `[wasm-dependencies]`
- `core/wasm-linker/README.md` — the merge algorithm, tier classification, and entry point API
- `core/wasm-codegen/docs/function-calls-lowering.md` — three-stage index pre-scan and import section emission
- `core/type-checker` — `ExternOrigin`, `extern_origins()`, and the `A024 ExternFunctionCall` analysis rule
- [WebAssembly import section](https://webassembly.github.io/spec/core/binary/modules.html#import-section) — binary format reference
