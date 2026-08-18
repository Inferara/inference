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

Parameter names are optional in the declaration. The following is equivalent:

```inference
external fn sum(i32, i32) -> i32;
```

The type signature must match the exported function in the external module exactly.
If the types disagree, the validation step (`validate_extern`, run by the link
driver when it resolves each binding against the real `.wasm` bytes) reports a
`SignatureMismatch` error and no linked module is produced.

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
is the import index, resolved statically from the `extern_name_to_idx` table built
during the pre-scan phase.

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

## Example: Two Libraries, One Module

```inference
external fn sort(ptr: i32, len: i32);
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
