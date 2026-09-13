# inference-stellar-abi

Rewrites a compiled Inference module into an uploadable Soroban contract: the
Val-ABI layer of the Stellar compilation target.

## Overview

A Stellar contract does not use an ordinary WebAssembly calling convention.
Every method takes and returns 64-bit `Val` words — tagged unions whose low byte
names what the rest of the word holds — and every contract carries a
`contractenvmetav0` custom section declaring the environment protocol it was
built for. An Inference module has neither: it exports `add(i32, i32) -> i32`
and no metadata at all.

This crate is the layer between the two. Given the linked module and the
per-export source-type descriptor code generation recorded
(`inference_wasm_codegen::ExportSignature`), it returns the same module with one
`Val` wrapper appended per exported function, the export section retargeted onto
the wrappers, and the metadata section on the end.

```rust,ignore
let contract = inference_stellar_abi::rewrite(
    &linked_bytes,
    output.export_signatures(),
    inference_stellar_abi::STELLAR_ENV_PROTOCOL,
)?;
```

## Why a rewriter and not an emitter

Nothing in code generation knows about Stellar. The bytes `codegen()` produces
for the Stellar target are the same bytes it produces for Wasm32, which is what
makes *prove the Wasm32 build, deploy the Stellar build* a theorem rather than a
hope: the marshalling layer is a post-link pass over an artifact that has already
been verified, and the wrappers it appends contain no arithmetic, no memory
access, and no control flow beyond one guard per argument.

## The rewrite

```text
1. Validate the input at the WebAssembly 1.0 feature set and parse it,
   recording every section's byte range
2. Match each exported function against the descriptor BY NAME
3. Check admissibility; the first refusal ends the pass and nothing is written
4. Synthesize one wrapper per exported function: a deduplicated
   (i64 x n) -> i64 type entry, a function entry, and a code body
5. Rebuild: every untouched section copied through by byte range; the type,
   function and code sections keep their entries and gain the new ones; the
   export section moves only the targets of wrapped methods
6. Append the wrappers AFTER every existing function, so no index moves
7. Extend the name section, when there is one, with one entry per wrapper
8. Append the contractenvmetav0 custom section
9. Validate the result at WebAssembly 1.0 and fail closed
```

### By name, never by index

The static-merge linker renumbers functions when it folds an external module in,
so a function index recorded at code generation is stale by the time this pass
runs. The export section is what survives. The match is total in both
directions and fails closed on either mismatch: an exported function with no
descriptor would ship as a method that is reachable and always broken, and a
descriptor with no export would ship a contract silently missing a method.

### Appended, never renumbered

The module carries two records naming raw function indices that this crate does
not decode: `inference.checked`, the overflow-guard record, and the name
section's function names. Appending leaves the first byte-identical and lets the
second be extended. It is also the truthful arrangement: a wrapper holds no
guardable arithmetic, so its absence from the guard record is accurate rather
than merely convenient.

## The Val ABI

| Tag | Value | Body |
|---|---|---|
| `False` | 0 | none — the tag is the value |
| `True` | 1 | none — the tag is the value |
| `Void` | 2 | none |
| `U32Val` | 4 | the 32-bit payload in bits 32..64 |
| `I32Val` | 5 | the 32-bit payload in bits 32..64, two's complement |

Every instruction sequence this crate emits was executed against
`soroban-env-host` 28.0.2 and is recorded byte for byte in
`tests/tests/stellar/MEASURED_ABI.md`. The unit tests in `src/val.rs` pin the
same bytes, so the two records cannot drift apart silently.

Two of those measurements are load-bearing and easy to lose:

- **The boolean wrap normalizes before it tags.** A returned `bool` is only
  guaranteed nonzero, and `True` is tag 1 with an empty body, so extending an
  arbitrary truthy `i32` produces a word the host refuses at invoke time.
  Dropping `i32.const 0; i32.ne` ships a contract that uploads cleanly and then
  fails every call returning a truthy value other than exactly 1. A
  bool-identity fixture cannot see this; the golden here uses an inner value of
  42.
- **Two exports sharing a name is an upload refusal.** A wrapper carries its
  method's own name, so the original export of every wrapped function is
  displaced rather than joined.

## What it refuses

The admissible set is M1: `u32`, `i32` and `bool` parameters; those three or
nothing as a return. Every refusal is a `StellarAbiError` variant naming the
export and the offending element:

| Refusal | Variant |
|---|---|
| no exported function | `NoExportedFunctions` |
| an export the descriptor is silent about | `UnknownExport` |
| a described export the module lacks | `MissingExport` |
| a descriptor that disagrees with the module's own signature | `DescriptorDisagreesWithModule` |
| an empty method name | `EmptyExportName` |
| a name over 32 bytes | `ExportNameTooLong` |
| a `__`-prefixed name | `ReservedExportName` |
| a name outside `[A-Za-z0-9_]` | `ExportNameNotASymbol` |
| more than 32 parameters | `TooManyParameters` |
| `i64`, `u64`, a narrow integer, a struct, an array or an enum parameter | `UnsupportedParameter` |
| the same as a return | `UnsupportedReturn` |
| a struct or array return, passed through a hidden pointer | `CompoundReturn` |
| a surviving import | `ImportsUnsupported` |
| a start section | `StartSectionPresent` |
| a module that is already a contract | `AlreadyAContract` |
| more than one `name` custom section | `MultipleNameSections` |
| a declared protocol older than the one Soroban arrived in | `ProtocolPredatesSoroban` |
| bytes outside WebAssembly 1.0, before or after the rewrite | `InputNotWasm1`, `RewrittenNotWasm1` |
| a section that does not decode | `MalformedModule` |

`MalformedModule` is the one row with no reachable input: validation at the
WebAssembly 1.0 feature set runs before the parse, so anything that would fail
to decode has already been refused as `InputNotWasm1`. It exists so that no
decode step in this crate has to guess, and it is listed here so that its
absence from the table cannot be read as an omission.

Two of these are refusals of inputs nothing else rejects. WebAssembly permits
repeated custom sections and no validator complains about two `name` sections,
but this pass rebuilds exactly one function-name map, so carrying both would
duplicate one body and drop the other. And a `contractenvmetav0` section
declaring a protocol below 20 is perfectly well formed — it is refused by every
host that has ever run, at upload, long after the compiler said yes.

## Dependencies

`wasmparser` here is the **stock** upstream crate, deliberately not the in-tree
`inf-wasmparser` fork: the fork decodes and validates the custom
non-deterministic opcodes with no feature gate, so it cannot testify that a
module is WebAssembly 1.0 — which is the one question asked of a parser in this
crate. Default features are off, because the only consumer parses and validates
a core module.
